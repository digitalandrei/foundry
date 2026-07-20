//! Project-scoped persistent-volume file sessions. This is a second
//! reverse-WebSocket tunnel beside the container shell: the browser opens
//! here, the pull-only agent discovers the pending session and dials back.
//! The controller authorizes GitLab project access and sends only approved
//! volume roots; it never accepts a host path from the browser.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use foundry_shared::dto::{
    FileClientMessage, FileServerMessage, FileSessionRequest, FileVolumeRoot,
};
use foundry_shared::{ActorType, GitlabProjectId, ServerId, ServerVolumeId, UserId};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{mpsc, Notify};
use uuid::Uuid;

use crate::audit::{self, AuditEntry};
use crate::auth::agent::AuthenticatedAgent;
use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::access::authorize_project;
use crate::repos::servers::AgentContext;
use crate::repos::{mirror, volumes};
use crate::state::AppState;

pub type FileRegistry = Arc<Mutex<HashMap<Uuid, PendingFileSession>>>;

enum BridgeFrame {
    Text(String),
    Close,
}

type ClaimedBridge = (
    mpsc::Receiver<BridgeFrame>,
    mpsc::Sender<BridgeFrame>,
    Arc<Notify>,
);

pub struct PendingFileSession {
    server_id: ServerId,
    project_id: GitlabProjectId,
    volumes: Vec<FileVolumeRoot>,
    dispatched: bool,
    created_at: Instant,
    to_agent_rx: Option<mpsc::Receiver<BridgeFrame>>,
    to_browser_tx: Option<mpsc::Sender<BridgeFrame>>,
    attached: Arc<Notify>,
}

const CHANNEL_CAP: usize = 128;
const ATTACH_TIMEOUT: Duration = Duration::from_secs(25);
const PING_EVERY: Duration = Duration::from_secs(30);
const SESSION_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct BrowserQuery {
    project_id: GitlabProjectId,
}

/// Open one dual-pane session scoped to the accessible volumes for a
/// project on one server. Project membership is resolved live before the
/// WebSocket upgrade; private roots remain creator-only.
pub async fn browser(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(server_id): Path<ServerId>,
    Query(query): Query<BrowserQuery>,
) -> Result<Response, AppError> {
    let project = mirror::project_by_id(&state.pool, query.project_id).await?;
    authorize_project(
        &state,
        user.id,
        project.instance_id,
        project.gitlab_project_id,
    )
    .await?;
    volumes::require_file_support(&state.pool, server_id).await?;
    let visible = volumes::list(
        &state.pool,
        server_id,
        query.project_id,
        None,
        user.id,
        user.is_admin,
    )
    .await?;
    if visible.is_empty() {
        return Err(AppError::BadRequest(
            "this project has no accessible volumes on the selected server".into(),
        ));
    }
    let roots: Vec<FileVolumeRoot> = visible
        .into_iter()
        .map(|volume| FileVolumeRoot {
            volume_id: volume.id,
            name: volume.name,
            path: volume.path,
        })
        .collect();
    let root_ids: Vec<String> = roots
        .iter()
        .map(|root| root.volume_id.to_string())
        .collect();
    let ip = client_ip(&headers);
    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "VOLUME_FILES_OPENED",
            subject_type: Some("gitlab_project"),
            subject_id: Some(query.project_id.0),
            detail: Some(serde_json::json!({
                "server_id": server_id.to_string(),
                "volume_ids": root_ids,
            })),
            ip_address: ip.as_deref(),
        },
    )
    .await?;

    let session_id = Uuid::now_v7();
    Ok(ws.on_upgrade(move |socket| {
        browser_session(
            state,
            socket,
            session_id,
            server_id,
            query.project_id,
            roots,
            user.id,
            ip,
        )
    }))
}

#[allow(clippy::too_many_arguments)]
async fn browser_session(
    state: AppState,
    socket: WebSocket,
    session_id: Uuid,
    server_id: ServerId,
    project_id: GitlabProjectId,
    volumes: Vec<FileVolumeRoot>,
    user_id: UserId,
    ip_address: Option<String>,
) {
    let (to_agent_tx, to_agent_rx) = mpsc::channel(CHANNEL_CAP);
    let (to_browser_tx, mut to_browser_rx) = mpsc::channel(CHANNEL_CAP);
    let attached = Arc::new(Notify::new());
    crate::state::lock_recover(&state.files).insert(
        session_id,
        PendingFileSession {
            server_id,
            project_id,
            volumes,
            dispatched: false,
            created_at: Instant::now(),
            to_agent_rx: Some(to_agent_rx),
            to_browser_tx: Some(to_browser_tx.clone()),
            attached: attached.clone(),
        },
    );

    let (mut ws_tx, mut ws_rx) = socket.split();
    let audit_state = state.clone();
    let audit_errors_tx = to_browser_tx;
    // Browser → agent. Mutations are audited before forwarding; content
    // and upload chunks are deliberately never copied into the audit log.
    let inbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let Message::Text(text) = msg else {
                if matches!(msg, Message::Close(_)) {
                    break;
                }
                continue;
            };
            let raw = text.to_string();
            let parsed = serde_json::from_str::<FileClientMessage>(&raw);
            let Ok(operation) = parsed else {
                continue;
            };
            if let Some((volume_id, detail)) = mutation_detail(&operation) {
                if let Err(error) = audit::record(
                    &audit_state.pool,
                    AuditEntry {
                        actor_type: ActorType::User,
                        actor_id: Some(user_id),
                        action: "VOLUME_FILE_MUTATION_REQUESTED",
                        subject_type: Some("server_volume"),
                        subject_id: Some(volume_id.0),
                        detail: Some(detail),
                        ip_address: ip_address.as_deref(),
                    },
                )
                .await
                {
                    tracing::error!(%error, "failed to audit volume file mutation");
                    let response = FileServerMessage::Error {
                        request_id: operation.request_id(),
                        message: "the operation could not be audited".into(),
                    };
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = audit_errors_tx.send(BridgeFrame::Text(json)).await;
                    }
                    continue;
                }
            }
            if to_agent_tx.send(BridgeFrame::Text(raw)).await.is_err() {
                break;
            }
        }
    });

    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let timeout = tokio::time::sleep(ATTACH_TIMEOUT);
    tokio::pin!(timeout);
    let mut is_attached = false;
    loop {
        tokio::select! {
            _ = attached.notified(), if !is_attached => { is_attached = true; }
            _ = &mut timeout, if !is_attached => {
                let _ = ws_tx.send(Message::Close(Some(CloseFrame {
                    code: 1011,
                    reason: "the server's agent did not connect — update it to enable volume files".into(),
                }))).await;
                break;
            }
            frame = to_browser_rx.recv() => match frame {
                Some(BridgeFrame::Text(text)) => {
                    if ws_tx.send(Message::text(text)).await.is_err() { break }
                }
                Some(BridgeFrame::Close) | None => break,
            },
            _ = ping.tick() => {
                if ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }
    inbound.abort();
    crate::state::lock_recover(&state.files).remove(&session_id);
}

fn mutation_detail(message: &FileClientMessage) -> Option<(ServerVolumeId, serde_json::Value)> {
    let detail = match message {
        FileClientMessage::WriteText {
            volume_id, path, ..
        } => (
            *volume_id,
            serde_json::json!({"operation":"write_text","path":path}),
        ),
        FileClientMessage::Mkdir {
            volume_id, path, ..
        } => (
            *volume_id,
            serde_json::json!({"operation":"mkdir","path":path}),
        ),
        FileClientMessage::Rename {
            volume_id,
            from,
            to,
            ..
        } => (
            *volume_id,
            serde_json::json!({"operation":"rename","from":from,"to":to}),
        ),
        FileClientMessage::Copy {
            from_volume_id,
            from,
            to_volume_id,
            to,
            ..
        } => (
            *to_volume_id,
            serde_json::json!({
                "operation":"copy",
                "from_volume_id":from_volume_id.to_string(),
                "from":from,
                "to":to,
            }),
        ),
        FileClientMessage::Move {
            from_volume_id,
            from,
            to_volume_id,
            to,
            ..
        } => (
            *to_volume_id,
            serde_json::json!({
                "operation":"move",
                "from_volume_id":from_volume_id.to_string(),
                "from":from,
                "to":to,
            }),
        ),
        FileClientMessage::Delete {
            volume_id, path, ..
        } => (
            *volume_id,
            serde_json::json!({"operation":"delete","path":path}),
        ),
        FileClientMessage::UploadStart {
            volume_id,
            path,
            size,
            ..
        } => (
            *volume_id,
            serde_json::json!({"operation":"upload","path":path,"size":size}),
        ),
        FileClientMessage::List { .. }
        | FileClientMessage::ReadText { .. }
        | FileClientMessage::Download { .. }
        | FileClientMessage::UploadChunk { .. }
        | FileClientMessage::UploadFinish { .. } => return None,
    };
    Some(detail)
}

/// Long-poll for a pending file session on this authenticated server.
pub async fn agent_next(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    for _ in 0..20 {
        if let Some(request) = take_pending(&state.files, ctx.server_id) {
            return Ok(axum::Json(request).into_response());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

fn take_pending(registry: &FileRegistry, server_id: ServerId) -> Option<FileSessionRequest> {
    let mut sessions = crate::state::lock_recover(registry);
    sessions.retain(|_, pending| pending.dispatched || pending.created_at.elapsed() < SESSION_TTL);
    let (session_id, pending) = sessions.iter_mut().find(|(_, pending)| {
        pending.server_id == server_id && !pending.dispatched && pending.to_agent_rx.is_some()
    })?;
    pending.dispatched = true;
    Some(FileSessionRequest {
        session_id: *session_id,
        project_id: pending.project_id,
        volumes: pending.volumes.clone(),
    })
}

/// The target server's agent dials this WebSocket back; the controller
/// validates its server identity and bridges JSON frames verbatim.
pub async fn agent_attach(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Path(session_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (to_agent_rx, to_browser_tx, attached) = take_bridge(&state, &ctx, session_id)?;
    Ok(ws.on_upgrade(move |socket| agent_session(socket, to_agent_rx, to_browser_tx, attached)))
}

fn take_bridge(
    state: &AppState,
    ctx: &AgentContext,
    session_id: Uuid,
) -> Result<ClaimedBridge, AppError> {
    let mut sessions = crate::state::lock_recover(&state.files);
    let pending = sessions
        .get_mut(&session_id)
        .ok_or(AppError::NotFound("file session not found"))?;
    if pending.server_id != ctx.server_id {
        return Err(AppError::Forbidden);
    }
    let receiver = pending
        .to_agent_rx
        .take()
        .ok_or(AppError::BadRequest("file session already attached".into()))?;
    let sender = pending
        .to_browser_tx
        .take()
        .ok_or(AppError::BadRequest("file session already attached".into()))?;
    Ok((receiver, sender, pending.attached.clone()))
}

async fn agent_session(
    socket: WebSocket,
    mut to_agent_rx: mpsc::Receiver<BridgeFrame>,
    to_browser_tx: mpsc::Sender<BridgeFrame>,
    attached: Arc<Notify>,
) {
    attached.notify_one();
    let (mut ws_tx, mut ws_rx) = socket.split();
    let close_tx = to_browser_tx.clone();
    let outbound = tokio::spawn(async move {
        while let Some(Ok(message)) = ws_rx.next().await {
            match message {
                Message::Text(text)
                    if to_browser_tx
                        .send(BridgeFrame::Text(text.to_string()))
                        .await
                        .is_err() =>
                {
                    break;
                }
                Message::Text(_) => {}
                Message::Close(_) => break,
                _ => {}
            }
        }
        let _ = close_tx.send(BridgeFrame::Close).await;
    });

    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            frame = to_agent_rx.recv() => match frame {
                Some(BridgeFrame::Text(text)) => {
                    if ws_tx.send(Message::text(text)).await.is_err() { break }
                }
                Some(BridgeFrame::Close) | None => break,
            },
            _ = ping.tick() => {
                if ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            },
        }
    }
    outbound.abort();
}

#[cfg(test)]
mod tests {
    use super::mutation_detail;
    use foundry_shared::dto::FileClientMessage;
    use foundry_shared::ServerVolumeId;
    use uuid::Uuid;

    #[test]
    fn audit_detail_never_contains_text_or_upload_content() {
        let volume_id = ServerVolumeId::new();
        let write = FileClientMessage::WriteText {
            request_id: Uuid::now_v7(),
            volume_id,
            path: "settings/config.json".into(),
            content: "secret material".into(),
        };
        let (_, detail) = mutation_detail(&write).expect("mutation");
        let json = detail.to_string();
        assert!(json.contains("settings/config.json"));
        assert!(!json.contains("secret material"));

        let chunk = FileClientMessage::UploadChunk {
            request_id: Uuid::now_v7(),
            data: "secret bytes".into(),
        };
        assert!(mutation_detail(&chunk).is_none());
    }
}

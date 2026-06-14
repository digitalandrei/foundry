//! Interactive container shell — a web terminal without SSH (operator
//! request), built to respect the pull-only invariant (docs/ARCHITECTURE.md
//! § Pull-Based Agent Model): the controller never dials the agent.
//!
//! Flow: the browser opens a WebSocket to `/api/deployments/{id}/shell`;
//! the controller registers a *pending session* and the target server's
//! agent — already long-polling `/agent/shell/next` — learns of it and
//! dials BACK its own WebSocket to `/agent/shell/attach/{id}`. The
//! controller then bridges the two sockets **verbatim** (binary = TTY
//! I/O, text = a `{"type":"resize",…}` control the agent interprets); the
//! agent runs `docker exec` (bash→sh) on the managed container.
//!
//! Lock discipline: the registry Mutex is only held for short synchronous
//! sections, never across an await (docs/RUST_RULES.md).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use foundry_shared::dto::ShellRequest;
use foundry_shared::{ActorType, DeploymentId, DeploymentState, ServerId};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Notify};
use uuid::Uuid;

use crate::audit::{self, AuditEntry};
use crate::auth::agent::AuthenticatedAgent;
use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::state::AppState;

/// A WS frame forwarded verbatim between browser and agent. The
/// controller never parses the payload — binary is raw TTY bytes, text is
/// a resize control the agent reads.
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
}

/// A shell session waiting for (or bridged to) its server's agent.
pub struct PendingShell {
    server_id: ServerId,
    deployment_id: DeploymentId,
    /// Set once the agent has been handed this session (no re-dispatch).
    dispatched: bool,
    created_at: Instant,
    /// Agent reads browser input here (taken by the attach handler).
    to_agent_rx: Option<mpsc::Receiver<Frame>>,
    /// Agent writes container output here (taken by the attach handler).
    to_browser_tx: Option<mpsc::Sender<Frame>>,
    /// Fired when the agent attaches — unblocks the browser's wait.
    attached: Arc<Notify>,
}

pub type ShellRegistry = Arc<Mutex<HashMap<Uuid, PendingShell>>>;

const CHANNEL_CAP: usize = 512;
const ATTACH_TIMEOUT: Duration = Duration::from_secs(25);
const PING_EVERY: Duration = Duration::from_secs(30);
/// Sweep sessions that were created but never bridged (browser vanished
/// before the agent attached) after this long.
const SESSION_TTL: Duration = Duration::from_secs(60);

// ── Browser side (`GET /api/deployments/{id}/shell`) ─────────────────

/// Owner/admin only; the deployment must be RUNNING. Authorization
/// happens before the upgrade so a refusal is a normal HTTP error.
pub async fn browser(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<DeploymentId>,
) -> Result<Response, AppError> {
    let d = crate::repos::deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    if d.state != DeploymentState::Running {
        return Err(AppError::BadRequest(
            "a shell can only be opened on a running deployment".into(),
        ));
    }

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "SHELL_OPENED",
            subject_type: Some("deployment"),
            subject_id: Some(id.0),
            detail: None,
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    let session_id = Uuid::now_v7();
    Ok(ws.on_upgrade(move |socket| browser_session(state, socket, session_id, d.server_id, id)))
}

async fn browser_session(
    state: AppState,
    socket: WebSocket,
    session_id: Uuid,
    server_id: ServerId,
    deployment_id: DeploymentId,
) {
    let (to_agent_tx, to_agent_rx) = mpsc::channel::<Frame>(CHANNEL_CAP);
    let (to_browser_tx, mut to_browser_rx) = mpsc::channel::<Frame>(CHANNEL_CAP);
    let attached = Arc::new(Notify::new());
    state.shells.lock().expect("shells lock").insert(
        session_id,
        PendingShell {
            server_id,
            deployment_id,
            dispatched: false,
            created_at: Instant::now(),
            to_agent_rx: Some(to_agent_rx),
            to_browser_tx: Some(to_browser_tx),
            attached: attached.clone(),
        },
    );

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Browser → agent: pump keystrokes (binary) and resize (text).
    let inbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let frame = match msg {
                Message::Binary(b) => Frame::Binary(b.to_vec()),
                Message::Text(t) => Frame::Text(t.to_string()),
                Message::Close(_) => break,
                _ => continue,
            };
            if to_agent_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    // Agent → browser, with attach-timeout + keepalive pings (defeat the
    // nginx/Cloudflare idle close on a quiet shell).
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await; // immediate first tick
    let timeout = tokio::time::sleep(ATTACH_TIMEOUT);
    tokio::pin!(timeout);
    let mut is_attached = false;
    loop {
        tokio::select! {
            _ = attached.notified(), if !is_attached => { is_attached = true; }
            _ = &mut timeout, if !is_attached => {
                let _ = ws_tx
                    .send(Message::Close(Some(CloseFrame {
                        code: 1011,
                        reason: "the server's agent did not connect — update the agent to enable shells".into(),
                    })))
                    .await;
                break;
            }
            frame = to_browser_rx.recv() => match frame {
                Some(Frame::Binary(b)) => if ws_tx.send(Message::binary(b)).await.is_err() { break },
                Some(Frame::Text(t)) => if ws_tx.send(Message::text(t)).await.is_err() { break },
                None => break, // agent side closed the exec
            },
            _ = ping.tick() => if ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() { break },
        }
    }

    inbound.abort();
    state
        .shells
        .lock()
        .expect("shells lock")
        .remove(&session_id);
}

// ── Agent side ───────────────────────────────────────────────────────

/// `GET /agent/shell/next` — long-poll for a pending shell on this
/// server. Returns the first undispatched session (and marks it
/// dispatched) or 204 after a short hold.
pub async fn agent_next(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    for _ in 0..20 {
        if let Some(req) = take_pending(&state.shells, ctx.server_id) {
            return Ok(axum::Json(req).into_response());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Claim the first undispatched session for a server (short sync section).
fn take_pending(registry: &ShellRegistry, server_id: ServerId) -> Option<ShellRequest> {
    let mut reg = registry.lock().expect("shells lock");
    // Opportunistic TTL sweep of sessions whose browser vanished pre-bridge.
    reg.retain(|_, p| p.dispatched || p.created_at.elapsed() < SESSION_TTL);
    let (id, pending) = reg
        .iter_mut()
        .find(|(_, p)| p.server_id == server_id && !p.dispatched && p.to_agent_rx.is_some())?;
    pending.dispatched = true;
    Some(ShellRequest {
        session_id: *id,
        deployment_id: pending.deployment_id,
    })
}

/// `GET /agent/shell/attach/{session_id}` — the agent dials this back as a
/// WebSocket; we bridge it to the waiting browser session.
pub async fn agent_attach(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Path(session_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (to_agent_rx, to_browser_tx, attached) = {
        let mut reg = state.shells.lock().expect("shells lock");
        let p = reg
            .get_mut(&session_id)
            .ok_or(AppError::NotFound("shell session not found"))?;
        if p.server_id != ctx.server_id {
            return Err(AppError::Forbidden);
        }
        let rx = p.to_agent_rx.take().ok_or(AppError::BadRequest(
            "shell session already attached".into(),
        ))?;
        let tx = p.to_browser_tx.take().ok_or(AppError::BadRequest(
            "shell session already attached".into(),
        ))?;
        (rx, tx, p.attached.clone())
    };
    Ok(ws.on_upgrade(move |socket| agent_session(socket, to_agent_rx, to_browser_tx, attached)))
}

async fn agent_session(
    socket: WebSocket,
    mut to_agent_rx: mpsc::Receiver<Frame>,
    to_browser_tx: mpsc::Sender<Frame>,
    attached: Arc<Notify>,
) {
    attached.notify_one();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Agent → browser (container output).
    let outbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let frame = match msg {
                Message::Binary(b) => Frame::Binary(b.to_vec()),
                Message::Text(t) => Frame::Text(t.to_string()),
                Message::Close(_) => break,
                _ => continue,
            };
            if to_browser_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    // Browser → agent (stdin + resize), with keepalive pings.
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            frame = to_agent_rx.recv() => match frame {
                Some(Frame::Binary(b)) => if ws_tx.send(Message::binary(b)).await.is_err() { break },
                Some(Frame::Text(t)) => if ws_tx.send(Message::text(t)).await.is_err() { break },
                None => break, // browser closed
            },
            _ = ping.tick() => if ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() { break },
        }
    }
    outbound.abort();
}

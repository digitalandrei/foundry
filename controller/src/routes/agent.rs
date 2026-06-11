//! `/agent/*` — the pull-only agent protocol (docs/API.md § Agent API).
//! Enroll authenticates with a single-use token; everything else with
//! the permanent agent identity.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use foundry_shared::dto::{AgentEnrollRequest, AgentEnrollResponse, HeartbeatRequest};
use foundry_shared::ActorType;

use crate::audit::{self, AuditEntry};
use crate::auth::agent::AuthenticatedAgent;
use crate::error::AppError;
use crate::repos::servers;
use crate::state::AppState;

/// Default agent poll cadence handed out at enrollment.
const POLL_INTERVAL_SECS: u64 = 15;

pub async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<AgentEnrollRequest>,
) -> Result<Json<AgentEnrollResponse>, AppError> {
    let hostname = req.hostname.trim();
    if hostname.is_empty() || hostname.len() > 255 {
        return Err(AppError::BadRequest("hostname is required".into()));
    }

    let enrolled = servers::enroll(
        &state.pool,
        req.token.trim(),
        hostname,
        req.agent_version.trim(),
        req.os_version.as_deref(),
    )
    .await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::Agent,
            actor_id: None,
            action: "AGENT_ENROLLED",
            subject_type: Some("server"),
            subject_id: Some(enrolled.server_id.0),
            detail: Some(serde_json::json!({
                "server": enrolled.server_name,
                "hostname": hostname,
                "agent_version": req.agent_version,
            })),
            ip_address: None,
        },
    )
    .await?;
    tracing::info!(server = %enrolled.server_name, %hostname, "agent enrolled");

    Ok(Json(AgentEnrollResponse {
        agent_id: enrolled.agent_id.to_string(),
        agent_secret: enrolled.agent_secret,
        server_id: enrolled.server_id,
        server_name: enrolled.server_name,
        poll_interval_secs: POLL_INTERVAL_SECS,
    }))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(req): Json<HeartbeatRequest>,
) -> Result<StatusCode, AppError> {
    servers::record_heartbeat(&state.pool, ctx.server_id, req.agent_version.trim()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Full snapshot upload (docs/GPU-MIG.md). Bounds: an authenticated
/// agent is not blindly trusted (docs/SECURITY.md § Input hygiene).
pub async fn inventory(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(snap): Json<foundry_shared::dto::InventorySnapshot>,
) -> Result<StatusCode, AppError> {
    if snap.gpus.len() > 64 || snap.containers.len() > 1024 {
        return Err(AppError::BadRequest("snapshot exceeds sane bounds".into()));
    }
    crate::repos::inventory::apply_snapshot(&state.pool, ctx.server_id, &snap).await?;
    tracing::debug!(
        server = %ctx.server_id,
        gpus = snap.gpus.len(),
        containers = snap.containers.len(),
        "inventory applied"
    );
    Ok(StatusCode::NO_CONTENT)
}

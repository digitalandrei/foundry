//! `/api/servers` — list, create (named, GitLab-agent style), and
//! enrollment-token minting (docs/API.md).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{CreateServerRequest, EnrollmentTokenResponse, ServerSummary};
use foundry_shared::{ActorType, ServerId};

use crate::audit::{self, AuditEntry};
use crate::auth::client_ip;
use crate::auth::session::{AdminUser, CurrentUser};
use crate::error::AppError;
use crate::repos::servers;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<ServerSummary>>, AppError> {
    Ok(Json(servers::list(&state.pool).await?))
}

fn registration_command(public_url: &str, token: &str) -> String {
    format!("sudo foundry-agent --register --url {public_url} --token {token}")
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Json(req): Json<CreateServerRequest>,
) -> Result<Json<EnrollmentTokenResponse>, AppError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(AppError::BadRequest("name must be 1–255 characters".into()));
    }

    let server_id = servers::create(&state.pool, name).await?;
    let (token, expires_at) =
        servers::issue_enrollment_token(&state.pool, server_id, admin.id).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(admin.id),
            action: "SERVER_CREATED",
            subject_type: Some("server"),
            subject_id: Some(server_id.0),
            detail: Some(serde_json::json!({ "name": name })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    Ok(Json(EnrollmentTokenResponse {
        server: servers::get_summary(&state.pool, server_id).await?,
        command: registration_command(&state.public_url, &token),
        token,
        expires_at,
    }))
}

/// Re-mint the enrollment token (e.g. expired before use, or
/// deliberate re-enrollment). Older unused tokens are revoked.
pub async fn regenerate_token(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(server_id): Path<ServerId>,
) -> Result<Json<EnrollmentTokenResponse>, AppError> {
    let server = servers::get_summary(&state.pool, server_id).await?;
    let (token, expires_at) =
        servers::issue_enrollment_token(&state.pool, server_id, admin.id).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(admin.id),
            action: "ENROLLMENT_TOKEN_CREATED",
            subject_type: Some("server"),
            subject_id: Some(server_id.0),
            detail: Some(serde_json::json!({ "name": server.name })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    Ok(Json(EnrollmentTokenResponse {
        server,
        command: registration_command(&state.public_url, &token),
        token,
        expires_at,
    }))
}

//! `/api/servers` — list, create (named, GitLab-agent style), and
//! enrollment-token minting (docs/API.md).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{
    CreateFleetTokenRequest, CreateServerRequest, EnrollmentTokenResponse, FleetTokenResponse,
    FleetTokenSummary, ServerSummary,
};
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

#[derive(serde::Deserialize)]
pub struct MetricsQuery {
    minutes: Option<i64>,
}

/// Newest sample per server — live labels on the dashboard slot grid.
pub async fn metrics_latest(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<foundry_shared::dto::LatestMetricsResponse>, AppError> {
    Ok(Json(foundry_shared::dto::LatestMetricsResponse {
        servers: crate::repos::metrics::latest_per_server(&state.pool).await?,
    }))
}

/// Telemetry series for the dedicated server page.
pub async fn metrics(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
    axum::extract::Query(q): axum::extract::Query<MetricsQuery>,
) -> Result<Json<Vec<foundry_shared::dto::MetricsPoint>>, AppError> {
    let minutes = q.minutes.unwrap_or(60).clamp(5, 1440);
    Ok(Json(
        crate::repos::metrics::range(&state.pool, server_id, minutes).await?,
    ))
}

/// Detail: GPUs/slots + the docker-ps snapshot (docs/API.md).
pub async fn detail(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
) -> Result<Json<foundry_shared::dto::ServerDetail>, AppError> {
    let server = servers::get_summary(&state.pool, server_id).await?;
    let (docker_version, nvidia_driver_version) =
        servers::runtime_versions(&state.pool, server_id).await?;
    let containers = crate::repos::inventory::containers_for_server(&state.pool, server_id).await?;
    let gpus = server.gpus.clone();
    Ok(Json(foundry_shared::dto::ServerDetail {
        server,
        docker_version,
        nvidia_driver_version,
        gpus,
        containers,
    }))
}

fn registration_command(public_url: &str, token: &str) -> String {
    format!("sudo foundry-agent --register --url {public_url} --token {token}")
}

fn fleet_registration_command(public_url: &str, token: &str) -> String {
    format!("sudo foundry-agent --register --url {public_url} --fleet-token {token}")
}

/// Adopt an externally-created container into a managed deployment (admin
/// only) — gives it Foundry's control surface (logs, console, stop, delete,
/// replace). The container must occupy a GPU slot (docs/API.md § Adopt).
pub async fn adopt_container(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path((server_id, container_id)): Path<(ServerId, String)>,
) -> Result<Json<foundry_shared::dto::DeploymentSummary>, AppError> {
    let container_id = container_id.trim();
    let dep_id =
        crate::repos::deployments::adopt(&state.pool, server_id, container_id, admin.id).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(admin.id),
            action: "CONTAINER_ADOPTED",
            subject_type: Some("deployment"),
            subject_id: Some(dep_id.0),
            detail: Some(serde_json::json!({
                "server_id": server_id,
                "container_id": container_id,
            })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    let detail = crate::repos::deployments::detail(&state.pool, dep_id).await?;
    Ok(Json(detail.summary))
}

/// Mint a reusable, time-limited fleet enrollment key (admin only). Agents
/// presenting it auto-enroll under their own hostname (docs/API.md § Fleet
/// Enrollment).
pub async fn create_fleet_token(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Json(req): Json<CreateFleetTokenRequest>,
) -> Result<Json<FleetTokenResponse>, AppError> {
    // Expiry bounds: min 1 week, max 3 months (operator rule; default 1
    // month is the UI's pick). Kept in sync with FleetKeyDialog.
    let ttl_hours = req.ttl_hours.clamp(24 * 7, 24 * 90);
    if let Some(max) = req.max_uses {
        if max == 0 {
            return Err(AppError::BadRequest(
                "max_uses must be at least 1 (omit for unlimited)".into(),
            ));
        }
    }

    let (token, expires_at) =
        servers::issue_fleet_token(&state.pool, ttl_hours, req.max_uses, admin.id).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(admin.id),
            action: "FLEET_TOKEN_CREATED",
            subject_type: None,
            subject_id: None,
            detail: Some(serde_json::json!({
                "ttl_hours": ttl_hours,
                "max_uses": req.max_uses,
            })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    Ok(Json(FleetTokenResponse {
        command: fleet_registration_command(&state.public_url, &token),
        token,
        expires_at,
        max_uses: req.max_uses,
    }))
}

/// List live fleet keys (admin) — metadata only, never the raw token.
pub async fn list_fleet_tokens(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<FleetTokenSummary>>, AppError> {
    Ok(Json(servers::list_fleet_tokens(&state.pool).await?))
}

/// Delete (revoke) a fleet key (admin) — works even before it expires.
pub async fn delete_fleet_token(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    servers::delete_fleet_token(&state.pool, id).await?;
    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(admin.id),
            action: "FLEET_TOKEN_DELETED",
            subject_type: None,
            subject_id: Some(id),
            detail: None,
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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

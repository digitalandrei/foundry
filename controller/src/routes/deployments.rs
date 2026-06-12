//! `/api/deployments` — create (drag-drop), list, lifecycle actions,
//! replacement (docs/API.md; plans/phase-06.md). Plus per-server
//! persistent volumes.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{CreateDeploymentRequest, DeploymentSummary, ServerVolume};
use foundry_shared::{
    ActorType, DeploymentId, DeploymentState, ServerId, ServerVolumeId, TaskType,
};

use crate::audit::{self, AuditEntry};
use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::repos::{deployments, mirror, tasks, users, volumes};
use crate::state::AppState;

/// Strip the scheme off a registry URL for image references
/// (`https://g.protv.ro:5050` → `g.protv.ro:5050`).
fn registry_host(registry_url: &str) -> &str {
    registry_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let tag = mirror::tag_ref(&state.pool, req.registry_tag_id).await?;
    let image_ref = format!(
        "{}/{}:{}",
        registry_host(&tag.registry_url),
        tag.repo_path,
        tag.tag_name
    );

    // Authorization stays personal: the deployer needs an account on
    // the image's instance (pull token is minted from THEIR token at
    // dispatch). Local operator accounts may deploy public images.
    let has_account = users::account_tokens(&state.pool, &state.secrets, user.id)
        .await?
        .iter()
        .any(|a| a.instance_id == tag.instance_id);
    if !has_account && !user.is_admin {
        return Err(AppError::Forbidden);
    }

    let new = deployments::create(
        &state.pool,
        &state.secrets,
        &req,
        &image_ref,
        tag.instance_id,
        user.id,
        None,
        state.apps_domain.as_deref(),
    )
    .await?;

    let mut tx = state.pool.begin().await?;
    tasks::enqueue_deploy(&mut tx, new.id).await?;
    tx.commit().await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "DEPLOYMENT_CREATED",
            subject_type: Some("deployment"),
            subject_id: Some(new.id.0),
            detail: Some(serde_json::json!({
                "image_ref": image_ref,
                "name": new.container_name,
                "slot_id": req.slot_id.to_string(),
            })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    summary_of(&state, new.id).await.map(Json)
}

async fn summary_of(state: &AppState, id: DeploymentId) -> Result<DeploymentSummary, AppError> {
    deployments::list(&state.pool)
        .await?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(AppError::NotFound("deployment not found"))
}

/// Overlay the in-memory live-progress text (AppState.progress) onto
/// summaries fresh from the DB.
fn overlay_progress(state: &AppState, deployments: &mut [DeploymentSummary]) {
    let map = state.progress.lock().expect("progress lock");
    for d in deployments.iter_mut() {
        d.status_detail = map.get(&d.id.0).cloned();
    }
}

pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<DeploymentSummary>>, AppError> {
    let mut out = deployments::list(&state.pool).await?;
    overlay_progress(&state, &mut out);
    Ok(Json(out))
}

/// Slot/deployment detail dialog: summary + mounts + env names.
/// Org-visible like the list (docs/SECURITY.md — control stays
/// owner/admin); env *values* never leave the server.
pub async fn detail(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<DeploymentId>,
) -> Result<Json<foundry_shared::dto::DeploymentDetail>, AppError> {
    let mut detail = deployments::detail(&state.pool, id).await?;
    overlay_progress(&state, std::slice::from_mut(&mut detail.summary));
    Ok(Json(detail))
}

pub async fn stop(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<DeploymentId>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let d = deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    tasks::enqueue_lifecycle(
        &state.pool,
        &d,
        TaskType::StopContainer,
        (d.state, DeploymentState::Stopping),
        user.id,
    )
    .await?;
    summary_of(&state, id).await.map(Json)
}

pub async fn restart(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<DeploymentId>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let d = deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    tasks::enqueue_lifecycle(
        &state.pool,
        &d,
        TaskType::RestartContainer,
        (d.state, DeploymentState::Restarting),
        user.id,
    )
    .await?;
    summary_of(&state, id).await.map(Json)
}

pub async fn remove(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<DeploymentId>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let d = deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    tasks::enqueue_lifecycle(
        &state.pool,
        &d,
        TaskType::RemoveContainer,
        (d.state, DeploymentState::Removing),
        user.id,
    )
    .await?;
    summary_of(&state, id).await.map(Json)
}

/// Dismiss a FAILED deployment — clears the error and frees a stuck
/// slot (controller-side; no agent needed). Owner/admin.
pub async fn dismiss(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<DeploymentId>,
) -> Result<axum::http::StatusCode, AppError> {
    let d = deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    deployments::dismiss(&state.pool, id).await?;
    state.progress.lock().expect("progress lock").remove(&id.0);

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "DEPLOYMENT_DISMISSED",
            subject_type: Some("deployment"),
            subject_id: Some(id.0),
            detail: None,
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;
    // The deployment is REMOVED now (gone from the active list).
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Replace the deployment occupying a slot: create the successor, link
/// it, and start the stop→remove→deploy chain
/// (docs/ARCHITECTURE.md § Replacement workflow).
pub async fn replace(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(old_id): Path<DeploymentId>,
    Json(mut req): Json<CreateDeploymentRequest>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let old = deployments::get(&state.pool, old_id).await?;
    // Lifecycle control is owner/admin-only (fleet *visibility* stays
    // org-wide by design — docs/SECURITY.md).
    if old.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    req.slot_id = old.slot_id;

    let tag = mirror::tag_ref(&state.pool, req.registry_tag_id).await?;
    let image_ref = format!(
        "{}/{}:{}",
        registry_host(&tag.registry_url),
        tag.repo_path,
        tag.tag_name
    );
    let has_account = users::account_tokens(&state.pool, &state.secrets, user.id)
        .await?
        .iter()
        .any(|a| a.instance_id == tag.instance_id);
    if !has_account && !user.is_admin {
        return Err(AppError::Forbidden);
    }

    // create() validates the old deployment, links it, transitions it,
    // and enqueues its stop/remove — all in one transaction.
    let new = deployments::create(
        &state.pool,
        &state.secrets,
        &req,
        &image_ref,
        tag.instance_id,
        user.id,
        Some(old_id),
        state.apps_domain.as_deref(),
    )
    .await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "DEPLOYMENT_REPLACED",
            subject_type: Some("deployment"),
            subject_id: Some(old_id.0),
            detail: Some(serde_json::json!({
                "replaced_by": new.id.to_string(),
                "image_ref": image_ref,
            })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;

    summary_of(&state, new.id).await.map(Json)
}

// ── Persistent volumes ──────────────────────────────────────────────

pub async fn list_volumes(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(server_id): Path<ServerId>,
) -> Result<Json<Vec<ServerVolume>>, AppError> {
    let scope = if user.is_admin { None } else { Some(user.id) };
    Ok(Json(volumes::list(&state.pool, server_id, scope).await?))
}

/// Delete a volume AND its data (explicit, irreversible). Creator or
/// admin only; refused while any active deployment mounts it.
pub async fn delete_volume(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(volume_id): Path<ServerVolumeId>,
) -> Result<axum::http::StatusCode, AppError> {
    let vol = volumes::get(&state.pool, volume_id).await?;
    if vol.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    // Attached-check + task + row delete in ONE transaction (review
    // finding: TOCTOU vs a concurrent deploy mounting it).
    volumes::delete_guarded(&state.pool, volume_id, vol.server_id, &vol.path).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "VOLUME_DELETED",
            subject_type: Some("server_volume"),
            subject_id: Some(volume_id.0),
            detail: Some(serde_json::json!({ "name": vol.name, "path": vol.path })),
            ip_address: client_ip(&headers).as_deref(),
        },
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

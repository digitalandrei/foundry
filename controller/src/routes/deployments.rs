//! `/api/deployments` — create (drag-drop), list, lifecycle actions,
//! replacement (docs/API.md; plans/phase-06.md).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{CreateDeploymentRequest, DeployTarget, DeploymentSummary};
use foundry_shared::{DeploymentId, DeploymentState, TaskType};

use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::access::authorize_project;
use crate::repos::{deployments, mirror, tasks};
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

    authorize_project(&state, user.id, tag.instance_id, tag.gitlab_project_id).await?;

    let new = deployments::create(
        &state.pool,
        &state.secrets,
        &req,
        &image_ref,
        tag.instance_id,
        tag.project_id,
        user.id,
        None,
        state.apps_domain.as_deref(),
        client_ip(&headers).as_deref(),
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
    let map = crate::state::lock_recover(&state.progress);
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

/// Captured container logs for the deployment detail view (merged
/// stdout+stderr, bounded recent window). Org-visible like the list —
/// fleet *visibility* is org-wide (docs/SECURITY.md).
pub async fn logs(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<DeploymentId>,
) -> Result<Json<foundry_shared::dto::DeploymentLogsView>, AppError> {
    // 404 on an unknown deployment (don't leak an empty body for a typo).
    deployments::get(&state.pool, id).await?;
    Ok(Json(crate::repos::logs::recent(&state.pool, id).await?))
}

pub async fn stop(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
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
        client_ip(&headers).as_deref(),
    )
    .await?;
    summary_of(&state, id).await.map(Json)
}

pub async fn restart(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<DeploymentId>,
) -> Result<Json<DeploymentSummary>, AppError> {
    let d = deployments::get(&state.pool, id).await?;
    if d.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    // Stop tears the container and image down (no host garbage), so there
    // is nothing to "start" — restart re-pulls and recreates from the
    // stored spec.
    tasks::enqueue_restart(&state.pool, &d, user.id, client_ip(&headers).as_deref()).await?;
    summary_of(&state, id).await.map(Json)
}

pub async fn remove(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
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
        client_ip(&headers).as_deref(),
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
    deployments::dismiss(&state.pool, id, user.id, client_ip(&headers).as_deref()).await?;
    crate::state::lock_recover(&state.progress).remove(&id.0);
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
    // The successor re-locks exactly what the outgoing deployment held —
    // the same group (re-locks every member GPU) or the same single slot.
    req.target = match old.gpu_group_id {
        Some(gpu_group_id) => DeployTarget::Group { gpu_group_id },
        None => DeployTarget::Slot {
            slot_id: old.slot_id,
        },
    };

    let tag = mirror::tag_ref(&state.pool, req.registry_tag_id).await?;
    let image_ref = format!(
        "{}/{}:{}",
        registry_host(&tag.registry_url),
        tag.repo_path,
        tag.tag_name
    );
    authorize_project(&state, user.id, tag.instance_id, tag.gitlab_project_id).await?;
    // Creator/admin may replace with another accessible project. A
    // collaborator may replace only within the deployment's own project;
    // the live check above proves current GitLab membership.
    if old.created_by != user.id && !user.is_admin && old.project_id != Some(tag.project_id) {
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
        tag.project_id,
        user.id,
        Some(old_id),
        state.apps_domain.as_deref(),
        client_ip(&headers).as_deref(),
    )
    .await?;

    summary_of(&state, new.id).await.map(Json)
}

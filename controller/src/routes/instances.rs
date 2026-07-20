//! `/api/instances` — public minimal list for the login picker (the
//! one unauthenticated /api endpoint, by design), full list + onboard
//! for admins (docs/GITLAB-INTEGRATION.md § Multi-Instance Model).

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use foundry_shared::dto::{
    CreateInstanceRequest, InstanceAdmin, InstancePublic, UpdateInstanceRequest,
};
use foundry_shared::GitlabInstanceId;

use crate::auth::client_ip;
use crate::auth::session::AdminUser;
use crate::error::AppError;
use crate::repos::instances;
use crate::state::AppState;

pub async fn list_public(
    State(state): State<AppState>,
) -> Result<Json<Vec<InstancePublic>>, AppError> {
    Ok(Json(instances::list_public(&state.pool).await?))
}

pub async fn list_admin(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<InstanceAdmin>>, AppError> {
    Ok(Json(instances::list_admin(&state.pool).await?))
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<Json<InstanceAdmin>, AppError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest("name must be 1–100 characters".into()));
    }
    if req.oauth_client_id.trim().is_empty() || req.oauth_client_secret.trim().is_empty() {
        return Err(AppError::BadRequest(
            "OAuth client id and secret are required".into(),
        ));
    }
    let base_url = instances::normalize_url(&req.base_url, "base_url")?;
    let registry_url = instances::normalize_url(&req.registry_url, "registry_url")?;

    let id = instances::insert(
        &state.pool,
        &state.secrets,
        instances::NewInstance {
            name,
            base_url: &base_url,
            registry_url: &registry_url,
            oauth_client_id: req.oauth_client_id.trim(),
            oauth_client_secret: req.oauth_client_secret.trim(),
        },
        Some(admin.id),
        client_ip(&headers).as_deref(),
    )
    .await?;

    Ok(Json(InstanceAdmin {
        id,
        name: name.to_string(),
        base_url,
        registry_url,
        oauth_client_id: req.oauth_client_id.trim().to_string(),
        enabled: true,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(id): Path<GitlabInstanceId>,
    Json(req): Json<UpdateInstanceRequest>,
) -> Result<Json<InstanceAdmin>, AppError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest("name must be 1–100 characters".into()));
    }
    if req.oauth_client_id.trim().is_empty() {
        return Err(AppError::BadRequest("OAuth client id is required".into()));
    }
    let base_url = instances::normalize_url(&req.base_url, "base_url")?;
    let registry_url = instances::normalize_url(&req.registry_url, "registry_url")?;
    // An empty secret field means "keep the existing one".
    let secret = req
        .oauth_client_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    instances::update(
        &state.pool,
        &state.secrets,
        id,
        instances::InstanceUpdate {
            name,
            base_url: &base_url,
            registry_url: &registry_url,
            oauth_client_id: req.oauth_client_id.trim(),
            oauth_client_secret: secret,
            enabled: req.enabled,
        },
        admin.id,
        client_ip(&headers).as_deref(),
    )
    .await?;

    Ok(Json(InstanceAdmin {
        id,
        name: name.to_string(),
        base_url,
        registry_url,
        oauth_client_id: req.oauth_client_id.trim().to_string(),
        enabled: req.enabled,
    }))
}

pub async fn delete(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(id): Path<GitlabInstanceId>,
) -> Result<StatusCode, AppError> {
    instances::delete(&state.pool, id, admin.id, client_ip(&headers).as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

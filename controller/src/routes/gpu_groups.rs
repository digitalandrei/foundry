//! `/api/servers/{id}/gpu-groups` (list, create) + `/api/gpu-groups/{id}`
//! (delete) and `/api/slots/{id}` slot use-mode. Group/slot-config
//! changes are **admin-only** and audited — they change what the fleet
//! will schedule (docs/API.md § GPU groups; docs/SECURITY.md).

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use foundry_shared::dto::{
    CreateGpuGroupRequest, GpuGroup, SetGroupUseModeRequest, SetSlotUseModeRequest,
};
use foundry_shared::{GpuGroupId, ServerId, SlotId};

use crate::auth::client_ip;
use crate::auth::session::{AdminUser, CurrentUser};
use crate::error::AppError;
use crate::repos::{gpu_groups, slots};
use crate::state::AppState;

/// Any authenticated user can see groups (fleet visibility is org-wide);
/// only admins manage them.
pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
) -> Result<Json<Vec<GpuGroup>>, AppError> {
    Ok(Json(gpu_groups::list(&state.pool, server_id).await?))
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(server_id): Path<ServerId>,
    Json(req): Json<CreateGpuGroupRequest>,
) -> Result<Json<GpuGroup>, AppError> {
    let id = gpu_groups::create(
        &state.pool,
        server_id,
        &req,
        admin.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    // Return the freshly created group with its computed deployability.
    gpu_groups::list(&state.pool, server_id)
        .await?
        .into_iter()
        .find(|g| g.id == id)
        .map(Json)
        .ok_or(AppError::NotFound("group not found"))
}

pub async fn delete(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(group_id): Path<GpuGroupId>,
) -> Result<StatusCode, AppError> {
    gpu_groups::delete(
        &state.pool,
        group_id,
        admin.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set a group's concurrency cap (single/multi-use). Admin-only, audited.
pub async fn set_group_use_mode(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(group_id): Path<GpuGroupId>,
    Json(req): Json<SetGroupUseModeRequest>,
) -> Result<StatusCode, AppError> {
    gpu_groups::set_max_occupants(
        &state.pool,
        group_id,
        req.max_occupants,
        admin.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set a slot's concurrency cap (multi-use). Admin-only, audited.
pub async fn set_slot_use_mode(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(slot_id): Path<SlotId>,
    Json(req): Json<SetSlotUseModeRequest>,
) -> Result<StatusCode, AppError> {
    slots::set_max_occupants(
        &state.pool,
        slot_id,
        req.max_occupants,
        admin.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

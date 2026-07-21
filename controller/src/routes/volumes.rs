//! Slot/server-scoped persistent-volume listing and management.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{ServerVolume, SetVolumeQuotaRequest};
use foundry_shared::{GpuGroupId, ServerId, ServerVolumeId, SlotId};
use serde::Deserialize;

use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::repos::volumes;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct VolumeListQuery {
    slot_id: Option<SlotId>,
    gpu_group_id: Option<GpuGroupId>,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(server_id): Path<ServerId>,
    Query(query): Query<VolumeListQuery>,
) -> Result<Json<Vec<ServerVolume>>, AppError> {
    if query.slot_id.is_some() && query.gpu_group_id.is_some() {
        return Err(AppError::BadRequest(
            "choose either a slot or GPU group target".into(),
        ));
    }
    let target_placement = match (query.slot_id, query.gpu_group_id) {
        (Some(slot_id), None) => {
            let belongs = sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM gpu_slots gs
                   JOIN gpus g ON g.id = gs.gpu_id
                   WHERE gs.id = ? AND g.server_id = ?"#,
                slot_id.0,
                server_id.0
            )
            .fetch_one(&state.pool)
            .await?;
            if belongs == 0 {
                return Err(AppError::BadRequest(
                    "slot does not belong to this server".into(),
                ));
            }
            Some(slot_id.0)
        }
        (None, Some(group_id)) => {
            let belongs = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM gpu_groups WHERE id = ? AND server_id = ?",
                group_id.0,
                server_id.0,
            )
            .fetch_one(&state.pool)
            .await?;
            if belongs == 0 {
                return Err(AppError::BadRequest(
                    "GPU group does not belong to this server".into(),
                ));
            }
            Some(group_id.0)
        }
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!(),
    };
    Ok(Json(
        volumes::list(
            &state.pool,
            server_id,
            target_placement,
            user.id,
            user.is_admin,
        )
        .await?,
    ))
}

/// Delete a volume AND its data (explicit, irreversible). Creator or admin
/// only; active mounts still block it.
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(volume_id): Path<ServerVolumeId>,
) -> Result<axum::http::StatusCode, AppError> {
    let volume = volumes::get(&state.pool, volume_id).await?;
    if volume.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    volumes::delete_guarded(
        &state.pool,
        volume_id,
        volume.server_id,
        &volume.path,
        &volume.name,
        user.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Remove all contents while keeping the reusable volume identity.
pub async fn clean(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(volume_id): Path<ServerVolumeId>,
) -> Result<axum::http::StatusCode, AppError> {
    let volume = volumes::get(&state.pool, volume_id).await?;
    if volume.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    volumes::clean_guarded(
        &state.pool,
        volume_id,
        volume.server_id,
        &volume.path,
        &volume.name,
        user.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn set_quota(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(volume_id): Path<ServerVolumeId>,
    Json(request): Json<SetVolumeQuotaRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    let volume = volumes::get(&state.pool, volume_id).await?;
    if volume.created_by != user.id && !user.is_admin {
        return Err(AppError::Forbidden);
    }
    volumes::set_quota(
        &state.pool,
        volume_id,
        request.quota_bytes,
        user.id,
        client_ip(&headers).as_deref(),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

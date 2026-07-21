//! Project-scoped persistent-volume listing and destructive management.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use foundry_shared::dto::{ServerVolume, SetVolumeQuotaRequest};
use foundry_shared::{GitlabProjectId, GpuGroupId, ServerId, ServerVolumeId, SlotId};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::client_ip;
use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::access::authorize_project;
use crate::repos::{mirror, volumes};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct VolumeListQuery {
    project_id: GitlabProjectId,
    slot_id: Option<SlotId>,
    gpu_group_id: Option<GpuGroupId>,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(server_id): Path<ServerId>,
    Query(query): Query<VolumeListQuery>,
) -> Result<Json<Vec<ServerVolume>>, AppError> {
    let project = mirror::project_by_id(&state.pool, query.project_id).await?;
    authorize_project(
        &state,
        user.id,
        project.instance_id,
        project.gitlab_project_id,
    )
    .await?;
    if query.slot_id.is_some() && query.gpu_group_id.is_some() {
        return Err(AppError::BadRequest(
            "choose either a slot or GPU group target".into(),
        ));
    }
    let target_slot = match (query.slot_id, query.gpu_group_id) {
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
            Some(slot_id)
        }
        (None, Some(group_id)) => {
            let slot = sqlx::query_scalar!(
                r#"SELECT gs.id AS "id: Uuid"
                   FROM gpu_group_members gm
                   JOIN gpu_groups gg ON gg.id = gm.group_id
                   JOIN gpus g ON g.id = gm.gpu_id
                   JOIN gpu_slots gs ON gs.gpu_id = g.id AND gs.slot_type = 'FULL_GPU'
                   WHERE gm.group_id = ? AND gg.server_id = ?
                   ORDER BY g.display_index, gs.id LIMIT 1"#,
                group_id.0,
                server_id.0,
            )
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::BadRequest(
                "GPU group has no deployable slot".into(),
            ))?;
            Some(slot.into())
        }
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!(),
    };
    Ok(Json(
        volumes::list(
            &state.pool,
            server_id,
            query.project_id,
            target_slot,
            user.id,
            user.is_admin,
        )
        .await?,
    ))
}

/// Delete a volume AND its data (explicit, irreversible). Creator or admin
/// only; refused while any active deployment mounts it.
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

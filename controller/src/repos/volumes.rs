//! Placement-scoped local persistent storage.
//!
//! A volume belongs either to one physical GPU slot or to its whole server.
//! GitLab projects never participate in storage identity: deploying another
//! project into the same placement may reuse the same logical volume. Host
//! directories use opaque IDs under `/storage/containers/volumes/`; logical
//! names provide the folder layer in the browser.

use foundry_shared::dto::{ServerVolume, TaskPayload, VolumeBatchTarget, VolumeSpec, VolumeTarget};
use foundry_shared::{
    GpuGroupId, ServerId, ServerVolumeId, SlotId, TaskType, UserId, VolumePlacement,
};
use sqlx::{MySqlConnection, MySqlPool, Row};
use uuid::Uuid;

use crate::error::AppError;

pub const VOLUME_ROOT: &str = "/storage/containers";
const PURGE_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 54, 0);
const FILES_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 63, 0);

fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let version = version.trim().trim_start_matches('v');
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

/// A file session carries a new long-poll/WS protocol, so fail before the
/// browser upgrade when the selected server has not installed it yet.
pub async fn require_file_support(pool: &MySqlPool, server_id: ServerId) -> Result<(), AppError> {
    let version = sqlx::query_scalar!(
        "SELECT agent_version FROM server_agents WHERE server_id = ?",
        server_id.0
    )
    .fetch_optional(pool)
    .await?
    .flatten();
    if version
        .as_deref()
        .and_then(parse_version)
        .is_some_and(|version| version >= FILES_MIN_AGENT_VERSION)
    {
        return Ok(());
    }
    Err(AppError::BadRequest(format!(
        "placement-scoped volume files require foundry-agent 0.63.0 or newer on this server (reported {})",
        version.as_deref().unwrap_or("unknown")
    )))
}

/// PURGE_VOLUMES is a new wire enum. Never enqueue it to an older agent:
/// unknown task variants would make its long-poll response fail to decode.
pub async fn require_purge_support(
    tx: &mut MySqlConnection,
    server_id: ServerId,
) -> Result<(), AppError> {
    let version = sqlx::query_scalar!(
        "SELECT agent_version FROM server_agents WHERE server_id = ?",
        server_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if version
        .as_deref()
        .and_then(parse_version)
        .is_some_and(|version| version >= PURGE_MIN_AGENT_VERSION)
    {
        return Ok(());
    }
    Err(AppError::BadRequest(format!(
        "volume purge requires foundry-agent 0.54.0 or newer on this server (reported {})",
        version.as_deref().unwrap_or("unknown")
    )))
}

pub fn validate_volume_name(name: &str) -> Result<(), AppError> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if !ok {
        return Err(AppError::BadRequest(format!(
            "invalid volume name {name:?} (alphanumeric/dash/underscore, ≤63 chars)"
        )));
    }
    Ok(())
}

pub fn validate_container_path(path: &str) -> Result<(), AppError> {
    let path = path.trim();
    if !path.starts_with('/') || path.len() > 255 || path.contains("..") {
        return Err(AppError::BadRequest(format!(
            "mount path {path:?} must be an absolute path without traversal"
        )));
    }
    Ok(())
}

/// Resolve an explicit volume or create/reuse the canonical volume for the
/// requested placement and logical name. The selected
/// row is locked inside the deployment transaction, serialising against
/// destructive actions.
pub async fn ensure(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    slot_id: SlotId,
    group_id: Option<GpuGroupId>,
    project_name: &str,
    spec: &VolumeSpec,
    created_by: UserId,
) -> Result<(ServerVolumeId, String), AppError> {
    validate_volume_name(&spec.volume_name)?;
    validate_volume_name(project_name)?;

    if let Some(volume_id) = spec.volume_id {
        let row = sqlx::query!(
            r#"SELECT server_id AS "server_id: Uuid",
                      name, path, placement, placement_id AS "placement_id: Uuid", project_name,
                      gpu_slot_id AS "slot_id: Uuid"
               FROM server_volumes WHERE id = ? FOR UPDATE"#,
            volume_id.0
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("volume not found"))?;
        let placement: VolumePlacement = row.placement.parse().map_err(AppError::internal)?;
        let accessible = row.server_id == server_id.0
            && row.name == spec.volume_name
            && row.project_name == project_name
            && placement == spec.placement
            && (row.placement_id
                == match placement {
                    VolumePlacement::Slot => group_id.map_or(slot_id.0, |id| id.0),
                    VolumePlacement::Server => server_id.0,
                }
                || (placement == VolumePlacement::Slot
                    && group_id.is_some()
                    && row.slot_id == Some(slot_id.0)));
        if !accessible {
            return Err(AppError::Forbidden);
        }
        return Ok((volume_id, row.path));
    }

    let (placement_id, gpu_slot_id, gpu_group_id) = match spec.placement {
        VolumePlacement::Slot => match group_id {
            Some(group_id) => (group_id.0, None, Some(group_id.0)),
            None => (slot_id.0, Some(slot_id.0), None),
        },
        VolumePlacement::Server => (server_id.0, None, None),
    };

    let candidate_id = Uuid::now_v7();
    let placement_path = match spec.placement {
        VolumePlacement::Server => "shared".to_string(),
        VolumePlacement::Slot => group_id.map_or_else(
            || format!("slots/{slot_id}"),
            |group_id| format!("groups/{group_id}"),
        ),
    };
    let path = format!(
        "{VOLUME_ROOT}/{placement_path}/{project_name}/{}",
        spec.volume_name
    );
    let now = chrono::Utc::now().naive_utc();
    // The scope unique key makes concurrent first-use deterministic. The
    // no-op duplicate clause lets both deploy transactions then select the
    // same row instead of leaking a uniqueness error to either user.
    sqlx::query!(
        r#"INSERT INTO server_volumes
           (id, server_id, name, placement, placement_id, project_name, gpu_slot_id, gpu_group_id, path,
            created_by, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON DUPLICATE KEY UPDATE updated_at = updated_at"#,
        candidate_id,
        server_id.0,
        spec.volume_name,
        spec.placement.as_str(),
        placement_id,
        project_name,
        gpu_slot_id,
        gpu_group_id,
        path,
        created_by.0,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query!(
        r#"SELECT id AS "id: Uuid", path
           FROM server_volumes
           WHERE server_id = ? AND placement = ? AND placement_id = ?
             AND project_name = ? AND name = ?
           FOR UPDATE"#,
        server_id.0,
        spec.placement.as_str(),
        placement_id,
        project_name,
        spec.volume_name,
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok((row.id.into(), row.path))
}

pub struct VolumeRow {
    pub server_id: ServerId,
    pub name: String,
    pub path: String,
    pub created_by: UserId,
}

pub async fn get(pool: &MySqlPool, id: ServerVolumeId) -> Result<VolumeRow, AppError> {
    let r = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid", name, path,
                  created_by AS "created_by: Uuid"
           FROM server_volumes WHERE id = ?"#,
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("volume not found"))?;
    Ok(VolumeRow {
        server_id: r.server_id.into(),
        name: r.name,
        path: r.path,
        created_by: r.created_by.into(),
    })
}

pub async fn set_quota(
    pool: &MySqlPool,
    id: ServerVolumeId,
    quota_bytes: Option<u64>,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    if quota_bytes.is_some_and(|quota| quota < 1024 * 1024) {
        return Err(AppError::BadRequest(
            "volume quota must be at least 1 MiB".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        "SELECT used_bytes AS `used_bytes?: u64` FROM server_volumes WHERE id = ? FOR UPDATE",
        id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("volume not found"))?;
    if let (Some(quota), Some(used)) = (quota_bytes, row.used_bytes) {
        if quota < used {
            return Err(AppError::BadRequest(format!(
                "quota is below current measured usage ({used} bytes)"
            )));
        }
    }
    sqlx::query!(
        "UPDATE server_volumes SET quota_bytes = ?, updated_at = ? WHERE id = ?",
        quota_bytes,
        chrono::Utc::now().naive_utc(),
        id.0,
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "VOLUME_QUOTA_CHANGED",
            subject_type: Some("server_volume"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({ "quota_bytes": quota_bytes })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Placement volumes on one server. A target placement id (physical slot or
/// GPU-group slot) narrows SLOT volumes while keeping every SERVER volume;
/// no target lists every placement (Storage).
pub async fn list(
    pool: &MySqlPool,
    server_id: ServerId,
    target_placement_id: Option<Uuid>,
    requester: UserId,
    is_admin: bool,
) -> Result<Vec<ServerVolume>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT v.id AS "id: Uuid", v.name, v.path, v.created_at,
                  v.used_bytes AS "used_bytes?: u64", v.quota_bytes AS "quota_bytes?: u64",
                  v.usage_measured_at,
                  v.project_name, v.placement,
                  v.gpu_slot_id AS "slot_id: Uuid", gs.name AS "slot_name?",
                  v.gpu_group_id AS "gpu_group_id: Uuid", gg.name AS "group_name?",
                  v.created_by AS "created_by: Uuid",
                  u.display_name AS created_by_name
           FROM server_volumes v
           JOIN users u ON u.id = v.created_by
           LEFT JOIN gpu_slots gs ON gs.id = v.gpu_slot_id
           LEFT JOIN gpu_groups gg ON gg.id = v.gpu_group_id
           WHERE v.server_id = ?
             AND (? IS NULL OR v.placement = 'SERVER' OR v.placement_id = ?)
           ORDER BY v.project_name, v.name, v.placement"#,
        server_id.0,
        target_placement_id,
        target_placement_id,
    )
    .fetch_all(pool)
    .await?;

    let mut attachments: std::collections::HashMap<ServerVolumeId, Vec<String>> =
        std::collections::HashMap::new();
    for attached in sqlx::query(
        r#"SELECT DISTINCT dv.server_volume_id, d.container_name
           FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           JOIN server_volumes v ON v.id = dv.server_volume_id
           WHERE v.server_id = ?
             AND d.container_name IS NOT NULL
             AND d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           ORDER BY dv.server_volume_id, d.container_name"#,
    )
    .bind(server_id.0)
    .fetch_all(pool)
    .await?
    {
        let volume_id: Uuid = attached
            .try_get("server_volume_id")
            .map_err(AppError::internal)?;
        attachments.entry(volume_id.into()).or_default().push(
            attached
                .try_get("container_name")
                .map_err(AppError::internal)?,
        );
    }

    rows.into_iter()
        .map(|r| {
            let id: ServerVolumeId = r.id.into();
            Ok(ServerVolume {
                id,
                name: r.name,
                path: r.path,
                used_bytes: r.used_bytes,
                quota_bytes: r.quota_bytes,
                usage_measured_at: r.usage_measured_at.map(|at| at.and_utc()),
                project_name: r.project_name,
                placement: r.placement.parse().map_err(AppError::internal)?,
                slot_id: r.slot_id.map(Into::into),
                slot_name: r.slot_name,
                gpu_group_id: r.gpu_group_id.map(Into::into),
                group_name: r.group_name,
                created_by_name: r.created_by_name,
                can_manage: is_admin || r.created_by == requester.0,
                attached_to: attachments.remove(&id).unwrap_or_default(),
                created_at: r.created_at.and_utc(),
            })
        })
        .collect()
}

async fn lock_and_require_detached(
    tx: &mut MySqlConnection,
    id: ServerVolumeId,
) -> Result<(), AppError> {
    sqlx::query!(
        "SELECT id AS `i: Uuid` FROM server_volumes WHERE id = ? FOR UPDATE",
        id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("volume not found"))?;
    let attached = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           WHERE dv.server_volume_id = ?
             AND d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           FOR UPDATE"#,
        id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    if attached > 0 {
        return Err(AppError::BadRequest(
            "volume is mounted by an active deployment".into(),
        ));
    }
    Ok(())
}

/// Remove a volume record and enqueue irreversible host-directory removal.
#[allow(clippy::too_many_arguments)]
pub async fn delete_guarded(
    pool: &MySqlPool,
    id: ServerVolumeId,
    server_id: ServerId,
    path: &str,
    name: &str,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    lock_and_require_detached(&mut tx, id).await?;
    super::tasks::enqueue(
        &mut tx,
        server_id,
        None,
        TaskType::RemoveVolume,
        &TaskPayload::Volume(VolumeTarget {
            volume_id: id,
            path: path.to_string(),
        }),
    )
    .await?;
    sqlx::query!(
        "UPDATE deployment_volumes SET server_volume_id = NULL WHERE server_volume_id = ?",
        id.0
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM server_volumes WHERE id = ?", id.0)
        .execute(&mut *tx)
        .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "VOLUME_DELETED",
            subject_type: Some("server_volume"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({ "name": name, "path": path })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Delete all contents but retain the reusable volume identity. Cleaning is
/// refused while mounted and is performed asynchronously by the server agent.
#[allow(clippy::too_many_arguments)]
pub async fn clean_guarded(
    pool: &MySqlPool,
    id: ServerVolumeId,
    server_id: ServerId,
    path: &str,
    name: &str,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    lock_and_require_detached(&mut tx, id).await?;
    require_purge_support(&mut tx, server_id).await?;
    super::tasks::enqueue(
        &mut tx,
        server_id,
        None,
        TaskType::PurgeVolumes,
        &TaskPayload::VolumeBatch(VolumeBatchTarget {
            volumes: vec![VolumeTarget {
                volume_id: id,
                path: path.to_string(),
            }],
        }),
    )
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "VOLUME_CLEAN_REQUESTED",
            subject_type: Some("server_volume"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({ "name": name, "path": path })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn agent_versions_compare_without_accepting_malformed_values() {
        assert_eq!(parse_version("0.54.0"), Some((0, 54, 0)));
        assert_eq!(parse_version("v1.2.3-dev"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.53.9"), Some((0, 53, 9)));
        assert_eq!(parse_version("0.54"), None);
        assert_eq!(parse_version("unknown"), None);
    }
}

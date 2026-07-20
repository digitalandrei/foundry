//! Project-scoped local persistent storage.
//!
//! A volume belongs to one GitLab project and has two independent policy
//! axes: PRIVATE/PROJECT visibility and SLOT/SERVER placement. New host
//! directories use opaque IDs under `/storage/containers/volumes/`; names
//! are logical keys, not path components. Data survives deployments until
//! an authorised creator/admin explicitly cleans or deletes it.

use foundry_shared::dto::{ServerVolume, TaskPayload, VolumeBatchTarget, VolumeSpec, VolumeTarget};
use foundry_shared::{
    GitlabProjectId, ServerId, ServerVolumeId, SlotId, TaskType, UserId, VolumePlacement,
    VolumeVisibility,
};
use sqlx::{MySqlConnection, MySqlPool, Row};
use uuid::Uuid;

use crate::error::AppError;

pub const VOLUME_ROOT: &str = "/storage/containers";
const PURGE_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 54, 0);

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
/// requested project, visibility, placement and logical name. The selected
/// row is locked inside the deployment transaction, serialising against
/// destructive actions.
pub async fn ensure(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    project_id: GitlabProjectId,
    slot_id: SlotId,
    spec: &VolumeSpec,
    created_by: UserId,
) -> Result<(ServerVolumeId, String), AppError> {
    validate_volume_name(&spec.volume_name)?;

    if let Some(volume_id) = spec.volume_id {
        let row = sqlx::query!(
            r#"SELECT server_id AS "server_id: Uuid",
                      gitlab_project_id AS "project_id: Uuid",
                      name, path, visibility, placement,
                      gpu_slot_id AS "slot_id: Uuid",
                      created_by AS "created_by: Uuid"
               FROM server_volumes WHERE id = ? FOR UPDATE"#,
            volume_id.0
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("volume not found"))?;
        let visibility: VolumeVisibility = row.visibility.parse().map_err(AppError::internal)?;
        let placement: VolumePlacement = row.placement.parse().map_err(AppError::internal)?;
        let accessible = row.server_id == server_id.0
            && row.project_id == Some(project_id.0)
            && row.name == spec.volume_name
            && visibility == spec.visibility
            && placement == spec.placement
            && (visibility == VolumeVisibility::Project || row.created_by == created_by.0)
            && (placement == VolumePlacement::Server || row.slot_id == Some(slot_id.0));
        if !accessible {
            return Err(AppError::Forbidden);
        }
        return Ok((volume_id, row.path));
    }

    let scope_id = match spec.visibility {
        VolumeVisibility::Private => created_by.0,
        VolumeVisibility::Project => project_id.0,
    };
    let (placement_id, gpu_slot_id) = match spec.placement {
        VolumePlacement::Slot => (slot_id.0, Some(slot_id.0)),
        VolumePlacement::Server => (server_id.0, None),
    };

    let candidate_id = Uuid::now_v7();
    let path = format!("{VOLUME_ROOT}/volumes/{candidate_id}");
    let now = chrono::Utc::now().naive_utc();
    // The scope unique key makes concurrent first-use deterministic. The
    // no-op duplicate clause lets both deploy transactions then select the
    // same row instead of leaking a uniqueness error to either user.
    sqlx::query!(
        r#"INSERT INTO server_volumes
           (id, server_id, gitlab_project_id, name, visibility, placement,
            scope_id, placement_id, gpu_slot_id, owner_slug, path,
            created_by, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON DUPLICATE KEY UPDATE updated_at = updated_at"#,
        candidate_id,
        server_id.0,
        project_id.0,
        spec.volume_name,
        spec.visibility.as_str(),
        spec.placement.as_str(),
        scope_id,
        placement_id,
        gpu_slot_id,
        spec.visibility.as_str().to_lowercase(),
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
           WHERE server_id = ? AND gitlab_project_id = ?
             AND visibility = ? AND scope_id = ?
             AND placement = ? AND placement_id = ? AND name = ?
           FOR UPDATE"#,
        server_id.0,
        project_id.0,
        spec.visibility.as_str(),
        scope_id,
        spec.placement.as_str(),
        placement_id,
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

/// Project volumes visible to the requester on one server. A target slot
/// narrows SLOT volumes while keeping all SERVER volumes; no target lists
/// every placement (the Storage management page).
pub async fn list(
    pool: &MySqlPool,
    server_id: ServerId,
    project_id: GitlabProjectId,
    target_slot_id: Option<SlotId>,
    requester: UserId,
    is_admin: bool,
) -> Result<Vec<ServerVolume>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT v.id AS "id: Uuid", v.name, v.path, v.created_at,
                  v.gitlab_project_id AS "project_id: Uuid",
                  p.path_with_namespace AS "project_name?",
                  v.visibility, v.placement,
                  v.gpu_slot_id AS "slot_id: Uuid", gs.name AS "slot_name?",
                  v.created_by AS "created_by: Uuid",
                  u.display_name AS created_by_name
           FROM server_volumes v
           JOIN users u ON u.id = v.created_by
           LEFT JOIN gitlab_projects p ON p.id = v.gitlab_project_id
           LEFT JOIN gpu_slots gs ON gs.id = v.gpu_slot_id
           WHERE v.server_id = ? AND v.gitlab_project_id = ?
             AND (v.visibility = 'PROJECT' OR v.created_by = ?)
             AND (? IS NULL OR v.placement = 'SERVER' OR v.gpu_slot_id = ?)
           ORDER BY v.name, v.visibility, v.placement"#,
        server_id.0,
        project_id.0,
        requester.0,
        target_slot_id.map(|id| id.0),
        target_slot_id.map(|id| id.0),
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
           WHERE v.server_id = ? AND v.gitlab_project_id = ?
             AND (v.visibility = 'PROJECT' OR v.created_by = ?)
             AND d.container_name IS NOT NULL
             AND d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           ORDER BY dv.server_volume_id, d.container_name"#,
    )
    .bind(server_id.0)
    .bind(project_id.0)
    .bind(requester.0)
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
                project_id: r.project_id.map(Into::into),
                project_name: r.project_name,
                visibility: r.visibility.parse().map_err(AppError::internal)?,
                placement: r.placement.parse().map_err(AppError::internal)?,
                slot_id: r.slot_id.map(Into::into),
                slot_name: r.slot_name,
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
             AND d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING',
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

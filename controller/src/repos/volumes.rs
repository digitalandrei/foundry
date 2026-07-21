//! Placement-scoped local persistent storage.
//!
//! Identity is server + physical/GPU-group slot (or shared server) + the
//! user-given deployment name + mount name. GitLab projects never participate.
//! New physical roots live below the reserved `.foundry` tree and end in an
//! immutable volume UUID; legacy rows retain their recorded paths. The UI
//! presents only the stable logical hierarchy.

use foundry_shared::dto::{
    ServerVolume, TaskPayload, VolumeAttachment, VolumeBatchTarget, VolumeSpec, VolumeTarget,
};
use foundry_shared::{
    DeploymentId, GpuGroupId, ServerId, ServerVolumeId, SlotId, TaskType, UserId, VolumePlacement,
};
use sqlx::{mysql::MySqlRow, MySqlConnection, MySqlPool, Row};
use uuid::Uuid;

use crate::error::AppError;

pub const VOLUME_ROOT: &str = "/storage/containers";
const PURGE_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 54, 0);
const FILES_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 63, 0);
const RECENT_ATTACHMENT_HISTORY_PER_VOLUME: i64 = 4;

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

/// Normalize a Docker container bind destination. Docker's compact bind
/// syntax uses `:` as a separator, so allowing it in the destination would
/// make a valid-looking API request reach the agent as an ambiguous bind.
pub fn normalize_container_path(path: &str) -> Result<String, AppError> {
    let path = path.trim();
    if !path.starts_with('/')
        || path.len() > 255
        || path.contains(':')
        || path.chars().any(|character| character.is_control())
    {
        return Err(AppError::BadRequest(format!(
            "mount path {path:?} must be a safe absolute Docker destination"
        )));
    }
    let mut components = Vec::new();
    for component in path.split('/').skip(1) {
        if component.is_empty() {
            continue;
        }
        if matches!(component, "." | "..") {
            return Err(AppError::BadRequest(format!(
                "mount path {path:?} must not contain . or .. components"
            )));
        }
        components.push(component);
    }
    Ok(match components.is_empty() {
        true => "/".to_string(),
        false => format!("/{}", components.join("/")),
    })
}

pub fn validate_container_path(path: &str) -> Result<(), AppError> {
    normalize_container_path(path).map(|_| ())
}

/// A resolved root always carries the source volume's logical identity. The
/// destination path belongs to a deployment mapping, never to this root.
#[derive(Debug)]
pub struct ResolvedVolume {
    pub id: ServerVolumeId,
    pub path: String,
    pub name: String,
    pub project_name: String,
    pub placement: VolumePlacement,
}

/// Existing roots may be reused only where their host directory can be seen
/// by the selected Docker target. Request placement is deliberately not an
/// authority: it must match the selected row, and this helper checks the
/// row's physical placement against the locked target.
pub(crate) fn explicit_volume_compatible(
    source_server_id: Uuid,
    source_placement: VolumePlacement,
    source_placement_id: Uuid,
    target_server_id: ServerId,
    target_slot_id: SlotId,
    target_group_id: Option<GpuGroupId>,
) -> bool {
    if source_server_id != target_server_id.0 {
        return false;
    }
    match source_placement {
        VolumePlacement::Server => source_placement_id == target_server_id.0,
        VolumePlacement::Slot => {
            source_placement_id == target_group_id.map_or(target_slot_id.0, |group| group.0)
        }
    }
}

/// Resolve an explicit volume or create/reuse the canonical volume for the
/// requested placement and logical name. The selected
/// row is locked inside the deployment transaction, serialising against
/// destructive actions.
#[allow(clippy::too_many_arguments)]
pub async fn ensure(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    slot_id: SlotId,
    group_id: Option<GpuGroupId>,
    project_name: &str,
    spec: &VolumeSpec,
    created_by: UserId,
    replaces: Option<DeploymentId>,
) -> Result<ResolvedVolume, AppError> {
    validate_volume_name(&spec.volume_name)?;
    validate_volume_name(project_name)?;

    if let Some(volume_id) = spec.volume_id {
        let row = sqlx::query!(
            r#"SELECT server_id AS "server_id: Uuid",
                      name AS "name: String", path AS "path: String", placement,
                      placement_id AS "placement_id: Uuid", project_name AS "project_name: String"
               FROM server_volumes WHERE id = ? FOR UPDATE"#,
            volume_id.0
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("volume not found"))?;
        let placement: VolumePlacement = row.placement.parse().map_err(AppError::internal)?;
        if row.name != spec.volume_name || placement != spec.placement {
            return Err(AppError::BadRequest(
                "selected volume no longer matches the requested mount name or placement".into(),
            ));
        }
        let directly_compatible = explicit_volume_compatible(
            row.server_id,
            placement,
            row.placement_id,
            server_id,
            slot_id,
            group_id,
        );
        // Migration 00002 intentionally retained a mixed-use group root on
        // its original physical slot. Only a replacement may carry that
        // legacy root forward, and only after proving the predecessor already
        // mounted this exact volume. New group deployments remain exact.
        let legacy_replacement_root = match (replaces, group_id) {
            (Some(predecessor_id), Some(target_group_id))
                if !directly_compatible
                    && row.server_id == server_id.0
                    && placement == VolumePlacement::Slot =>
            {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*)
                       FROM deployment_volumes dv
                       JOIN deployments d ON d.id = dv.deployment_id
                       WHERE dv.deployment_id = ? AND dv.server_volume_id = ?
                         AND d.server_id = ? AND d.gpu_group_id = ?"#,
                    predecessor_id.0,
                    volume_id.0,
                    server_id.0,
                    target_group_id.0,
                )
                .fetch_one(&mut *tx)
                .await?
                    > 0
            }
            _ => false,
        };
        if !directly_compatible && !legacy_replacement_root {
            return Err(AppError::Forbidden);
        }
        return Ok(ResolvedVolume {
            id: volume_id,
            path: row.path,
            name: row.name,
            project_name: row.project_name,
            placement,
        });
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
        "{VOLUME_ROOT}/.foundry/{placement_path}/{project_name}/{}/{candidate_id}",
        spec.volume_name,
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
        r#"SELECT id AS "id: Uuid", path AS "path: String", name AS "name: String",
                  project_name AS "project_name: String", placement
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
    Ok(ResolvedVolume {
        id: row.id.into(),
        path: row.path,
        name: row.name,
        project_name: row.project_name,
        placement: row.placement.parse().map_err(AppError::internal)?,
    })
}

/// A new purge-on-redeploy mapping cannot point at a root shared with an
/// unrelated live deployment. Non-purging mappings remain allowed even when
/// another attachment can purge the root: that shared-storage coordination is
/// deliberate and shown to the operator. The volume row is locked by `ensure`
/// before this check and before the caller inserts its mapping, serialising
/// concurrent selections. The successor and its direct predecessor are one
/// replacement operation, so they are intentionally exempt from each other.
pub async fn require_safe_purge_mapping(
    tx: &mut MySqlConnection,
    volume_id: ServerVolumeId,
    deployment_id: DeploymentId,
    predecessor_id: Option<DeploymentId>,
    requests_purge: bool,
) -> Result<(), AppError> {
    let predecessor = predecessor_id.map(|id| id.0);
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS foreign_mounts
           FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           WHERE dv.server_volume_id = ?
             AND d.id <> ?
             AND (? IS NULL OR d.id <> ?)
             AND d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           FOR UPDATE"#,
    )
    .bind(volume_id.0)
    .bind(deployment_id.0)
    .bind(predecessor)
    .bind(predecessor)
    .fetch_one(&mut *tx)
    .await?;
    let foreign_mounts: i64 = row.try_get("foreign_mounts").map_err(AppError::internal)?;
    if requests_purge && foreign_mounts > 0 {
        return Err(AppError::BadRequest(
            "purge_on_redeploy is unsafe because the selected volume is referenced by another active or retained deployment".into(),
        ));
    }
    Ok(())
}

pub struct VolumeRow {
    pub server_id: ServerId,
    pub name: String,
    pub path: String,
    pub created_by: UserId,
}

pub async fn get(pool: &MySqlPool, id: ServerVolumeId) -> Result<VolumeRow, AppError> {
    let r = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid", name AS "name: String", path AS "path: String",
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

/// Exact controller-owned roots for one authenticated server agent. The
/// agent measures this catalog instead of inferring identity from host path
/// conventions, which also keeps every legacy root accounted for.
pub async fn catalog(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Vec<foundry_shared::dto::VolumeTarget>, AppError> {
    Ok(sqlx::query!(
        r#"SELECT id AS "id: Uuid", path AS "path: String"
           FROM server_volumes WHERE server_id = ? ORDER BY id"#,
        server_id.0,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| foundry_shared::dto::VolumeTarget {
        volume_id: row.id.into(),
        path: row.path,
    })
    .collect())
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

fn attachment_from_row(
    row: &MySqlRow,
) -> Result<(ServerVolumeId, String, VolumeAttachment), AppError> {
    let volume_id: Uuid = row
        .try_get("server_volume_id")
        .map_err(AppError::internal)?;
    let deployment_id: Uuid = row.try_get("deployment_id").map_err(AppError::internal)?;
    let deployment_name: String = row.try_get("deployment_name").map_err(AppError::internal)?;
    let state: String = row.try_get("state").map_err(AppError::internal)?;
    Ok((
        volume_id.into(),
        deployment_name.clone(),
        VolumeAttachment {
            deployment_id: deployment_id.into(),
            deployment_name,
            state: state.parse().map_err(AppError::internal)?,
            container_path: row.try_get("container_path").map_err(AppError::internal)?,
            read_only: row.try_get("read_only").map_err(AppError::internal)?,
            purge_on_redeploy: row
                .try_get("purge_on_redeploy")
                .map_err(AppError::internal)?,
        },
    ))
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
        r#"SELECT v.id AS "id: Uuid", v.name AS "name: String", v.path AS "path: String", v.created_at,
                  v.used_bytes AS "used_bytes?: u64", v.quota_bytes AS "quota_bytes?: u64",
                  v.usage_measured_at,
                  v.project_name AS "project_name: String", v.placement,
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

    let mut attached_to: std::collections::HashMap<ServerVolumeId, Vec<String>> =
        std::collections::HashMap::new();
    let mut attachments: std::collections::HashMap<ServerVolumeId, Vec<VolumeAttachment>> =
        std::collections::HashMap::new();
    for attached in sqlx::query(
        r#"SELECT dv.server_volume_id, d.id AS deployment_id,
                  COALESCE(d.container_name, CONCAT('deployment-', LOWER(HEX(d.id)))) AS deployment_name,
                  d.state, dv.container_path, dv.read_only, dv.purge_on_redeploy
           FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           JOIN server_volumes v ON v.id = dv.server_volume_id
           WHERE v.server_id = ?
             AND (? IS NULL OR v.placement = 'SERVER' OR v.placement_id = ?)
             AND d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           ORDER BY dv.server_volume_id, d.updated_at DESC, dv.container_path"#,
    )
    .bind(server_id.0)
    .bind(target_placement_id)
    .bind(target_placement_id)
    .fetch_all(pool)
    .await?
    {
        let (volume_id, deployment_name, attachment) = attachment_from_row(&attached)?;
        let names = attached_to.entry(volume_id).or_default();
        if !names.contains(&deployment_name) {
            names.push(deployment_name);
        }
        attachments.entry(volume_id).or_default().push(attachment);
    }
    for historical in sqlx::query(
        r#"SELECT server_volume_id, deployment_id, deployment_name, state,
                  container_path, read_only, purge_on_redeploy
           FROM (
               SELECT dv.server_volume_id, d.id AS deployment_id,
                      COALESCE(d.container_name, CONCAT('deployment-', LOWER(HEX(d.id)))) AS deployment_name,
                      d.state, dv.container_path, dv.read_only, dv.purge_on_redeploy,
                      ROW_NUMBER() OVER (
                          PARTITION BY dv.server_volume_id
                          ORDER BY d.updated_at DESC, d.created_at DESC, dv.created_at DESC
                      ) AS history_rank
               FROM deployment_volumes dv
               JOIN deployments d ON d.id = dv.deployment_id
               JOIN server_volumes v ON v.id = dv.server_volume_id
               WHERE v.server_id = ?
                 AND (? IS NULL OR v.placement = 'SERVER' OR v.placement_id = ?)
                 AND d.state NOT IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                                     'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING',
                                     'REMOVING','FAILED')
           ) history
           WHERE history_rank <= ?
           ORDER BY server_volume_id, history_rank"#,
    )
    .bind(server_id.0)
    .bind(target_placement_id)
    .bind(target_placement_id)
    .bind(RECENT_ATTACHMENT_HISTORY_PER_VOLUME)
    .fetch_all(pool)
    .await?
    {
        let (volume_id, _, attachment) = attachment_from_row(&historical)?;
        attachments.entry(volume_id).or_default().push(attachment);
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
                attached_to: attached_to.remove(&id).unwrap_or_default(),
                attachments: attachments.remove(&id).unwrap_or_default(),
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
            "volume is referenced by an active or retained deployment".into(),
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
    use super::{explicit_volume_compatible, normalize_container_path, parse_version};
    use foundry_shared::{GpuGroupId, ServerId, SlotId, VolumePlacement};

    #[test]
    fn agent_versions_compare_without_accepting_malformed_values() {
        assert_eq!(parse_version("0.54.0"), Some((0, 54, 0)));
        assert_eq!(parse_version("v1.2.3-dev"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.53.9"), Some((0, 53, 9)));
        assert_eq!(parse_version("0.54"), None);
        assert_eq!(parse_version("unknown"), None);
    }

    #[test]
    fn container_destinations_are_normalized_without_ambiguous_bind_syntax() {
        assert_eq!(
            normalize_container_path(" /data//workflows/ ").expect("valid path"),
            "/data/workflows"
        );
        assert_eq!(
            normalize_container_path("/").expect("root is absolute"),
            "/"
        );
        assert!(normalize_container_path("relative").is_err());
        assert!(normalize_container_path("/data:broken").is_err());
        assert!(normalize_container_path("/data/../other").is_err());
        assert!(normalize_container_path("/data/./other").is_err());
        assert!(normalize_container_path("/data\u{0000}other").is_err());
        assert_eq!(
            normalize_container_path("/data/..cache").expect("only exact dot parts reject"),
            "/data/..cache"
        );
    }

    #[test]
    fn explicit_roots_require_their_actual_server_and_physical_placement() {
        let server = ServerId::new();
        let other_server = ServerId::new();
        let slot = SlotId::new();
        let other_slot = SlotId::new();
        let group = GpuGroupId::new();
        let other_group = GpuGroupId::new();

        assert!(explicit_volume_compatible(
            server.0,
            VolumePlacement::Server,
            server.0,
            server,
            slot,
            None,
        ));
        assert!(!explicit_volume_compatible(
            other_server.0,
            VolumePlacement::Server,
            other_server.0,
            server,
            slot,
            None,
        ));
        assert!(explicit_volume_compatible(
            server.0,
            VolumePlacement::Slot,
            slot.0,
            server,
            slot,
            None,
        ));
        assert!(!explicit_volume_compatible(
            server.0,
            VolumePlacement::Slot,
            other_slot.0,
            server,
            slot,
            None,
        ));
        assert!(explicit_volume_compatible(
            server.0,
            VolumePlacement::Slot,
            group.0,
            server,
            slot,
            Some(group),
        ));
        assert!(!explicit_volume_compatible(
            server.0,
            VolumePlacement::Slot,
            other_group.0,
            server,
            slot,
            Some(group),
        ));
        assert!(!explicit_volume_compatible(
            server.0,
            VolumePlacement::Slot,
            slot.0,
            server,
            slot,
            Some(group),
        ));
    }
}

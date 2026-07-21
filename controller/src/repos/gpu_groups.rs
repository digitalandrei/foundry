//! GPU groups (aggregation: 1 container : N whole GPUs) — admin CRUD
//! and the deploy-time member resolution. A group is a named template
//! over whole GPUs on one server; membership is overlay. A group occupies
//! the *group*, not its members' individual slots, so members stay
//! individually deployable even while a group container runs — the
//! operator owns any over-subscription. (A *new* group deploy still
//! requires its members not be held by an outside individual/other-group
//! deploy.) See docs/ARCHITECTURE.md § GPU groups and docs/plans/gpu-groups.md.

use std::collections::HashMap;

use foundry_shared::dto::{CreateGpuGroupRequest, GpuGroup};
use foundry_shared::{GpuGroupId, GpuId, ServerId, SlotId, UserId};
use sqlx::{MySqlConnection, MySqlPool, Row};
use uuid::Uuid;

use crate::error::AppError;

/// A member GPU's FULL slot, resolved (and locked) with its Docker/NVML
/// device UUID for a group deploy.
pub struct MemberSlot {
    pub slot_id: SlotId,
    pub gpu_index: u32,
    pub slot_state: String,
    pub mig_enabled: bool,
    /// Docker/NVML device UUID used to detect unmanaged containers.
    pub device_uuid: String,
    /// **Non-group** active holders on this slot (individual deploys or
    /// other groups). A group takes whole GPUs from outsiders, so this
    /// must be 0 to deploy; the group's own concurrent containers
    /// (multi-use) are counted separately as `group_occupants`. Excludes a
    /// replacement's outgoing deployment. A non-terminal deployment holds
    /// a slot; a FAILED one never does (0.11.0 auto-heal).
    pub foreign_occupants: i64,
}

/// A group resolved for a deploy: its use-mode + current occupancy and its
/// locked member slots.
pub struct GroupDeployContext {
    pub server_id: ServerId,
    /// Group concurrency cap (1 = single-use exclusive; >1 = multi-use).
    pub max_occupants: u32,
    /// Active deployments already on this group (excludes a replacement's
    /// outgoing one).
    pub group_occupants: i64,
    pub members: Vec<MemberSlot>,
}

/// Resolve a group's member FULL slots, **locked FOR UPDATE**, ordered by
/// GPU display index (deterministic lock order → no deadlock between two
/// overlapping-group deploys). `exclude` drops one deployment from the
/// occupant counts (the outgoing side of a replacement). Returns an error
/// if the group has no members (deleted concurrently).
pub async fn member_slots_for_deploy(
    tx: &mut MySqlConnection,
    group_id: GpuGroupId,
    exclude: Option<foundry_shared::DeploymentId>,
) -> Result<GroupDeployContext, AppError> {
    let group: (Uuid, u32) =
        sqlx::query_as("SELECT server_id, max_occupants FROM gpu_groups WHERE id = ? FOR UPDATE")
            .bind(group_id.0)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("group not found"))?;

    // Lock every member FULL slot. Occupancy is counted separately (a
    // FOR UPDATE with an aggregate would lock nothing useful).
    let rows = sqlx::query!(
        r#"SELECT gs.id AS "slot_id: Uuid", g.display_index AS gpu_index,
                  gs.state AS slot_state, g.mig_enabled AS "mig_enabled: bool"
           FROM gpu_group_members m
           JOIN gpus g ON g.id = m.gpu_id
           JOIN gpu_slots gs ON gs.gpu_id = g.id AND gs.slot_type = 'FULL_GPU'
           WHERE m.group_id = ?
           ORDER BY g.display_index, gs.id
           FOR UPDATE"#,
        group_id.0
    )
    .fetch_all(&mut *tx)
    .await?;
    if rows.is_empty() {
        return Err(AppError::NotFound("group has no members"));
    }

    let exclude = exclude.map(|d| d.0).unwrap_or_else(Uuid::nil);
    // How many containers already run on this group (multi-use cap check).
    let group_occupants: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployments d
           WHERE d.gpu_group_id = ? AND d.id <> ?
             AND d.state NOT IN ('REMOVED','REPLACED','FAILED')"#,
        group_id.0,
        exclude,
    )
    .fetch_one(&mut *tx)
    .await?;

    let mut members = Vec::with_capacity(rows.len());
    for r in rows {
        // Count only holders that are NOT this group's own deploys — the
        // group's concurrent containers share the GPUs (multi-use), but no
        // outsider may hold a member while the group is in use.
        let foreign_occupants: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM deployment_slots ds
               JOIN deployments d ON d.id = ds.deployment_id
               WHERE ds.gpu_slot_id = ? AND d.id <> ?
                 AND d.state NOT IN ('REMOVED','REPLACED','FAILED')
                 AND (d.gpu_group_id IS NULL OR d.gpu_group_id <> ?)"#,
            r.slot_id,
            exclude,
            group_id.0,
        )
        .fetch_one(&mut *tx)
        .await?;
        members.push(MemberSlot {
            slot_id: r.slot_id.into(),
            gpu_index: r.gpu_index,
            slot_state: r.slot_state,
            mig_enabled: r.mig_enabled,
            device_uuid: sqlx::query_scalar(
                "SELECT COALESCE(gs.mig_uuid, g.gpu_uuid) \
                 FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id WHERE gs.id = ?",
            )
            .bind(r.slot_id)
            .fetch_one(&mut *tx)
            .await?,
            foreign_occupants,
        });
    }
    Ok(GroupDeployContext {
        server_id: group.0.into(),
        max_occupants: group.1,
        group_occupants,
        members,
    })
}

/// Set a group's concurrency cap (1 = single-use, 1–4). Lowering below the
/// current occupant count is allowed and does not evict — it just stops
/// new group deploys until tenants drain. Returns the group's server id
/// (for the audit trail).
pub async fn set_max_occupants(
    pool: &MySqlPool,
    group_id: GpuGroupId,
    max_occupants: u32,
    changed_by: UserId,
    ip_address: Option<&str>,
) -> Result<ServerId, AppError> {
    use foundry_shared::dto::{MAX_OCCUPANTS_MAX, MAX_OCCUPANTS_MIN};
    if !(MAX_OCCUPANTS_MIN..=MAX_OCCUPANTS_MAX).contains(&max_occupants) {
        return Err(AppError::BadRequest(format!(
            "max_occupants must be {MAX_OCCUPANTS_MIN}–{MAX_OCCUPANTS_MAX}"
        )));
    }
    let mut tx = pool.begin().await?;
    let server_id: Option<Uuid> =
        sqlx::query_scalar("SELECT server_id FROM gpu_groups WHERE id = ? FOR UPDATE")
            .bind(group_id.0)
            .fetch_optional(&mut *tx)
            .await?;
    let server_id = server_id.ok_or(AppError::NotFound("group not found"))?;
    sqlx::query!(
        "UPDATE gpu_groups SET max_occupants = ?, updated_at = ? WHERE id = ?",
        max_occupants,
        chrono::Utc::now().naive_utc(),
        group_id.0,
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(changed_by),
            action: "GPU_GROUP_USE_MODE_SET",
            subject_type: Some("gpu_group"),
            subject_id: Some(group_id.0),
            detail: Some(serde_json::json!({ "max_occupants": max_occupants })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(server_id.into())
}

/// All groups on a server, each with combined VRAM, use-mode, current
/// occupancy, and live deployability (below its cap, every member online,
/// MIG-disabled, and free of non-group holders).
pub async fn list(pool: &MySqlPool, server_id: ServerId) -> Result<Vec<GpuGroup>, AppError> {
    let groups = sqlx::query!(
        r#"SELECT gg.id AS "id: Uuid", gg.name, gg.created_at,
                  gg.max_occupants AS "max_occupants: u32",
                  u.display_name AS created_by_name,
                  (SELECT COUNT(*) FROM deployments d
                   WHERE d.gpu_group_id = gg.id
                     AND d.state NOT IN ('REMOVED','REPLACED','FAILED')) AS "occupants!: i64"
           FROM gpu_groups gg
           JOIN users u ON u.id = gg.created_by
           WHERE gg.server_id = ?
           ORDER BY gg.name"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;

    struct ListedMember {
        gpu_id: Uuid,
        gpu_index: u32,
        memory_mb: Option<u32>,
        mig_enabled: bool,
        slot_id: Option<Uuid>,
        slot_state: Option<String>,
        foreign_occupants: i64,
    }
    let mut members_by_group: HashMap<GpuGroupId, Vec<ListedMember>> = HashMap::new();
    for m in sqlx::query(
        r#"SELECT m.group_id, g.id AS gpu_id, g.display_index AS gpu_index,
                  g.memory_mb, g.mig_enabled, gs.id AS slot_id, gs.state AS slot_state,
                  (SELECT COUNT(*) FROM deployment_slots ds
                   JOIN deployments d ON d.id = ds.deployment_id
                   WHERE ds.gpu_slot_id = gs.id
                     AND d.state NOT IN ('REMOVED','REPLACED','FAILED')
                     AND (d.gpu_group_id IS NULL OR d.gpu_group_id <> m.group_id))
                      AS foreign_occupants
           FROM gpu_group_members m
           JOIN gpu_groups gg ON gg.id = m.group_id
           JOIN gpus g ON g.id = m.gpu_id
           LEFT JOIN gpu_slots gs ON gs.gpu_id = g.id AND gs.slot_type = 'FULL_GPU'
           WHERE gg.server_id = ?
           ORDER BY m.group_id, g.display_index"#,
    )
    .bind(server_id.0)
    .fetch_all(pool)
    .await?
    {
        let group_id: Uuid = m.try_get("group_id").map_err(AppError::internal)?;
        members_by_group
            .entry(group_id.into())
            .or_default()
            .push(ListedMember {
                gpu_id: m.try_get("gpu_id").map_err(AppError::internal)?,
                gpu_index: m.try_get("gpu_index").map_err(AppError::internal)?,
                memory_mb: m.try_get("memory_mb").map_err(AppError::internal)?,
                mig_enabled: m.try_get("mig_enabled").map_err(AppError::internal)?,
                slot_id: m.try_get("slot_id").map_err(AppError::internal)?,
                slot_state: m.try_get("slot_state").map_err(AppError::internal)?,
                foreign_occupants: m.try_get("foreign_occupants").map_err(AppError::internal)?,
            });
    }

    let mut out = Vec::with_capacity(groups.len());
    for g in groups {
        let group_id: GpuGroupId = g.id.into();
        // Per-member facts for deployability + combined VRAM, in index
        // order so the reason names GPUs the operator recognises.
        // `foreign_occupants` excludes this group's own deploys — a
        // multi-use group sharing its GPUs is not "busy" with itself.
        let members = members_by_group.remove(&group_id).unwrap_or_default();

        let mut gpu_ids = Vec::with_capacity(members.len());
        let mut combined_vram_mb: u32 = 0;
        let mut busy_reason: Option<String> = None;
        for m in &members {
            gpu_ids.push(GpuId::from(m.gpu_id));
            combined_vram_mb = combined_vram_mb.saturating_add(m.memory_mb.unwrap_or(0));
            if busy_reason.is_some() {
                continue;
            }
            let label = format!("GPU {}", m.gpu_index);
            if m.slot_id.is_none() || m.slot_state.as_deref() == Some("OFFLINE") {
                busy_reason = Some(format!("{label} is offline"));
            } else if m.mig_enabled {
                busy_reason = Some(format!(
                    "{label} has MIG enabled — remove it from the group or disable MIG"
                ));
            } else if m.foreign_occupants > 0 {
                busy_reason = Some(format!("{label} is in individual use"));
            }
        }
        // Below the member-level checks: the group's own concurrency cap.
        // A multi-use group is deployable until `occupants == max`.
        let occupants = g.occupants.max(0) as u32;
        if busy_reason.is_none() && occupants >= g.max_occupants {
            busy_reason = Some(if g.max_occupants == 1 {
                "in use".into()
            } else {
                format!("full · {}/{}", occupants, g.max_occupants)
            });
        }

        out.push(GpuGroup {
            id: group_id,
            server_id,
            name: g.name,
            gpu_ids,
            combined_vram_mb,
            max_occupants: g.max_occupants,
            occupants,
            deployable: busy_reason.is_none(),
            busy_reason,
            created_by_name: g.created_by_name,
            created_at: g.created_at.and_utc(),
        });
    }
    Ok(out)
}

/// Create a group: 2…all eligible (FULL, MIG-disabled, on this server)
/// GPUs, individually picked; members may overlap other groups. Returns
/// the new id.
pub async fn create(
    pool: &MySqlPool,
    server_id: ServerId,
    req: &CreateGpuGroupRequest,
    created_by: UserId,
    ip_address: Option<&str>,
) -> Result<GpuGroupId, AppError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::BadRequest(
            "group name must be 1–64 characters".into(),
        ));
    }
    // Dedupe while preserving the operator's order.
    let mut seen = std::collections::HashSet::new();
    let gpu_ids: Vec<GpuId> = req
        .gpu_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect();
    if gpu_ids.len() < 2 {
        return Err(AppError::BadRequest(
            "a group needs at least 2 distinct GPUs".into(),
        ));
    }

    let mut tx = pool.begin().await?;
    // Every member must be on this server, MIG-disabled, and have a FULL
    // slot — checked one by one so the error names the offender.
    for gpu_id in &gpu_ids {
        let row = sqlx::query!(
            r#"SELECT g.display_index AS gpu_index, g.mig_enabled AS "mig_enabled: bool",
                      g.server_id AS "server_id: Uuid",
                      EXISTS(SELECT 1 FROM gpu_slots gs
                             WHERE gs.gpu_id = g.id AND gs.slot_type = 'FULL_GPU')
                          AS "has_full: bool"
               FROM gpus g WHERE g.id = ?"#,
            gpu_id.0
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::BadRequest("a selected GPU does not exist".into()))?;
        if row.server_id != server_id.0 {
            return Err(AppError::BadRequest(
                "all members must be on the same server".into(),
            ));
        }
        if row.mig_enabled {
            return Err(AppError::BadRequest(format!(
                "GPU {} has MIG enabled and cannot join a group",
                row.gpu_index
            )));
        }
        if !row.has_full {
            return Err(AppError::BadRequest(format!(
                "GPU {} has no full-GPU slot",
                row.gpu_index
            )));
        }
    }

    let id = GpuGroupId::new();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query!(
        r#"INSERT INTO gpu_groups (id, server_id, name, created_by, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        id.0,
        server_id.0,
        name,
        created_by.0,
        now,
        now,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("a group with this name already exists on this server".into())
        }
        _ => AppError::Db(e),
    })?;
    for gpu_id in &gpu_ids {
        sqlx::query!(
            "INSERT INTO gpu_group_members (group_id, gpu_id) VALUES (?, ?)",
            id.0,
            gpu_id.0,
        )
        .execute(&mut *tx)
        .await?;
    }
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(created_by),
            action: "GPU_GROUP_CREATED",
            subject_type: Some("gpu_group"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({
                "name": name,
                "gpu_ids": gpu_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Delete a group — refused while a deploy or placement volume still belongs
/// to it (mirrors the volume "refused while mounted" choke point).
pub async fn delete(
    pool: &MySqlPool,
    group_id: GpuGroupId,
    deleted_by: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM gpu_groups WHERE id = ? FOR UPDATE")
            .bind(group_id.0)
            .fetch_optional(&mut *tx)
            .await?;
    let name = name.ok_or(AppError::NotFound("group not found"))?;
    let live: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployments d
           WHERE d.gpu_group_id = ? AND d.state NOT IN ('REMOVED','REPLACED','FAILED')"#,
        group_id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    if live > 0 {
        return Err(AppError::BadRequest(
            "a deployment is live on this group — stop it first".into(),
        ));
    }
    let volumes = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM server_volumes WHERE gpu_group_id = ?",
        group_id.0,
    )
    .fetch_one(&mut *tx)
    .await?;
    if volumes > 0 {
        return Err(AppError::BadRequest(
            "persistent volumes belong to this group — delete them from Storage first".into(),
        ));
    }
    // Clear the FK on historical (terminal) deployments so the row can
    // be removed — live ones were just refused above. Their slot history
    // survives in deployment_slots/events; only the group label is lost.
    sqlx::query!(
        "UPDATE deployments SET gpu_group_id = NULL, updated_at = ? WHERE gpu_group_id = ?",
        chrono::Utc::now().naive_utc(),
        group_id.0,
    )
    .execute(&mut *tx)
    .await?;
    // Members cascade (ON DELETE CASCADE), but delete explicitly so the
    // intent is visible.
    sqlx::query!(
        "DELETE FROM gpu_group_members WHERE group_id = ?",
        group_id.0
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM gpu_groups WHERE id = ?", group_id.0)
        .execute(&mut *tx)
        .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(deleted_by),
            action: "GPU_GROUP_DELETED",
            subject_type: Some("gpu_group"),
            subject_id: Some(group_id.0),
            detail: Some(serde_json::json!({ "name": name })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

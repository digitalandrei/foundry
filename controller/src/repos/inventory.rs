//! Inventory reconciliation (docs/GPU-MIG.md § Snapshots &
//! Reconciliation): UUID-keyed upserts; slots that vanish go OFFLINE;
//! new slots appear FREE. Containers are a replace-all snapshot.
//!
//! Phase 6 note: once deployments exist, slot-state reconciliation
//! must respect RESERVED/DEPLOYING/RUNNING (a vanished RUNNING slot
//! flags its deployment) — today only FREE/OFFLINE transitions apply.

use chrono::Utc;
use foundry_shared::dto::{GpuInfo, InventorySnapshot, ServerContainer};
use foundry_shared::{ServerId, SlotState, SlotType};
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder};
use uuid::Uuid;

use crate::error::AppError;

pub async fn apply_snapshot(
    pool: &MySqlPool,
    server_id: ServerId,
    snap: &InventorySnapshot,
) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    let readiness_json = snap
        .readiness
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(AppError::internal)?;
    let setup_revision = snap.readiness.as_ref().and_then(|r| r.setup_revision);
    let readiness_checked_at = snap.readiness.as_ref().map(|r| r.checked_at.naive_utc());
    let storage_total = snap.storage.as_ref().map(|storage| storage.total_bytes);
    let storage_available = snap.storage.as_ref().map(|storage| storage.available_bytes);
    sqlx::query!(
        "UPDATE servers SET nvidia_driver_version = ?, docker_version = ?,
             docker_ok = ?, app_publishing_ready = ?, nginx_status = ?, setup_revision = ?,
             readiness_json = ?, readiness_checked_at = ?, storage_total_bytes = ?,
             storage_available_bytes = ?, updated_at = ?
         WHERE id = ?",
        snap.nvidia_driver_version,
        snap.docker_version,
        snap.docker_ok,
        snap.app_publishing,
        snap.nginx_status,
        setup_revision,
        readiness_json,
        readiness_checked_at,
        storage_total,
        storage_available,
        now,
        server_id.0,
    )
    .execute(&mut *tx)
    .await?;

    if let Some(storage) = &snap.storage {
        for volume in &storage.volumes {
            sqlx::query!(
                "UPDATE server_volumes SET used_bytes = ?, usage_measured_at = ?, updated_at = ?
                 WHERE id = ? AND server_id = ?",
                volume.used_bytes,
                now,
                now,
                volume.volume_id.0,
                server_id.0,
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    // ── GPUs + slots, UUID-keyed, two-phase ─────────────────────────
    // Phase 1: everything on this server provisionally OFFLINE;
    // phase 2: each slot present in the snapshot flips back (upsert).
    // Whatever stays OFFLINE genuinely vanished (MIG reshape, GPU
    // removed, driver down). Single transaction — readers never see
    // the intermediate state.
    sqlx::query!(
        r#"UPDATE gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id
           SET gs.state = 'OFFLINE', gs.updated_at = ?
           WHERE g.server_id = ? AND gs.state <> 'OFFLINE'"#,
        now,
        server_id.0,
    )
    .execute(&mut *tx)
    .await?;

    for gpu in &snap.gpus {
        let gpu_id = upsert_gpu(&mut tx, server_id, gpu, now).await?;

        if gpu.mig_enabled {
            for mig in &gpu.mig_devices {
                upsert_slot(
                    &mut tx,
                    gpu_id,
                    SlotType::MigSlot,
                    Some(&mig.uuid),
                    Some(&mig.profile),
                    // Slot name = "<card>.<slice>", slice 1-based
                    // (instance_id is 0-based from `nvidia-smi -L`): GPU 3
                    // split ×4 → "3.1".."3.4". The full-GPU slot uses the
                    // bare card index ("3"). Display only — recomputed on
                    // every snapshot (docs/GPU-MIG.md).
                    &format!("{}.{}", gpu.index, mig.instance_id + 1),
                    Some(mig.memory_mb),
                    now,
                )
                .await?;
            }
        } else {
            upsert_slot(
                &mut tx,
                gpu_id,
                SlotType::FullGpu,
                None,
                None,
                &gpu.index.to_string(),
                Some(gpu.memory_mb),
                now,
            )
            .await?;
        }
    }

    // ── MIG ⇒ not group-eligible: self-heal stale membership ────────
    // A group member must be a full, MIG-disabled GPU (create() enforces
    // this; the builder hides MIG cards). If a member later has MIG
    // enabled, drop its membership so nothing stale lingers — idempotent,
    // a no-op once clean.
    sqlx::query!(
        r#"DELETE m FROM gpu_group_members m
           JOIN gpus g ON g.id = m.gpu_id
           WHERE g.server_id = ? AND g.mig_enabled = 1"#,
        server_id.0,
    )
    .execute(&mut *tx)
    .await?;
    // A group emptied by that removal is dead stale — delete it (clear the
    // historical FK first, like gpu_groups::delete). Guard on no live
    // deployment; a live group deploy can't have a MIG member anyway.
    let empty_groups = sqlx::query_scalar!(
        r#"SELECT gg.id AS "id: Uuid" FROM gpu_groups gg
           WHERE gg.server_id = ?
             AND NOT EXISTS (SELECT 1 FROM gpu_group_members m WHERE m.group_id = gg.id)
             AND NOT EXISTS (
                 SELECT 1 FROM deployments d
                 WHERE d.gpu_group_id = gg.id
                   AND d.state NOT IN ('REMOVED','REPLACED','FAILED')
             )"#,
        server_id.0,
    )
    .fetch_all(&mut *tx)
    .await?;
    for group_id in empty_groups {
        sqlx::query!(
            "UPDATE deployments SET gpu_group_id = NULL, updated_at = ? WHERE gpu_group_id = ?",
            now,
            group_id,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!("DELETE FROM gpu_groups WHERE id = ?", group_id)
            .execute(&mut *tx)
            .await?;
        tracing::info!(server = %server_id, group = %group_id, "deleted group emptied by MIG enablement");
    }

    // ── Containers: replace-all snapshot ────────────────────────────
    sqlx::query!(
        "DELETE FROM server_containers WHERE server_id = ?",
        server_id.0
    )
    .execute(&mut *tx)
    .await?;
    for c in &snap.containers {
        sqlx::query!(
            r#"INSERT INTO server_containers
               (id, server_id, container_id, name, image, state, status, managed, ports,
                gpu_uuids, mounts, reported_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            server_id.0,
            c.container_id,
            c.name.chars().take(255).collect::<String>(),
            c.image.chars().take(1024).collect::<String>(),
            c.state.chars().take(32).collect::<String>(),
            c.status.chars().take(255).collect::<String>(),
            c.managed,
            serde_json::to_string(&c.ports).map_err(AppError::internal)?,
            serde_json::to_string(&c.gpu_uuids).map_err(AppError::internal)?,
            serde_json::to_string(&c.mounts).map_err(AppError::internal)?,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }

    // ── Slot ↔ deployment restore pass ──────────────────────────────
    // A slot that came back from OFFLINE was reset to FREE above, but
    // active deployments may still hold it (review finding): restore the
    // slot state from `deployment_slots` so nobody double-books the GPU.
    // A slot is held by one deployment (single-use), several (multi-use),
    // or as one member of a group — so aggregate occupants and let the
    // most-advanced state win (RUNNING > STOPPING > DEPLOYING > other).
    // FAILED is excluded by design (0.11.0 auto-heal): a failed
    // deployment never holds a slot — the slot stays FREE and the
    // failure remains visible only as a deployment log.
    sqlx::query!(
        r#"UPDATE gpu_slots gs
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN (
               SELECT ds.gpu_slot_id AS slot_id,
                      MIN(CASE d.state
                            WHEN 'RUNNING' THEN 1
                            WHEN 'STOPPING' THEN 2
                            WHEN 'PULLING_IMAGE' THEN 3
                            WHEN 'PREPARED' THEN 3
                            WHEN 'CREATING_CONTAINER' THEN 3
                            WHEN 'STARTING' THEN 3
                            WHEN 'WAITING_HEALTH' THEN 3
                            WHEN 'PUBLISHING' THEN 3
                            WHEN 'PUBLISH_FAILED' THEN 1
                            ELSE 4 END) AS prio
               FROM deployment_slots ds
               JOIN deployments d ON d.id = ds.deployment_id
                    AND d.state NOT IN ('REMOVED','REPLACED','FAILED')
               GROUP BY ds.gpu_slot_id
           ) occ ON occ.slot_id = gs.id
           SET gs.state = CASE occ.prio
                 WHEN 1 THEN 'RUNNING'
                 WHEN 2 THEN 'STOPPING'
                 WHEN 3 THEN 'DEPLOYING'
                 ELSE 'RESERVED' END,
               gs.updated_at = ?
           WHERE g.server_id = ? AND gs.state = 'FREE'"#,
        now,
        server_id.0,
    )
    .execute(&mut *tx)
    .await?;

    // ── Deployment ↔ container reconcile (invariant #5) ─────────────
    // A deployment we believe RUNNING must show its managed container
    // running in this snapshot; otherwise it crashed/was killed
    // outside Foundry → FAILED, and the slot is FREE (the snapshot
    // confirms nothing is on the GPU, so it auto-heals — 0.11.0). 90s
    // grace avoids racing a snapshot collected before the start was
    // reported.
    // All running containers, managed or not — an adopted deployment wraps
    // an unmanaged container, so the liveness check below must see those too
    // (else adopted RUNNING deployments would be falsely marked FAILED).
    let running_short_ids: std::collections::HashSet<String> = snap
        .containers
        .iter()
        .filter(|c| c.state == "running")
        .map(|c| c.container_id.chars().take(12).collect())
        .collect();
    let grace = (Utc::now() - chrono::Duration::seconds(90)).naive_utc();
    let expected = sqlx::query!(
        r#"SELECT id AS "id: Uuid", container_id FROM deployments
           WHERE server_id = ? AND state = 'RUNNING'
             AND started_at IS NOT NULL AND started_at < ?
           FOR UPDATE"#,
        server_id.0,
        grace,
    )
    .fetch_all(&mut *tx)
    .await?;
    for d in expected {
        let known = d
            .container_id
            .as_deref()
            .map(|cid| running_short_ids.contains(&cid.chars().take(12).collect::<String>()))
            .unwrap_or(false);
        if !known {
            let deployment_id: foundry_shared::DeploymentId = d.id.into();
            tracing::warn!(deployment = %deployment_id, "managed container missing/stopped — marking FAILED");
            sqlx::query!(
                "UPDATE deployments SET error_message = ?, updated_at = ? WHERE id = ?",
                "container is no longer running on the host (crashed or removed outside Foundry)",
                now,
                deployment_id.0,
            )
            .execute(&mut *tx)
            .await?;
            crate::lifecycle::transition_deployment(
                &mut tx,
                deployment_id,
                foundry_shared::DeploymentState::Failed,
                &crate::lifecycle::Actor::controller(),
                Some(serde_json::json!({ "reason": "container missing from snapshot" })),
            )
            .await?;
            // Free every member slot (group → all GPUs; a co-tenant on a
            // multi-use slot keeps it occupied via its own active row).
            crate::lifecycle::transition_member_slots(&mut tx, deployment_id, SlotState::Free)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

async fn upsert_gpu(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    gpu: &GpuInfo,
    now: chrono::NaiveDateTime,
) -> Result<Uuid, AppError> {
    let existing = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM gpus WHERE gpu_uuid = ?"#,
        gpu.uuid
    )
    .fetch_optional(&mut *tx)
    .await?;
    match existing {
        Some(id) => {
            sqlx::query!(
                "UPDATE gpus SET server_id = ?, display_index = ?, model = ?, memory_mb = ?,
                     mig_enabled = ?, last_seen_at = ?, updated_at = ? WHERE id = ?",
                server_id.0,
                gpu.index,
                gpu.model,
                gpu.memory_mb,
                gpu.mig_enabled,
                now,
                now,
                id,
            )
            .execute(&mut *tx)
            .await?;
            Ok(id)
        }
        None => {
            let id = Uuid::now_v7();
            sqlx::query!(
                "INSERT INTO gpus (id, server_id, gpu_uuid, display_index, model, memory_mb,
                     mig_enabled, last_seen_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                id,
                server_id.0,
                gpu.uuid,
                gpu.index,
                gpu.model,
                gpu.memory_mb,
                gpu.mig_enabled,
                now,
                now,
                now,
            )
            .execute(&mut *tx)
            .await?;
            Ok(id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn upsert_slot(
    tx: &mut MySqlConnection,
    gpu_id: Uuid,
    slot_type: SlotType,
    mig_uuid: Option<&str>,
    mig_profile: Option<&str>,
    name: &str,
    capacity_mb: Option<u32>,
    now: chrono::NaiveDateTime,
) -> Result<(), AppError> {
    // Identity: MIG slots by MIG UUID; the full-GPU slot by its GPU row.
    let existing = match mig_uuid {
        Some(uuid) => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id: Uuid" FROM gpu_slots WHERE mig_uuid = ?"#,
                uuid
            )
            .fetch_optional(&mut *tx)
            .await?
        }
        None => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id: Uuid" FROM gpu_slots
                   WHERE gpu_id = ? AND slot_type = 'FULL_GPU'"#,
                gpu_id
            )
            .fetch_optional(&mut *tx)
            .await?
        }
    };

    match existing {
        Some(id) => {
            // Present again: OFFLINE slots come back FREE (no
            // deployments yet — see module note for Phase 6).
            sqlx::query!(
                "UPDATE gpu_slots SET gpu_id = ?, name = ?, mig_profile = ?, capacity_mb = ?,
                     state = IF(state = 'OFFLINE', 'FREE', state),
                     last_seen_at = ?, updated_at = ? WHERE id = ?",
                gpu_id,
                name,
                mig_profile,
                capacity_mb,
                now,
                now,
                id,
            )
            .execute(&mut *tx)
            .await?;
        }
        None => {
            sqlx::query!(
                "INSERT INTO gpu_slots (id, gpu_id, slot_type, mig_uuid, mig_profile, name,
                     capacity_mb, state, last_seen_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                Uuid::now_v7(),
                gpu_id,
                slot_type.as_str(),
                mig_uuid,
                mig_profile,
                name,
                capacity_mb,
                SlotState::Free.as_str(),
                now,
                now,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// GPUs + slots for a server (the dashboard grid). Each slot also
/// carries any **external** (non-Foundry) container occupying its
/// GPU/MIG device, mapped from the latest inventory.
pub async fn gpus_for_server(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Vec<foundry_shared::dto::GpuSummary>, AppError> {
    Ok(gpus_for_servers(pool, &[server_id])
        .await?
        .remove(&server_id)
        .unwrap_or_default())
}

#[derive(sqlx::FromRow)]
struct BatchGpuRow {
    id: Uuid,
    server_id: Uuid,
    gpu_uuid: String,
    display_index: u32,
    model: Option<String>,
    memory_mb: Option<u32>,
    mig_enabled: bool,
}

#[derive(sqlx::FromRow)]
struct BatchSlotRow {
    id: Uuid,
    gpu_id: Uuid,
    name: String,
    slot_type: String,
    mig_uuid: Option<String>,
    mig_profile: Option<String>,
    capacity_mb: Option<u32>,
    state: String,
    max_occupants: u32,
}

#[derive(sqlx::FromRow)]
struct BatchMembershipRow {
    gpu_id: Uuid,
    group_id: Uuid,
    name: String,
}

#[derive(sqlx::FromRow)]
struct BatchExternalRow {
    server_id: Uuid,
    name: String,
    image: String,
    gpu_uuids: Option<String>,
    running: bool,
}

fn push_uuid_filter<'a>(builder: &mut QueryBuilder<'a, MySql>, ids: &'a [ServerId]) {
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id.0);
    }
}

/// Batch the full fleet GPU tree in four queries total: GPUs, slots, group
/// memberships, and unmanaged occupants. This is the hot 10-second polling
/// path, so query count must depend on relation types, never fleet size.
pub async fn gpus_for_servers(
    pool: &MySqlPool,
    server_ids: &[ServerId],
) -> Result<std::collections::HashMap<ServerId, Vec<foundry_shared::dto::GpuSummary>>, AppError> {
    use std::collections::HashMap;
    if server_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut gpu_query = QueryBuilder::<MySql>::new(
        "SELECT id, server_id, gpu_uuid, display_index, model, memory_mb, mig_enabled \
         FROM gpus WHERE server_id IN (",
    );
    push_uuid_filter(&mut gpu_query, server_ids);
    gpu_query.push(") ORDER BY server_id, display_index, gpu_uuid");
    let gpu_rows = gpu_query
        .build_query_as::<BatchGpuRow>()
        .fetch_all(pool)
        .await?;
    let gpu_ids: Vec<foundry_shared::GpuId> = gpu_rows.iter().map(|g| g.id.into()).collect();

    let slot_rows = if gpu_ids.is_empty() {
        Vec::new()
    } else {
        let mut q = QueryBuilder::<MySql>::new(
            "SELECT id, gpu_id, name, slot_type, mig_uuid, mig_profile, capacity_mb, state, \
             max_occupants FROM gpu_slots WHERE gpu_id IN (",
        );
        {
            let mut separated = q.separated(", ");
            for id in &gpu_ids {
                separated.push_bind(id.0);
            }
        }
        q.push(") ORDER BY gpu_id, LENGTH(name), name");
        q.build_query_as::<BatchSlotRow>().fetch_all(pool).await?
    };

    let mut memberships_query = QueryBuilder::<MySql>::new(
        "SELECT m.gpu_id, gg.id AS group_id, gg.name FROM gpu_group_members m \
         JOIN gpu_groups gg ON gg.id = m.group_id WHERE gg.server_id IN (",
    );
    push_uuid_filter(&mut memberships_query, server_ids);
    memberships_query.push(") ORDER BY m.gpu_id, gg.name");
    let memberships = memberships_query
        .build_query_as::<BatchMembershipRow>()
        .fetch_all(pool)
        .await?;

    // Adopted containers are no longer foreign, while stopped external
    // containers stay visible as non-blocking context.
    let mut external_query = QueryBuilder::<MySql>::new(
        r#"SELECT sc.server_id, sc.name, sc.image, sc.gpu_uuids,
                  (sc.state = 'running') AS running
           FROM server_containers sc WHERE sc.server_id IN ("#,
    );
    push_uuid_filter(&mut external_query, server_ids);
    external_query.push(
        r#") AND sc.managed = 0
             AND NOT EXISTS (
                 SELECT 1 FROM deployments d
                 WHERE d.server_id = sc.server_id
                   AND d.adopted_container_id = sc.container_id
                   AND d.state NOT IN ('REMOVED','REPLACED','FAILED','STOPPED')
             )
           ORDER BY sc.server_id, (sc.state = 'running') DESC, sc.name"#,
    );
    let external_rows = external_query
        .build_query_as::<BatchExternalRow>()
        .fetch_all(pool)
        .await?;

    let mut external = HashMap::new();
    for r in external_rows {
        let uuids: Vec<String> = r
            .gpu_uuids
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();
        for u in uuids {
            external
                .entry((ServerId::from(r.server_id), u))
                .or_insert_with(|| foundry_shared::dto::ExternalOccupant {
                    name: r.name.clone(),
                    image: r.image.clone(),
                    running: r.running,
                });
        }
    }

    let mut memberships_by_gpu: HashMap<foundry_shared::GpuId, Vec<_>> = HashMap::new();
    for m in memberships {
        memberships_by_gpu.entry(m.gpu_id.into()).or_default().push(
            foundry_shared::dto::GpuGroupRef {
                id: m.group_id.into(),
                name: m.name,
            },
        );
    }
    let mut slots_by_gpu: HashMap<foundry_shared::GpuId, Vec<BatchSlotRow>> = HashMap::new();
    for slot in slot_rows {
        slots_by_gpu
            .entry(slot.gpu_id.into())
            .or_default()
            .push(slot);
    }

    let mut out: HashMap<ServerId, Vec<foundry_shared::dto::GpuSummary>> = HashMap::new();
    for g in gpu_rows {
        let gpu_id: foundry_shared::GpuId = g.id.into();
        let raw_slots = slots_by_gpu.remove(&gpu_id).unwrap_or_default();
        let has_live = raw_slots.iter().any(|s| s.state != "OFFLINE");
        let slots = raw_slots
            .into_iter()
            .filter(|s| !has_live || s.state != "OFFLINE")
            .map(|s| {
                let device = s.mig_uuid.clone().unwrap_or_else(|| g.gpu_uuid.clone());
                Ok(foundry_shared::dto::SlotSummary {
                    id: s.id.into(),
                    name: s.name,
                    slot_type: s.slot_type.parse().map_err(AppError::internal)?,
                    mig_uuid: s.mig_uuid,
                    mig_profile: s.mig_profile,
                    capacity_mb: s.capacity_mb,
                    state: s.state.parse().map_err(AppError::internal)?,
                    max_occupants: s.max_occupants,
                    external: external.get(&(g.server_id.into(), device)).cloned(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        out.entry(g.server_id.into())
            .or_default()
            .push(foundry_shared::dto::GpuSummary {
                id: gpu_id,
                gpu_uuid: g.gpu_uuid,
                index: g.display_index,
                model: g.model,
                memory_mb: g.memory_mb,
                mig_enabled: g.mig_enabled,
                slots,
                groups: memberships_by_gpu.remove(&gpu_id).unwrap_or_default(),
            });
    }
    Ok(out)
}

pub async fn containers_for_server(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Vec<ServerContainer>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT container_id, name, image, state, status, managed AS "managed: bool",
                  ports, mounts, reported_at
           FROM server_containers WHERE server_id = ?
           ORDER BY managed DESC, name"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ServerContainer {
            container_id: r.container_id,
            name: r.name,
            image: r.image,
            state: r.state,
            status: r.status,
            managed: r.managed,
            ports: r
                .ports
                .as_deref()
                .and_then(|p| serde_json::from_slice(p).ok())
                .unwrap_or_default(),
            mounts: r
                .mounts
                .as_deref()
                .and_then(|m| serde_json::from_str(m).ok())
                .unwrap_or_default(),
            reported_at: r.reported_at.and_utc(),
        })
        .collect())
}

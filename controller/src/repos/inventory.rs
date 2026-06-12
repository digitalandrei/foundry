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
use sqlx::{MySqlConnection, MySqlPool};
use uuid::Uuid;

use crate::error::AppError;

pub async fn apply_snapshot(
    pool: &MySqlPool,
    server_id: ServerId,
    snap: &InventorySnapshot,
) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    sqlx::query!(
        "UPDATE servers SET nvidia_driver_version = ?, docker_version = ?, updated_at = ?
         WHERE id = ?",
        snap.nvidia_driver_version,
        snap.docker_version,
        now,
        server_id.0,
    )
    .execute(&mut *tx)
    .await?;

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
                    &format!("{}:{}", gpu.index, mig.instance_id),
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
               (id, server_id, container_id, name, image, state, status, managed, ports, reported_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            server_id.0,
            c.container_id,
            c.name.chars().take(255).collect::<String>(),
            c.image.chars().take(1024).collect::<String>(),
            c.state.chars().take(32).collect::<String>(),
            c.status.chars().take(255).collect::<String>(),
            c.managed,
            serde_json::to_string(&c.ports).map_err(AppError::internal)?,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }

    // ── Slot ↔ deployment restore pass ──────────────────────────────
    // A slot that came back from OFFLINE was reset to FREE above, but
    // an active deployment may still hold it (review finding): restore
    // the slot state from its deployment so nobody double-books the GPU.
    // FAILED is excluded by design (0.11.0 auto-heal): a failed
    // deployment never holds a slot — the slot stays FREE and the
    // failure remains visible only as a deployment log.
    sqlx::query!(
        r#"UPDATE gpu_slots gs
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN deployments d ON d.gpu_slot_id = gs.id
                AND d.state NOT IN ('REMOVED','REPLACED','FAILED')
           SET gs.state = CASE
                 WHEN d.state = 'RUNNING' THEN 'RUNNING'
                 WHEN d.state = 'STOPPING' THEN 'STOPPING'
                 WHEN d.state IN ('PULLING_IMAGE','CREATING_CONTAINER','STARTING') THEN 'DEPLOYING'
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
    let running_short_ids: std::collections::HashSet<String> = snap
        .containers
        .iter()
        .filter(|c| c.managed && c.state == "running")
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
            let slot_id: foundry_shared::SlotId = sqlx::query_scalar!(
                r#"SELECT gpu_slot_id AS "slot_id: Uuid" FROM deployments WHERE id = ?"#,
                deployment_id.0
            )
            .fetch_one(&mut *tx)
            .await?
            .into();
            crate::lifecycle::transition_deployment(
                &mut tx,
                deployment_id,
                foundry_shared::DeploymentState::Failed,
                &crate::lifecycle::Actor::controller(),
                Some(serde_json::json!({ "reason": "container missing from snapshot" })),
            )
            .await?;
            crate::lifecycle::transition_slot(&mut tx, slot_id, SlotState::Free).await?;
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

/// GPUs + slots for a set of servers (the dashboard grid).
pub async fn gpus_for_server(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Vec<foundry_shared::dto::GpuSummary>, AppError> {
    let gpu_rows = sqlx::query!(
        r#"SELECT id AS "id: Uuid", gpu_uuid, display_index, model, memory_mb,
                  mig_enabled AS "mig_enabled: bool"
           FROM gpus WHERE server_id = ?
           ORDER BY display_index, gpu_uuid"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(gpu_rows.len());
    for g in gpu_rows {
        // LENGTH-first gives natural ordering for g:i names
        // ("0:5" < "0:10").
        let slot_rows = sqlx::query!(
            r#"SELECT id AS "id: Uuid", name, slot_type, mig_profile, capacity_mb, state
               FROM gpu_slots WHERE gpu_id = ? ORDER BY LENGTH(name), name"#,
            g.id
        )
        .fetch_all(pool)
        .await?;
        let slots = slot_rows
            .into_iter()
            .map(|s| {
                Ok(foundry_shared::dto::SlotSummary {
                    id: s.id.into(),
                    name: s.name,
                    slot_type: s.slot_type.parse().map_err(AppError::internal)?,
                    mig_profile: s.mig_profile,
                    capacity_mb: s.capacity_mb,
                    state: s.state.parse().map_err(AppError::internal)?,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        out.push(foundry_shared::dto::GpuSummary {
            id: g.id.into(),
            gpu_uuid: g.gpu_uuid,
            index: g.display_index,
            model: g.model,
            memory_mb: g.memory_mb,
            mig_enabled: g.mig_enabled,
            slots,
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
                  ports, reported_at
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
            reported_at: r.reported_at.and_utc(),
        })
        .collect())
}

/// `running` containers per the latest snapshot (System Status card).
pub async fn running_count(pool: &MySqlPool, server_id: ServerId) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM server_containers WHERE server_id = ? AND state = 'running'",
        server_id.0
    )
    .fetch_one(pool)
    .await?)
}

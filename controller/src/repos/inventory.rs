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
               (id, server_id, container_id, name, image, state, status, managed, reported_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            server_id.0,
            c.container_id,
            c.name.chars().take(255).collect::<String>(),
            c.image.chars().take(1024).collect::<String>(),
            c.state.chars().take(32).collect::<String>(),
            c.status.chars().take(255).collect::<String>(),
            c.managed,
            now,
        )
        .execute(&mut *tx)
        .await?;
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
                "UPDATE gpus SET server_id = ?, model = ?, memory_mb = ?, mig_enabled = ?,
                     last_seen_at = ?, updated_at = ? WHERE id = ?",
                server_id.0,
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
                "INSERT INTO gpus (id, server_id, gpu_uuid, model, memory_mb, mig_enabled,
                     last_seen_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                id,
                server_id.0,
                gpu.uuid,
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
        r#"SELECT id AS "id: Uuid", gpu_uuid, model, memory_mb,
                  mig_enabled AS "mig_enabled: bool"
           FROM gpus WHERE server_id = ? ORDER BY gpu_uuid"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(gpu_rows.len());
    for g in gpu_rows {
        let slot_rows = sqlx::query!(
            r#"SELECT id AS "id: Uuid", name, slot_type, mig_profile, capacity_mb, state
               FROM gpu_slots WHERE gpu_id = ? ORDER BY name"#,
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
                  reported_at
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
            reported_at: r.reported_at.and_utc(),
        })
        .collect())
}

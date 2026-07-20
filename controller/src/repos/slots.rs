//! Slot use-mode (multi-use sharing). A slot's `max_occupants` is
//! operator config: 1 = single-use, >1 = soft sharing with no VRAM
//! isolation (MIG remains the hardware-isolated path). Inventory
//! reconcile preserves this column — it is config, not an agent-reported
//! fact. See docs/ARCHITECTURE.md § Multi-use slots.

use foundry_shared::dto::{MAX_OCCUPANTS_MAX, MAX_OCCUPANTS_MIN};
use foundry_shared::{SlotId, UserId};
use sqlx::MySqlPool;

use crate::error::AppError;

/// Set a slot's concurrency cap. Lowering it below the current occupant
/// count is allowed and does not evict — it just stops new deploys until
/// tenants drain (the UI surfaces that). Returns the slot's server id for
/// the audit trail.
pub async fn set_max_occupants(
    pool: &MySqlPool,
    slot_id: SlotId,
    max_occupants: u32,
    changed_by: UserId,
    ip_address: Option<&str>,
) -> Result<foundry_shared::ServerId, AppError> {
    if !(MAX_OCCUPANTS_MIN..=MAX_OCCUPANTS_MAX).contains(&max_occupants) {
        return Err(AppError::BadRequest(format!(
            "max_occupants must be {MAX_OCCUPANTS_MIN}–{MAX_OCCUPANTS_MAX}"
        )));
    }
    let mut tx = pool.begin().await?;
    let server_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT g.server_id FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id \
         WHERE gs.id = ? FOR UPDATE",
    )
    .bind(slot_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("slot not found"))?;

    sqlx::query!(
        "UPDATE gpu_slots SET max_occupants = ?, updated_at = ? WHERE id = ?",
        max_occupants,
        chrono::Utc::now().naive_utc(),
        slot_id.0,
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(changed_by),
            action: "SLOT_USE_MODE_SET",
            subject_type: Some("gpu_slot"),
            subject_id: Some(slot_id.0),
            detail: Some(serde_json::json!({ "max_occupants": max_occupants })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(server_id.into())
}

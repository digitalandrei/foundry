//! Slot use-mode (multi-use sharing). A slot's `max_occupants` is
//! operator config: 1 = single-use, >1 = soft sharing with no VRAM
//! isolation (MIG remains the hardware-isolated path). Inventory
//! reconcile preserves this column — it is config, not an agent-reported
//! fact. See docs/ARCHITECTURE.md § Multi-use slots.

use foundry_shared::dto::{MAX_OCCUPANTS_MAX, MAX_OCCUPANTS_MIN};
use foundry_shared::SlotId;
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
) -> Result<foundry_shared::ServerId, AppError> {
    if !(MAX_OCCUPANTS_MIN..=MAX_OCCUPANTS_MAX).contains(&max_occupants) {
        return Err(AppError::BadRequest(format!(
            "max_occupants must be {MAX_OCCUPANTS_MIN}–{MAX_OCCUPANTS_MAX}"
        )));
    }
    let server_id = sqlx::query_scalar!(
        r#"SELECT g.server_id AS "server_id: uuid::Uuid"
           FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id
           WHERE gs.id = ?"#,
        slot_id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("slot not found"))?;

    sqlx::query!(
        "UPDATE gpu_slots SET max_occupants = ?, updated_at = ? WHERE id = ?",
        max_occupants,
        chrono::Utc::now().naive_utc(),
        slot_id.0,
    )
    .execute(pool)
    .await?;
    Ok(server_id.into())
}

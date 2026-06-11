//! THE deployment/slot state machines (docs/ARCHITECTURE.md
//! § Deployment Lifecycle, § Slot states). One transition function per
//! machine — every legal move validates against the table below, and
//! deployment transitions persist state + `deployment_events` + audit
//! atomically with whatever else rides in the caller's transaction
//! (docs/RUST_RULES.md § State Machines). Scattered
//! `UPDATE … SET state` elsewhere is a review-blocking bug.

use foundry_shared::{ActorType, DeploymentId, DeploymentState, SlotId, SlotState, UserId};
use sqlx::MySqlConnection;
use uuid::Uuid;

use crate::error::AppError;

use DeploymentState as D;

/// Legal deployment transitions. The controller advances coarse steps
/// (dispatch → PULLING_IMAGE, result → RUNNING/FAILED), so the table
/// allows the documented skips.
const DEPLOYMENT_TRANSITIONS: &[(D, D)] = &[
    (D::Pending, D::Validating),
    (D::Validating, D::PullingImage),
    (D::Validating, D::Failed),
    (D::PullingImage, D::CreatingContainer),
    (D::PullingImage, D::Running),
    (D::PullingImage, D::Failed),
    (D::CreatingContainer, D::Starting),
    (D::CreatingContainer, D::Failed),
    (D::Starting, D::Running),
    (D::Starting, D::Failed),
    (D::Running, D::Stopping),
    (D::Running, D::Failed),
    (D::Running, D::Replaced),
    (D::Stopping, D::Stopped),
    (D::Stopping, D::Failed),
    (D::Stopping, D::Replaced),
    (D::Stopped, D::Restarting),
    (D::Stopped, D::Removing),
    (D::Stopped, D::Replaced),
    (D::Restarting, D::Running),
    (D::Restarting, D::Failed),
    (D::Failed, D::Removing),
    (D::Failed, D::Restarting),
    (D::Removing, D::Removed),
    (D::Removing, D::Failed),
];

pub fn is_legal(from: D, to: D) -> bool {
    DEPLOYMENT_TRANSITIONS.contains(&(from, to))
}

pub struct Actor {
    pub actor_type: ActorType,
    pub user_id: Option<UserId>,
}

impl Actor {
    pub fn controller() -> Self {
        Self {
            actor_type: ActorType::Controller,
            user_id: None,
        }
    }
    pub fn agent() -> Self {
        Self {
            actor_type: ActorType::Agent,
            user_id: None,
        }
    }
    pub fn user(id: UserId) -> Self {
        Self {
            actor_type: ActorType::User,
            user_id: Some(id),
        }
    }
}

/// Move a deployment `from → to` inside the caller's transaction:
/// validates legality against the CURRENT row (locked), writes the
/// state, the event row, and the audit row. Returns the actual
/// previous state.
pub async fn transition_deployment(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
    to: D,
    actor: &Actor,
    detail: Option<serde_json::Value>,
) -> Result<D, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let row = sqlx::query!(
        "SELECT state FROM deployments WHERE id = ? FOR UPDATE",
        deployment_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;
    let from: D = row.state.parse().map_err(AppError::internal)?;

    if !is_legal(from, to) {
        return Err(AppError::BadRequest(format!(
            "illegal deployment transition {from} → {to}"
        )));
    }

    sqlx::query!(
        "UPDATE deployments SET state = ?, updated_at = ?,
             started_at = IF(? = 'RUNNING' AND started_at IS NULL, ?, started_at),
             stopped_at = IF(? IN ('STOPPED','REMOVED','REPLACED','FAILED'), ?, stopped_at)
         WHERE id = ?",
        to.as_str(),
        now,
        to.as_str(),
        now,
        to.as_str(),
        now,
        deployment_id.0,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"INSERT INTO deployment_events
           (id, deployment_id, from_state, to_state, actor_type, actor_id, detail, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        Uuid::now_v7(),
        deployment_id.0,
        from.as_str(),
        to.as_str(),
        actor.actor_type.as_str(),
        actor.user_id.map(|u| u.0),
        detail
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(AppError::internal)?,
        now,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"INSERT INTO audit_logs
           (id, actor_type, actor_id, action, subject_type, subject_id, detail, ip_address, created_at)
           VALUES (?, ?, ?, ?, 'deployment', ?, ?, NULL, ?)"#,
        Uuid::now_v7(),
        actor.actor_type.as_str(),
        actor.user_id.map(|u| u.0),
        format!("DEPLOYMENT_{to}"),
        deployment_id.0,
        serde_json::to_string(&serde_json::json!({ "from": from.as_str(), "to": to.as_str() }))
            .map_err(AppError::internal)?,
        now,
    )
    .execute(&mut *tx)
    .await?;

    Ok(from)
}

/// Slot state move inside the caller's transaction. Slots have no
/// event table; legality here is a guard against impossible jumps —
/// inventory reconciliation (OFFLINE handling) bypasses this via its
/// own snapshot rules.
pub async fn transition_slot(
    tx: &mut MySqlConnection,
    slot_id: SlotId,
    to: SlotState,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE gpu_slots SET state = ?, updated_at = ? WHERE id = ?",
        to.as_str(),
        chrono::Utc::now().naive_utc(),
        slot_id.0,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_is_legal() {
        for (from, to) in [
            (D::Pending, D::Validating),
            (D::Validating, D::PullingImage),
            (D::PullingImage, D::Running),
            (D::Running, D::Stopping),
            (D::Stopping, D::Stopped),
            (D::Stopped, D::Restarting),
            (D::Restarting, D::Running),
            (D::Stopped, D::Removing),
            (D::Removing, D::Removed),
        ] {
            assert!(is_legal(from, to), "{from} → {to} must be legal");
        }
    }

    #[test]
    fn replacement_chain_is_legal() {
        assert!(is_legal(D::Running, D::Stopping));
        assert!(is_legal(D::Stopping, D::Replaced));
        assert!(is_legal(D::Stopped, D::Replaced));
    }

    #[test]
    fn nonsense_is_illegal() {
        for (from, to) in [
            (D::Removed, D::Running),
            (D::Pending, D::Running),
            (D::Stopped, D::Running),
            (D::Failed, D::Running),
            (D::Replaced, D::Restarting),
            (D::Running, D::Pending),
        ] {
            assert!(!is_legal(from, to), "{from} → {to} must be illegal");
        }
    }
}

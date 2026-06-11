//! Append-only audit trail writes (docs/SECURITY.md § Audit Logging).
//! INSERT only — never UPDATE or DELETE against audit_logs.

use foundry_shared::{ActorType, UserId};
use sqlx::MySqlExecutor;

use crate::error::AppError;

pub struct AuditEntry<'a> {
    pub actor_type: ActorType,
    pub actor_id: Option<UserId>,
    pub action: &'a str,
    pub subject_type: Option<&'a str>,
    pub subject_id: Option<uuid::Uuid>,
    pub detail: Option<serde_json::Value>,
    pub ip_address: Option<&'a str>,
}

pub async fn record<'e>(
    exec: impl MySqlExecutor<'e>,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::now_v7();
    let detail = entry
        .detail
        .map(|d| serde_json::to_string(&d))
        .transpose()
        .map_err(AppError::internal)?;
    sqlx::query!(
        r#"INSERT INTO audit_logs
           (id, actor_type, actor_id, action, subject_type, subject_id, detail, ip_address, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        id,
        entry.actor_type.as_str(),
        entry.actor_id.map(|u| u.0),
        entry.action,
        entry.subject_type,
        entry.subject_id,
        detail,
        entry.ip_address,
        chrono::Utc::now().naive_utc(),
    )
    .execute(exec)
    .await?;
    Ok(())
}

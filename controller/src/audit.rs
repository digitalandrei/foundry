//! Append-only audit trail writes (docs/SECURITY.md § Audit Logging).
//! INSERT only — never UPDATE or DELETE against audit_logs.

use chrono::{DateTime, Utc};
use foundry_shared::dto::{AuditLogEntry, AuditPage};
use foundry_shared::{ActorType, UserId};
use sqlx::{MySqlExecutor, MySqlPool};
use uuid::Uuid;

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

/// Read a newest-first, cursor-paginated page of audit rows.
///
/// `actor_scope = Some(uid)` restricts to that actor (a non-admin sees
/// only their own actions); `None` returns every row (admin). `before`
/// is the exclusive `id` cursor — pass the previous page's `next_cursor`
/// to walk further back in time. `action` is an optional exact-match
/// filter. `actor_name` is resolved from `users` (None for agent/system
/// actors or a since-deleted user).
pub async fn list_page(
    pool: &MySqlPool,
    actor_scope: Option<UserId>,
    before: Option<Uuid>,
    action: Option<&str>,
    limit: u32,
) -> Result<AuditPage, AppError> {
    let scope = actor_scope.map(|u| u.0);
    let fetch = i64::from(limit) + 1; // over-fetch one to detect a next page
    let rows = sqlx::query!(
        r#"SELECT a.id            AS "id!: Uuid",
                  a.actor_type    AS "actor_type!",
                  a.actor_id      AS "actor_id?: Uuid",
                  u.display_name  AS "actor_name?",
                  a.action        AS "action!",
                  a.subject_type  AS "subject_type?",
                  a.subject_id    AS "subject_id?: Uuid",
                  CAST(a.detail AS CHAR) AS "detail?: String",
                  a.ip_address    AS "ip_address?",
                  a.created_at    AS "created_at!"
           FROM audit_logs a
           LEFT JOIN users u ON u.id = a.actor_id
           WHERE (? IS NULL OR a.actor_id = ?)
             AND (? IS NULL OR a.id < ?)
             AND (? IS NULL OR a.action = ?)
           ORDER BY a.id DESC
           LIMIT ?"#,
        scope,
        scope,
        before,
        before,
        action,
        action,
        fetch,
    )
    .fetch_all(pool)
    .await?;

    let entries = rows
        .into_iter()
        .map(|r| AuditLogEntry {
            id: r.id.to_string(),
            actor_type: r.actor_type,
            actor_id: r.actor_id.map(|u| u.to_string()),
            actor_name: r.actor_name,
            action: r.action,
            subject_type: r.subject_type,
            subject_id: r.subject_id.map(|u| u.to_string()),
            detail: r.detail.and_then(|s| serde_json::from_str(&s).ok()),
            ip_address: r.ip_address,
            created_at: DateTime::<Utc>::from_naive_utc_and_offset(r.created_at, Utc),
        })
        .collect();

    Ok(into_page(entries, limit as usize))
}

/// Trim an over-fetched (`limit + 1`) newest-first run into a page plus
/// the `id` cursor for the next one. Pure — unit-tested without a DB.
fn into_page(mut entries: Vec<AuditLogEntry>, limit: usize) -> AuditPage {
    let next_cursor = if entries.len() > limit {
        entries.truncate(limit);
        entries.last().map(|e| e.id.clone())
    } else {
        None
    };
    AuditPage {
        entries,
        next_cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> AuditLogEntry {
        AuditLogEntry {
            id: id.to_string(),
            actor_type: "USER".into(),
            actor_id: None,
            actor_name: None,
            action: "LOGIN".into(),
            subject_type: None,
            subject_id: None,
            detail: None,
            ip_address: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn full_page_yields_cursor_from_last_kept_row() {
        // over-fetched limit+1 (3 for limit 2) → trims to 2, cursor = 2nd id
        let page = into_page(vec![entry("a"), entry("b"), entry("c")], 2);
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.next_cursor.as_deref(), Some("b"));
    }

    #[test]
    fn exact_fill_has_no_next_cursor() {
        // got exactly `limit` real rows (no extra) → last page
        let page = into_page(vec![entry("a"), entry("b")], 2);
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn short_page_has_no_next_cursor() {
        let page = into_page(vec![entry("a")], 2);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn empty_is_last_page() {
        let page = into_page(vec![], 2);
        assert!(page.entries.is_empty());
        assert_eq!(page.next_cursor, None);
    }
}

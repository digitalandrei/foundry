//! `GET /api/audit` — append-only audit-trail read model
//! (docs/API.md § Audit; rows are written by `controller::audit`).
//! Admin sees every row; a non-admin sees only the rows they are the
//! actor of. Newest-first, cursor-paginated.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One audit row, shaped for the operator table. `actor_name` is
/// resolved from `users` at read time (None for agent/system actors that
/// carry no user identity, or a since-deleted user). `detail` is the raw
/// JSON captured when the action happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    /// Stored `actor_type`: "USER" | "AGENT" | "CONTROLLER".
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub action: String,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    pub detail: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A newest-first page of audit rows. `next_cursor` is the `id` to pass
/// back as `?before=` for the following page; None means this was the
/// last page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPage {
    pub entries: Vec<AuditLogEntry>,
    pub next_cursor: Option<String>,
}

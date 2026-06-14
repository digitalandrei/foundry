//! `GET /api/audit` — read the append-only audit trail (docs/API.md
//! § Audit). Writes live in `crate::audit`; this is the read side.
//! Admin sees every row; a non-admin sees only the rows they are the
//! actor of. Newest-first, cursor-paginated.

use axum::extract::{Query, State};
use axum::Json;
use foundry_shared::dto::AuditPage;
use serde::Deserialize;

use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Cursor: return rows strictly older than this audit id.
    pub before: Option<uuid::Uuid>,
    /// Page size; defaults to 50, clamped to 1..=200.
    pub limit: Option<u32>,
    /// Exact-match action filter (e.g. "DEPLOYMENT_CREATED").
    pub action: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AuditQuery>,
) -> Result<Json<AuditPage>, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    // Non-admins are pinned to their own actor_id; admins see everything.
    let actor_scope = if user.is_admin { None } else { Some(user.id) };
    let action = q.action.as_deref().filter(|s| !s.is_empty());
    let page = crate::audit::list_page(&state.pool, actor_scope, q.before, action, limit).await?;
    Ok(Json(page))
}

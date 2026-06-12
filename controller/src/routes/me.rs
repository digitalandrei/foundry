//! `GET /api/me` (docs/API.md).

use axum::extract::State;
use axum::Json;
use foundry_shared::dto::MeResponse;

use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::repos::users;
use crate::state::AppState;

pub async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<MeResponse>, AppError> {
    let accounts = users::account_summaries(&state.pool, user.id).await?;
    Ok(Json(MeResponse {
        id: user.id,
        display_name: user.display_name,
        email: user.email,
        avatar_url: user.avatar_url,
        is_admin: user.is_admin,
        accounts,
        apps_domain: state.apps_domain.as_deref().map(str::to_string),
    }))
}

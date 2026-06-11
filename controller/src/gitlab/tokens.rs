//! Access-token freshness: refresh transparently when close to expiry
//! and persist the rotated pair (docs/GITLAB-INTEGRATION.md § OAuth).

use chrono::{Duration, Utc};

use super::{oauth, InstanceConfig};
use crate::error::AppError;
use crate::repos::users::{self, AccountTokens};
use crate::state::AppState;

/// Returns a usable access token for this account, refreshing first if
/// it expires within the next minute.
pub async fn ensure_fresh(
    state: &AppState,
    instance: &InstanceConfig,
    account: &AccountTokens,
) -> Result<String, AppError> {
    let needs_refresh = account
        .expires_at
        .map(|t| t - Utc::now() < Duration::seconds(60))
        .unwrap_or(false);
    if !needs_refresh {
        return Ok(account.access_token.clone());
    }
    let Some(refresh_token) = account.refresh_token.clone() else {
        // No refresh token: use it until GitLab says 401.
        return Ok(account.access_token.clone());
    };

    let fresh = oauth::refresh_tokens(&state.http, instance, &state.public_url, refresh_token)
        .await
        .map_err(|err| {
            tracing::warn!(instance = %instance.name, ?err, "token refresh failed");
            AppError::Unauthorized
        })?;
    users::update_account_tokens(&state.pool, &state.secrets, account.account_id, &fresh).await?;
    Ok(fresh.access_token)
}

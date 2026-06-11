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

#[derive(serde::Deserialize)]
struct JwtAuthResponse {
    token: String,
}

/// Mint a scoped, short-lived registry pull token from the user's
/// access token (docs/GITLAB-INTEGRATION.md § Image Pulls; variant 1).
/// GitLab's registry auth endpoint takes Basic credentials — for OAuth
/// tokens the conventional username is `oauth2`.
pub async fn registry_pull_token(
    http: &reqwest::Client,
    base_url: &str,
    user_access_token: &str,
    repo_path: &str,
) -> Result<String, AppError> {
    let url =
        format!("{base_url}/jwt/auth?service=container_registry&scope=repository:{repo_path}:pull");
    let resp = http
        .get(&url)
        .basic_auth("oauth2", Some(user_access_token))
        .send()
        .await
        .map_err(AppError::gitlab)?;
    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "registry token mint failed ({})",
            resp.status()
        )));
    }
    let body: JwtAuthResponse = resp.json().await.map_err(AppError::gitlab)?;
    Ok(body.token)
}

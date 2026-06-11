//! `/auth/login/{instance}`, `/auth/callback`, `/auth/logout`
//! (docs/GITLAB-INTEGRATION.md § OAuth). The callback redirect URI is
//! instance-independent (`{public_url}/auth/callback`); the pending
//! instance travels in the encrypted state cookie.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Redirect;
use axum_extra::extract::CookieJar;
use foundry_shared::{ActorType, GitlabInstanceId};
use serde::Deserialize;

use super::cookies;
use super::session;
use crate::audit::{self, AuditEntry};
use crate::error::AppError;
use crate::gitlab::client::GitlabApi;
use crate::gitlab::oauth;
use crate::repos::{instances, users};
use crate::state::AppState;

pub async fn login(
    State(state): State<AppState>,
    Path(instance_id): Path<GitlabInstanceId>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    let instance = instances::fetch_config(&state.pool, &state.secrets, instance_id).await?;
    let (url, pending) = oauth::begin_login(&instance, &state.public_url)?;
    let jar = jar.add(cookies::oauth_cookie(&state.secrets, &pending)?);
    Ok((jar, Redirect::to(&url)))
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
    jar: CookieJar,
) -> (CookieJar, Redirect) {
    match callback_inner(&state, q, &headers, &jar).await {
        Ok((jar, redirect)) => (jar, redirect),
        Err(err) => {
            tracing::warn!(error = ?err, "login callback failed");
            let jar = jar
                .add(cookies::clear_oauth_cookie())
                .add(cookies::clear_session_cookie());
            (jar, Redirect::to("/login?error=login_failed"))
        }
    }
}

async fn callback_inner(
    state: &AppState,
    q: CallbackQuery,
    headers: &HeaderMap,
    jar: &CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    if let Some(error) = q.error {
        let desc = q.error_description.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "GitLab refused the authorization: {error} {desc}"
        )));
    }
    let code = q
        .code
        .ok_or_else(|| AppError::BadRequest("missing authorization code".into()))?;
    let returned_state = q
        .state
        .ok_or_else(|| AppError::BadRequest("missing state".into()))?;

    let pending = jar
        .get(cookies::OAUTH_COOKIE)
        .ok_or_else(|| AppError::BadRequest("no pending login".into()))
        .and_then(|c| cookies::read_oauth_cookie(&state.secrets, c.value()))?;

    if pending.is_expired() {
        return Err(AppError::BadRequest("login attempt expired, retry".into()));
    }
    // Constant-time not required: the state is a one-time CSRF nonce
    // bound to this browser via the encrypted cookie.
    if pending.csrf != returned_state {
        return Err(AppError::BadRequest("state mismatch".into()));
    }

    let instance =
        instances::fetch_config(&state.pool, &state.secrets, pending.instance_id).await?;
    let tokens = oauth::exchange_code(
        &state.http,
        &instance,
        &state.public_url,
        code,
        pending.pkce_verifier,
    )
    .await?;

    let api = GitlabApi {
        http: &state.http,
        base_url: &instance.base_url,
        access_token: &tokens.access_token,
    };
    let gl_user = api.current_user().await?;

    let (user_id, _is_admin) = users::upsert_login(
        &state.pool,
        &state.secrets,
        instance.id,
        &gl_user,
        &tokens,
        &state.admin_emails,
    )
    .await?;

    let ip = super::client_ip(headers);
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let token = session::create(&state.pool, user_id, ip.as_deref(), user_agent).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user_id),
            action: "LOGIN",
            subject_type: Some("gitlab_instance"),
            subject_id: Some(instance.id.0),
            detail: Some(serde_json::json!({
                "instance": instance.name,
                "username": gl_user.username,
            })),
            ip_address: ip.as_deref(),
        },
    )
    .await?;

    let jar = jar
        .clone()
        .add(cookies::clear_oauth_cookie())
        .add(cookies::session_cookie(token));
    Ok((jar, Redirect::to("/")))
}

/// Local operator sign-in (docs/SECURITY.md § Identity & Sessions).
/// Failures are uniformly 401 — no username enumeration. Brute force
/// is rate-limited by the nginx `/auth/` zone.
pub async fn local_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    axum::Json(req): axum::Json<foundry_shared::dto::LocalLoginRequest>,
) -> Result<(CookieJar, StatusCode), AppError> {
    let account = crate::repos::local_admins::find_by_username(&state.pool, req.username.trim())
        .await?
        .filter(|acc| {
            crate::repos::local_admins::verify_password(&acc.password_hash, &req.password)
        })
        .ok_or(AppError::Unauthorized)?;

    let now = chrono::Utc::now().naive_utc();
    sqlx::query!(
        "UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?",
        now,
        now,
        account.user_id.0,
    )
    .execute(&state.pool)
    .await?;

    let ip = super::client_ip(&headers);
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let token = session::create(&state.pool, account.user_id, ip.as_deref(), user_agent).await?;

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(account.user_id),
            action: "LOGIN",
            subject_type: None,
            subject_id: None,
            detail: Some(serde_json::json!({
                "method": "local",
                "username": req.username.trim(),
            })),
            ip_address: ip.as_deref(),
        },
    )
    .await?;

    let jar = jar.clone().add(cookies::session_cookie(token));
    Ok((jar, StatusCode::NO_CONTENT))
}

pub async fn logout(
    State(state): State<AppState>,
    user: session::CurrentUser,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(cookie) = jar.get(cookies::SESSION_COOKIE) {
        session::delete_by_token(&state.pool, cookie.value()).await?;
    }
    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "LOGOUT",
            subject_type: None,
            subject_id: None,
            detail: None,
            ip_address: super::client_ip(&headers).as_deref(),
        },
    )
    .await?;
    let jar = jar.add(cookies::clear_session_cookie());
    Ok((jar, StatusCode::NO_CONTENT))
}

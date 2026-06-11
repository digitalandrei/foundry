//! Cookie construction. Two cookies exist:
//! `foundry_session` — opaque random token, server-side state;
//! `foundry_oauth` — encrypted PendingLogin, only alive across the
//! GitLab redirect (10 min).

use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine as _;
use time::Duration;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::gitlab::oauth::PendingLogin;

pub const SESSION_COOKIE: &str = "foundry_session";
pub const OAUTH_COOKIE: &str = "foundry_oauth";
pub const SESSION_TTL_DAYS: i64 = 7;

fn base(name: &'static str, value: String, path: &'static str) -> Cookie<'static> {
    Cookie::build((name, value))
        .path(path)
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .build()
}

pub fn session_cookie(token: String) -> Cookie<'static> {
    let mut c = base(SESSION_COOKIE, token, "/");
    c.set_max_age(Duration::days(SESSION_TTL_DAYS));
    c
}

pub fn clear_session_cookie() -> Cookie<'static> {
    let mut c = base(SESSION_COOKIE, String::new(), "/");
    c.set_max_age(Duration::ZERO);
    c
}

pub fn oauth_cookie(
    secrets: &SecretBox,
    pending: &PendingLogin,
) -> Result<Cookie<'static>, AppError> {
    let json = serde_json::to_vec(pending).map_err(AppError::internal)?;
    let value = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secrets.encrypt(&json));
    let mut c = base(OAUTH_COOKIE, value, "/auth");
    c.set_max_age(Duration::minutes(10));
    Ok(c)
}

pub fn read_oauth_cookie(secrets: &SecretBox, value: &str) -> Result<PendingLogin, AppError> {
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::BadRequest("malformed login state".into()))?;
    let plain = secrets
        .decrypt(&raw)
        .map_err(|_| AppError::BadRequest("malformed login state".into()))?;
    serde_json::from_slice(&plain).map_err(|_| AppError::BadRequest("malformed login state".into()))
}

pub fn clear_oauth_cookie() -> Cookie<'static> {
    let mut c = base(OAUTH_COOKIE, String::new(), "/auth");
    c.set_max_age(Duration::ZERO);
    c
}

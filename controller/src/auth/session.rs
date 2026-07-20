//! Server-side sessions. The cookie holds a 256-bit random token;
//! only its SHA-256 is stored (a DB leak yields no usable sessions).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use foundry_shared::UserId;
use sqlx::MySqlPool;
use uuid::Uuid;

use super::cookies::{SESSION_COOKIE, SESSION_TTL_DAYS};
use crate::crypto::{random_token, token_hash};
use crate::error::AppError;
use crate::state::AppState;

/// Create a session row and return the raw token for the cookie.
pub async fn create(
    pool: &MySqlPool,
    user_id: UserId,
    ip: Option<&str>,
    user_agent: Option<&str>,
    subject_type: Option<&str>,
    subject_id: Option<uuid::Uuid>,
    detail: serde_json::Value,
) -> Result<String, AppError> {
    let token = random_token();
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?",
        now.naive_utc(),
        now.naive_utc(),
        user_id.0,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO sessions
           (id, token_hash, user_id, ip_address, user_agent, expires_at, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        Uuid::now_v7(),
        token_hash(&token),
        user_id.0,
        ip,
        user_agent.map(|ua| ua.chars().take(255).collect::<String>()),
        (now + Duration::days(SESSION_TTL_DAYS)).naive_utc(),
        now.naive_utc(),
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user_id),
            action: "LOGIN",
            subject_type,
            subject_id,
            detail: Some(detail),
            ip_address: ip,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(token)
}

pub async fn delete_with_audit(
    pool: &MySqlPool,
    token: Option<&str>,
    user_id: UserId,
    ip: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    if let Some(token) = token {
        sqlx::query!(
            "DELETE FROM sessions WHERE token_hash = ?",
            token_hash(token)
        )
        .execute(&mut *tx)
        .await?;
    }
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user_id),
            action: "LOGOUT",
            subject_type: None,
            subject_id: None,
            detail: None,
            ip_address: ip,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Hourly cleanup of expired sessions; spawned at startup.
pub fn spawn_sweeper(pool: MySqlPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match sqlx::query!(
                "DELETE FROM sessions WHERE expires_at < ?",
                Utc::now().naive_utc()
            )
            .execute(&pool)
            .await
            {
                Ok(res) if res.rows_affected() > 0 => {
                    tracing::info!(deleted = res.rows_affected(), "expired sessions swept");
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(?err, "session sweep failed"),
            }
        }
    });
}

/// The authenticated portal user — extractor for every protected
/// handler. Rejects with 401 when the cookie is absent/expired.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: UserId,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;

        let row = sqlx::query!(
            r#"SELECT u.id AS "id: Uuid", u.display_name, u.email, u.avatar_url,
                      u.is_admin AS "is_admin: bool"
               FROM sessions s JOIN users u ON u.id = s.user_id
               WHERE s.token_hash = ? AND s.expires_at > ?"#,
            token_hash(&token),
            Utc::now().naive_utc(),
        )
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

        Ok(CurrentUser {
            id: row.id.into(),
            display_name: row.display_name,
            email: row.email,
            avatar_url: row.avatar_url,
            is_admin: row.is_admin,
        })
    }
}

/// CurrentUser + is_admin, for operator-only endpoints.
#[derive(Debug, Clone)]
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(AppError::Forbidden);
        }
        Ok(AdminUser(user))
    }
}

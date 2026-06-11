//! users + gitlab_accounts access: login upsert, token storage
//! (encrypted), and the account list for /api/me.

use chrono::{DateTime, Utc};
use foundry_shared::dto::GitlabAccountSummary;
use foundry_shared::{GitlabInstanceId, UserId};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::gitlab::oauth::ExchangedTokens;
use crate::gitlab::types::GitlabUser;

/// A user's account on one instance, tokens decrypted for use.
pub struct AccountTokens {
    pub account_id: Uuid,
    pub instance_id: GitlabInstanceId,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Upsert (user, gitlab_account) at login. Returns the portal user id
/// and admin flag. Admin status is granted (never revoked) when the
/// GitLab-reported email is in FOUNDRY_ADMIN_EMAILS.
pub async fn upsert_login(
    pool: &MySqlPool,
    secrets: &SecretBox,
    instance_id: GitlabInstanceId,
    gl_user: &GitlabUser,
    tokens: &ExchangedTokens,
    admin_emails: &[String],
) -> Result<(UserId, bool), AppError> {
    let now = chrono::Utc::now().naive_utc();
    let email_is_admin = gl_user
        .email
        .as_deref()
        .map(|e| admin_emails.contains(&e.to_lowercase()))
        .unwrap_or(false);
    let access_ct = secrets.encrypt_str(&tokens.access_token);
    let refresh_ct = tokens
        .refresh_token
        .as_deref()
        .map(|t| secrets.encrypt_str(t));
    let expires_naive = tokens.expires_at.map(|t| t.naive_utc());

    let mut tx = pool.begin().await?;

    let existing = sqlx::query!(
        r#"SELECT id AS "id: Uuid", user_id AS "user_id: Uuid" FROM gitlab_accounts
           WHERE gitlab_instance_id = ? AND gitlab_user_id = ?"#,
        instance_id.0,
        gl_user.id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let user_id: Uuid = match existing {
        Some(acc) => {
            sqlx::query!(
                r#"UPDATE gitlab_accounts
                   SET username = ?, access_token = ?, refresh_token = ?,
                       token_expires_at = ?, updated_at = ?
                   WHERE id = ?"#,
                gl_user.username,
                access_ct,
                refresh_ct,
                expires_naive,
                now,
                acc.id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"UPDATE users SET display_name = ?, email = ?, avatar_url = ?,
                       is_admin = (is_admin OR ?), last_login_at = ?, updated_at = ?
                   WHERE id = ?"#,
                gl_user.name,
                gl_user.email,
                gl_user.avatar_url,
                email_is_admin,
                now,
                now,
                acc.user_id,
            )
            .execute(&mut *tx)
            .await?;
            acc.user_id
        }
        None => {
            let user_id = Uuid::now_v7();
            sqlx::query!(
                r#"INSERT INTO users
                   (id, display_name, email, avatar_url, is_admin, last_login_at,
                    created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
                user_id,
                gl_user.name,
                gl_user.email,
                gl_user.avatar_url,
                email_is_admin,
                now,
                now,
                now,
            )
            .execute(&mut *tx)
            .await?;
            let account_id = Uuid::now_v7();
            sqlx::query!(
                r#"INSERT INTO gitlab_accounts
                   (id, user_id, gitlab_instance_id, gitlab_user_id, username,
                    access_token, refresh_token, token_expires_at, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                account_id,
                user_id,
                instance_id.0,
                gl_user.id,
                gl_user.username,
                access_ct,
                refresh_ct,
                expires_naive,
                now,
                now,
            )
            .execute(&mut *tx)
            .await?;
            user_id
        }
    };

    let is_admin = sqlx::query_scalar!(
        r#"SELECT is_admin AS "is_admin: bool" FROM users WHERE id = ?"#,
        user_id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((user_id.into(), is_admin))
}

pub async fn account_summaries(
    pool: &MySqlPool,
    user_id: UserId,
) -> Result<Vec<GitlabAccountSummary>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT a.gitlab_instance_id AS "instance_id: Uuid", a.username, i.name AS instance_name
           FROM gitlab_accounts a
           JOIN gitlab_instances i ON i.id = a.gitlab_instance_id
           WHERE a.user_id = ?
           ORDER BY i.name"#,
        user_id.0
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| GitlabAccountSummary {
            instance_id: r.instance_id.into(),
            instance_name: r.instance_name,
            username: r.username,
        })
        .collect())
}

/// Decrypted tokens for every enabled-instance account of a user.
pub async fn account_tokens(
    pool: &MySqlPool,
    secrets: &SecretBox,
    user_id: UserId,
) -> Result<Vec<AccountTokens>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT a.id AS "id: Uuid", a.gitlab_instance_id AS "instance_id: Uuid",
                  a.access_token, a.refresh_token, a.token_expires_at
           FROM gitlab_accounts a
           JOIN gitlab_instances i ON i.id = a.gitlab_instance_id AND i.enabled = 1
           WHERE a.user_id = ?"#,
        user_id.0
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let Some(access_ct) = r.access_token else {
            continue; // account without a stored token: skip
        };
        out.push(AccountTokens {
            account_id: r.id,
            instance_id: r.instance_id.into(),
            access_token: secrets
                .decrypt_str(&access_ct)
                .map_err(AppError::internal)?,
            refresh_token: r
                .refresh_token
                .map(|ct| secrets.decrypt_str(&ct))
                .transpose()
                .map_err(AppError::internal)?,
            expires_at: r.token_expires_at.map(|t| t.and_utc()),
        });
    }
    Ok(out)
}

pub async fn update_account_tokens(
    pool: &MySqlPool,
    secrets: &SecretBox,
    account_id: Uuid,
    tokens: &ExchangedTokens,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"UPDATE gitlab_accounts
           SET access_token = ?, refresh_token = ?, token_expires_at = ?, updated_at = ?
           WHERE id = ?"#,
        secrets.encrypt_str(&tokens.access_token),
        tokens
            .refresh_token
            .as_deref()
            .map(|t| secrets.encrypt_str(t)),
        tokens.expires_at.map(|t| t.naive_utc()),
        chrono::Utc::now().naive_utc(),
        account_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

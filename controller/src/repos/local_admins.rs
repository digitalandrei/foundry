//! Local (non-GitLab) operator accounts: argon2id-hashed credentials
//! joined to a regular `users` row with `is_admin = 1`. They carry no
//! GitLab identity — operational management only (docs/SECURITY.md).

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use foundry_shared::UserId;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::AppError;

pub const MIN_PASSWORD_LEN: usize = 12;

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(AppError::internal)
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Create a local admin: users row (is_admin=1) + credentials, one
/// transaction. Fails if the username is taken.
pub async fn create(
    pool: &MySqlPool,
    username: &str,
    display_name: &str,
    password: &str,
) -> Result<UserId, AppError> {
    validate_password(password)?;
    let hash = hash_password(password)?;
    let now = chrono::Utc::now().naive_utc();
    let user_id = Uuid::now_v7();

    let mut tx = pool.begin().await?;
    sqlx::query!(
        r#"INSERT INTO users
           (id, display_name, email, avatar_url, is_admin, last_login_at, created_at, updated_at)
           VALUES (?, ?, NULL, NULL, 1, NULL, ?, ?)"#,
        user_id,
        display_name,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO local_credentials (user_id, username, password_hash, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?)"#,
        user_id,
        username,
        hash,
        now,
        now,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("username already exists".into())
        }
        _ => AppError::Db(e),
    })?;
    tx.commit().await?;
    Ok(user_id.into())
}

pub struct LocalAccount {
    pub user_id: UserId,
    pub password_hash: String,
}

pub async fn find_by_username(
    pool: &MySqlPool,
    username: &str,
) -> Result<Option<LocalAccount>, AppError> {
    let row = sqlx::query!(
        r#"SELECT user_id AS "user_id: Uuid", password_hash
           FROM local_credentials WHERE username = ?"#,
        username
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| LocalAccount {
        user_id: r.user_id.into(),
        password_hash: r.password_hash,
    }))
}

pub async fn set_password(
    pool: &MySqlPool,
    username: &str,
    password: &str,
) -> Result<(), AppError> {
    validate_password(password)?;
    let hash = hash_password(password)?;
    let res = sqlx::query!(
        "UPDATE local_credentials SET password_hash = ?, updated_at = ? WHERE username = ?",
        hash,
        chrono::Utc::now().naive_utc(),
        username,
    )
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("no such local account"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_round_trip() {
        let hash = hash_password("correct horse battery staple").expect("hashes");
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(&hash, "correct horse battery staple"));
        assert!(!verify_password(&hash, "wrong password entirely"));
        assert!(!verify_password("not-a-hash", "anything"));
    }

    #[test]
    fn short_passwords_rejected() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("exactly12chr").is_ok());
    }
}

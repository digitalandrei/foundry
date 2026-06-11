//! gitlab_instances access. Client secrets are AES-encrypted at rest;
//! decryption happens only here, on demand.

use foundry_shared::dto::{InstanceAdmin, InstancePublic};
use foundry_shared::GitlabInstanceId;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::gitlab::InstanceConfig;

pub async fn list_public(pool: &MySqlPool) -> Result<Vec<InstancePublic>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id: Uuid", name FROM gitlab_instances
           WHERE enabled = 1 ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| InstancePublic {
            id: r.id.into(),
            name: r.name,
        })
        .collect())
}

pub async fn list_admin(pool: &MySqlPool) -> Result<Vec<InstanceAdmin>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id: Uuid", name, base_url, registry_url, oauth_client_id,
                  enabled AS "enabled: bool"
           FROM gitlab_instances ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| InstanceAdmin {
            id: r.id.into(),
            name: r.name,
            base_url: r.base_url,
            registry_url: r.registry_url,
            oauth_client_id: r.oauth_client_id,
            enabled: r.enabled,
        })
        .collect())
}

/// Load + decrypt one enabled instance for OAuth/API use.
pub async fn fetch_config(
    pool: &MySqlPool,
    secrets: &SecretBox,
    id: GitlabInstanceId,
) -> Result<InstanceConfig, AppError> {
    let row = sqlx::query!(
        r#"SELECT id AS "id: Uuid", name, base_url,
                  oauth_client_id, oauth_client_secret
           FROM gitlab_instances WHERE id = ? AND enabled = 1"#,
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("GitLab instance not found"))?;

    let secret = secrets
        .decrypt_str(&row.oauth_client_secret)
        .map_err(AppError::internal)?;
    Ok(InstanceConfig {
        id: row.id.into(),
        name: row.name,
        base_url: row.base_url,
        oauth_client_id: row.oauth_client_id,
        oauth_client_secret: secret,
    })
}

pub struct NewInstance<'a> {
    pub name: &'a str,
    pub base_url: &'a str,
    pub registry_url: &'a str,
    pub oauth_client_id: &'a str,
    pub oauth_client_secret: &'a str,
}

pub async fn insert(
    pool: &MySqlPool,
    secrets: &SecretBox,
    new: NewInstance<'_>,
) -> Result<GitlabInstanceId, AppError> {
    let id = Uuid::now_v7();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query!(
        r#"INSERT INTO gitlab_instances
           (id, name, base_url, registry_url, oauth_client_id, oauth_client_secret,
            enabled, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)"#,
        id,
        new.name,
        new.base_url,
        new.registry_url,
        new.oauth_client_id,
        secrets.encrypt_str(new.oauth_client_secret),
        now,
        now,
    )
    .execute(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("an instance with this name already exists".into())
        }
        _ => AppError::Db(e),
    })?;
    Ok(id.into())
}

/// Normalize and sanity-check an instance URL (https, no trailing /).
pub fn normalize_url(raw: &str, field: &'static str) -> Result<String, AppError> {
    let url = raw.trim().trim_end_matches('/').to_string();
    let ok = url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1");
    if !ok || url.len() < 12 {
        return Err(AppError::BadRequest(format!(
            "{field} must be an https:// URL"
        )));
    }
    Ok(url)
}

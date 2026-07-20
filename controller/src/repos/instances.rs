//! gitlab_instances access. Client secrets are AES-encrypted at rest;
//! decryption happens only here, on demand.

use foundry_shared::dto::{InstanceAdmin, InstancePublic};
use foundry_shared::{GitlabInstanceId, UserId};
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
        r#"SELECT id AS "id: Uuid", name, base_url, registry_url,
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
        registry_url: row.registry_url,
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
    created_by: Option<UserId>,
    ip_address: Option<&str>,
) -> Result<GitlabInstanceId, AppError> {
    let id = Uuid::now_v7();
    let now = chrono::Utc::now().naive_utc();
    let mut tx = pool.begin().await?;
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
    .execute(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("an instance with this name already exists".into())
        }
        _ => AppError::Db(e),
    })?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: if created_by.is_some() {
                foundry_shared::ActorType::User
            } else {
                foundry_shared::ActorType::Controller
            },
            actor_id: created_by,
            action: "INSTANCE_ONBOARDED",
            subject_type: Some("gitlab_instance"),
            subject_id: Some(id),
            detail: Some(serde_json::json!({ "name": new.name, "base_url": new.base_url })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(id.into())
}

pub struct InstanceUpdate<'a> {
    pub name: &'a str,
    pub base_url: &'a str,
    pub registry_url: &'a str,
    pub oauth_client_id: &'a str,
    /// None keeps the stored secret; Some re-encrypts a new one.
    pub oauth_client_secret: Option<&'a str>,
    pub enabled: bool,
}

pub async fn update(
    pool: &MySqlPool,
    secrets: &SecretBox,
    id: GitlabInstanceId,
    upd: InstanceUpdate<'_>,
    changed_by: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().naive_utc();
    let secret_rotated = upd.oauth_client_secret.is_some();
    let mut tx = pool.begin().await?;
    let result = match upd.oauth_client_secret {
        Some(secret) => {
            sqlx::query!(
                r#"UPDATE gitlab_instances
                   SET name = ?, base_url = ?, registry_url = ?, oauth_client_id = ?,
                       oauth_client_secret = ?, enabled = ?, updated_at = ?
                   WHERE id = ?"#,
                upd.name,
                upd.base_url,
                upd.registry_url,
                upd.oauth_client_id,
                secrets.encrypt_str(secret),
                upd.enabled,
                now,
                id.0,
            )
            .execute(&mut *tx)
            .await
        }
        None => {
            sqlx::query!(
                r#"UPDATE gitlab_instances
                   SET name = ?, base_url = ?, registry_url = ?, oauth_client_id = ?,
                       enabled = ?, updated_at = ?
                   WHERE id = ?"#,
                upd.name,
                upd.base_url,
                upd.registry_url,
                upd.oauth_client_id,
                upd.enabled,
                now,
                id.0,
            )
            .execute(&mut *tx)
            .await
        }
    }
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("an instance with this name already exists".into())
        }
        _ => AppError::Db(e),
    })?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("GitLab instance not found"));
    }
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(changed_by),
            action: "INSTANCE_UPDATED",
            subject_type: Some("gitlab_instance"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({
                "name": upd.name,
                "base_url": upd.base_url,
                "enabled": upd.enabled,
                "secret_rotated": secret_rotated,
            })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Delete an onboarded instance — refused while anything references it
/// (linked accounts, mirrored projects, deployments). The admin
/// disables it instead in that case (disabled hides it from login and
/// blocks new use without losing history).
pub async fn delete(
    pool: &MySqlPool,
    id: GitlabInstanceId,
    deleted_by: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM gitlab_instances WHERE id = ? FOR UPDATE")
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("GitLab instance not found"))?;
    let accounts = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM gitlab_accounts WHERE gitlab_instance_id = ?",
        id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    let projects = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM gitlab_projects WHERE gitlab_instance_id = ?",
        id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    let deployments = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM deployments WHERE gitlab_instance_id = ?",
        id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    if accounts + projects + deployments > 0 {
        return Err(AppError::BadRequest(format!(
            "instance still has {accounts} linked account(s), {projects} mirrored project(s), \
             and {deployments} deployment(s) — disable it instead of deleting"
        )));
    }
    let result = sqlx::query!("DELETE FROM gitlab_instances WHERE id = ?", id.0)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("GitLab instance not found"));
    }
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(deleted_by),
            action: "INSTANCE_DELETED",
            subject_type: Some("gitlab_instance"),
            subject_id: Some(id.0),
            detail: None,
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
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

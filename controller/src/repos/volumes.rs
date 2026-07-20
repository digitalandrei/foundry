//! Persistent per-server volumes (operator requirement, Phase 6):
//! named host directories under /storage/containers/<owner>/<name>,
//! namespaced per user. Created on first use at deploy time, they
//! outlive deployments and can be remounted into later containers;
//! deletion is explicit and removes the data via a REMOVE_VOLUME
//! agent task. Users see and mount only their own volumes; admins
//! see all.

use foundry_shared::dto::ServerVolume;
use foundry_shared::{ServerId, ServerVolumeId, UserId};
use sqlx::{MySqlConnection, MySqlPool, Row};
use uuid::Uuid;

use crate::error::AppError;

pub const VOLUME_ROOT: &str = "/storage/containers";

/// Stable path segment for a user: local username, else first GitLab
/// username, else a uuid prefix. Sanitized; collisions are caught by
/// the unique (server, path) key and retried with a suffix.
pub async fn owner_slug(pool: &MySqlPool, user_id: UserId) -> Result<String, AppError> {
    let local = sqlx::query_scalar!(
        "SELECT username FROM local_credentials WHERE user_id = ?",
        user_id.0
    )
    .fetch_optional(pool)
    .await?;
    let name = match local {
        Some(n) => n,
        None => sqlx::query_scalar!(
            "SELECT username FROM gitlab_accounts WHERE user_id = ? ORDER BY created_at LIMIT 1",
            user_id.0
        )
        .fetch_optional(pool)
        .await?
        .unwrap_or_else(|| user_id.0.simple().to_string()[..8].to_string()),
    };
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    Ok(slug.trim_matches('-').to_string())
}

pub fn validate_volume_name(name: &str) -> Result<(), AppError> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if !ok {
        return Err(AppError::BadRequest(format!(
            "invalid volume name {name:?} (alphanumeric/dash/underscore, ≤63 chars)"
        )));
    }
    Ok(())
}

/// Create-or-reuse the requester's named volume on a server (inside
/// the caller's deploy transaction). Returns (id, host path).
pub async fn ensure(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    name: &str,
    created_by: UserId,
    owner_slug: &str,
) -> Result<(ServerVolumeId, String), AppError> {
    validate_volume_name(name)?;
    let existing = sqlx::query!(
        r#"SELECT id AS "id: Uuid", path FROM server_volumes
           WHERE server_id = ? AND created_by = ? AND name = ?"#,
        server_id.0,
        created_by.0,
        name,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = existing {
        return Ok((row.id.into(), row.path));
    }

    let now = chrono::Utc::now().naive_utc();
    // Slug collision across users → unique (server, path) fires; retry
    // once with a user-id suffix.
    for candidate in [
        owner_slug.to_string(),
        format!("{owner_slug}-{}", &created_by.0.simple().to_string()[..6]),
    ] {
        let id = Uuid::now_v7();
        let path = format!("{VOLUME_ROOT}/{candidate}/{name}");
        let res = sqlx::query!(
            "INSERT INTO server_volumes
                 (id, server_id, name, owner_slug, path, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            server_id.0,
            name,
            candidate,
            path,
            created_by.0,
            now,
            now,
        )
        .execute(&mut *tx)
        .await;
        match res {
            Ok(_) => return Ok((id.into(), path)),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => continue,
            Err(e) => return Err(AppError::Db(e)),
        }
    }
    Err(AppError::BadRequest(
        "volume path collision — pick a different volume name".into(),
    ))
}

pub struct VolumeRow {
    pub server_id: ServerId,
    pub name: String,
    pub path: String,
    pub created_by: UserId,
}

pub async fn get(pool: &MySqlPool, id: ServerVolumeId) -> Result<VolumeRow, AppError> {
    let r = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid", name, path,
                  created_by AS "created_by: Uuid"
           FROM server_volumes WHERE id = ?"#,
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("volume not found"))?;
    Ok(VolumeRow {
        server_id: r.server_id.into(),
        name: r.name,
        path: r.path,
        created_by: r.created_by.into(),
    })
}

/// Volumes of one server with the active deployments mounting them.
/// `only_owner` scopes the list for non-admin requesters.
pub async fn list(
    pool: &MySqlPool,
    server_id: ServerId,
    only_owner: Option<UserId>,
) -> Result<Vec<ServerVolume>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT v.id AS "id: Uuid", v.name, v.path, v.created_at,
                  v.created_by AS "created_by: Uuid",
                  u.display_name AS created_by_name
           FROM server_volumes v
           JOIN users u ON u.id = v.created_by
           WHERE v.server_id = ?
           ORDER BY v.name"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| only_owner.is_none_or(|owner| r.created_by == owner.0))
        .collect();

    let mut attachments: std::collections::HashMap<ServerVolumeId, Vec<String>> =
        std::collections::HashMap::new();
    let owner = only_owner.map(|u| u.0);
    for attached in sqlx::query(
        r#"SELECT DISTINCT dv.server_volume_id, d.container_name
           FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           JOIN server_volumes v ON v.id = dv.server_volume_id
           WHERE v.server_id = ? AND (? IS NULL OR v.created_by = ?)
             AND d.container_name IS NOT NULL
             AND d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           ORDER BY dv.server_volume_id, d.container_name"#,
    )
    .bind(server_id.0)
    .bind(owner)
    .bind(owner)
    .fetch_all(pool)
    .await?
    {
        let volume_id: Uuid = attached
            .try_get("server_volume_id")
            .map_err(AppError::internal)?;
        attachments.entry(volume_id.into()).or_default().push(
            attached
                .try_get("container_name")
                .map_err(AppError::internal)?,
        );
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let volume_id: ServerVolumeId = r.id.into();
        out.push(ServerVolume {
            id: volume_id,
            name: r.name,
            path: r.path,
            created_by_name: r.created_by_name,
            attached_to: attachments.remove(&volume_id).unwrap_or_default(),
            created_at: r.created_at.and_utc(),
        });
    }
    Ok(out)
}

/// Delete a volume atomically: the attached-check, the REMOVE_VOLUME
/// task, and the row removal share one transaction so a concurrent
/// deploy cannot mount it mid-delete (the volume row is locked; a
/// concurrent ensure() of the same volume blocks on it).
pub async fn delete_guarded(
    pool: &MySqlPool,
    id: ServerVolumeId,
    server_id: ServerId,
    path: &str,
    name: &str,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    // Lock the volume row itself — serializes against ensure()/other
    // deletes — then check attachments with the lock held.
    sqlx::query!(
        "SELECT id AS `i: Uuid` FROM server_volumes WHERE id = ? FOR UPDATE",
        id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("volume not found"))?;
    let attached = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployment_volumes dv
           JOIN deployments d ON d.id = dv.deployment_id
           WHERE dv.server_volume_id = ?
             AND d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                             'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING',
                             'REMOVING','FAILED')
           FOR UPDATE"#,
        id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    if attached > 0 {
        return Err(AppError::BadRequest(
            "volume is mounted by an active deployment".into(),
        ));
    }
    super::tasks::enqueue(
        &mut tx,
        server_id,
        None,
        foundry_shared::TaskType::RemoveVolume,
        &foundry_shared::dto::TaskPayload::Volume(foundry_shared::dto::VolumeTarget {
            volume_id: id,
            path: path.to_string(),
        }),
    )
    .await?;
    sqlx::query!(
        "UPDATE deployment_volumes SET server_volume_id = NULL WHERE server_volume_id = ?",
        id.0
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM server_volumes WHERE id = ?", id.0)
        .execute(&mut *tx)
        .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "VOLUME_DELETED",
            subject_type: Some("server_volume"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({ "name": name, "path": path })),
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

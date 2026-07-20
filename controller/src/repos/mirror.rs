//! GitLab mirror upserts (gitlab_projects, registry_repositories,
//! registry_tags). Cache for browsing speed — never an ACL
//! (docs/GITLAB-INTEGRATION.md § Authorization Resolution).

use foundry_shared::{GitlabInstanceId, GitlabProjectId, RegistryRepositoryId};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::gitlab::types::{GitlabProject, GitlabRegistryRepository, GitlabRegistryTagDetail};

pub async fn upsert_project(
    pool: &MySqlPool,
    instance_id: GitlabInstanceId,
    p: &GitlabProject,
) -> Result<GitlabProjectId, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let candidate_id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO gitlab_projects
           (id, gitlab_instance_id, gitlab_project_id, path_with_namespace,
            name, avatar_url, last_synced_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON DUPLICATE KEY UPDATE
             path_with_namespace = VALUES(path_with_namespace),
             name = VALUES(name),
             avatar_url = VALUES(avatar_url),
             last_synced_at = VALUES(last_synced_at),
             updated_at = VALUES(updated_at)"#,
        candidate_id,
        instance_id.0,
        p.id,
        p.path_with_namespace,
        p.name,
        p.avatar_url,
        now,
        now,
        now,
    )
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM gitlab_projects
           WHERE gitlab_instance_id = ? AND gitlab_project_id = ?"#,
        instance_id.0,
        p.id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id.into())
}

pub struct ProjectRow {
    pub id: GitlabProjectId,
    pub instance_id: GitlabInstanceId,
    pub gitlab_project_id: i64,
}

pub async fn project_by_id(pool: &MySqlPool, id: GitlabProjectId) -> Result<ProjectRow, AppError> {
    let row = sqlx::query!(
        r#"SELECT id AS "id: Uuid", gitlab_instance_id AS "instance_id: Uuid",
                  gitlab_project_id
           FROM gitlab_projects WHERE id = ?"#,
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("project not found"))?;
    Ok(ProjectRow {
        id: row.id.into(),
        instance_id: row.instance_id.into(),
        gitlab_project_id: row.gitlab_project_id,
    })
}

pub async fn upsert_repository(
    pool: &MySqlPool,
    project_id: GitlabProjectId,
    r: &GitlabRegistryRepository,
) -> Result<RegistryRepositoryId, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let candidate_id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO registry_repositories
           (id, gitlab_project_id, gitlab_repository_id, path,
            last_synced_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           ON DUPLICATE KEY UPDATE
             path = VALUES(path),
             last_synced_at = VALUES(last_synced_at),
             updated_at = VALUES(updated_at)"#,
        candidate_id,
        project_id.0,
        r.id,
        r.path,
        now,
        now,
        now,
    )
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM registry_repositories
           WHERE gitlab_project_id = ? AND gitlab_repository_id = ?"#,
        project_id.0,
        r.id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id.into())
}

pub async fn upsert_tag(
    pool: &MySqlPool,
    repository_id: RegistryRepositoryId,
    t: &GitlabRegistryTagDetail,
) -> Result<foundry_shared::RegistryTagId, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let candidate_id = Uuid::now_v7();
    // GitLab can explicitly report zero for a valid image when its
    // registry metadata is unavailable. Zero is not a useful image size;
    // preserve a positive registry-manifest fallback if one was cached.
    let size_bytes = t.total_size.filter(|size| *size > 0);
    sqlx::query!(
        r#"INSERT INTO registry_tags
           (id, registry_repository_id, name, digest, size_bytes, pushed_at,
            last_synced_at, created_at, updated_at)
           VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?)
           ON DUPLICATE KEY UPDATE
             size_bytes = COALESCE(VALUES(size_bytes), NULLIF(size_bytes, 0)),
             pushed_at = COALESCE(VALUES(pushed_at), pushed_at),
             last_synced_at = VALUES(last_synced_at),
             updated_at = VALUES(updated_at)"#,
        candidate_id,
        repository_id.0,
        t.name,
        size_bytes,
        t.created_at.map(|d| d.naive_utc()),
        now,
        now,
        now,
    )
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM registry_tags
           WHERE registry_repository_id = ? AND name = ?"#,
        repository_id.0,
        t.name,
    )
    .fetch_one(pool)
    .await?;
    Ok(id.into())
}

/// A tag the poller just discovered (name-only insert).
pub struct NewTag {
    pub id: foundry_shared::RegistryTagId,
    pub name: String,
}

/// Insert any tag NAMES not yet mirrored for this repo (name-only — no
/// size/date; a later full browse fills those). Returns the rows it
/// actually inserted, i.e. the freshly-discovered tags — the cheap
/// detector behind the new-image poller. `INSERT IGNORE` makes a
/// concurrent poll racing the same `(repo, name)` a no-op, not an error.
pub async fn insert_new_tag_names(
    pool: &MySqlPool,
    repository_id: RegistryRepositoryId,
    names: &[String],
) -> Result<Vec<NewTag>, AppError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let existing: std::collections::HashSet<String> = sqlx::query_scalar!(
        "SELECT name FROM registry_tags WHERE registry_repository_id = ?",
        repository_id.0,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let now = chrono::Utc::now().naive_utc();
    let mut inserted = Vec::new();
    for name in names {
        if existing.contains(name) {
            continue;
        }
        let id = Uuid::now_v7();
        let res = sqlx::query!(
            r#"INSERT IGNORE INTO registry_tags
               (id, registry_repository_id, name, digest, size_bytes, pushed_at,
                last_synced_at, created_at, updated_at)
               VALUES (?, ?, ?, NULL, NULL, NULL, ?, ?, ?)"#,
            id,
            repository_id.0,
            name,
            now,
            now,
            now,
        )
        .execute(pool)
        .await?;
        if res.rows_affected() == 1 {
            inserted.push(NewTag {
                id: id.into(),
                name: name.clone(),
            });
        }
    }
    Ok(inserted)
}

/// Everything needed to build a pullable image_ref + mint a pull token.
pub struct TagRef {
    pub tag_name: String,
    pub repo_path: String,
    pub instance_id: GitlabInstanceId,
    pub registry_url: String,
}

pub async fn tag_ref(
    pool: &MySqlPool,
    tag_id: foundry_shared::RegistryTagId,
) -> Result<TagRef, AppError> {
    let row = sqlx::query!(
        r#"SELECT t.name AS tag_name, r.path AS repo_path,
                  p.gitlab_instance_id AS "instance_id: Uuid", i.registry_url
           FROM registry_tags t
           JOIN registry_repositories r ON r.id = t.registry_repository_id
           JOIN gitlab_projects p ON p.id = r.gitlab_project_id
           JOIN gitlab_instances i ON i.id = p.gitlab_instance_id
           WHERE t.id = ?"#,
        tag_id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("image tag not found"))?;
    Ok(TagRef {
        tag_name: row.tag_name,
        repo_path: row.repo_path,
        instance_id: row.instance_id.into(),
        registry_url: row.registry_url,
    })
}

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
    let existing = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM gitlab_projects
           WHERE gitlab_instance_id = ? AND gitlab_project_id = ?"#,
        instance_id.0,
        p.id,
    )
    .fetch_optional(pool)
    .await?;

    let id = match existing {
        Some(id) => {
            sqlx::query!(
                r#"UPDATE gitlab_projects
                   SET name = ?, path_with_namespace = ?, avatar_url = ?,
                       last_synced_at = ?, updated_at = ?
                   WHERE id = ?"#,
                p.name,
                p.path_with_namespace,
                p.avatar_url,
                now,
                now,
                id,
            )
            .execute(pool)
            .await?;
            id
        }
        None => {
            let id = Uuid::now_v7();
            sqlx::query!(
                r#"INSERT INTO gitlab_projects
                   (id, gitlab_instance_id, gitlab_project_id, path_with_namespace,
                    name, avatar_url, last_synced_at, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                id,
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
            id
        }
    };
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
    let existing = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM registry_repositories
           WHERE gitlab_project_id = ? AND gitlab_repository_id = ?"#,
        project_id.0,
        r.id,
    )
    .fetch_optional(pool)
    .await?;

    let id = match existing {
        Some(id) => {
            sqlx::query!(
                r#"UPDATE registry_repositories
                   SET path = ?, last_synced_at = ?, updated_at = ? WHERE id = ?"#,
                r.path,
                now,
                now,
                id,
            )
            .execute(pool)
            .await?;
            id
        }
        None => {
            let id = Uuid::now_v7();
            sqlx::query!(
                r#"INSERT INTO registry_repositories
                   (id, gitlab_project_id, gitlab_repository_id, path,
                    last_synced_at, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#,
                id,
                project_id.0,
                r.id,
                r.path,
                now,
                now,
                now,
            )
            .execute(pool)
            .await?;
            id
        }
    };
    Ok(id.into())
}

pub async fn upsert_tag(
    pool: &MySqlPool,
    repository_id: RegistryRepositoryId,
    t: &GitlabRegistryTagDetail,
) -> Result<foundry_shared::RegistryTagId, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let existing = sqlx::query_scalar!(
        r#"SELECT id AS "id: Uuid" FROM registry_tags
           WHERE registry_repository_id = ? AND name = ?"#,
        repository_id.0,
        t.name,
    )
    .fetch_optional(pool)
    .await?;
    let id = match existing {
        Some(id) => {
            sqlx::query!(
                "UPDATE registry_tags SET size_bytes = ?, pushed_at = ?,
                     last_synced_at = ?, updated_at = ? WHERE id = ?",
                t.total_size,
                t.created_at.map(|d| d.naive_utc()),
                now,
                now,
                id,
            )
            .execute(pool)
            .await?;
            id
        }
        None => {
            let id = Uuid::now_v7();
            sqlx::query!(
                r#"INSERT INTO registry_tags
                   (id, registry_repository_id, name, digest, size_bytes, pushed_at,
                    last_synced_at, created_at, updated_at)
                   VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?)"#,
                id,
                repository_id.0,
                t.name,
                t.total_size,
                t.created_at.map(|d| d.naive_utc()),
                now,
                now,
                now,
            )
            .execute(pool)
            .await?;
            id
        }
    };
    Ok(id.into())
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

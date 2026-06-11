//! `GET /api/registry/{project_id}` — repositories + tags of one
//! project, fetched live with the user's token (authorization is
//! GitLab's 401/403), mirrored as cache.

use axum::extract::{Path, State};
use axum::Json;
use foundry_shared::dto::{RegistryBrowseResponse, RegistryRepository, RegistryTag};
use foundry_shared::GitlabProjectId;

use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::client::GitlabApi;
use crate::gitlab::tokens;
use crate::repos::{instances, mirror, users};
use crate::state::AppState;

pub async fn browse(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<GitlabProjectId>,
) -> Result<Json<RegistryBrowseResponse>, AppError> {
    let project = mirror::project_by_id(&state.pool, project_id).await?;
    let accounts = users::account_tokens(&state.pool, &state.secrets, user.id).await?;
    let account = accounts
        .into_iter()
        .find(|a| a.instance_id == project.instance_id)
        .ok_or(AppError::Forbidden)?;

    let instance =
        instances::fetch_config(&state.pool, &state.secrets, project.instance_id).await?;
    let token = tokens::ensure_fresh(&state, &instance, &account).await?;
    let api = GitlabApi {
        http: &state.http,
        base_url: &instance.base_url,
        access_token: &token,
    };

    let mut repositories = Vec::new();
    for repo in api.registry_repositories(project.gitlab_project_id).await? {
        let repo_id = mirror::upsert_repository(&state.pool, project.id, &repo).await?;
        let mut tags = Vec::new();
        for tag in api
            .registry_tags(project.gitlab_project_id, repo.id)
            .await?
        {
            mirror::upsert_tag(&state.pool, repo_id, &tag).await?;
            tags.push(RegistryTag {
                name: tag.name,
                size_bytes: tag.total_size,
                pushed_at: tag.created_at,
            });
        }
        // Newest first — the mockup lists most recent versions on top.
        tags.sort_by_key(|t| std::cmp::Reverse(t.pushed_at));
        repositories.push(RegistryRepository {
            id: repo_id,
            path: repo.path,
            tags,
        });
    }

    Ok(Json(RegistryBrowseResponse { repositories }))
}

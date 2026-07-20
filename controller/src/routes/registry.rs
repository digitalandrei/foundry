//! `GET /api/registry/{project_id}` — repositories + tags of one
//! project, fetched live with the user's token (authorization is
//! GitLab's 401/403), mirrored as cache.

use axum::extract::{Path, State};
use axum::Json;
use foundry_shared::dto::{
    ImageMetadataResponse, RegistryBrowseResponse, RegistryNewTag, RegistryRepository, RegistryTag,
    RegistryUpdates,
};
use foundry_shared::{GitlabProjectId, RegistryTagId};

use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::client::GitlabApi;
use crate::gitlab::{registry, tokens};
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
        let mut upstream_tags = api
            .registry_tags(project.gitlab_project_id, repo.id)
            .await?;
        // Self-managed GitLab may explicitly report a valid image as
        // zero bytes when registry size metadata is unavailable. Only
        // those explicit zeros get the registry-manifest fallback;
        // missing detail remains unknown and avoids unbounded fan-out.
        let pull_token = if upstream_tags.iter().any(|tag| tag.total_size == Some(0)) {
            tokens::registry_pull_token(&state.http, &instance.base_url, &token, &repo.path)
                .await
                .ok()
        } else {
            None
        };
        let mut tags = Vec::new();
        for tag in &mut upstream_tags {
            if tag.total_size == Some(0) {
                tag.total_size = registry::compressed_size(
                    &state.http,
                    &instance.registry_url,
                    pull_token.as_deref(),
                    &repo.path,
                    &tag.name,
                )
                .await
                .ok()
                .flatten();
            }
            let tag_id = mirror::upsert_tag(&state.pool, repo_id, tag).await?;
            tags.push(RegistryTag {
                id: tag_id,
                name: tag.name.clone(),
                size_bytes: tag.total_size.filter(|size| *size > 0),
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

/// `GET /api/registry/tags/{tag_id}/metadata` — read deploy defaults
/// from the image manifest/config. Best-effort by contract: any
/// upstream hiccup degrades to empty editable defaults.
pub async fn image_metadata(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tag_id): Path<RegistryTagId>,
) -> Result<Json<ImageMetadataResponse>, AppError> {
    let tag = mirror::tag_ref(&state.pool, tag_id).await?;

    // Same authorization shape as deployment create: an account on the
    // image's instance, or an admin (who may inspect public images
    // anonymously).
    let accounts = users::account_tokens(&state.pool, &state.secrets, user.id).await?;
    let account = accounts
        .into_iter()
        .find(|a| a.instance_id == tag.instance_id);
    let pull_token = match account {
        Some(account) => {
            let instance =
                instances::fetch_config(&state.pool, &state.secrets, tag.instance_id).await?;
            let access = tokens::ensure_fresh(&state, &instance, &account).await?;
            tokens::registry_pull_token(&state.http, &instance.base_url, &access, &tag.repo_path)
                .await
                .ok() // mint failure → try anonymous below
        }
        None if user.is_admin => None,
        None => return Err(AppError::Forbidden),
    };

    let metadata = registry::image_metadata(
        &state.http,
        &tag.registry_url,
        pull_token.as_deref(),
        &tag.repo_path,
        &tag.tag_name,
    )
    .await
    .unwrap_or_else(|err| {
        tracing::debug!(?err, repo = %tag.repo_path, tag = %tag.tag_name,
            "image metadata discovery failed (non-fatal)");
        ImageMetadataResponse::default()
    });
    Ok(Json(metadata))
}

/// Hard cap on repos scanned per account per poll — bounds GitLab load
/// for a user in very many registry projects; the rest are picked up on
/// a later poll or a manual browse.
const MAX_WATCHED_REPOS: usize = 100;

/// `GET /api/registry/updates` — cheap new-image poller. Walks the user's
/// available projects → registry repos → tag NAMES (no per-tag detail)
/// and returns the tags first seen this round (mirror inserts). The SPA
/// polls it on a timer, treats its first response as a silent baseline,
/// and toasts thereafter. Authorization is the user's own token per
/// instance, exactly like browse/projects.
pub async fn updates(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<RegistryUpdates>, AppError> {
    let accounts = users::account_tokens(&state.pool, &state.secrets, user.id).await?;
    let mut new_tags = Vec::new();

    for account in accounts {
        let instance =
            match instances::fetch_config(&state.pool, &state.secrets, account.instance_id).await {
                Ok(i) => i,
                Err(_) => continue,
            };
        // One unreachable instance degrades that account, not the whole
        // poll (docs/GITLAB-INTEGRATION.md § Failure Modes).
        let result: Result<(), AppError> = async {
            let token = tokens::ensure_fresh(&state, &instance, &account).await?;
            let api = GitlabApi {
                http: &state.http,
                base_url: &instance.base_url,
                access_token: &token,
            };
            let mut scanned = 0usize;
            for p in api.projects().await? {
                let project_id = mirror::upsert_project(&state.pool, instance.id, &p).await?;
                for repo in api.registry_repositories(p.id).await? {
                    if scanned >= MAX_WATCHED_REPOS {
                        tracing::warn!(instance = %instance.name,
                            "registry-updates repo cap hit; remaining repos polled next round");
                        return Ok(());
                    }
                    scanned += 1;
                    let repo_id = mirror::upsert_repository(&state.pool, project_id, &repo).await?;
                    let names: Vec<String> = api
                        .registry_tag_names(p.id, repo.id)
                        .await?
                        .into_iter()
                        .map(|t| t.name)
                        .collect();
                    for fresh in mirror::insert_new_tag_names(&state.pool, repo_id, &names).await? {
                        new_tags.push(RegistryNewTag {
                            id: fresh.id,
                            tag_name: fresh.name,
                            repo_path: repo.path.clone(),
                            project_id,
                        });
                    }
                }
            }
            Ok(())
        }
        .await;
        if let Err(err) = result {
            tracing::warn!(instance = %instance.name, ?err, "registry updates poll degraded");
        }
    }

    Ok(Json(RegistryUpdates { new_tags }))
}

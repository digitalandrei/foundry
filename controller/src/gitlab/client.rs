//! Authenticated GitLab REST v4 calls with the *user's* token — that
//! is the authorization mechanism: GitLab decides what each user sees
//! (docs/GITLAB-INTEGRATION.md § Authorization Resolution).
//!
//! Every list call paginates with `per_page=100` following
//! `x-next-page` until exhausted.

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use super::types::*;
use crate::error::AppError;

const PER_PAGE: u32 = 100;
/// Hard cap on pagination rounds — a runaway instance cannot make us
/// loop forever (docs/SECURITY.md § Input & Secrets Hygiene).
const MAX_PAGES: u32 = 50;
/// Tag-detail fan-out cap per repository (size/date come from per-tag
/// detail requests).
const MAX_TAG_DETAILS: usize = 50;

pub struct GitlabApi<'a> {
    pub http: &'a reqwest::Client,
    pub base_url: &'a str,
    pub access_token: &'a str,
}

impl GitlabApi<'_> {
    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<(T, HeaderMap), AppError> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(self.access_token)
            .send()
            .await
            .map_err(AppError::gitlab)?;
        let status = resp.status();
        let headers = resp.headers().clone();
        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::Unauthorized);
        }
        if !status.is_success() {
            return Err(AppError::BadRequest(format!(
                "GitLab returned {status} for this request"
            )));
        }
        let body = resp.json::<T>().await.map_err(AppError::gitlab)?;
        Ok((body, headers))
    }

    async fn get_paginated<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, AppError> {
        let mut out = Vec::new();
        let sep = if path.contains('?') { '&' } else { '?' };
        let mut page = 1u32;
        loop {
            let url = format!(
                "{}{path}{sep}per_page={PER_PAGE}&page={page}",
                self.base_url
            );
            let (mut items, headers) = self.get_json::<Vec<T>>(&url).await?;
            out.append(&mut items);
            let next = headers
                .get("x-next-page")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u32>().ok());
            match next {
                Some(n) if n > page && page < MAX_PAGES => page = n,
                _ => break,
            }
        }
        Ok(out)
    }

    /// The authenticated user (`/api/v4/user`).
    pub async fn current_user(&self) -> Result<GitlabUser, AppError> {
        let url = format!("{}/api/v4/user", self.base_url);
        Ok(self.get_json(&url).await?.0)
    }

    /// Projects the user is a member of.
    pub async fn projects(&self) -> Result<Vec<GitlabProject>, AppError> {
        self.get_paginated("/api/v4/projects?membership=true&simple=true&archived=false")
            .await
    }

    /// Registry repositories of one project.
    pub async fn registry_repositories(
        &self,
        project_id: i64,
    ) -> Result<Vec<GitlabRegistryRepository>, AppError> {
        self.get_paginated(&format!(
            "/api/v4/projects/{project_id}/registry/repositories"
        ))
        .await
    }

    /// Tag NAMES of one repository — a single paginated list call, no
    /// per-tag detail (cheap; the new-image poller uses this).
    pub async fn registry_tag_names(
        &self,
        project_id: i64,
        repository_id: i64,
    ) -> Result<Vec<GitlabRegistryTag>, AppError> {
        self.get_paginated(&format!(
            "/api/v4/projects/{project_id}/registry/repositories/{repository_id}/tags"
        ))
        .await
    }

    /// Tags of one repository, with per-tag detail (size, created_at)
    /// for the first `MAX_TAG_DETAILS` tags. Detail requests run
    /// sequentially — bounded and simple; revisit if real registries
    /// make this slow.
    pub async fn registry_tags(
        &self,
        project_id: i64,
        repository_id: i64,
    ) -> Result<Vec<GitlabRegistryTagDetail>, AppError> {
        let names = self.registry_tag_names(project_id, repository_id).await?;

        let mut detailed = Vec::with_capacity(names.len());
        for (i, tag) in names.into_iter().enumerate() {
            if i < MAX_TAG_DETAILS {
                let url = format!(
                    "{}/api/v4/projects/{project_id}/registry/repositories/{repository_id}/tags/{}",
                    self.base_url, tag.name
                );
                match self.get_json::<GitlabRegistryTagDetail>(&url).await {
                    Ok((detail, _)) => {
                        detailed.push(detail);
                        continue;
                    }
                    Err(AppError::Unauthorized) => return Err(AppError::Unauthorized),
                    // Detail is best-effort; fall through to the bare name.
                    Err(_) => {}
                }
            }
            detailed.push(GitlabRegistryTagDetail {
                name: tag.name,
                total_size: None,
                created_at: None,
            });
        }
        Ok(detailed)
    }
}

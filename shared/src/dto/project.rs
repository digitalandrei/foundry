//! `GET /api/projects` (docs/API.md) — projects visible to the current
//! user, resolved live against GitLab (mirror rows are a cache).

use serde::{Deserialize, Serialize};

use crate::{GitlabInstanceId, GitlabProjectId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// Foundry mirror-row id (used to browse the registry).
    pub id: GitlabProjectId,
    pub instance_id: GitlabInstanceId,
    /// Numeric id on the GitLab instance.
    pub gitlab_project_id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub avatar_url: Option<String>,
}

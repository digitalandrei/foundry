//! `GET /api/registry/{project_id}` (docs/API.md) — container registry
//! browse: repositories and tags for one project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::RegistryRepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryBrowseResponse {
    pub repositories: Vec<RegistryRepository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRepository {
    pub id: RegistryRepositoryId,
    /// Full registry path, e.g. `group/project/image`.
    pub path: String,
    pub tags: Vec<RegistryTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryTag {
    pub name: String,
    /// Image size in bytes when the instance reports it.
    pub size_bytes: Option<i64>,
    pub pushed_at: Option<DateTime<Utc>>,
}

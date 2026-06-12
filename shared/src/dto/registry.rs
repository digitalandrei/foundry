//! `GET /api/registry/{project_id}` (docs/API.md) — container registry
//! browse: repositories and tags for one project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{RegistryRepositoryId, RegistryTagId};

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
    /// Mirror-row id — what a deployment references (drag payload).
    pub id: RegistryTagId,
    pub name: String,
    /// Image size in bytes when the instance reports it.
    pub size_bytes: Option<i64>,
    pub pushed_at: Option<DateTime<Utc>>,
}

/// `GET /api/registry/tags/{tag_id}/exposed-ports` — the image's
/// EXPOSE'd ports read from its config blob (deploy-dialog prefill).
/// Best-effort: discovery failures return an empty list, never an
/// error — EXPOSE is metadata, not a contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedPortsResponse {
    pub ports: Vec<ExposedPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedPort {
    pub container_port: u16,
    /// `tcp` / `udp`.
    pub protocol: String,
}

//! `GET /api/registry/{project_id}` (docs/API.md) — container registry
//! browse: repositories and tags for one project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::deployment::VolumeSpec;
use crate::{GitlabProjectId, RegistryRepositoryId, RegistryTagId};

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

/// `GET /api/registry/tags/{tag_id}/metadata` — deploy defaults read
/// from the selected linux/amd64 image manifest and config blob.
/// Best-effort: discovery failures return empty defaults, never an
/// error — image metadata is advisory and remains editable in the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageMetadataResponse {
    /// Owning mirror project. Always populated by the route even when
    /// config-blob discovery degrades to empty defaults.
    pub project_id: Option<GitlabProjectId>,
    pub ports: Vec<ExposedPort>,
    /// Persistent mounts declared by Docker `VOLUME` and/or Foundry's
    /// richer `ai.protv.foundry.volumes` image label.
    pub volumes: Vec<VolumeSpec>,
    /// Compressed sum of manifest layer descriptors. Used when GitLab
    /// reports an invalid zero tag size.
    pub size_bytes: Option<i64>,
    /// Immutable manifest digest used for the actual deployment pull.
    pub digest: Option<String>,
    /// Rich web application policy from `ai.protv.foundry.apps`.
    #[serde(default)]
    pub apps: Vec<ApplicationMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMetadata {
    pub container_port: u16,
    #[serde(default = "default_http_kind")]
    pub scheme: crate::PortKind,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub max_body_size_bytes: Option<u64>,
    #[serde(default)]
    pub proxy_timeout_seconds: Option<u32>,
}

fn default_http_kind() -> crate::PortKind {
    crate::PortKind::Http
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedPort {
    pub container_port: u16,
    /// `tcp` / `udp`.
    pub protocol: String,
}

/// One freshly-discovered image tag (`GET /api/registry/updates`) — just
/// enough for the SPA to toast it and refresh the right project tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryNewTag {
    pub id: RegistryTagId,
    pub tag_name: String,
    /// Full registry path, e.g. `group/project/image`.
    pub repo_path: String,
    /// The mirror project id — the SPA invalidates this project's tree.
    pub project_id: GitlabProjectId,
}

/// `GET /api/registry/updates` — image tags first seen during this poll
/// across the user's available repos. Empty when nothing is new (and on
/// the SPA's first/baseline poll, which it suppresses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryUpdates {
    pub new_tags: Vec<RegistryNewTag>,
}

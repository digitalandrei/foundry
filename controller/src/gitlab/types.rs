//! GitLab REST v4 response shapes — only the fields Foundry reads.
//! Unknown fields are ignored by serde, so instance-version drift in
//! unrelated fields cannot break us.

use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GitlabUser {
    pub id: i64,
    pub username: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitlabProject {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitlabRegistryRepository {
    pub id: i64,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitlabRegistryTag {
    pub name: String,
}

/// Per-tag detail (`GET .../tags/{name}`) — carries size + created_at.
#[derive(Debug, Clone, Deserialize)]
pub struct GitlabRegistryTagDetail {
    pub name: String,
    pub total_size: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
}

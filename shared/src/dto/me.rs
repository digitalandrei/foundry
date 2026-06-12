//! `GET /api/me` (docs/API.md).

use serde::{Deserialize, Serialize};

use crate::{GitlabInstanceId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
    pub id: UserId,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub accounts: Vec<GitlabAccountSummary>,
    /// Wildcard apps domain (`ai.protv.ro`) when HTTP/S publishing is
    /// enabled — the deploy dialog offers HTTP/HTTPS port kinds and
    /// previews hostnames. None → kinds stay TCP/UDP.
    pub apps_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabAccountSummary {
    pub instance_id: GitlabInstanceId,
    pub instance_name: String,
    pub username: String,
}

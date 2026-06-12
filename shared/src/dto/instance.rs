//! GitLab instance DTOs (`/api/instances`, docs/GITLAB-INTEGRATION.md).

use serde::{Deserialize, Serialize};

use crate::GitlabInstanceId;

/// Pre-auth shape for the login picker — nothing sensitive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstancePublic {
    pub id: GitlabInstanceId,
    pub name: String,
}

/// Admin view; never includes the client secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceAdmin {
    pub id: GitlabInstanceId,
    pub name: String,
    pub base_url: String,
    pub registry_url: String,
    pub oauth_client_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    pub name: String,
    pub base_url: String,
    pub registry_url: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

/// Edit an onboarded instance. The secret is optional — omitted/empty
/// keeps the stored one (it is never sent back to the client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInstanceRequest {
    pub name: String,
    pub base_url: String,
    pub registry_url: String,
    pub oauth_client_id: String,
    #[serde(default)]
    pub oauth_client_secret: Option<String>,
    pub enabled: bool,
}

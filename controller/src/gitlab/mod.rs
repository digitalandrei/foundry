//! Everything that talks to a GitLab instance
//! (docs/GITLAB-INTEGRATION.md; skill: gitlab-api-oauth-registry).
//! All URLs derive from the per-instance `base_url` — nothing is
//! hardcoded.

pub mod access;
pub mod client;
pub mod oauth;
pub mod registry;
pub mod tokens;
pub mod types;

use foundry_shared::GitlabInstanceId;

/// A decrypted, ready-to-use instance row (client secret in memory
/// only for the duration of the request).
#[derive(Clone)]
pub struct InstanceConfig {
    pub id: GitlabInstanceId,
    pub name: String,
    pub base_url: String,
    pub registry_url: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

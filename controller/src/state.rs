//! Shared application state injected into every handler.

use std::sync::Arc;

use sqlx::MySqlPool;

use crate::crypto::SecretBox;

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub secrets: SecretBox,
    /// Outbound client for GitLab API calls (rustls, timeouts set).
    pub http: reqwest::Client,
    pub public_url: Arc<str>,
    pub admin_emails: Arc<[String]>,
}

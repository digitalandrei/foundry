//! Shared application state injected into every handler.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    /// `ai.protv.ro` — None disables HTTP/S publishing.
    pub apps_domain: Option<Arc<str>>,
    /// Live DEPLOY progress text by deployment id (agent posts every
    /// ~2s while pulling). Deliberately in-memory: it is transient by
    /// definition — the durable truth is the state machine; a restart
    /// just blanks the text until the next report. Lock is never held
    /// across an await (docs/RUST_RULES.md).
    pub progress: Arc<Mutex<HashMap<uuid::Uuid, String>>>,
}

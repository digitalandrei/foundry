//! `/api/servers` DTOs (docs/API.md).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ServerId, ServerStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSummary {
    pub id: ServerId,
    pub name: String,
    pub hostname: Option<String>,
    pub status: ServerStatus,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub agent_version: Option<String>,
    pub os_version: Option<String>,
    /// Whether an agent has ever enrolled for this server.
    pub enrolled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
}

/// Returned exactly once, at server creation / token regeneration —
/// the raw token is never retrievable again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentTokenResponse {
    pub server: ServerSummary,
    pub token: String,
    /// Ready-to-paste registration command for the GPU server.
    pub command: String,
    pub expires_at: DateTime<Utc>,
}

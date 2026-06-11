//! Agent↔controller protocol DTOs (`/agent/*`, docs/API.md).

use serde::{Deserialize, Serialize};

use crate::ServerId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEnrollRequest {
    /// Single-use enrollment token (burned on success).
    pub token: String,
    pub hostname: String,
    pub agent_version: String,
    pub os_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEnrollResponse {
    /// Permanent identity, presented on every request
    /// (`X-Foundry-Agent-Id` + `Authorization: Bearer <secret>`).
    pub agent_id: String,
    pub agent_secret: String,
    pub server_id: ServerId,
    pub server_name: String,
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub agent_version: String,
}

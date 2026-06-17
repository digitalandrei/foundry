//! Agent↔controller protocol DTOs (`/agent/*`, docs/API.md).

use serde::{Deserialize, Serialize};

use crate::{DeploymentId, ServerId};

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

/// Heartbeat reply. Carries the set of adopted (externally-created)
/// containers the controller currently tracks for this server, so the
/// agent's log collector can ship their logs too (they have no
/// `foundry.managed` label to key on).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    #[serde(default)]
    pub adopted_containers: Vec<AdoptedContainerRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptedContainerRef {
    /// Short (12-char) docker id, as reported in inventory.
    pub container_id: String,
    pub deployment_id: DeploymentId,
}

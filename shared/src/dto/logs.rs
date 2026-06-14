//! Container log capture (Phase 7, docs/API.md § Logs). The agent ships
//! *incremental* stdout+stderr for each managed running container; the
//! controller keeps a bounded, 7-day window per deployment and serves it
//! to the UI. Foreign (non-Foundry) containers are never captured — only
//! containers Foundry deployed (label `foundry.managed=true`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::DeploymentId;

/// One new-output chunk for a single managed container
/// (`POST /agent/logs` carries a batch — one per container that produced
/// output since the agent's last upload). Merged stdout+stderr, in
/// docker `--timestamps` form, chronological — exactly `docker logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentLogChunk {
    pub deployment_id: DeploymentId,
    /// Short docker id (12 chars) the lines came from — lets the UI note
    /// that the container was recreated.
    pub container_id: String,
    /// Newest docker log timestamp in this chunk — the agent's dedup
    /// cursor and the controller's retention clock ("7 days of logs").
    pub through: DateTime<Utc>,
    /// Merged stdout+stderr lines, oldest→newest (bounded per upload).
    pub content: String,
}

/// `GET /api/deployments/{id}/logs` — the bounded recent window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentLogsView {
    /// Merged stdout+stderr (oldest→newest), capped to the response
    /// budget; empty string when nothing has been captured yet.
    pub content: String,
    /// Timestamp of the newest captured line (None → no logs yet).
    pub collected_at: Option<DateTime<Utc>>,
    /// True once at least one chunk has been stored — distinguishes "no
    /// logs yet" from "container never logged".
    pub available: bool,
}

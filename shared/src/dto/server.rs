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
    /// HTTP/S app-publishing readiness from the latest snapshot:
    /// `Some(true)` → ready, `Some(false)` → not ready (see
    /// `nginx_status` for why), `None` → unknown / no recent snapshot.
    pub app_publishing_ready: Option<bool>,
    /// Granular nginx/publishing status for display (`READY` /
    /// `NGINX_MISSING` / `NGINX_OUTDATED` / `NGINX_INACTIVE` /
    /// `NOT_CONFIGURED` / `TLS_MISSING`); `None` when not reported
    /// (pre-0.16 agent or no snapshot).
    pub nginx_status: Option<String>,
    /// Docker daemon liveness from the latest snapshot: `Some(true)` →
    /// active, `Some(false)` → daemon down (deploys blocked), `None` →
    /// unknown / no snapshot yet (pre-0.20 agent included).
    pub docker_ok: Option<bool>,
    /// Whether an agent has ever enrolled for this server.
    pub enrolled: bool,
    /// GPUs with their slots (from the latest inventory snapshot) —
    /// the dashboard slot grid feeds from this.
    pub gpus: Vec<super::GpuSummary>,
    /// `running` containers in the latest snapshot (System Status card).
    pub containers_running: i64,
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

/// Mint a reusable fleet enrollment key (admin only). `max_uses = None`
/// means unlimited within the TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFleetTokenRequest {
    pub ttl_hours: i64,
    pub max_uses: Option<u32>,
}

/// Returned exactly once, at fleet-key creation — not bound to any server;
/// agents auto-enroll under their own hostname.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetTokenResponse {
    pub token: String,
    /// Ready-to-paste registration command for a fleet host.
    pub command: String,
    pub expires_at: DateTime<Utc>,
    pub max_uses: Option<u32>,
}

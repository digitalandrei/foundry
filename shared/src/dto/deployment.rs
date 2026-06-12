//! Deployment DTOs (`/api/deployments`, docs/API.md;
//! plans/phase-06.md § Networking).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{DeploymentState, PortKind, RegistryTagId, ServerId, SlotId};

/// One published port — a container may expose any number; each gets
/// its own kind and host allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
    pub container_port: u16,
    pub kind: PortKind,
    /// TCP/UDP only: pin a specific host port (must be free and inside
    /// the server pool); omitted → allocated automatically.
    pub host_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSpec {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

/// Persistent storage mount: create-or-reuse the named per-server
/// volume at `/storage/containers/<volume_name>` and bind it at
/// `container_path`. Volumes outlive deployments; deletion is explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub volume_name: String,
    pub container_path: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeploymentRequest {
    pub slot_id: SlotId,
    pub registry_tag_id: RegistryTagId,
    /// Container/deployment name; generated (`image-xxxx`) when empty.
    pub name: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortSpec>,
    #[serde(default)]
    pub env: Vec<EnvSpec>,
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,
}

/// `GET /api/servers/{id}/volumes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerVolume {
    pub id: crate::ServerVolumeId,
    pub name: String,
    pub path: String,
    pub created_by_name: String,
    /// Names of active deployments currently mounting it (empty →
    /// deletable).
    pub attached_to: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentPort {
    pub container_port: u16,
    pub host_port: u16,
    pub protocol: String,
    pub kind: PortKind,
    /// HTTP/HTTPS: the published app hostname (`https://{hostname}`).
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSummary {
    pub id: crate::DeploymentId,
    pub name: String,
    pub image_ref: String,
    pub state: DeploymentState,
    pub error_message: Option<String>,
    pub server_id: ServerId,
    pub server_name: String,
    pub slot_id: SlotId,
    pub slot_name: String,
    pub gpu_label: String,
    pub created_by_name: String,
    pub ports: Vec<DeploymentPort>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
}

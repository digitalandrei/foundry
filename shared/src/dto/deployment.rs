//! Deployment DTOs (`/api/deployments`, docs/API.md;
//! plans/phase-06.md § Networking).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{DeploymentState, GpuGroupId, PortKind, RegistryTagId, ServerId, SlotId};

/// Docker memory-cap slider bounds (MB). Operator request (2026-06-16):
/// min 32 GB, max 256 GB, or unlimited (the default — `None`). When a
/// cap is set the controller clamps it into `[MIN, MAX]`; the agent then
/// applies it as the container's `--memory` limit.
pub const MEM_LIMIT_MIN_MB: u32 = 32 * 1024;
pub const MEM_LIMIT_MAX_MB: u32 = 256 * 1024;

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

/// Where a deployment lands: a single slot (individual deploy, honours
/// the slot's `max_occupants`) or a GPU group (one container across all
/// members, exclusive). Exactly one — the enum makes "both/neither"
/// unrepresentable on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeployTarget {
    Slot { slot_id: SlotId },
    Group { gpu_group_id: GpuGroupId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeploymentRequest {
    pub target: DeployTarget,
    pub registry_tag_id: RegistryTagId,
    /// Container/deployment name; generated (`image-xxxx`) when empty.
    pub name: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortSpec>,
    #[serde(default)]
    pub env: Vec<EnvSpec>,
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,
    /// Docker memory cap in MB (deploy slider). `None` → unlimited (the
    /// default); a value is clamped to `[MEM_LIMIT_MIN_MB,
    /// MEM_LIMIT_MAX_MB]` by the controller.
    #[serde(default)]
    pub mem_limit_mb: Option<u32>,
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
    /// Live progress while a DEPLOY task runs (`pulling: 3/7 layers …`);
    /// cleared when the task reports its result.
    pub status_detail: Option<String>,
    /// Docker container id once the agent created it — joins the
    /// telemetry sample's container metrics.
    pub container_id: Option<String>,
    pub error_message: Option<String>,
    pub server_id: ServerId,
    pub server_name: String,
    /// Denormalised primary (first/only) member slot — kept for the
    /// single-slot detail UI and back-compat.
    pub slot_id: SlotId,
    pub slot_name: String,
    /// Every member slot this deployment occupies (1 for an individual
    /// deploy, N for a group). The grid folds each occupant across all
    /// of these so every member cell shows occupied-by-group.
    pub slot_ids: Vec<SlotId>,
    /// Set for a group deploy (NULL = single-GPU); `group_name` is the
    /// group's display name for the grid strip click-through.
    pub gpu_group_id: Option<GpuGroupId>,
    pub group_name: Option<String>,
    pub gpu_label: String,
    pub created_by_name: String,
    pub ports: Vec<DeploymentPort>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
}

/// `GET /api/deployments/{id}` — the slot/deployment detail dialog:
/// everything the summary has plus mounts and env *names* (values never
/// leave the server; secrets are encrypted at rest — docs/SECURITY.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDetail {
    #[serde(flatten)]
    pub summary: DeploymentSummary,
    pub mounts: Vec<DeploymentMount>,
    pub env: Vec<DeploymentEnvKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentMount {
    /// None when the backing persistent volume was deleted later.
    pub volume_name: Option<String>,
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEnvKey {
    pub key: String,
    pub is_secret: bool,
}

//! Deployment DTOs (`/api/deployments`, docs/API.md;
//! plans/phase-06.md § Networking).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DeploymentState, GitlabProjectId, GpuGroupId, PortKind, RegistryTagId, ServerId,
    ServerVolumeId, SlotId, VolumePlacement, VolumeVisibility,
};

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
    /// Application metadata from `ai.protv.foundry.apps`. All fields are
    /// optional so ordinary EXPOSE-based images stay deployable.
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub max_body_size_bytes: Option<u64>,
    #[serde(default)]
    pub proxy_timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSpec {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

/// Persistent storage mount. `volume_id` explicitly reuses an accessible
/// existing volume; otherwise Foundry creates or reuses the canonical
/// project/scope/placement/name volume. Volumes outlive deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    #[serde(default)]
    pub volume_id: Option<ServerVolumeId>,
    pub volume_name: String,
    pub container_path: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub visibility: VolumeVisibility,
    #[serde(default)]
    pub placement: VolumePlacement,
    /// Remove all contents immediately before this deployment is
    /// recreated (restart or replacement), then recreate the directory.
    #[serde(default)]
    pub purge_on_redeploy: bool,
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
    pub id: ServerVolumeId,
    pub name: String,
    pub path: String,
    pub used_bytes: Option<u64>,
    pub quota_bytes: Option<u64>,
    pub usage_measured_at: Option<DateTime<Utc>>,
    /// None only for an unattached pre-scope migration volume.
    pub project_id: Option<GitlabProjectId>,
    pub project_name: Option<String>,
    pub visibility: VolumeVisibility,
    pub placement: VolumePlacement,
    pub slot_id: Option<SlotId>,
    pub slot_name: Option<String>,
    pub created_by_name: String,
    /// Creator/admin may clean or delete. Project membership grants reuse,
    /// not destructive management.
    pub can_manage: bool,
    /// Names of active deployments currently mounting it (empty →
    /// deletable).
    pub attached_to: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetVolumeQuotaRequest {
    /// `None` removes the soft quota. Values below 1 MiB are rejected.
    pub quota_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentPort {
    pub container_port: u16,
    pub host_port: u16,
    pub protocol: String,
    pub kind: PortKind,
    /// HTTP/HTTPS: the published app hostname (`https://{hostname}`).
    pub hostname: Option<String>,
    pub primary: bool,
    pub health_path: Option<String>,
    pub max_body_size_bytes: u64,
    pub proxy_timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSummary {
    pub id: crate::DeploymentId,
    pub name: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub state: DeploymentState,
    /// Live progress while a DEPLOY task runs (`pulling: 3/7 layers …`);
    /// cleared when the task reports its result.
    pub status_detail: Option<String>,
    /// Docker container id once the agent created it — joins the
    /// telemetry sample's container metrics.
    pub container_id: Option<String>,
    pub error_message: Option<String>,
    pub health_status: Option<String>,
    pub health_detail: Option<String>,
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
    /// True when this deployment wraps an *adopted* (externally-created)
    /// container — Foundry did not create it. The UI badges it and
    /// double-confirms destructive actions (docs/SECURITY.md).
    #[serde(default)]
    pub adopted: bool,
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
    pub volume_id: Option<ServerVolumeId>,
    pub volume_name: Option<String>,
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
    pub visibility: Option<VolumeVisibility>,
    pub placement: Option<VolumePlacement>,
    pub purge_on_redeploy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEnvKey {
    pub key: String,
    pub is_secret: bool,
}

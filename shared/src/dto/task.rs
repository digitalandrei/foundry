//! Agent task protocol (`/agent/tasks/next`, `/agent/tasks/result` —
//! docs/API.md § Agent API). Execution must be idempotent: re-delivery
//! after an agent crash is normal (docs/ARCHITECTURE.md § Agent Tasks).

use serde::{Deserialize, Serialize};

use crate::{DeploymentId, TaskId, TaskType};

/// What `/agent/tasks/next` hands the agent (204 when queue is empty).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEnvelope {
    pub id: TaskId,
    pub task_type: TaskType,
    pub payload: TaskPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPayload {
    Deploy(Box<DeployPayload>),
    /// STOP / RESTART / REMOVE all target one managed container.
    Container(ContainerTarget),
    /// REMOVE_VOLUME: delete a persistent volume directory. The agent
    /// hard-validates the prefix (`/storage/containers/`).
    Volume(VolumeTarget),
    /// REFRESH_INVENTORY / UPLOAD_LOGS need no payload yet.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeTarget {
    pub volume_id: crate::ServerVolumeId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployPayload {
    pub deployment_id: DeploymentId,
    /// Full pullable reference, e.g. `g.protv.ro:5050/grp/proj:tag`.
    pub image_ref: String,
    pub container_name: String,
    /// NVML UUID for Docker DeviceRequests (GPU-… or MIG-…).
    pub gpu_device_uuid: String,
    /// For the `foundry.slot_id` container label
    /// (docs/ARCHITECTURE.md § Container Labels).
    pub slot_id: crate::SlotId,
    /// Display slot name (`0`, `0:3`) — the `foundry.slot` hint label.
    pub slot_name: String,
    pub ports: Vec<PortBinding>,
    pub env: Vec<(String, String)>,
    /// Bind mounts; the agent creates missing host dirs first
    /// (all under /storage/containers/).
    pub volumes: Vec<VolumeBinding>,
    /// Short-lived registry credential; in-memory only on the agent,
    /// never logged (docs/GITLAB-INTEGRATION.md § Image Pulls). None →
    /// anonymous pull.
    pub registry_auth: Option<RegistryAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBinding {
    pub container_port: u16,
    pub host_port: u16,
    /// `tcp` / `udp`.
    pub protocol: String,
    /// Defaulted (TCP) so DEPLOY payloads queued by a pre-0.8 controller
    /// survive an upgrade instead of poisoning the dispatch loop.
    #[serde(default)]
    pub kind: crate::PortKind,
    /// HTTP/HTTPS only: the vhost the agent publishes
    /// (`<name>.ai.protv.ro`); the wildcard cert lives at
    /// /etc/foundry-agent/tls/ on the server (operator-managed).
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeBinding {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegistryAuth {
    /// Pre-minted registry JWT (preferred — single-repo, pull-only,
    /// minutes-lived).
    RegistryToken { token: String },
    /// Fallback: username + token pair; the Docker daemon performs the
    /// /jwt/auth dance itself.
    UserPassword { username: String, password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerTarget {
    pub deployment_id: DeploymentId,
}

/// `POST /agent/tasks/result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultReport {
    pub task_id: TaskId,
    pub success: bool,
    /// Docker container id on successful DEPLOY.
    pub container_id: Option<String>,
    /// Operator-readable failure summary (no secrets).
    pub error: Option<String>,
}

/// `POST /agent/tasks/progress` — best-effort live status while a
/// DEPLOY executes (docs/API.md § Agent API). `state` is one of
/// PULLING_IMAGE / CREATING_CONTAINER / STARTING; the controller
/// advances the deployment state machine and stores `detail` as the
/// transient `status_detail` shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressReport {
    pub task_id: TaskId,
    pub state: crate::DeploymentState,
    /// e.g. `pulling: 3/7 layers · 410 MB / 1.2 GB` (no secrets).
    pub detail: Option<String>,
}

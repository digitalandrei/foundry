//! `POST /agent/inventory` — full snapshot, agent → controller
//! (docs/GPU-MIG.md; the controller reconciles by UUID, never index).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySnapshot {
    pub agent_version: String,
    pub docker_version: Option<String>,
    pub nvidia_driver_version: Option<String>,
    /// HTTP/S app-publishing readiness: `Some(true)` only when nginx
    /// (≥ the version the vhost template needs), the service, the
    /// Foundry include AND the wildcard TLS cert are all in place.
    #[serde(default)]
    pub app_publishing: Option<bool>,
    /// Granular reason for the UI (`READY` / `NGINX_MISSING` /
    /// `NGINX_OUTDATED` / `NGINX_INACTIVE` / `NOT_CONFIGURED` /
    /// `TLS_MISSING`); `None` from pre-0.16 agents.
    #[serde(default)]
    pub nginx_status: Option<String>,
    pub gpus: Vec<GpuInfo>,
    /// Every container on the host (docker ps -a); `managed` marks
    /// Foundry-created ones. Visibility only — unmanaged containers
    /// are never touched.
    pub containers: Vec<ContainerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// NVML GPU UUID (`GPU-…`) — identity.
    pub uuid: String,
    /// Display ordinal at snapshot time (presentation only).
    pub index: u32,
    pub model: String,
    pub memory_mb: u32,
    pub mig_enabled: bool,
    /// Present only when MIG is enabled.
    pub mig_devices: Vec<MigDeviceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigDeviceInfo {
    /// NVML MIG device UUID (`MIG-…`) — identity.
    pub uuid: String,
    /// e.g. `1g.10gb` (derived from the MIG device name).
    pub profile: String,
    pub memory_mb: u32,
    /// GPU-instance ordinal for the `g:i` display name.
    pub instance_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub container_id: String,
    pub name: String,
    pub image: String,
    /// Docker state (`running`, `exited`, …).
    pub state: String,
    /// Human status line (`Up 3 hours`, …).
    pub status: String,
    pub managed: bool,
    /// Exposed/mapped ports (a container may expose any number).
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    /// GPU/MIG device UUIDs this container is bound to — resolved by the
    /// agent from the container's device requests / NVIDIA_VISIBLE_DEVICES
    /// (indices mapped to UUIDs via NVML). Lets the dashboard map even
    /// non-Foundry containers onto the slot whose GPU they occupy.
    #[serde(default)]
    pub gpu_uuids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub container_port: u16,
    /// Present when published on the host.
    pub host_port: Option<u16>,
    /// `tcp` / `udp`.
    pub protocol: String,
}

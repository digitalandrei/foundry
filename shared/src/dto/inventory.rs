//! `POST /agent/inventory` — full snapshot, agent → controller
//! (docs/GPU-MIG.md; the controller reconciles by UUID, never index).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySnapshot {
    pub agent_version: String,
    pub docker_version: Option<String>,
    pub nvidia_driver_version: Option<String>,
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
}

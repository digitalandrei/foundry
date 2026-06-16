//! `GET /api/servers` GPU/slot shape and `GET /api/servers/{id}`
//! detail (docs/API.md).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::GpuGroupRef;
use crate::{GpuId, SlotId, SlotState, SlotType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSummary {
    pub id: GpuId,
    pub gpu_uuid: String,
    /// NVML index from the latest snapshot — lists are ordered by this
    /// and labels use it (identity stays the UUID).
    pub index: u32,
    pub model: Option<String>,
    pub memory_mb: Option<u32>,
    pub mig_enabled: bool,
    pub slots: Vec<SlotSummary>,
    /// Groups this GPU belongs to (may be several — overlap is allowed).
    /// Surfaced on the cell as `grp A, B` chips so overlap is visible.
    #[serde(default)]
    pub groups: Vec<GpuGroupRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSummary {
    pub id: SlotId,
    pub name: String,
    pub slot_type: SlotType,
    pub mig_profile: Option<String>,
    pub capacity_mb: Option<u32>,
    pub state: SlotState,
    /// Concurrency cap (multi-use). 1 = single-use; >1 = soft sharing,
    /// no VRAM isolation. The grid shows occupancy as `k / max_occupants`
    /// where k is the live count of active deployments on the slot.
    #[serde(default = "one")]
    pub max_occupants: u32,
    /// A non-Foundry container occupying this slot's GPU/MIG device
    /// (mapped from inventory). Present → the GPU is in external use;
    /// the dashboard surfaces it and the slot is not a deploy target.
    #[serde(default)]
    pub external: Option<ExternalOccupant>,
}

/// Serde default for `SlotSummary.max_occupants` (single-use).
fn one() -> u32 {
    1
}

/// A container Foundry did not create, mapped to a slot's device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalOccupant {
    pub name: String,
    pub image: String,
    /// True when the container is actually running (using the GPU);
    /// false when stopped/exited (the device is free, but the container
    /// is still surfaced so the operator sees it).
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerContainer {
    pub container_id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub managed: bool,
    #[serde(default)]
    pub ports: Vec<super::PortMapping>,
    pub reported_at: DateTime<Utc>,
}

/// `GET /api/servers/{id}` — everything the Servers page detail shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerDetail {
    pub server: super::ServerSummary,
    pub docker_version: Option<String>,
    pub nvidia_driver_version: Option<String>,
    pub gpus: Vec<GpuSummary>,
    pub containers: Vec<ServerContainer>,
}

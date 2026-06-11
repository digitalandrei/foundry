//! `GET /api/servers` GPU/slot shape and `GET /api/servers/{id}`
//! detail (docs/API.md).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSummary {
    pub id: SlotId,
    pub name: String,
    pub slot_type: SlotType,
    pub mig_profile: Option<String>,
    pub capacity_mb: Option<u32>,
    pub state: SlotState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerContainer {
    pub container_id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub managed: bool,
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

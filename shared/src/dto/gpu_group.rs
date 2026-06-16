//! GPU-group DTOs (`/api/servers/{id}/gpu-groups`, docs/API.md).
//!
//! A group is a named template over whole GPUs on one server; deploying
//! to it runs one container across all members (overlay membership —
//! members stay individually deployable when no group job runs). See
//! docs/ARCHITECTURE.md § GPU groups.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{GpuGroupId, GpuId, ServerId};

/// Cap on `gpu_slots.max_occupants` (multi-use sharing). Operator
/// decision (2026-06-16): min 1 (single-use), max 4 — a typo can't
/// oversubscribe a card into uselessness. Mirrored by a DB CHECK.
pub const MAX_OCCUPANTS_MIN: u32 = 1;
pub const MAX_OCCUPANTS_MAX: u32 = 4;

/// Full group record (`GET /api/servers/{id}/gpu-groups` rows; also the
/// shape the editor lists).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuGroup {
    pub id: GpuGroupId,
    pub server_id: ServerId,
    pub name: String,
    /// Member GPU ids, in NVML-index order.
    pub gpu_ids: Vec<GpuId>,
    /// Combined VRAM across members (MB) — what the deploy summary shows.
    pub combined_vram_mb: u32,
    /// Group use-mode: 1 = single-use (one exclusive container across the
    /// GPUs); `>1` = multi-use (shared by up to N containers, soft sharing
    /// with no VRAM isolation). Capped 1–4.
    pub max_occupants: u32,
    /// Active deployments on this group right now (`k` in `k / max`).
    pub occupants: u32,
    /// Deployable iff the group is below its occupant cap and every member
    /// GPU is online, MIG-disabled, and free of non-group holders. When
    /// false, `busy_reason` names the blocker (e.g. the holding group).
    pub deployable: bool,
    pub busy_reason: Option<String>,
    pub created_by_name: String,
    pub created_at: DateTime<Utc>,
}

/// A GPU's membership in a group — surfaced on every GPU cell so overlap
/// (a GPU in multiple groups) is visible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuGroupRef {
    pub id: GpuGroupId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGpuGroupRequest {
    pub name: String,
    /// 2…all eligible GPUs on the server, individually picked. Members
    /// must be FULL-slot, MIG-disabled, and on this server; they may
    /// overlap other groups.
    pub gpu_ids: Vec<GpuId>,
}

/// `PATCH /api/slots/{id}` — admin sets a slot's concurrency cap
/// (1 = single-use, >1 = multi-use soft sharing, no VRAM isolation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSlotUseModeRequest {
    pub max_occupants: u32,
}

/// `PATCH /api/gpu-groups/{id}` — admin sets a group's concurrency cap
/// (1 = single-use exclusive, >1 = multi-use soft sharing). Same bounds
/// as a slot's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetGroupUseModeRequest {
    pub max_occupants: u32,
}

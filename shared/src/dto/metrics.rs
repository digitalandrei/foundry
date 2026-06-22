//! Telemetry sample, agent → controller (`POST /agent/metrics`) and
//! back out to the UI (`GET /api/servers/{id}/metrics`). Stored as an
//! opaque JSON payload in `server_metrics` — the sample shape is the
//! wire contract; the DB does not decompose it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSample {
    pub host: HostMetrics,
    pub gpus: Vec<GpuMetrics>,
    /// Per-MIG-instance memory (only on hosts with MIG enabled). NVML
    /// can't attribute utilization per slice, so this carries memory
    /// only; full-GPU util/power/temp stay on `gpus`. `#[serde(default)]`
    /// keeps older stored samples and pre-MIG agents deserializing.
    #[serde(default)]
    pub migs: Vec<MigMetrics>,
    pub containers: Vec<ContainerMetrics>,
}

/// `GET /api/metrics/latest` — the newest sample per server, for live
/// labels on the dashboard slot grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestMetricsResponse {
    pub servers: Vec<LatestServerMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestServerMetrics {
    pub server_id: crate::ServerId,
    pub sampled_at: DateTime<Utc>,
    pub sample: MetricsSample,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMetrics {
    pub cpu_pct: f32,
    /// 1-minute load average and logical core count — the "load / cores"
    /// readout (load == cores means fully saturated).
    pub load_avg_1m: f32,
    pub cpu_cores: u32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    /// Root filesystem.
    pub disk_used_gb: f32,
    pub disk_total_gb: f32,
    /// Bytes/sec across non-loopback interfaces since the last sample.
    pub net_rx_bps: u64,
    pub net_tx_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    /// NVML GPU UUID — joins to the inventory GPU.
    pub uuid: String,
    pub util_pct: u32,
    pub mem_used_mb: u64,
    pub temperature_c: u32,
    pub power_w: f32,
}

/// Per-MIG-instance memory. Memory only by design: NVML does not report
/// utilization for MIG slices (it reads as N/A), so per-slice util is
/// intentionally absent — the full-GPU `util_pct` on the parent covers it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigMetrics {
    /// NVML MIG device UUID (`MIG-…`) — joins to the inventory MIG slot
    /// (`gpu_slots.mig_uuid`, surfaced as `SlotSummary.mig_uuid`).
    pub uuid: String,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    /// Short id — joins to the server_containers snapshot.
    pub container_id: String,
    pub cpu_pct: f32,
    /// Logical cores visible to the container (Docker `online_cpus`) —
    /// the denominator for "load / cores" (load == cpu_pct / 100).
    pub cpu_cores: u32,
    pub mem_used_mb: u64,
    pub mem_limit_mb: u64,
}

/// One stored point of the series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsPoint {
    pub sampled_at: DateTime<Utc>,
    pub sample: MetricsSample,
}

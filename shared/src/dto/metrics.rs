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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    /// Short id — joins to the server_containers snapshot.
    pub container_id: String,
    pub cpu_pct: f32,
    pub mem_used_mb: u64,
    pub mem_limit_mb: u64,
}

/// One stored point of the series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsPoint {
    pub sampled_at: DateTime<Utc>,
    pub sample: MetricsSample,
}

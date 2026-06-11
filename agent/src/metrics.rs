//! Telemetry collection (plans/phase-05.md § Telemetry extension):
//! host CPU/mem/disk/network via sysinfo, GPU utilization via NVML,
//! per-container CPU/mem via the Docker stats endpoint. Stateful —
//! CPU and network rates need deltas between samples.

use std::time::Instant;

use bollard::query_parameters::{ListContainersOptions, StatsOptions};
use bollard::Docker;
use foundry_shared::dto::{ContainerMetrics, GpuMetrics, HostMetrics, MetricsSample};
use futures_util::StreamExt;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use sysinfo::{Disks, Networks, System};

pub struct MetricsCollector {
    sys: System,
    networks: Networks,
    nvml: Option<Nvml>,
    last_net: Option<(u64, u64, Instant)>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let nvml = match Nvml::init() {
            Ok(n) => Some(n),
            Err(err) => {
                tracing::info!(%err, "NVML unavailable — GPU metrics disabled");
                None
            }
        };
        Self {
            sys: System::new(),
            networks: Networks::new_with_refreshed_list(),
            nvml,
            last_net: None,
        }
    }

    pub async fn collect(&mut self) -> MetricsSample {
        MetricsSample {
            host: self.collect_host(),
            gpus: self.collect_gpus(),
            containers: collect_containers().await,
        }
    }

    fn collect_host(&mut self) -> HostMetrics {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        let disks = Disks::new_with_refreshed_list();
        let (disk_used_gb, disk_total_gb) = disks
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/"))
            .map(|d| {
                let total = d.total_space() as f32 / 1e9;
                (total - d.available_space() as f32 / 1e9, total)
            })
            .unwrap_or((0.0, 0.0));

        self.networks.refresh(true);
        let (rx_total, tx_total) = self
            .networks
            .iter()
            .filter(|(name, _)| *name != "lo")
            .fold((0u64, 0u64), |(rx, tx), (_, data)| {
                (rx + data.total_received(), tx + data.total_transmitted())
            });
        let now = Instant::now();
        let (net_rx_bps, net_tx_bps) = match self.last_net {
            Some((prev_rx, prev_tx, at)) => {
                let secs = now.duration_since(at).as_secs_f64().max(0.001);
                (
                    (rx_total.saturating_sub(prev_rx) as f64 / secs) as u64,
                    (tx_total.saturating_sub(prev_tx) as f64 / secs) as u64,
                )
            }
            None => (0, 0),
        };
        self.last_net = Some((rx_total, tx_total, now));

        HostMetrics {
            cpu_pct: self.sys.global_cpu_usage(),
            mem_used_mb: self.sys.used_memory() / 1024 / 1024,
            mem_total_mb: self.sys.total_memory() / 1024 / 1024,
            disk_used_gb,
            disk_total_gb,
            net_rx_bps,
            net_tx_bps,
        }
    }

    fn collect_gpus(&self) -> Vec<GpuMetrics> {
        let Some(nvml) = &self.nvml else {
            return Vec::new();
        };
        let count = nvml.device_count().unwrap_or(0);
        let mut out = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };
            let Ok(uuid) = device.uuid() else { continue };
            out.push(GpuMetrics {
                uuid,
                util_pct: device.utilization_rates().map(|u| u.gpu).unwrap_or(0),
                mem_used_mb: device
                    .memory_info()
                    .map(|m| m.used / 1024 / 1024)
                    .unwrap_or(0),
                temperature_c: device.temperature(TemperatureSensor::Gpu).unwrap_or(0),
                power_w: device
                    .power_usage()
                    .map(|mw| mw as f32 / 1000.0)
                    .unwrap_or(0.0),
            });
        }
        out
    }
}

/// One stats sample per running container. `stream=false, one_shot=
/// false` makes the daemon include precpu so the standard CPU%% delta
/// formula works from a single response (costs ~1s daemon-side; samples
/// run concurrently).
async fn collect_containers() -> Vec<ContainerMetrics> {
    let Ok(docker) = Docker::connect_with_local_defaults() else {
        return Vec::new();
    };
    let Ok(list) = docker
        .list_containers(Some(ListContainersOptions::default()))
        .await
    else {
        return Vec::new();
    };

    let futures = list.into_iter().filter_map(|c| {
        let id = c.id?;
        let docker = docker.clone();
        Some(async move {
            let stats = docker
                .stats(
                    &id,
                    Some(StatsOptions {
                        stream: false,
                        one_shot: false,
                    }),
                )
                .next()
                .await?
                .ok()?;
            let cpu = stats.cpu_stats.as_ref()?;
            let precpu = stats.precpu_stats.as_ref()?;
            let cpu_delta = cpu
                .cpu_usage
                .as_ref()?
                .total_usage?
                .saturating_sub(precpu.cpu_usage.as_ref().and_then(|u| u.total_usage)?);
            let sys_delta = cpu
                .system_cpu_usage?
                .saturating_sub(precpu.system_cpu_usage.unwrap_or(0));
            let ncpus = cpu.online_cpus.unwrap_or(1).max(1) as f32;
            let cpu_pct = if sys_delta > 0 {
                cpu_delta as f32 / sys_delta as f32 * ncpus * 100.0
            } else {
                0.0
            };
            let mem = stats.memory_stats.as_ref();
            Some(ContainerMetrics {
                container_id: id.chars().take(12).collect(),
                cpu_pct,
                mem_used_mb: mem.and_then(|m| m.usage).unwrap_or(0) / 1024 / 1024,
                mem_limit_mb: mem.and_then(|m| m.limit).unwrap_or(0) / 1024 / 1024,
            })
        })
    });

    futures_util::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}

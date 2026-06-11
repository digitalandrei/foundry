//! Inventory collection: NVML GPUs/MIG + Docker containers
//! (docs/GPU-MIG.md; skill: nvidia-gpu-mig). Full snapshots — no diff
//! state kept here. Both sources are optional at runtime: a host
//! without GPUs or without Docker still reports what it has (the
//! controller treats absence as an empty section, and we log the
//! reason once at startup level).

use std::collections::HashMap;

use bollard::query_parameters::ListContainersOptions;
use bollard::Docker;
use foundry_shared::dto::{ContainerInfo, GpuInfo, InventorySnapshot, MigDeviceInfo};
use nvml_wrapper::Nvml;

pub async fn collect() -> InventorySnapshot {
    let (docker_version, containers) = collect_docker().await;
    let (nvidia_driver_version, gpus) = collect_gpus();
    InventorySnapshot {
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        docker_version,
        nvidia_driver_version,
        gpus,
        containers,
    }
}

async fn collect_docker() -> (Option<String>, Vec<ContainerInfo>) {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(err) => {
            tracing::debug!(%err, "docker unavailable");
            return (None, Vec::new());
        }
    };
    let version = match docker.version().await {
        Ok(v) => v.version,
        Err(err) => {
            tracing::warn!(%err, "docker socket present but not responding");
            return (None, Vec::new());
        }
    };

    let containers = match docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await
    {
        Ok(list) => list
            .into_iter()
            .map(|c| {
                let managed = c
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("foundry.managed"))
                    .is_some_and(|v| v == "true");
                ContainerInfo {
                    container_id: c.id.unwrap_or_default().chars().take(12).collect(),
                    name: c
                        .names
                        .unwrap_or_default()
                        .first()
                        .map(|n| n.trim_start_matches('/').to_string())
                        .unwrap_or_default(),
                    image: c.image.unwrap_or_default(),
                    state: c
                        .state
                        .map(|s| format!("{s:?}").to_lowercase())
                        .unwrap_or_default(),
                    status: c.status.unwrap_or_default(),
                    managed,
                }
            })
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "container listing failed");
            Vec::new()
        }
    };
    (version, containers)
}

fn collect_gpus() -> (Option<String>, Vec<GpuInfo>) {
    let nvml = match Nvml::init() {
        Ok(n) => n,
        Err(err) => {
            tracing::debug!(%err, "NVML unavailable (no NVIDIA driver?)");
            return (None, Vec::new());
        }
    };
    let driver = nvml.sys_driver_version().ok();

    let count = match nvml.device_count() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(%err, "NVML device enumeration failed");
            return (driver, Vec::new());
        }
    };

    let mut gpus = Vec::with_capacity(count as usize);
    let mut any_mig = false;
    for index in 0..count {
        let device = match nvml.device_by_index(index) {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!(index, %err, "GPU handle failed");
                continue;
            }
        };
        let uuid = match device.uuid() {
            Ok(u) => u,
            Err(err) => {
                tracing::warn!(index, %err, "GPU UUID read failed — skipping device");
                continue;
            }
        };
        let model = device.name().unwrap_or_else(|_| "unknown".into());
        let memory_mb = device
            .memory_info()
            .map(|m| (m.total / 1024 / 1024) as u32)
            .unwrap_or(0);
        // NVML_DEVICE_MIG_ENABLE == 1
        let mig_enabled = device.mig_mode().map(|m| m.current == 1).unwrap_or(false);
        any_mig |= mig_enabled;

        gpus.push(GpuInfo {
            uuid,
            index,
            model,
            memory_mb,
            mig_enabled,
            mig_devices: Vec::new(),
        });
    }

    // MIG *device* enumeration: nvml-wrapper 0.11 does not expose the
    // MIG device handles, so the layout comes from `nvidia-smi -L`
    // (docs/GPU-MIG.md records this deviation; NVML stays authoritative
    // for GPUs and MIG mode).
    if any_mig {
        match std::process::Command::new("nvidia-smi").arg("-L").output() {
            Ok(out) if out.status.success() => {
                let by_gpu = parse_smi_list(&String::from_utf8_lossy(&out.stdout));
                for gpu in &mut gpus {
                    if let Some(devices) = by_gpu.get(&gpu.uuid) {
                        gpu.mig_devices = devices.clone();
                    }
                }
            }
            Ok(out) => tracing::warn!(status = %out.status, "nvidia-smi -L failed"),
            Err(err) => tracing::warn!(%err, "nvidia-smi not runnable; MIG layout unknown"),
        }
    }
    (driver, gpus)
}

/// Parse `nvidia-smi -L`:
/// ```text
/// GPU 0: NVIDIA A100-SXM4-80GB (UUID: GPU-aaaa…)
///   MIG 1g.10gb     Device  0: (UUID: MIG-bbbb…)
/// ```
fn parse_smi_list(text: &str) -> HashMap<String, Vec<MigDeviceInfo>> {
    let mut out: HashMap<String, Vec<MigDeviceInfo>> = HashMap::new();
    let mut current_gpu: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("GPU ") {
            current_gpu = extract_uuid(trimmed, "GPU-");
        } else if trimmed.starts_with("MIG ") {
            let Some(gpu_uuid) = current_gpu.clone() else {
                continue;
            };
            let Some(mig_uuid) = extract_uuid(trimmed, "MIG-") else {
                continue;
            };
            // "MIG 1g.10gb Device 0: (UUID: …)"
            let mut words = trimmed.split_whitespace();
            let profile = words.nth(1).unwrap_or("unknown").to_string();
            let instance_id = trimmed
                .split("Device")
                .nth(1)
                .and_then(|s| s.trim().trim_end_matches(':').split(':').next())
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or_default();
            out.entry(gpu_uuid).or_default().push(MigDeviceInfo {
                uuid: mig_uuid,
                memory_mb: profile_memory_mb(&profile).unwrap_or(0),
                profile,
                instance_id,
            });
        }
    }
    out
}

/// `(UUID: GPU-xxx)` / `(UUID: MIG-xxx)` → the UUID token.
fn extract_uuid(line: &str, prefix: &str) -> Option<String> {
    let start = line.find(prefix)?;
    let rest = &line[start..];
    let end = rest.find([')', ' ']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// `1g.10gb` → 10240 MB (declared profile size; NVML detail needs the
/// unwrapped MIG handles).
fn profile_memory_mb(profile: &str) -> Option<u32> {
    let gb: u32 = profile
        .split('.')
        .nth(1)?
        .trim_end_matches("gb")
        .parse()
        .ok()?;
    Some(gb * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
GPU 0: NVIDIA A100-SXM4-80GB (UUID: GPU-aaaaaaaa-1111-2222-3333-444444444444)
  MIG 1g.10gb     Device  0: (UUID: MIG-bbbbbbbb-1111-2222-3333-444444444444)
  MIG 2g.20gb     Device  1: (UUID: MIG-cccccccc-1111-2222-3333-444444444444)
GPU 1: NVIDIA A100-SXM4-80GB (UUID: GPU-dddddddd-1111-2222-3333-444444444444)
";

    #[test]
    fn parses_mig_layout() {
        let map = parse_smi_list(SAMPLE);
        let gpu0 = &map["GPU-aaaaaaaa-1111-2222-3333-444444444444"];
        assert_eq!(gpu0.len(), 2);
        assert_eq!(gpu0[0].profile, "1g.10gb");
        assert_eq!(gpu0[0].memory_mb, 10240);
        assert_eq!(gpu0[0].instance_id, 0);
        assert_eq!(gpu0[1].profile, "2g.20gb");
        assert_eq!(gpu0[1].instance_id, 1);
        assert!(!map.contains_key("GPU-dddddddd-1111-2222-3333-444444444444"));
    }

    #[test]
    fn profile_memory() {
        assert_eq!(profile_memory_mb("1g.10gb"), Some(10240));
        assert_eq!(profile_memory_mb("7g.80gb"), Some(81920));
        assert_eq!(profile_memory_mb("weird"), None);
    }
}

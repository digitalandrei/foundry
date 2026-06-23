//! Inventory collection: NVML GPUs/MIG + Docker containers
//! (docs/GPU-MIG.md; skill: nvidia-gpu-mig). Full snapshots — no diff
//! state kept here. Both sources are optional at runtime: a host
//! without GPUs or without Docker still reports what it has (the
//! controller treats absence as an empty section, and we log the
//! reason once at startup level).

use std::collections::HashMap;

use bollard::query_parameters::ListContainersOptions;
use bollard::Docker;
use foundry_shared::dto::{
    ContainerInfo, ContainerMount, GpuInfo, InventorySnapshot, MigDeviceInfo,
};
use nvml_wrapper::Nvml;

pub async fn collect(nvml: Option<&Nvml>) -> InventorySnapshot {
    // GPUs first: their NVML index→UUID map resolves the device refs of
    // running containers (so even non-Foundry containers map to a slot).
    let (nvidia_driver_version, gpus) = collect_gpus(nvml);
    let gpu_index: HashMap<u32, String> = gpus.iter().map(|g| (g.index, g.uuid.clone())).collect();
    let (docker_version, docker_ok, containers) = collect_docker(&gpu_index).await;
    let nginx = crate::vhost::app_publishing_status();
    InventorySnapshot {
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        docker_version,
        docker_ok,
        nvidia_driver_version,
        app_publishing: Some(nginx == "READY"),
        nginx_status: Some(nginx.to_string()),
        gpus,
        containers,
    }
}

/// Returns `(version, daemon_reachable, containers)`. `daemon_reachable`
/// drives the per-server "Docker active" indicator + deploy gate: it is
/// true only when the daemon answered `version()`, independent of
/// whether that response carried a version string.
async fn collect_docker(
    gpu_index: &HashMap<u32, String>,
) -> (Option<String>, bool, Vec<ContainerInfo>) {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(err) => {
            tracing::debug!(%err, "docker unavailable");
            return (None, false, Vec::new());
        }
    };
    let version = match docker.version().await {
        Ok(v) => v.version,
        Err(err) => {
            tracing::warn!(%err, "docker socket present but not responding");
            return (None, false, Vec::new());
        }
    };

    let list = match docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await
    {
        Ok(list) => list,
        Err(err) => {
            tracing::warn!(%err, "container listing failed");
            return (version, true, Vec::new());
        }
    };

    // Inspecting is one API call each. Running containers are always
    // inspected (they hold a GPU); stopped ones are inspected up to a
    // budget so an external container that exited is still mapped to
    // its slot (shown as stopped) without scanning a huge exited pile.
    let mut stopped_budget = 100usize;
    let mut containers = Vec::with_capacity(list.len());
    for c in list {
        let managed = c
            .labels
            .as_ref()
            .and_then(|l| l.get("foundry.managed"))
            .is_some_and(|v| v == "true");
        let mut ports: Vec<foundry_shared::dto::PortMapping> = c
            .ports
            .iter()
            .flatten()
            .map(|p| foundry_shared::dto::PortMapping {
                container_port: p.private_port,
                host_port: p.public_port,
                protocol: p
                    .typ
                    .map(|t| format!("{t:?}").to_lowercase())
                    .unwrap_or_else(|| "tcp".into()),
            })
            .collect();
        // Docker repeats a mapping per host interface (0.0.0.0 and ::) —
        // dedup on (container_port, host_port, proto).
        ports.sort_by_key(|p| (p.container_port, p.host_port, p.protocol.clone()));
        ports.dedup_by_key(|p| (p.container_port, p.host_port, p.protocol.clone()));

        let id_full = c.id.unwrap_or_default();
        let state = c
            .state
            .map(|s| format!("{s:?}").to_lowercase())
            .unwrap_or_default();
        let (gpu_uuids, mounts) = if state == "running" {
            inspect_container_details(&docker, &id_full, gpu_index).await
        } else if stopped_budget > 0 {
            stopped_budget -= 1;
            if stopped_budget == 0 {
                tracing::debug!(
                    "stopped-container inspection budget reached — \
                     further exited containers are reported without GPU/mount detail"
                );
            }
            inspect_container_details(&docker, &id_full, gpu_index).await
        } else {
            (Vec::new(), Vec::new())
        };

        containers.push(ContainerInfo {
            container_id: id_full.chars().take(12).collect(),
            name: c
                .names
                .unwrap_or_default()
                .first()
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default(),
            image: c.image.unwrap_or_default(),
            state,
            status: c.status.unwrap_or_default(),
            managed,
            ports,
            gpu_uuids,
            mounts,
        });
    }
    (version, true, containers)
}

/// Inspect a container once and resolve both its GPU/MIG device UUIDs and
/// its volume mounts.
///
/// GPU UUIDs come from `--gpus` device requests and
/// `NVIDIA_VISIBLE_DEVICES`: numeric indices map to UUIDs via the NVML
/// index map; `GPU-…`/`MIG-…` refs pass through; `all`/`count=-1` expands
/// to every GPU. Mounts come from the resolved `Mounts` list (bind /
/// volume / tmpfs), surfaced for visibility/adoption.
async fn inspect_container_details(
    docker: &Docker,
    id: &str,
    gpu_index: &HashMap<u32, String>,
) -> (Vec<String>, Vec<ContainerMount>) {
    let info = match docker
        .inspect_container(
            id,
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
    {
        Ok(i) => i,
        Err(err) => {
            tracing::debug!(%err, container = id, "container inspect failed");
            return (Vec::new(), Vec::new());
        }
    };

    let mounts = info
        .mounts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|m| ContainerMount {
            source: m.source.clone().unwrap_or_default(),
            destination: m.destination.clone().unwrap_or_default(),
            // Docker reports `RW`; absence is treated as writable.
            read_only: !m.rw.unwrap_or(true),
            mount_type: m
                .typ
                .map(|t| format!("{t:?}").to_lowercase())
                .unwrap_or_else(|| "bind".into()),
        })
        .collect();

    // BTreeSet → deterministic, deduped.
    let mut uuids = std::collections::BTreeSet::new();

    if let Some(reqs) = info.host_config.and_then(|h| h.device_requests) {
        for req in reqs {
            let is_gpu = req.driver.as_deref() == Some("nvidia")
                || req
                    .capabilities
                    .iter()
                    .flatten()
                    .any(|caps| caps.iter().any(|c| c == "gpu"));
            if !is_gpu {
                continue;
            }
            match req.device_ids {
                Some(ids) if !ids.is_empty() => {
                    for r in ids {
                        if let Some(u) = resolve_device_ref(&r, gpu_index) {
                            uuids.insert(u);
                        }
                    }
                }
                // No explicit ids + count -1 → all GPUs.
                _ if req.count == Some(-1) => {
                    uuids.extend(gpu_index.values().cloned());
                }
                _ => {}
            }
        }
    }

    if let Some(env) = info.config.and_then(|c| c.env) {
        for e in env {
            let Some(v) = e.strip_prefix("NVIDIA_VISIBLE_DEVICES=") else {
                continue;
            };
            match v {
                "all" => uuids.extend(gpu_index.values().cloned()),
                "none" | "void" | "" => {}
                list => {
                    for r in list.split(',') {
                        if let Some(u) = resolve_device_ref(r.trim(), gpu_index) {
                            uuids.insert(u);
                        }
                    }
                }
            }
        }
    }

    (uuids.into_iter().collect(), mounts)
}

/// A device reference is a UUID (`GPU-…`/`MIG-…`, used as-is) or an NVML
/// index resolved via the snapshot's index map.
fn resolve_device_ref(r: &str, gpu_index: &HashMap<u32, String>) -> Option<String> {
    if r.starts_with("GPU-") || r.starts_with("MIG-") {
        Some(r.to_string())
    } else {
        r.parse::<u32>()
            .ok()
            .and_then(|i| gpu_index.get(&i).cloned())
    }
}

fn collect_gpus(nvml: Option<&Nvml>) -> (Option<String>, Vec<GpuInfo>) {
    // Uses the process-lifetime NVML handle from main.rs — never
    // re-initialized (cycling nvmlInit/Shutdown leaks FDs). A MIG layout
    // changed after the agent started is therefore only reflected on the
    // next agent restart (docs/GPU-MIG.md).
    let Some(nvml) = nvml else {
        tracing::debug!("NVML unavailable (no NVIDIA driver?)");
        return (None, Vec::new());
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

    // MIG *device* enumeration comes from `nvidia-smi -L`: it gives the
    // human profile string ("1g.10gb") and the stable device ordering we
    // name slots from, which NVML exposes only as raw instance/profile
    // ids. (nvml-wrapper 0.12 does wrap the MIG handles — metrics.rs uses
    // them for per-slice memory — but the layout stays on -L for the
    // labels. docs/GPU-MIG.md records this split; NVML stays authoritative
    // for GPUs and MIG mode.)
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

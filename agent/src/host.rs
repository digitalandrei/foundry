//! Live host readiness and persistent-storage accounting.
//!
//! Readiness is evidence, not a version comparison: every inventory cycle
//! executes the same probes the deployment executor depends on.

use std::path::Path;
use std::process::Command;

use chrono::Utc;
use foundry_shared::dto::{CheckStatus, HostReadiness, ReadinessCheck, StorageUsage, VolumeUsage};
use sysinfo::Disks;

pub type StorageCache = std::sync::Arc<tokio::sync::RwLock<Option<StorageUsage>>>;

pub const SETUP_REVISION: u32 = foundry_shared::dto::REQUIRED_SETUP_REVISION;
pub const SETUP_MARKER: &str = "/etc/foundry-agent/setup-revision";
const STORAGE_ROOT: &str = "/storage/containers";

pub async fn readiness(
    server_name: Option<&str>,
    docker_ok: bool,
    docker_gpu: Result<String, String>,
) -> HostReadiness {
    let mut checks = Vec::with_capacity(7);
    checks.push(check(
        "docker",
        if docker_ok {
            CheckStatus::Ready
        } else {
            CheckStatus::Failed
        },
        if docker_ok {
            "Docker daemon and socket are accessible"
        } else {
            "Docker daemon/socket is not accessible to foundry-agent"
        },
    ));
    checks.push(match docker_gpu {
        Ok(detail) => check("docker_gpu", CheckStatus::Ready, &detail),
        Err(detail) => check("docker_gpu", CheckStatus::Failed, &detail),
    });
    checks.push(storage_write_probe().await);
    checks.push(capability_probe());
    checks.push(nginx_probe().await);
    checks.push(certificate_probe(server_name));
    checks.push(check(
        "setup_revision",
        if setup_revision() == Some(SETUP_REVISION) {
            CheckStatus::Ready
        } else {
            CheckStatus::Failed
        },
        &format!(
            "host setup revision {} (required {SETUP_REVISION})",
            setup_revision().map_or_else(|| "missing".into(), |v| v.to_string())
        ),
    ));
    HostReadiness {
        setup_revision: setup_revision(),
        required_setup_revision: SETUP_REVISION,
        checked_at: Utc::now(),
        checks,
    }
}

fn check(code: &str, status: CheckStatus, detail: &str) -> ReadinessCheck {
    ReadinessCheck {
        code: code.into(),
        status,
        detail: detail.chars().take(600).collect(),
    }
}

pub fn setup_revision() -> Option<u32> {
    std::fs::read_to_string(SETUP_MARKER)
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

async fn storage_write_probe() -> ReadinessCheck {
    let path = format!("{STORAGE_ROOT}/.foundry-readiness-{}", uuid::Uuid::now_v7());
    match tokio::fs::write(&path, b"ready").await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(&path).await;
            check(
                "storage_write",
                CheckStatus::Ready,
                "persistent storage is writable",
            )
        }
        Err(error) => check(
            "storage_write",
            CheckStatus::Failed,
            &format!("{STORAGE_ROOT} is not writable: {error}"),
        ),
    }
}

fn capability_probe() -> ReadinessCheck {
    // CAP_DAC_OVERRIDE is bit 1. It is required to manage files created by
    // arbitrary container UIDs inside approved placement volumes.
    let effective = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("CapEff:")
                    .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
            })
        });
    match effective {
        Some(bits) if bits & (1 << 1) != 0 => check(
            "capabilities",
            CheckStatus::Ready,
            &format!("effective capabilities 0x{bits:x}; CAP_DAC_OVERRIDE present"),
        ),
        Some(bits) => check(
            "capabilities",
            CheckStatus::Failed,
            &format!("effective capabilities 0x{bits:x}; CAP_DAC_OVERRIDE missing"),
        ),
        None => check(
            "capabilities",
            CheckStatus::Unknown,
            "could not read CapEff",
        ),
    }
}

async fn nginx_probe() -> ReadinessCheck {
    let output = tokio::process::Command::new("sudo")
        .args(["-n", "/usr/sbin/nginx", "-t"])
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => check(
            "nginx_config",
            CheckStatus::Ready,
            "sudo -n nginx -t succeeded",
        ),
        Ok(output) => check(
            "nginx_config",
            CheckStatus::Failed,
            &format!(
                "sudo -n nginx -t failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ),
        Err(error) => check(
            "nginx_config",
            CheckStatus::Failed,
            &format!("could not execute sudo nginx: {error}"),
        ),
    }
}

fn certificate_probe(server_name: Option<&str>) -> ReadinessCheck {
    if !Path::new(crate::vhost::TLS_CERT).is_file() || !Path::new(crate::vhost::TLS_KEY).is_file() {
        return check(
            "tls_certificate",
            CheckStatus::Failed,
            "TLS certificate or private key is missing",
        );
    }
    let expiry = Command::new("openssl")
        .args([
            "x509",
            "-in",
            crate::vhost::TLS_CERT,
            "-noout",
            "-checkend",
            "604800",
        ])
        .status();
    if !matches!(expiry, Ok(status) if status.success()) {
        return check(
            "tls_certificate",
            CheckStatus::Failed,
            "certificate expires within 7 days or cannot be parsed",
        );
    }
    let sans = Command::new("openssl")
        .args([
            "x509",
            "-in",
            crate::vhost::TLS_CERT,
            "-noout",
            "-ext",
            "subjectAltName",
        ])
        .output();
    let Ok(sans) = sans else {
        return check(
            "tls_certificate",
            CheckStatus::Unknown,
            "certificate is valid but SANs could not be inspected",
        );
    };
    let names = String::from_utf8_lossy(&sans.stdout);
    let expected = server_name.map(|name| {
        let domain = std::env::var("FOUNDRY_APPS_DOMAIN").unwrap_or_else(|_| "ai.protv.ro".into());
        format!("*.{name}.{domain}")
    });
    match expected {
        Some(expected) if !names.contains(&format!("DNS:{expected}")) => check(
            "tls_certificate",
            CheckStatus::Failed,
            &format!("certificate does not cover expected wildcard {expected}"),
        ),
        Some(expected) => check(
            "tls_certificate",
            CheckStatus::Ready,
            &format!("certificate is valid for at least 7 days and covers {expected}"),
        ),
        None => check(
            "tls_certificate",
            CheckStatus::Warning,
            "certificate is valid for at least 7 days; server name is unknown",
        ),
    }
}

pub async fn storage_usage(
    targets: Vec<foundry_shared::dto::VolumeTarget>,
) -> Option<StorageUsage> {
    tokio::task::spawn_blocking(move || {
        let (total_bytes, available_bytes) = storage_capacity_sync()?;
        let volumes = catalog_volume_usage(targets);
        Some(StorageUsage {
            total_bytes,
            available_bytes,
            volumes,
        })
    })
    .await
    .ok()
    .flatten()
}

fn catalog_volume_usage(targets: Vec<foundry_shared::dto::VolumeTarget>) -> Vec<VolumeUsage> {
    catalog_volume_usage_with(targets, |path| {
        crate::file_system::existing_volume_root(path)
    })
}

fn catalog_volume_usage_with<F>(
    targets: Vec<foundry_shared::dto::VolumeTarget>,
    inspect_root: F,
) -> Vec<VolumeUsage>
where
    F: Fn(&Path) -> Result<Option<std::path::PathBuf>, String>,
{
    targets
        .into_iter()
        .filter_map(|target| match inspect_root(Path::new(&target.path)) {
            Ok(Some(path)) => Some(VolumeUsage {
                volume_id: target.volume_id,
                used_bytes: directory_size(&path, 0),
            }),
            // A volume is created on first deploy/file session; until then,
            // it is accurately empty rather than absent from the catalog.
            Ok(None) => Some(VolumeUsage {
                volume_id: target.volume_id,
                used_bytes: 0,
            }),
            Err(error) => {
                tracing::warn!(volume_id = %target.volume_id, %error,
                    "refusing unsafe volume root from controller catalog");
                None
            }
        })
        .collect()
}

fn cache_after_catalog<T>(current: Option<T>, refreshed: Result<T, ()>) -> Option<T> {
    refreshed.ok().or(current)
}

/// Fetch the authoritative roots assigned to this agent's server. Shared by
/// the periodic accounting worker and on-demand diagnostics so both measure
/// the same catalog rather than rediscovering host directories.
pub async fn volume_catalog(
    client: &reqwest::Client,
    config: &crate::config::AgentConfig,
) -> Result<Vec<foundry_shared::dto::VolumeTarget>, String> {
    let url = format!(
        "{}/agent/volumes",
        config.controller_url.trim_end_matches('/')
    );
    let response = client
        .get(url)
        .header("x-foundry-agent-id", &config.agent_id)
        .bearer_auth(&config.agent_secret)
        .send()
        .await
        .map_err(|error| format!("volume catalog unavailable: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "volume catalog rejected with status {}",
            response.status()
        ));
    }
    response
        .json::<Vec<foundry_shared::dto::VolumeTarget>>()
        .await
        .map_err(|error| format!("volume catalog response was invalid: {error}"))
}

/// Fast filesystem-capacity probe used in deployment preflight. It avoids the
/// recursive per-volume accounting performed by `storage_usage`, which may
/// legitimately take minutes on model trees containing millions of files.
pub async fn storage_capacity() -> Option<(u64, u64)> {
    tokio::task::spawn_blocking(storage_capacity_sync)
        .await
        .ok()
        .flatten()
}

fn storage_capacity_sync() -> Option<(u64, u64)> {
    let disks = Disks::new_with_refreshed_list();
    let root = Path::new(STORAGE_ROOT);
    let disk = disks
        .iter()
        .filter(|disk| root.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())?;
    Some((disk.total_space(), disk.available_space()))
}

/// Volume trees can contain millions of model files. Measure them outside the
/// heartbeat/inventory select loop so a slow filesystem can never make the
/// controller mark an otherwise healthy server offline.
pub async fn storage_loop(
    cache: StorageCache,
    client: &reqwest::Client,
    config: &crate::config::AgentConfig,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let measured = match volume_catalog(client, config).await {
                    Ok(targets) => storage_usage(targets).await.ok_or(()),
                    Err(error) => {
                        tracing::debug!(%error, "volume catalog refresh failed");
                        Err(())
                    }
                };
                let mut cached = cache.write().await;
                *cached = cache_after_catalog(cached.take(), measured);
            }
            _ = crate::shutdown_signal() => break,
        }
    }
}

fn directory_size(path: &Path, depth: u8) -> u64 {
    if depth > 64 {
        return 0;
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| directory_size(&entry.path(), depth + 1))
                .sum()
        })
        .unwrap_or(0)
}

pub fn directory_size_for(path: &Path) -> u64 {
    directory_size(path, 0)
}

#[cfg(test)]
mod tests {
    use super::{cache_after_catalog, catalog_volume_usage_with, STORAGE_ROOT};
    use foundry_shared::dto::{StorageUsage, VolumeTarget};
    use std::path::{Path, PathBuf};

    #[test]
    fn catalog_accounting_does_not_truncate_volume_targets() {
        let targets = (0..10_001)
            .map(|index| VolumeTarget {
                volume_id: foundry_shared::ServerVolumeId::new(),
                path: format!("{STORAGE_ROOT}/.foundry/accounting/{index}"),
            })
            .collect();

        let usage = catalog_volume_usage_with(targets, |_| Ok(None));

        assert_eq!(usage.len(), 10_001);
        assert!(usage.iter().all(|volume| volume.used_bytes == 0));
    }

    #[test]
    fn catalog_accounting_measures_listed_legacy_and_foundry_roots_only() {
        let sandbox = temporary_root("catalog");
        let storage = sandbox.join("storage/containers");
        let legacy = storage.join("volumes/legacy-volume");
        let foundry = storage.join(".foundry/slots/slot-a/app-a/data");
        let unlisted = storage.join("unlisted-sibling");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&foundry).unwrap();
        std::fs::create_dir_all(&unlisted).unwrap();
        std::fs::write(legacy.join("legacy.bin"), b"abc").unwrap();
        std::fs::write(foundry.join("model.bin"), b"12345").unwrap();
        std::fs::write(unlisted.join("ignored.bin"), b"ignored").unwrap();

        let legacy_id = foundry_shared::ServerVolumeId::new();
        let foundry_id = foundry_shared::ServerVolumeId::new();
        let storage_for_inspection = storage.clone();
        let usage = catalog_volume_usage_with(
            vec![
                VolumeTarget {
                    volume_id: legacy_id,
                    path: legacy.to_string_lossy().into_owned(),
                },
                VolumeTarget {
                    volume_id: foundry_id,
                    path: foundry.to_string_lossy().into_owned(),
                },
            ],
            |path| inspect_test_root(&storage_for_inspection, path),
        );

        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].volume_id, legacy_id);
        assert_eq!(usage[0].used_bytes, 3);
        assert_eq!(usage[1].volume_id, foundry_id);
        assert_eq!(usage[1].used_bytes, 5);
        std::fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn failed_catalog_keeps_the_previous_storage_snapshot() {
        let prior = StorageUsage {
            total_bytes: 100,
            available_bytes: 50,
            volumes: vec![],
        };

        let cached = cache_after_catalog(Some(prior), Err(()));

        assert_eq!(cached.expect("previous sample remains").available_bytes, 50);
    }

    fn inspect_test_root(storage: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
        let relative = path
            .strip_prefix(storage)
            .map_err(|_| "outside test storage root".to_string())?;
        if relative.as_os_str().is_empty() || !path.is_dir() {
            return Ok(None);
        }
        std::fs::canonicalize(path)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("foundry-host-{label}-{}", uuid::Uuid::now_v7()))
    }
}

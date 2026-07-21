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

pub async fn storage_usage() -> Option<StorageUsage> {
    tokio::task::spawn_blocking(|| {
        let (total_bytes, available_bytes) = storage_capacity_sync()?;
        let root = Path::new(STORAGE_ROOT);
        let mut volumes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(root.join("volumes")) {
            for entry in entries.flatten().take(10_000) {
                let Ok(id) = entry.file_name().to_string_lossy().parse() else {
                    continue;
                };
                volumes.push(VolumeUsage {
                    volume_id: foundry_shared::ServerVolumeId(id),
                    used_bytes: directory_size(&entry.path(), 0),
                });
            }
        }
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
pub async fn storage_loop(cache: StorageCache) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let measured = storage_usage().await;
                *cache.write().await = measured;
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

//! Docker Registry HTTP API v2, read-only: inspect an image's selected
//! linux/amd64 manifest + config for deploy defaults (ports, persistent
//! mounts, compressed layer size; docs/API.md § Registry). Auth is the
//! same short-lived pull token used for image pulls.

use std::collections::{HashMap, HashSet};

use foundry_shared::dto::{ExposedPort, ImageMetadataResponse, VolumeSpec};
use serde::Deserialize;

use crate::error::AppError;

/// Optional image label carrying richer persistent-volume defaults than
/// Docker's path-only `VOLUME` metadata. Value: JSON `VolumeSpec[]`.
const FOUNDRY_VOLUMES_LABEL: &str = "ai.protv.foundry.volumes";
const MAX_DECLARED_VOLUMES: usize = 16;

/// Every manifest media type we can read; the registry answers with
/// whichever matches (single-arch manifest or multi-arch index).
const ACCEPT: &str = "application/vnd.docker.distribution.manifest.v2+json, \
                      application/vnd.oci.image.manifest.v1+json, \
                      application/vnd.docker.distribution.manifest.list.v2+json, \
                      application/vnd.oci.image.index.v1+json";

#[derive(Deserialize)]
struct Manifest {
    /// Single-arch: the image config descriptor.
    config: Option<Descriptor>,
    /// Multi-arch index: per-platform sub-manifests.
    manifests: Option<Vec<PlatformDescriptor>>,
    /// Single-arch: compressed filesystem layer descriptors.
    layers: Option<Vec<LayerDescriptor>>,
}

#[derive(Deserialize)]
struct Descriptor {
    digest: String,
}

#[derive(Deserialize)]
struct LayerDescriptor {
    size: Option<i64>,
}

#[derive(Deserialize)]
struct PlatformDescriptor {
    digest: String,
    platform: Option<Platform>,
}

#[derive(Deserialize)]
struct Platform {
    os: Option<String>,
    architecture: Option<String>,
}

#[derive(Deserialize)]
struct ImageConfigBlob {
    config: Option<ContainerConfig>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerConfig {
    /// Keys like `8080/tcp`; values are always empty objects.
    exposed_ports: Option<HashMap<String, serde_json::Value>>,
    /// Standard Dockerfile `VOLUME` paths.
    volumes: Option<HashMap<String, serde_json::Value>>,
    /// OCI/Docker image labels, including the Foundry volume defaults.
    labels: Option<HashMap<String, String>>,
}

/// Inspect `repo_path:tag`. Two registry round trips for single-arch,
/// three for a multi-arch index: manifest → selected manifest → config.
pub async fn image_metadata(
    http: &reqwest::Client,
    registry_url: &str,
    pull_token: Option<&str>,
    repo_path: &str,
    tag: &str,
) -> Result<ImageMetadataResponse, AppError> {
    let base = registry_url.trim_end_matches('/');
    let manifest = selected_manifest(http, base, pull_token, repo_path, tag).await?;
    let size_bytes = compressed_size_from_manifest(&manifest);

    let config = manifest
        .config
        .ok_or_else(|| AppError::BadRequest("manifest carries no image config".into()))?;
    let url = format!("{base}/v2/{repo_path}/blobs/{}", config.digest);
    let blob: ImageConfigBlob = fetch(http, &url, pull_token).await?;
    let config = blob.config.unwrap_or_default();

    let mut ports: Vec<ExposedPort> = config
        .exposed_ports
        .unwrap_or_default()
        .into_keys()
        .filter_map(|key| parse_port_key(&key))
        .collect();
    ports.sort_by_key(|port| (port.container_port, port.protocol.clone()));

    let volumes = declared_volumes(
        repo_path,
        config.volumes.unwrap_or_default().into_keys(),
        config.labels.unwrap_or_default().get(FOUNDRY_VOLUMES_LABEL),
    );

    Ok(ImageMetadataResponse {
        ports,
        volumes,
        size_bytes,
    })
}

/// Compressed layer size without fetching the config blob. This is the
/// narrow fallback for GitLab tag-detail responses that explicitly say
/// a real image is zero bytes.
pub async fn compressed_size(
    http: &reqwest::Client,
    registry_url: &str,
    pull_token: Option<&str>,
    repo_path: &str,
    tag: &str,
) -> Result<Option<i64>, AppError> {
    let base = registry_url.trim_end_matches('/');
    let manifest = selected_manifest(http, base, pull_token, repo_path, tag).await?;
    Ok(compressed_size_from_manifest(&manifest))
}

async fn selected_manifest(
    http: &reqwest::Client,
    base: &str,
    pull_token: Option<&str>,
    repo_path: &str,
    tag: &str,
) -> Result<Manifest, AppError> {
    let url = format!("{base}/v2/{repo_path}/manifests/{tag}");
    let mut manifest: Manifest = fetch(http, &url, pull_token).await?;

    if let Some(list) = manifest.manifests.take() {
        // The GPU fleet is linux/amd64. Fall back to the first entry
        // rather than rejecting an index with incomplete platform data.
        let chosen = list
            .iter()
            .find(|item| {
                item.platform.as_ref().is_some_and(|platform| {
                    platform.os.as_deref() == Some("linux")
                        && platform.architecture.as_deref() == Some("amd64")
                })
            })
            .or_else(|| list.first())
            .ok_or_else(|| AppError::BadRequest("image index lists no platforms".into()))?;
        let url = format!("{base}/v2/{repo_path}/manifests/{}", chosen.digest);
        manifest = fetch(http, &url, pull_token).await?;
    }
    Ok(manifest)
}

fn compressed_size_from_manifest(manifest: &Manifest) -> Option<i64> {
    let layers = manifest.layers.as_ref()?;
    if layers.is_empty() {
        return None;
    }
    let total = layers.iter().try_fold(0i64, |total, layer| {
        let size = layer.size.filter(|size| *size >= 0)?;
        total.checked_add(size)
    })?;
    (total > 0).then_some(total)
}

fn declared_volumes(
    repo_path: &str,
    docker_paths: impl Iterator<Item = String>,
    label: Option<&String>,
) -> Vec<VolumeSpec> {
    let mut out = Vec::new();
    let mut paths = HashSet::new();

    if let Some(label) = label {
        if let Ok(defaults) = serde_json::from_str::<Vec<VolumeSpec>>(label) {
            for volume in defaults {
                if out.len() == MAX_DECLARED_VOLUMES {
                    break;
                }
                let path = volume.container_path.trim().to_string();
                if crate::repos::volumes::validate_volume_name(&volume.volume_name).is_ok()
                    && crate::repos::volumes::validate_container_path(&path).is_ok()
                    && paths.insert(path.clone())
                {
                    out.push(VolumeSpec {
                        container_path: path,
                        ..volume
                    });
                }
            }
        }
    }

    let mut docker_paths: Vec<_> = docker_paths.collect();
    docker_paths.sort();
    for path in docker_paths {
        if out.len() == MAX_DECLARED_VOLUMES {
            break;
        }
        let path = path.trim().to_string();
        if crate::repos::volumes::validate_container_path(&path).is_ok()
            && paths.insert(path.clone())
        {
            out.push(VolumeSpec {
                volume_name: suggested_volume_name(repo_path, &path),
                container_path: path,
                read_only: false,
            });
        }
    }
    out
}

fn suggested_volume_name(repo_path: &str, container_path: &str) -> String {
    let image = repo_path.rsplit('/').next().unwrap_or("image");
    let raw = format!(
        "{image}-{}",
        container_path.trim_matches('/').replace('/', "-")
    );
    let mut name = String::with_capacity(raw.len().min(63));
    let mut last_was_dash = false;
    for character in raw.chars() {
        let mapped = if character.is_ascii_alphanumeric() || character == '_' {
            character.to_ascii_lowercase()
        } else {
            '-'
        };
        if mapped == '-' && last_was_dash {
            continue;
        }
        name.push(mapped);
        last_was_dash = mapped == '-';
        if name.len() == 63 {
            break;
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "image-data".to_string()
    } else {
        name.to_string()
    }
}

/// `8080/tcp` → (8080, tcp); a bare `8080` defaults to tcp (the
/// Dockerfile EXPOSE default).
fn parse_port_key(key: &str) -> Option<ExposedPort> {
    let (port, protocol) = match key.split_once('/') {
        Some((port, protocol)) => (port, protocol),
        None => (key, "tcp"),
    };
    let container_port: u16 = port.parse().ok()?;
    if container_port == 0 || !matches!(protocol, "tcp" | "udp") {
        return None;
    }
    Some(ExposedPort {
        container_port,
        protocol: protocol.to_string(),
    })
}

async fn fetch<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    pull_token: Option<&str>,
) -> Result<T, AppError> {
    let mut request = http.get(url).header(reqwest::header::ACCEPT, ACCEPT);
    if let Some(token) = pull_token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(AppError::gitlab)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::BadRequest(format!(
            "registry returned {status} for this image"
        )));
    }
    response.json::<T>().await.map_err(AppError::gitlab)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_key_parsing() {
        let port = parse_port_key("8080/tcp").unwrap();
        assert_eq!((port.container_port, port.protocol.as_str()), (8080, "tcp"));
        let port = parse_port_key("53/udp").unwrap();
        assert_eq!((port.container_port, port.protocol.as_str()), (53, "udp"));
        let port = parse_port_key("9000").unwrap();
        assert_eq!((port.container_port, port.protocol.as_str()), (9000, "tcp"));
        assert!(parse_port_key("0/tcp").is_none());
        assert!(parse_port_key("8080/sctp").is_none());
        assert!(parse_port_key("notaport/tcp").is_none());
    }

    #[test]
    fn manifest_size_requires_complete_positive_layer_total() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"config":{"digest":"sha256:config"},"layers":[{"size":100},{"size":23}]}"#,
        )
        .unwrap();
        assert_eq!(compressed_size_from_manifest(&manifest), Some(123));

        let incomplete: Manifest = serde_json::from_str(r#"{"layers":[{"size":100},{}]}"#).unwrap();
        assert_eq!(compressed_size_from_manifest(&incomplete), None);
    }

    #[test]
    fn foundry_label_overrides_standard_volume_defaults() {
        let label = r#"[
            {"volume_name":"comfy-models","container_path":"/data/models","read_only":true},
            {"volume_name":"comfy-output","container_path":"/data/output","read_only":false}
        ]"#
        .to_string();
        let volumes = declared_volumes(
            "templates/comfyui-blank",
            ["/data/models".to_string(), "/data/settings".to_string()].into_iter(),
            Some(&label),
        );
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].volume_name, "comfy-models");
        assert!(volumes[0].read_only);
        assert_eq!(volumes[1].volume_name, "comfy-output");
        assert_eq!(volumes[2].volume_name, "comfyui-blank-data-settings");
    }

    #[test]
    fn invalid_label_entries_are_ignored() {
        let label = r#"[
            {"volume_name":"../escape","container_path":"/safe","read_only":false},
            {"volume_name":"valid","container_path":"relative","read_only":false}
        ]"#
        .to_string();
        assert!(declared_volumes("team/image", std::iter::empty(), Some(&label)).is_empty());
    }
}

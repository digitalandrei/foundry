//! Docker Registry HTTP API v2, read-only: fetch an image's config
//! blob to discover its EXPOSE'd ports (deploy-dialog prefill —
//! docs/API.md § Registry). Auth is the same short-lived pull token
//! used for image pulls (docs/GITLAB-INTEGRATION.md § Image Pulls);
//! anonymous works for public images.

use std::collections::HashMap;

use foundry_shared::dto::ExposedPort;
use serde::Deserialize;

use crate::error::AppError;

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
}

#[derive(Deserialize)]
struct Descriptor {
    digest: String,
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

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerConfig {
    /// Keys like `8080/tcp`; values are always empty objects.
    exposed_ports: Option<HashMap<String, serde_json::Value>>,
}

/// EXPOSE'd ports of `repo_path:tag`, sorted by port number. Two round
/// trips: manifest (+1 for multi-arch) → config blob.
pub async fn exposed_ports(
    http: &reqwest::Client,
    registry_url: &str,
    pull_token: Option<&str>,
    repo_path: &str,
    tag: &str,
) -> Result<Vec<ExposedPort>, AppError> {
    let base = registry_url.trim_end_matches('/');

    let url = format!("{base}/v2/{repo_path}/manifests/{tag}");
    let mut manifest: Manifest = fetch(http, &url, pull_token).await?;

    if let Some(list) = manifest.manifests.take() {
        // Multi-arch: the GPU fleet is linux/amd64; fall back to the
        // first entry rather than failing on exotic indexes.
        let chosen = list
            .iter()
            .find(|m| {
                m.platform.as_ref().is_some_and(|p| {
                    p.os.as_deref() == Some("linux") && p.architecture.as_deref() == Some("amd64")
                })
            })
            .or_else(|| list.first())
            .ok_or_else(|| AppError::BadRequest("image index lists no platforms".into()))?;
        let url = format!("{base}/v2/{repo_path}/manifests/{}", chosen.digest);
        manifest = fetch(http, &url, pull_token).await?;
    }

    let config = manifest
        .config
        .ok_or_else(|| AppError::BadRequest("manifest carries no image config".into()))?;
    let url = format!("{base}/v2/{repo_path}/blobs/{}", config.digest);
    let blob: ImageConfigBlob = fetch(http, &url, pull_token).await?;

    let mut ports: Vec<ExposedPort> = blob
        .config
        .and_then(|c| c.exposed_ports)
        .unwrap_or_default()
        .into_keys()
        .filter_map(|key| parse_port_key(&key))
        .collect();
    ports.sort_by_key(|p| (p.container_port, p.protocol.clone()));
    Ok(ports)
}

/// `8080/tcp` → (8080, tcp); a bare `8080` defaults to tcp (the
/// Dockerfile EXPOSE default).
fn parse_port_key(key: &str) -> Option<ExposedPort> {
    let (port, protocol) = match key.split_once('/') {
        Some((p, proto)) => (p, proto),
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
    let mut req = http.get(url).header(reqwest::header::ACCEPT, ACCEPT);
    if let Some(token) = pull_token {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.map_err(AppError::gitlab)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::BadRequest(format!(
            "registry returned {status} for this image"
        )));
    }
    resp.json::<T>().await.map_err(AppError::gitlab)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_key_parsing() {
        let p = parse_port_key("8080/tcp").unwrap();
        assert_eq!((p.container_port, p.protocol.as_str()), (8080, "tcp"));
        let p = parse_port_key("53/udp").unwrap();
        assert_eq!((p.container_port, p.protocol.as_str()), (53, "udp"));
        let p = parse_port_key("9000").unwrap();
        assert_eq!((p.container_port, p.protocol.as_str()), (9000, "tcp"));
        assert!(parse_port_key("0/tcp").is_none());
        assert!(parse_port_key("8080/sctp").is_none());
        assert!(parse_port_key("notaport/tcp").is_none());
    }
}

//! The Docker seam (architecture audit: candidate "DockerClient module";
//! skill: docker-engine-api). A small `DockerEngine` interface the
//! executor (tasks.rs) talks through, a `BollardEngine` adapter that
//! hides bollard, and — test-only — a stateful `FakeEngine`.
//!
//! Bollard types never cross this interface, so the deploy/replace/adopt
//! orchestration above it is unit-testable without a Docker daemon. The
//! depth lives in the executor; this adapter is a thin translation. The
//! continuous/interactive streams (logs, stats, exec PTY) keep their own
//! seams in logs.rs/metrics.rs/shell.rs and are deliberately not here.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use futures_util::StreamExt;

/// Errors the engine surfaces. The variants the executor branches on are
/// explicit; everything else collapses to `Other`.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("docker unavailable: {0}")]
    Unavailable(String),
    #[error("not found")]
    NotFound,
    #[error("name already in use")]
    Conflict,
    #[error("image pull failed: {message}")]
    ImagePull { message: String, unauthorized: bool },
    #[error("docker operation timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// One container as the engine reports it: id, lowercased state string
/// (`running`, `exited`, …), and its labels. The executor does the
/// `foundry.managed` / `foundry.deployment_id` label filtering, not the
/// engine — that decision is part of the testable orchestration.
#[derive(Debug, Clone)]
pub struct ContainerSummary {
    pub id: String,
    pub state: String,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ContainerHealth {
    pub status: String,
    pub detail: Option<String>,
}

/// Registry credential in domain terms (no bollard `DockerCredentials`).
#[derive(Debug, Clone)]
pub enum RegistryCreds {
    Token(String),
    UserPassword { username: String, password: String },
}

#[derive(Debug, Clone)]
pub struct PortSpec {
    pub container_port: u16,
    pub host_port: u16,
    /// `tcp` / `udp`.
    pub protocol: String,
}

#[derive(Debug, Clone)]
pub struct BindSpec {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

/// Everything needed to create one container, in domain terms. The
/// executor builds this from a `DeployPayload` (the bug-dense part:
/// device-uuid fallback, slot-id label, labels); `BollardEngine`
/// translates it into bollard's `ContainerCreateBody`.
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub env: Vec<(String, String)>,
    pub labels: BTreeMap<String, String>,
    pub ports: Vec<PortSpec>,
    pub binds: Vec<BindSpec>,
    /// Docker `--memory` cap in bytes; `None` → unlimited.
    pub memory_bytes: Option<i64>,
    /// NVML device UUIDs for the single `DeviceRequest`.
    pub device_uuids: Vec<String>,
}

/// Sink for streamed pull progress — one aggregated operator line per
/// update (`pulling: 3/7 layers · 410 / 1208 MB`). Implemented by the
/// executor's progress reporter; the engine owns the per-layer
/// aggregation so bollard's `CreateImageInfo` never leaks out.
#[async_trait]
pub trait PullSink: Send {
    async fn progress(&mut self, line: &str);
}

/// The seam. Primitive lifecycle ops only — `deploy`/`replace`/`adopt`
/// orchestration lives above it in the executor, tested against
/// `FakeEngine`.
#[async_trait]
pub trait DockerEngine: Send + Sync {
    /// All containers (running and stopped) with labels; the executor
    /// filters them.
    async fn list(&self) -> Result<Vec<ContainerSummary>, DockerError>;
    /// Confirm that this daemon can hand NVIDIA devices to containers.
    /// A reachable Docker socket alone is not sufficient: modern Docker
    /// may select CDI and fail only when the container is started.
    async fn gpu_support(&self) -> Result<String, DockerError>;
    /// The image reference a container was created from, if it exists.
    async fn inspect_image(&self, id: &str) -> Result<Option<String>, DockerError>;
    /// Pull an image, streaming aggregated progress into `sink`. An
    /// auth/missing-tag failure surfaces as `ImagePull { unauthorized }`.
    async fn pull(
        &self,
        image: &str,
        creds: Option<RegistryCreds>,
        sink: &mut (dyn PullSink + Send),
    ) -> Result<(), DockerError>;
    /// Create a container from a domain spec; returns its docker id.
    async fn create(&self, spec: &ContainerSpec) -> Result<String, DockerError>;
    async fn start(&self, id: &str) -> Result<(), DockerError>;
    async fn health(&self, id: &str) -> Result<ContainerHealth, DockerError>;
    async fn stop(&self, id: &str, timeout_secs: i32) -> Result<(), DockerError>;
    async fn restart(&self, id: &str, timeout_secs: i32) -> Result<(), DockerError>;
    /// Rename a retained predecessor to release its stable deployment name
    /// for a replacement successor, or restore that name during rollback.
    /// Implementations return success when it already has `name`.
    async fn rename(&self, id: &str, name: &str) -> Result<(), DockerError>;
    /// Force-remove a container; removing an absent container is `Ok`
    /// (teardown stays idempotent).
    async fn remove(&self, id: &str) -> Result<(), DockerError>;
    /// Best-effort image reclaim — no `force`, so Docker's own in-use
    /// refusal protects siblings sharing the layers.
    async fn remove_image(&self, image: &str) -> Result<(), DockerError>;
}

/// Aggregates bollard's per-layer pull stream into one operator line:
/// `pulling: 3/7 layers · 410 / 1208 MB`. Lives here (not the executor)
/// because it reads bollard's `CreateImageInfo`.
#[derive(Default)]
struct PullProgress {
    layers: HashMap<String, (u64, u64)>,
    done: std::collections::HashSet<String>,
}

impl PullProgress {
    fn update(&mut self, info: &bollard::models::CreateImageInfo) {
        let Some(id) = info.id.clone() else { return };
        match info.status.as_deref().unwrap_or("") {
            "Pull complete" | "Already exists" => {
                self.layers.entry(id.clone()).or_insert((0, 0));
                // A finished layer counts fully even when Docker never
                // reported its size.
                if let Some(l) = self.layers.get_mut(&id) {
                    l.0 = l.1;
                }
                self.done.insert(id);
            }
            "Downloading" => {
                if let Some(p) = &info.progress_detail {
                    let entry = self.layers.entry(id).or_insert((0, 0));
                    entry.0 = p.current.unwrap_or(0).max(0) as u64;
                    entry.1 = p.total.unwrap_or(0).max(0) as u64;
                }
            }
            _ => {
                self.layers.entry(id).or_insert((0, 0));
            }
        }
    }

    fn line(&self) -> String {
        let (cur, total) = self
            .layers
            .values()
            .fold((0u64, 0u64), |acc, (c, t)| (acc.0 + c, acc.1 + t));
        format!(
            "pulling: {}/{} layers · {} / {} MB",
            self.done.len(),
            self.layers.len(),
            cur / 1_048_576,
            total / 1_048_576,
        )
    }
}

fn looks_unauthorized(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("unauthor") || m.contains("denied") || m.contains("401") || m.contains("forbidden")
}

/// Map a bollard request error onto the executor-branchable variants.
/// Prefer bollard's structured status code; fall back to message text
/// only for transport-level errors that carry no code.
fn map_err(context: &str, e: bollard::errors::Error) -> DockerError {
    if let bollard::errors::Error::DockerResponseServerError { status_code, .. } = &e {
        match status_code {
            404 => return DockerError::NotFound,
            409 => return DockerError::Conflict,
            _ => {}
        }
    }
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if msg.contains("404") || lower.contains("no such container") {
        DockerError::NotFound
    } else if msg.contains("409") || lower.contains("conflict") {
        DockerError::Conflict
    } else if lower.contains("error trying to connect")
        || lower.contains("connection refused")
        || lower.contains("no such file or directory")
    {
        DockerError::Unavailable(msg)
    } else if lower.contains("timed out") || lower.contains("timeout") {
        DockerError::Timeout
    } else {
        DockerError::Other(format!("{context}: {msg}"))
    }
}

/// Connect a bollard client over the local socket. Bollard checks that the
/// socket path exists here, so callers must retry when Docker starts after
/// the agent. Once constructed, one handle is shared across all loops.
pub fn connect_local() -> Result<bollard::Docker, DockerError> {
    bollard::Docker::connect_with_local_defaults()
        .map_err(|e| DockerError::Unavailable(e.to_string()))
}

/// Process-wide, lazily recovering Docker handle. The agent remains useful
/// without Docker (heartbeats, host diagnostics, upgrades, storage tasks),
/// and the first caller after the socket appears initializes the one shared
/// Bollard client without requiring a service restart.
#[derive(Clone, Default)]
pub struct DockerRuntime {
    inner: Arc<DockerRuntimeInner>,
}

#[derive(Default)]
struct DockerRuntimeInner {
    client: OnceLock<bollard::Docker>,
    unavailable_logged: AtomicBool,
}

impl DockerRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the shared client, retrying initialization while the socket is
    /// absent. Failures are logged once; recovery is logged once.
    pub fn client(&self) -> Option<bollard::Docker> {
        self.client_with(connect_local)
    }

    fn client_with(
        &self,
        connect: impl FnOnce() -> Result<bollard::Docker, DockerError>,
    ) -> Option<bollard::Docker> {
        if let Some(client) = self.inner.client.get() {
            return Some(client.clone());
        }

        match connect() {
            Ok(client) => {
                if self.inner.client.set(client).is_ok() {
                    if self.inner.unavailable_logged.swap(false, Ordering::Relaxed) {
                        tracing::info!("Docker socket became available — Docker features enabled");
                    } else {
                        tracing::info!("docker: client ready (shared across loops)");
                    }
                }
                self.inner.client.get().cloned()
            }
            Err(error) => {
                if !self.inner.unavailable_logged.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        %error,
                        "Docker socket unavailable — operational tasks remain active; retrying automatically"
                    );
                }
                None
            }
        }
    }
}

/// The production adapter: wraps the one shared bollard connection.
pub struct BollardEngine {
    docker: bollard::Docker,
}

impl BollardEngine {
    /// Wrap the shared Docker client (see [`connect_local`]).
    pub fn new(docker: bollard::Docker) -> Self {
        Self { docker }
    }
}

#[async_trait]
impl DockerEngine for BollardEngine {
    async fn list(&self) -> Result<Vec<ContainerSummary>, DockerError> {
        let list = self
            .docker
            .list_containers(Some(bollard::query_parameters::ListContainersOptions {
                all: true,
                ..Default::default()
            }))
            .await
            .map_err(|e| map_err("container listing failed", e))?;
        Ok(list
            .into_iter()
            .map(|c| ContainerSummary {
                id: c.id.unwrap_or_default(),
                state: c
                    .state
                    .map(|s| format!("{s:?}").to_lowercase())
                    .unwrap_or_default(),
                labels: c.labels.unwrap_or_default().into_iter().collect(),
            })
            .collect())
    }

    async fn gpu_support(&self) -> Result<String, DockerError> {
        let info = self
            .docker
            .info()
            .await
            .map_err(|e| map_err("Docker GPU capability probe failed", e))?;
        if info
            .runtimes
            .as_ref()
            .is_some_and(|runtimes| runtimes.contains_key("nvidia"))
        {
            return Ok("Docker exposes the NVIDIA container runtime".into());
        }

        // Docker 29 prefers CDI for `--gpus` when NVIDIA CDI devices are
        // present. Bollard's API model predates Info.DiscoveredDevices, so
        // ask the local Docker CLI (which uses the same daemon/socket) for
        // that one field instead of treating an absent legacy runtime as a
        // false negative.
        let output = tokio::process::Command::new("docker")
            .args(["info", "--format", "{{json .DiscoveredDevices}}"])
            .output()
            .await;
        if let Ok(output) = output {
            if output.status.success() {
                let discovered = String::from_utf8_lossy(&output.stdout);
                if let Some(device) = nvidia_cdi_device(&discovered) {
                    return Ok(format!("Docker discovered NVIDIA CDI device {device}"));
                }
            }
        }

        Err(DockerError::Other(
            "Docker has no NVIDIA runtime or discovered nvidia.com/gpu CDI device; install nvidia-container-toolkit, run `sudo nvidia-ctk runtime configure --runtime=docker`, then `sudo systemctl restart docker`"
                .into(),
        ))
    }

    async fn inspect_image(&self, id: &str) -> Result<Option<String>, DockerError> {
        match self
            .docker
            .inspect_container(
                id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
        {
            Ok(c) => Ok(c.image),
            Err(e) => match map_err("inspect failed", e) {
                DockerError::NotFound => Ok(None),
                other => Err(other),
            },
        }
    }

    async fn pull(
        &self,
        image: &str,
        creds: Option<RegistryCreds>,
        sink: &mut (dyn PullSink + Send),
    ) -> Result<(), DockerError> {
        let credentials = creds.map(|c| match c {
            RegistryCreds::Token(token) => bollard::auth::DockerCredentials {
                registrytoken: Some(token),
                ..Default::default()
            },
            RegistryCreds::UserPassword { username, password } => {
                bollard::auth::DockerCredentials {
                    username: Some(username),
                    password: Some(password),
                    ..Default::default()
                }
            }
        });
        let mut stream = self.docker.create_image(
            Some(bollard::query_parameters::CreateImageOptions {
                from_image: Some(image.to_string()),
                ..Default::default()
            }),
            None,
            credentials,
        );
        let mut stats = PullProgress::default();
        while let Some(msg) = stream.next().await {
            let info = msg.map_err(|e| {
                let message = e.to_string();
                DockerError::ImagePull {
                    unauthorized: looks_unauthorized(&message),
                    message,
                }
            })?;
            // Auth/missing-tag failures arrive as 200-stream messages with
            // an embedded error — surface them.
            if let Some(error) = info.error {
                return Err(DockerError::ImagePull {
                    unauthorized: looks_unauthorized(&error),
                    message: error,
                });
            }
            stats.update(&info);
            sink.progress(&stats.line()).await;
        }
        Ok(())
    }

    async fn create(&self, spec: &ContainerSpec) -> Result<String, DockerError> {
        use bollard::models::{ContainerCreateBody, DeviceRequest, HostConfig, PortBinding};

        let nvidia_runtime = self
            .docker
            .info()
            .await
            .map_err(|e| map_err("Docker GPU runtime selection failed", e))?
            .runtimes
            .is_some_and(|runtimes| runtimes.contains_key("nvidia"));

        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
        for port in &spec.ports {
            let key = format!("{}/{}", port.container_port, port.protocol);
            exposed.insert(key.clone(), HashMap::new());
            port_bindings
                .entry(key)
                .or_insert_with(|| Some(Vec::new()))
                .get_or_insert_with(Vec::new)
                .push(PortBinding {
                    host_ip: None,
                    host_port: Some(port.host_port.to_string()),
                });
        }
        let binds: Vec<String> = spec
            .binds
            .iter()
            .map(|b| {
                format!(
                    "{}:{}{}",
                    b.host_path,
                    b.container_path,
                    if b.read_only { ":ro" } else { "" }
                )
            })
            .collect();

        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
            env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
            labels: Some(spec.labels.clone().into_iter().collect()),
            exposed_ports: Some(exposed),
            host_config: Some(HostConfig {
                port_bindings: Some(port_bindings),
                binds: (!binds.is_empty()).then_some(binds),
                memory: spec.memory_bytes,
                // Be explicit on Docker 29+: an omitted driver may take the
                // CDI auto-discovery path and fail at start with "no known
                // GPU vendor" even though the NVIDIA runtime is configured.
                // A CDI-only daemon keeps the empty driver so Docker can use
                // the discovered `nvidia.com/gpu` devices validated above.
                device_requests: Some(vec![DeviceRequest {
                    driver: nvidia_runtime.then(|| "nvidia".to_string()),
                    count: None,
                    device_ids: Some(spec.device_uuids.clone()),
                    capabilities: Some(vec![vec!["gpu".to_string()]]),
                    options: None,
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let created = self
            .docker
            .create_container(
                Some(bollard::query_parameters::CreateContainerOptions {
                    name: Some(spec.name.clone()),
                    ..Default::default()
                }),
                body,
            )
            .await
            .map_err(|e| map_err("container create failed", e))?;
        Ok(created.id)
    }

    async fn start(&self, id: &str) -> Result<(), DockerError> {
        self.docker
            .start_container(id, None::<bollard::query_parameters::StartContainerOptions>)
            .await
            .map_err(|e| map_err("container start failed", e))
    }

    async fn health(&self, id: &str) -> Result<ContainerHealth, DockerError> {
        let inspect = self
            .docker
            .inspect_container(
                id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .map_err(|e| map_err("container health inspect failed", e))?;
        let Some(health) = inspect.state.and_then(|state| state.health) else {
            return Ok(ContainerHealth {
                status: "none".into(),
                detail: None,
            });
        };
        let status = health
            .status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "none".into());
        let detail = health
            .log
            .and_then(|logs| logs.last().and_then(|log| log.output.clone()))
            .map(|output| output.trim().chars().take(800).collect());
        Ok(ContainerHealth { status, detail })
    }

    async fn stop(&self, id: &str, timeout_secs: i32) -> Result<(), DockerError> {
        self.docker
            .stop_container(
                id,
                Some(bollard::query_parameters::StopContainerOptions {
                    t: Some(timeout_secs),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| map_err("container stop failed", e))
    }

    async fn restart(&self, id: &str, timeout_secs: i32) -> Result<(), DockerError> {
        self.docker
            .restart_container(
                id,
                Some(bollard::query_parameters::RestartContainerOptions {
                    t: Some(timeout_secs),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| map_err("container restart failed", e))
    }

    async fn rename(&self, id: &str, name: &str) -> Result<(), DockerError> {
        let current_name = self
            .docker
            .inspect_container(
                id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .map_err(|e| map_err("container rename inspect failed", e))?
            .name;
        if current_name
            .as_deref()
            .map(|current| current.trim_start_matches('/'))
            == Some(name)
        {
            return Ok(());
        }
        self.docker
            .rename_container(
                id,
                bollard::query_parameters::RenameContainerOptions {
                    name: name.to_string(),
                },
            )
            .await
            .map_err(|e| map_err("container rename failed", e))
    }

    async fn remove(&self, id: &str) -> Result<(), DockerError> {
        match self
            .docker
            .remove_container(
                id,
                Some(bollard::query_parameters::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => match map_err("container remove failed", e) {
                DockerError::NotFound => Ok(()),
                other => Err(other),
            },
        }
    }

    async fn remove_image(&self, image: &str) -> Result<(), DockerError> {
        self.docker
            .remove_image(
                image,
                None::<bollard::query_parameters::RemoveImageOptions>,
                None,
            )
            .await
            .map(|_| ())
            .map_err(|e| map_err("image reclaim failed", e))
    }
}

fn nvidia_cdi_device(value: &str) -> Option<String> {
    let devices = serde_json::from_str::<serde_json::Value>(value.trim()).ok()?;
    devices.as_array()?.iter().find_map(|device| {
        ["ID", "Id", "id"]
            .into_iter()
            .find_map(|key| device.get(key).and_then(serde_json::Value::as_str))
            .filter(|id| id.starts_with("nvidia.com/gpu"))
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_progress_aggregates_layers() {
        let mut p = PullProgress::default();
        p.update(&bollard::models::CreateImageInfo {
            id: Some("l1".into()),
            status: Some("Downloading".into()),
            progress_detail: Some(bollard::models::ProgressDetail {
                current: Some(5 * 1_048_576),
                total: Some(10 * 1_048_576),
            }),
            ..Default::default()
        });
        p.update(&bollard::models::CreateImageInfo {
            id: Some("l2".into()),
            status: Some("Pull complete".into()),
            ..Default::default()
        });
        // l1: 5/10 MB downloading, l2: complete (counts as done).
        assert_eq!(p.line(), "pulling: 1/2 layers · 5 / 10 MB");
    }

    #[test]
    fn maps_status_codes_to_branchable_variants() {
        let mk = |s: &str| bollard::errors::Error::DockerResponseServerError {
            status_code: 0,
            message: s.to_string(),
        };
        assert!(matches!(
            map_err("x", mk("(404) no such container")),
            DockerError::NotFound
        ));
        assert!(matches!(
            map_err("x", mk("(409) Conflict")),
            DockerError::Conflict
        ));
        assert!(matches!(map_err("x", mk("weird")), DockerError::Other(_)));
    }

    #[test]
    fn unauthorized_detection() {
        assert!(looks_unauthorized("unauthorized: access denied"));
        assert!(looks_unauthorized(
            "denied: requested access to the resource"
        ));
        assert!(!looks_unauthorized("manifest unknown"));
    }

    #[test]
    fn identifies_nvidia_cdi_devices_from_docker_info() {
        assert_eq!(
            nvidia_cdi_device(
                r#"[{"Source":"cdi","ID":"nvidia.com/gpu=GPU-a"},{"Source":"cdi","ID":"vendor/device=x"}]"#,
            )
            .as_deref(),
            Some("nvidia.com/gpu=GPU-a")
        );
        assert_eq!(nvidia_cdi_device("null"), None);
        assert_eq!(nvidia_cdi_device("not-json"), None);
    }

    #[test]
    fn docker_runtime_retries_after_the_socket_appears() {
        let runtime = DockerRuntime::new();
        assert!(runtime
            .client_with(|| Err(DockerError::Unavailable("socket absent".into())))
            .is_none());

        let dir = std::env::temp_dir().join(format!("foundry-docker-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let socket = dir.join("docker.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let socket_path = socket.to_string_lossy().into_owned();

        assert!(runtime
            .client_with(|| {
                bollard::Docker::connect_with_socket(
                    &socket_path,
                    120,
                    bollard::API_DEFAULT_VERSION,
                )
                .map_err(|error| DockerError::Unavailable(error.to_string()))
            })
            .is_some());
        assert!(runtime
            .client_with(|| panic!("initialized runtime must reuse its Docker client"))
            .is_some());

        drop(listener);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Stateful in-memory engine for executor tests. `create`/`start`/`stop`/
/// `remove` mutate the container map, so `list`/`inspect` stay consistent
/// across a sequence; the knobs drive error paths.
#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone)]
    struct FakeContainer {
        name: String,
        state: String,
        image: String,
        labels: BTreeMap<String, String>,
    }

    #[derive(Default)]
    pub(crate) struct FakeEngine {
        containers: Mutex<Vec<(String, FakeContainer)>>,
        /// Specs passed to `create`, in order — the test's assertion surface.
        pub created: Mutex<Vec<ContainerSpec>>,
        /// Ids passed to `remove`, in order.
        pub removed: Mutex<Vec<String>>,
        /// `(container id, new name)` calls, in order.
        pub renamed: Mutex<Vec<(String, String)>>,
        /// If set, the next `pull` fails with this error.
        pub pull_error: Mutex<Option<DockerError>>,
        /// If set, the next health query fails with this error.
        pub health_error: Mutex<Option<DockerError>>,
        /// If true, `create` fails with `Conflict`.
        pub create_conflict: Mutex<bool>,
        /// If true, every call fails with `Unavailable`.
        pub down: Mutex<bool>,
        /// Optional NVIDIA-container capability failure.
        pub gpu_error: Mutex<Option<String>>,
        next_id: Mutex<u32>,
    }

    impl FakeEngine {
        pub fn new() -> Self {
            Self::default()
        }

        /// Seed a managed container for a deployment (idempotency tests).
        pub fn with_managed(self, id: &str, state: &str, deployment_id: &str) -> Self {
            self.with_managed_named(id, state, deployment_id, id)
        }

        pub fn with_managed_named(
            self,
            id: &str,
            state: &str,
            deployment_id: &str,
            name: &str,
        ) -> Self {
            let labels = BTreeMap::from([
                ("foundry.managed".to_string(), "true".to_string()),
                (
                    "foundry.deployment_id".to_string(),
                    deployment_id.to_string(),
                ),
            ]);
            self.containers.lock().unwrap().push((
                id.to_string(),
                FakeContainer {
                    name: name.to_string(),
                    state: state.to_string(),
                    image: format!("img-of-{id}"),
                    labels,
                },
            ));
            self
        }

        pub fn name(&self, id: &str) -> Option<String> {
            self.containers
                .lock()
                .unwrap()
                .iter()
                .find(|(container_id, _)| container_id == id)
                .map(|(_, container)| container.name.clone())
        }

        pub fn fail_pull(self, e: DockerError) -> Self {
            *self.pull_error.lock().unwrap() = Some(e);
            self
        }

        pub fn fail_health(self, e: DockerError) -> Self {
            *self.health_error.lock().unwrap() = Some(e);
            self
        }

        pub fn conflict_on_create(self) -> Self {
            *self.create_conflict.lock().unwrap() = true;
            self
        }

        pub fn fail_gpu_support(self, message: &str) -> Self {
            *self.gpu_error.lock().unwrap() = Some(message.into());
            self
        }

        /// Current container ids — lets a test assert presence/absence.
        pub fn ids(&self) -> Vec<String> {
            self.containers
                .lock()
                .unwrap()
                .iter()
                .map(|(id, _)| id.clone())
                .collect()
        }

        fn ensure_up(&self) -> Result<(), DockerError> {
            if *self.down.lock().unwrap() {
                Err(DockerError::Unavailable("fake daemon down".into()))
            } else {
                Ok(())
            }
        }

        fn set_state(&self, id: &str, state: &str) {
            if let Some((_, c)) = self
                .containers
                .lock()
                .unwrap()
                .iter_mut()
                .find(|(cid, _)| cid == id)
            {
                c.state = state.to_string();
            }
        }
    }

    #[async_trait]
    impl DockerEngine for FakeEngine {
        async fn list(&self) -> Result<Vec<ContainerSummary>, DockerError> {
            self.ensure_up()?;
            Ok(self
                .containers
                .lock()
                .unwrap()
                .iter()
                .map(|(id, c)| ContainerSummary {
                    id: id.clone(),
                    state: c.state.clone(),
                    labels: c.labels.clone(),
                })
                .collect())
        }

        async fn gpu_support(&self) -> Result<String, DockerError> {
            self.ensure_up()?;
            match self.gpu_error.lock().unwrap().as_ref() {
                Some(message) => Err(DockerError::Other(message.clone())),
                None => Ok("fake NVIDIA runtime ready".into()),
            }
        }

        async fn inspect_image(&self, id: &str) -> Result<Option<String>, DockerError> {
            self.ensure_up()?;
            Ok(self
                .containers
                .lock()
                .unwrap()
                .iter()
                .find(|(cid, _)| cid == id)
                .map(|(_, c)| c.image.clone()))
        }

        async fn pull(
            &self,
            _image: &str,
            _creds: Option<RegistryCreds>,
            sink: &mut (dyn PullSink + Send),
        ) -> Result<(), DockerError> {
            self.ensure_up()?;
            if let Some(e) = self.pull_error.lock().unwrap().take() {
                return Err(e);
            }
            sink.progress("pulling: 1/1 layers · 1 / 1 MB").await;
            Ok(())
        }

        async fn create(&self, spec: &ContainerSpec) -> Result<String, DockerError> {
            self.ensure_up()?;
            if *self.create_conflict.lock().unwrap() {
                return Err(DockerError::Conflict);
            }
            if self
                .containers
                .lock()
                .unwrap()
                .iter()
                .any(|(_, container)| container.name == spec.name)
            {
                return Err(DockerError::Conflict);
            }
            self.created.lock().unwrap().push(spec.clone());
            let id = {
                let mut n = self.next_id.lock().unwrap();
                *n += 1;
                format!("fake{n}")
            };
            self.containers.lock().unwrap().push((
                id.clone(),
                FakeContainer {
                    name: spec.name.clone(),
                    state: "created".to_string(),
                    image: spec.image.clone(),
                    labels: spec.labels.clone(),
                },
            ));
            Ok(id)
        }

        async fn start(&self, id: &str) -> Result<(), DockerError> {
            self.ensure_up()?;
            self.set_state(id, "running");
            Ok(())
        }

        async fn health(&self, _id: &str) -> Result<ContainerHealth, DockerError> {
            self.ensure_up()?;
            if let Some(error) = self.health_error.lock().unwrap().take() {
                return Err(error);
            }
            Ok(ContainerHealth {
                status: "none".into(),
                detail: None,
            })
        }

        async fn stop(&self, id: &str, _timeout_secs: i32) -> Result<(), DockerError> {
            self.ensure_up()?;
            self.set_state(id, "exited");
            Ok(())
        }

        async fn restart(&self, id: &str, _timeout_secs: i32) -> Result<(), DockerError> {
            self.ensure_up()?;
            self.set_state(id, "running");
            Ok(())
        }

        async fn rename(&self, id: &str, name: &str) -> Result<(), DockerError> {
            self.ensure_up()?;
            let mut containers = self.containers.lock().unwrap();
            if containers
                .iter()
                .any(|(container_id, container)| container_id != id && container.name == name)
            {
                return Err(DockerError::Conflict);
            }
            let Some((_, container)) = containers
                .iter_mut()
                .find(|(container_id, _)| container_id == id)
            else {
                return Err(DockerError::NotFound);
            };
            if container.name == name {
                return Ok(());
            }
            container.name = name.to_string();
            self.renamed
                .lock()
                .unwrap()
                .push((id.to_string(), name.to_string()));
            Ok(())
        }

        async fn remove(&self, id: &str) -> Result<(), DockerError> {
            self.ensure_up()?;
            self.removed.lock().unwrap().push(id.to_string());
            self.containers.lock().unwrap().retain(|(cid, _)| cid != id);
            Ok(())
        }

        async fn remove_image(&self, _image: &str) -> Result<(), DockerError> {
            Ok(())
        }
    }
}

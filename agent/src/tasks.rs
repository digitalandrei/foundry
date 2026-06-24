//! Task loop + Docker executors (docs/ARCHITECTURE.md § Agent Tasks;
//! skill: docker-engine-api). Sequential by design: one task at a time
//! per server, every executor idempotent (re-delivery after a crash is
//! normal). Only containers labeled foundry.managed=true are ever
//! touched; volume removal is hard-scoped under /storage/containers/.
//!
//! Docker is reached only through the `DockerEngine` seam (crate::docker),
//! so this executor — the bug-dense deploy/replace/adopt orchestration —
//! is unit-tested against an in-memory `FakeEngine`, no daemon required.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use foundry_shared::dto::{
    ContainerTarget, DeployPayload, RegistryAuth, TaskEnvelope, TaskPayload, TaskProgressReport,
    TaskResultReport, VolumeTarget,
};
use foundry_shared::{DeploymentState, TaskId, TaskType};

use crate::config::AgentConfig;
use crate::docker::{
    BindSpec, BollardEngine, ContainerSpec, DockerEngine, PortSpec, PullSink, RegistryCreds,
};

const VOLUME_ROOT: &str = "/storage/containers/";

/// Best-effort live progress for DEPLOY tasks (`/agent/tasks/progress`):
/// state changes post immediately, detail refreshes are throttled. A
/// failed post is logged and dropped — progress must never affect the
/// task outcome.
struct ProgressReporter<'a> {
    client: &'a reqwest::Client,
    config: &'a AgentConfig,
    url: String,
    task_id: TaskId,
    last_sent: std::time::Instant,
}

impl<'a> ProgressReporter<'a> {
    fn new(client: &'a reqwest::Client, config: &'a AgentConfig, task_id: TaskId) -> Self {
        let base = config.controller_url.trim_end_matches('/');
        Self {
            client,
            config,
            url: format!("{base}/agent/tasks/progress"),
            task_id,
            last_sent: std::time::Instant::now() - Duration::from_secs(60),
        }
    }

    /// Immediate post (state changes).
    async fn stage(&mut self, state: DeploymentState, detail: &str) {
        self.post(state, detail).await;
    }

    /// Throttled post (pull-progress refreshes, ≥2s apart).
    async fn tick(&mut self, state: DeploymentState, detail: &str) {
        if self.last_sent.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.post(state, detail).await;
    }

    async fn post(&mut self, state: DeploymentState, detail: &str) {
        self.last_sent = std::time::Instant::now();
        let report = TaskProgressReport {
            task_id: self.task_id,
            state,
            detail: Some(detail.chars().take(256).collect()),
        };
        let result = self
            .client
            .post(&self.url)
            .header("x-foundry-agent-id", &self.config.agent_id)
            .bearer_auth(&self.config.agent_secret)
            .timeout(Duration::from_secs(5))
            .json(&report)
            .send()
            .await;
        if let Err(err) = result {
            tracing::debug!(%err, "progress post failed (non-fatal)");
        }
    }
}

/// The engine streams aggregated pull-progress lines through here; each
/// becomes a throttled `PullingImage` detail refresh.
#[async_trait]
impl PullSink for ProgressReporter<'_> {
    async fn progress(&mut self, line: &str) {
        self.tick(DeploymentState::PullingImage, line).await;
    }
}

pub async fn run_loop(
    client: &reqwest::Client,
    config: &AgentConfig,
    docker: Option<bollard::Docker>,
) {
    // One Docker connection, shared with the telemetry loops (built in
    // main). `None` only when connect_local() itself failed — rare config
    // breakage a restart fixes — so disable the executor until then.
    let Some(docker) = docker else {
        tracing::error!("Docker unavailable — task executor disabled until restart");
        crate::shutdown_signal().await;
        return;
    };
    let engine = BollardEngine::new(docker);

    let base = config.controller_url.trim_end_matches('/');
    let next_url = format!("{base}/agent/tasks/next");
    let result_url = format!("{base}/agent/tasks/result");

    loop {
        tokio::select! {
            _ = crate::shutdown_signal() => break,
            envelope = poll_next(client, config, &next_url) => {
                let Some(envelope) = envelope else { continue };
                let task_id = envelope.id;
                tracing::info!(task = %task_id, task_type = %envelope.task_type, "executing task");
                let report = execute(&engine, client, config, envelope).await;
                tracing::info!(task = %task_id, success = report.success,
                    error = report.error.as_deref().unwrap_or(""), "task finished");
                report_result(client, config, &result_url, &report).await;
            }
        }
    }
}

/// One long-poll round; None on idle/error (caller just loops).
async fn poll_next(
    client: &reqwest::Client,
    config: &AgentConfig,
    url: &str,
) -> Option<TaskEnvelope> {
    let resp = client
        .get(url)
        .header("x-foundry-agent-id", &config.agent_id)
        .bearer_auth(&config.agent_secret)
        .timeout(Duration::from_secs(40))
        .send()
        .await;
    match resp {
        Ok(r) if r.status() == reqwest::StatusCode::OK => match r.json::<TaskEnvelope>().await {
            Ok(envelope) => Some(envelope),
            Err(err) => {
                tracing::warn!(%err, "task envelope parse failed");
                None
            }
        },
        Ok(r) if r.status() == reqwest::StatusCode::NO_CONTENT => None,
        Ok(r) => {
            tracing::warn!(status = %r.status(), "task poll rejected");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
        Err(err) => {
            tracing::debug!(%err, "task poll failed (controller unreachable)");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
    }
}

async fn report_result(
    client: &reqwest::Client,
    config: &AgentConfig,
    url: &str,
    report: &TaskResultReport,
) {
    // Results matter: retry a few times before giving up (the
    // controller re-dispatches lost tasks anyway).
    for attempt in 0..5 {
        let resp = client
            .post(url)
            .header("x-foundry-agent-id", &config.agent_id)
            .bearer_auth(&config.agent_secret)
            .json(report)
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => return,
            Ok(r) => tracing::warn!(status = %r.status(), attempt, "result rejected"),
            Err(err) => tracing::warn!(%err, attempt, "result upload failed"),
        }
        tokio::time::sleep(Duration::from_secs(3 * (attempt + 1))).await;
    }
    tracing::error!(task = %report.task_id, "giving up on result upload");
}

async fn execute(
    engine: &dyn DockerEngine,
    client: &reqwest::Client,
    config: &AgentConfig,
    envelope: TaskEnvelope,
) -> TaskResultReport {
    let task_id = envelope.id;
    let outcome = match (envelope.task_type, envelope.payload) {
        (TaskType::DeployContainer, TaskPayload::Deploy(p)) => {
            let mut progress = ProgressReporter::new(client, config, task_id);
            deploy(engine, *p, &mut progress).await
        }
        (TaskType::StopContainer, TaskPayload::Container(t)) => stop(engine, t).await.map(|_| None),
        (TaskType::RestartContainer, TaskPayload::Container(t)) => {
            restart(engine, t).await.map(|_| None)
        }
        (TaskType::RemoveContainer, TaskPayload::Container(t)) => {
            remove(engine, t).await.map(|_| None)
        }
        (TaskType::RemoveVolume, TaskPayload::Volume(v)) => remove_volume(v).await.map(|_| None),
        (tt, _) => Err(format!("unsupported task/payload combination: {tt}")),
    };
    match outcome {
        Ok(container_id) => TaskResultReport {
            task_id,
            success: true,
            container_id,
            error: None,
        },
        Err(error) => TaskResultReport {
            task_id,
            success: false,
            container_id: None,
            error: Some(error.chars().take(1000).collect()),
        },
    }
}

/// Find the managed container for a deployment (by label, never name).
async fn find_managed(
    engine: &dyn DockerEngine,
    deployment_id: &str,
) -> Result<Option<(String, String)>, String> {
    let list = engine.list().await.map_err(|e| e.to_string())?;
    Ok(list.into_iter().find_map(|c| {
        if c.labels.get("foundry.managed").map(String::as_str) != Some("true") {
            return None;
        }
        if c.labels.get("foundry.deployment_id").map(String::as_str) != Some(deployment_id) {
            return None;
        }
        Some((c.id, c.state))
    }))
}

/// Find any container by (short) docker id — the adopted-container path,
/// which deliberately ignores the `foundry.managed` label gate (the
/// container was created outside Foundry; the controller authorised this).
async fn find_by_id(
    engine: &dyn DockerEngine,
    short_id: &str,
) -> Result<Option<(String, String)>, String> {
    let list = engine.list().await.map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .find(|c| c.id.starts_with(short_id))
        .map(|c| (c.id, c.state)))
}

/// Resolve a lifecycle target's container: by docker id for an adopted
/// container, else by the managed label for a Foundry deployment.
async fn resolve_target(
    engine: &dyn DockerEngine,
    t: &ContainerTarget,
) -> Result<Option<(String, String)>, String> {
    match &t.container_id {
        Some(cid) => find_by_id(engine, cid).await,
        None => find_managed(engine, &t.deployment_id.to_string()).await,
    }
}

/// Build the container spec from a deploy payload. The bug-dense bits —
/// device-uuid fallback (older controller), comma-joined slot-id label,
/// group label — live here so they're tested without Docker.
fn container_spec(p: &DeployPayload) -> ContainerSpec {
    // All NVML device UUIDs for this deployment: prefer the plural field
    // (1 for an individual deploy, N for a group); fall back to the
    // singular for a payload queued by a one-release-older controller.
    let device_uuids: Vec<String> = if p.gpu_device_uuids.is_empty() {
        vec![p.gpu_device_uuid.clone()]
    } else {
        p.gpu_device_uuids.clone()
    };
    // Member slot ids for the comma-joined label; fall back to the
    // primary slot for an older controller's payload.
    let slot_ids_label = if p.slot_ids.is_empty() {
        p.slot_id.to_string()
    } else {
        p.slot_ids
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };

    // gpu_uuid + slot make GPU assignment visible host-side:
    // docker ps --format '{{.Names}} {{.Label "foundry.gpu_uuid"}}'
    let mut labels = BTreeMap::from([
        ("foundry.managed".to_string(), "true".to_string()),
        (
            "foundry.deployment_id".to_string(),
            p.deployment_id.to_string(),
        ),
        ("foundry.slot_id".to_string(), p.slot_id.to_string()),
        ("foundry.slot_ids".to_string(), slot_ids_label),
        ("foundry.slot".to_string(), p.slot_name.clone()),
        ("foundry.gpu_uuid".to_string(), p.gpu_device_uuid.clone()),
    ]);
    // Group deploys carry the group id so a host-side `docker ps` reveals
    // which container spans which group (docs/ARCHITECTURE.md § Labels).
    if let Some(group_id) = &p.gpu_group_id {
        labels.insert("foundry.group_id".to_string(), group_id.to_string());
    }

    ContainerSpec {
        name: p.container_name.clone(),
        image: p.image_ref.clone(),
        env: p.env.clone(),
        labels,
        ports: p
            .ports
            .iter()
            .map(|port| PortSpec {
                container_port: port.container_port,
                host_port: port.host_port,
                protocol: port.protocol.clone(),
            })
            .collect(),
        binds: p
            .volumes
            .iter()
            .map(|v| BindSpec {
                host_path: v.host_path.clone(),
                container_path: v.container_path.clone(),
                read_only: v.read_only,
            })
            .collect(),
        // Operator-set Docker memory cap (slider; controller-clamped to
        // 32–256 GB). None → unlimited. Bytes = MB × 1024².
        memory_bytes: p.mem_limit_mb.map(|mb| i64::from(mb) * 1024 * 1024),
        device_uuids,
    }
}

async fn deploy(
    engine: &dyn DockerEngine,
    p: DeployPayload,
    progress: &mut ProgressReporter<'_>,
) -> Result<Option<String>, String> {
    let deployment_id = p.deployment_id.to_string();

    // Idempotency: a previous attempt may have gotten partway.
    if let Some((existing, state)) = find_managed(engine, &deployment_id).await? {
        if state == "running" {
            // Re-delivery after a crash: make sure the vhosts exist too
            // (no-op reload when the conf is already in place).
            crate::vhost::apply(&deployment_id, &crate::vhost::web_ports(&p.ports)).await?;
            return Ok(Some(existing));
        }
        let _ = engine.remove(&existing).await;
    }

    // Persistent volume directories (hard-scoped). create_dir_all builds
    // the full per-user/per-volume path; the only realistic failure is
    // the systemd sandbox (the volume root must exist, be owned by the
    // service user, and sit in the unit's ReadWritePaths) — point there.
    for v in &p.volumes {
        if !v.host_path.starts_with(VOLUME_ROOT) || v.host_path.contains("..") {
            return Err(format!(
                "refusing mount outside {VOLUME_ROOT}: {}",
                v.host_path
            ));
        }
        tokio::fs::create_dir_all(&v.host_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem
                || e.kind() == std::io::ErrorKind::PermissionDenied
            {
                format!(
                    "creating {} failed: {e} — the volume root {VOLUME_ROOT} is not writable by \
                     the agent; run `sudo foundry-agent --setup-apps` on this server",
                    v.host_path
                )
            } else {
                format!("creating {} failed: {e}", v.host_path)
            }
        })?;
    }

    // Pull (credential stays in memory; never logged).
    let creds = p.registry_auth.as_ref().map(|auth| match auth {
        RegistryAuth::RegistryToken { token } => RegistryCreds::Token(token.clone()),
        RegistryAuth::UserPassword { username, password } => RegistryCreds::UserPassword {
            username: username.clone(),
            password: password.clone(),
        },
    });
    progress
        .stage(
            DeploymentState::PullingImage,
            "pulling: contacting registry",
        )
        .await;
    engine
        .pull(&p.image_ref, creds, progress)
        .await
        .map_err(|e| e.to_string())?;

    progress
        .stage(DeploymentState::CreatingContainer, "creating container")
        .await;
    let spec = container_spec(&p);
    let id = engine.create(&spec).await.map_err(|e| match e {
        crate::docker::DockerError::Conflict => format!(
            "container name {:?} is already used by a container not managed by Foundry — pick \
             another name or remove that container on the host",
            p.container_name
        ),
        // `e`'s Display already carries the "container create failed: …"
        // context from the adapter — don't prefix it a second time.
        other => other.to_string(),
    })?;

    progress
        .stage(DeploymentState::Starting, "starting container")
        .await;
    if let Err(e) = engine.start(&id).await {
        // A created-but-unstarted container would otherwise hold the
        // name and clutter the host; the deploy is failing anyway, so
        // remove it — a failed deploy leaves nothing behind.
        let _ = engine.remove(&id).await;
        return Err(format!("container start failed: {e}"));
    }

    // HTTP/S app publishing: the URL is part of the deployment contract
    // — a container nobody can reach is a failed deploy, so tear it
    // down rather than leave an orphan holding the slot and its ports.
    let web = crate::vhost::web_ports(&p.ports);
    if !web.is_empty() {
        progress
            .stage(DeploymentState::Starting, "publishing vhost (nginx)")
            .await;
        if let Err(err) = crate::vhost::apply(&deployment_id, &web).await {
            let _ = engine.remove(&id).await;
            return Err(format!("vhost publish failed: {err}"));
        }
    }

    Ok(Some(id))
}

/// The image a container was created from — captured before the container
/// is removed so its image can be reclaimed afterwards. Best-effort.
async fn container_image(engine: &dyn DockerEngine, id: &str) -> Option<String> {
    engine.inspect_image(id).await.ok().flatten()
}

/// Reclaim an image best-effort. A shared image (another container still
/// references it) and an already-deleted image are both non-errors: a
/// re-delivered teardown must stay idempotent, and we must never strand a
/// sibling deployment that needs the same layers.
async fn reclaim_image(engine: &dyn DockerEngine, image: &str) {
    if let Err(e) = engine.remove_image(image).await {
        tracing::debug!(%image, error = %e, "image not reclaimed (shared or already gone)");
    }
}

async fn stop(engine: &dyn DockerEngine, t: ContainerTarget) -> Result<(), String> {
    let Some((id, state)) = resolve_target(engine, &t).await? else {
        return Ok(()); // already gone — idempotent success
    };
    // Capture the image while the container still exists so we can reclaim
    // it once the container is gone.
    let image = container_image(engine, &id).await;
    if state == "running" {
        engine.stop(&id, 30).await.map_err(|e| e.to_string())?;
    }
    // Don't leave a stopped container lingering in `docker ps -a`. Restart
    // re-deploys from the stored spec (controller `enqueue_restart`), so
    // nothing here needs to survive — then drop the image so it doesn't
    // pile up in `docker images`.
    engine.remove(&id).await.map_err(|e| e.to_string())?;
    if let Some(image) = image {
        reclaim_image(engine, &image).await;
    }
    Ok(())
}

async fn restart(engine: &dyn DockerEngine, t: ContainerTarget) -> Result<(), String> {
    let Some((id, state)) = resolve_target(engine, &t).await? else {
        return Err("managed container not found on host".into());
    };
    if state == "running" {
        engine.restart(&id, 30).await.map_err(|e| e.to_string())
    } else {
        engine.start(&id).await.map_err(|e| e.to_string())
    }
}

async fn remove(engine: &dyn DockerEngine, t: ContainerTarget) -> Result<(), String> {
    // Vhost first (drain traffic before the upstream disappears); also
    // runs on re-delivery when the container is already gone.
    crate::vhost::remove(&t.deployment_id.to_string()).await?;
    let Some((id, _)) = resolve_target(engine, &t).await? else {
        return Ok(()); // already gone — idempotent success
    };
    let image = container_image(engine, &id).await;
    engine.remove(&id).await.map_err(|e| e.to_string())?;
    if let Some(image) = image {
        reclaim_image(engine, &image).await;
    }
    Ok(())
}

async fn remove_volume(v: VolumeTarget) -> Result<(), String> {
    // Hard scope: absolute, under the volume root, no traversal.
    if !v.path.starts_with(VOLUME_ROOT)
        || v.path.contains("..")
        || v.path.len() < VOLUME_ROOT.len() + 1
    {
        return Err(format!("refusing to remove path outside {VOLUME_ROOT}"));
    }
    match tokio::fs::remove_dir_all(&v.path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("volume removal failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::fake::FakeEngine;
    use crate::docker::DockerError;
    use foundry_shared::dto::PortBinding;

    fn cfg() -> AgentConfig {
        // Unreachable controller: progress posts fail fast and are
        // swallowed (best-effort), so the executor logic is what's tested.
        AgentConfig {
            controller_url: "http://127.0.0.1:1".into(),
            agent_id: "agent".into(),
            agent_secret: "secret".into(),
            server_name: None,
            poll_interval_secs: 15,
        }
    }

    fn payload() -> DeployPayload {
        DeployPayload {
            deployment_id: foundry_shared::DeploymentId::new(),
            image_ref: "registry.example/app:1".into(),
            container_name: "app-1".into(),
            gpu_device_uuid: "GPU-aaaa".into(),
            gpu_device_uuids: vec![],
            slot_id: foundry_shared::SlotId::new(),
            slot_ids: vec![],
            gpu_group_id: None,
            slot_name: "0".into(),
            // A non-web TCP port: web_ports() ignores it, so deploy never
            // shells out to nginx during the test.
            ports: vec![PortBinding {
                container_port: 8000,
                host_port: 18000,
                protocol: "tcp".into(),
                kind: foundry_shared::PortKind::default(),
                hostname: None,
            }],
            env: vec![("KEY".into(), "VAL".into())],
            volumes: vec![],
            registry_auth: None,
            mem_limit_mb: Some(1024),
        }
    }

    // --- pure spec construction ---

    #[test]
    fn spec_falls_back_to_singular_gpu_uuid() {
        let p = payload();
        let spec = container_spec(&p);
        assert_eq!(spec.device_uuids, vec!["GPU-aaaa".to_string()]);
        assert_eq!(spec.labels["foundry.managed"], "true");
        assert_eq!(spec.labels["foundry.gpu_uuid"], "GPU-aaaa");
        assert_eq!(spec.memory_bytes, Some(1024 * 1024 * 1024));
        assert!(!spec.labels.contains_key("foundry.group_id"));
    }

    #[test]
    fn spec_uses_plural_gpu_uuids_and_group_label() {
        let mut p = payload();
        p.gpu_device_uuids = vec!["GPU-a".into(), "GPU-b".into()];
        p.gpu_group_id = Some(foundry_shared::GpuGroupId::new());
        let spec = container_spec(&p);
        assert_eq!(
            spec.device_uuids,
            vec!["GPU-a".to_string(), "GPU-b".to_string()]
        );
        assert!(spec.labels.contains_key("foundry.group_id"));
    }

    // --- deploy orchestration vs the in-memory engine ---

    #[tokio::test]
    async fn deploy_creates_and_starts_a_managed_container() {
        let engine = FakeEngine::new();
        let client = reqwest::Client::new();
        let config = cfg();
        let mut progress = ProgressReporter::new(&client, &config, TaskId::new());

        let id = deploy(&engine, payload(), &mut progress)
            .await
            .expect("deploy ok")
            .expect("returns container id");

        // Exactly one container created, carrying the managed label.
        let created = engine.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].labels["foundry.managed"], "true");
        drop(created);
        assert!(engine.ids().contains(&id));
    }

    #[tokio::test]
    async fn deploy_recreates_a_stale_stopped_container() {
        let dep = foundry_shared::DeploymentId::new();
        let engine = FakeEngine::new().with_managed("old", "exited", &dep.to_string());
        let client = reqwest::Client::new();
        let config = cfg();
        let mut progress = ProgressReporter::new(&client, &config, TaskId::new());
        let mut p = payload();
        p.deployment_id = dep;

        deploy(&engine, p, &mut progress).await.expect("deploy ok");

        // The stale container was removed and a fresh one created.
        assert!(engine.removed.lock().unwrap().contains(&"old".to_string()));
        assert!(!engine.ids().contains(&"old".to_string()));
        assert_eq!(engine.created.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn deploy_surfaces_unauthorized_pull() {
        let engine = FakeEngine::new().fail_pull(DockerError::ImagePull {
            message: "denied".into(),
            unauthorized: true,
        });
        let client = reqwest::Client::new();
        let config = cfg();
        let mut progress = ProgressReporter::new(&client, &config, TaskId::new());

        let err = deploy(&engine, payload(), &mut progress)
            .await
            .expect_err("pull should fail");
        assert!(err.contains("image pull failed"), "got: {err}");
        // Nothing was created when the pull failed.
        assert!(engine.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn deploy_maps_create_conflict_to_operator_message() {
        let engine = FakeEngine::new().conflict_on_create();
        let client = reqwest::Client::new();
        let config = cfg();
        let mut progress = ProgressReporter::new(&client, &config, TaskId::new());

        let err = deploy(&engine, payload(), &mut progress)
            .await
            .expect_err("create should conflict");
        assert!(
            err.contains("already used by a container not managed by Foundry"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn stop_removes_a_running_managed_container() {
        let dep = foundry_shared::DeploymentId::new();
        let engine = FakeEngine::new().with_managed("c1", "running", &dep.to_string());

        let t = ContainerTarget {
            deployment_id: dep,
            container_id: None,
        };
        stop(&engine, t).await.expect("stop ok");

        assert!(engine.removed.lock().unwrap().contains(&"c1".to_string()));
        assert!(!engine.ids().contains(&"c1".to_string()));
    }

    #[tokio::test]
    async fn stop_is_idempotent_when_already_gone() {
        let engine = FakeEngine::new();
        let t = ContainerTarget {
            deployment_id: foundry_shared::DeploymentId::new(),
            container_id: None,
        };
        // No matching container — still a success.
        stop(&engine, t).await.expect("idempotent stop");
    }
}

//! Task loop + Docker executors (docs/ARCHITECTURE.md § Agent Tasks;
//! skill: docker-engine-api). Sequential by design: one task at a time
//! per server, every executor idempotent (re-delivery after a crash is
//! normal). Only containers labeled foundry.managed=true are ever
//! touched; volume removal is hard-scoped under /storage/containers/.

use std::collections::HashMap;
use std::time::Duration;

use bollard::auth::DockerCredentials;
use bollard::models::{ContainerCreateBody, DeviceRequest, HostConfig, PortBinding};
use bollard::Docker;
use foundry_shared::dto::{
    ContainerTarget, DeployPayload, RegistryAuth, TaskEnvelope, TaskPayload, TaskResultReport,
    VolumeTarget,
};
use foundry_shared::TaskType;
use futures_util::StreamExt;

use crate::config::AgentConfig;

const VOLUME_ROOT: &str = "/storage/containers/";

pub async fn run_loop(client: &reqwest::Client, config: &AgentConfig) {
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
                let report = execute(envelope).await;
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

async fn execute(envelope: TaskEnvelope) -> TaskResultReport {
    let task_id = envelope.id;
    let outcome = match (envelope.task_type, envelope.payload) {
        (TaskType::DeployContainer, TaskPayload::Deploy(p)) => deploy(*p).await,
        (TaskType::StopContainer, TaskPayload::Container(t)) => stop(t).await.map(|_| None),
        (TaskType::RestartContainer, TaskPayload::Container(t)) => restart(t).await.map(|_| None),
        (TaskType::RemoveContainer, TaskPayload::Container(t)) => remove(t).await.map(|_| None),
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

fn docker() -> Result<Docker, String> {
    Docker::connect_with_local_defaults().map_err(|e| format!("docker unavailable: {e}"))
}

/// Find the managed container for a deployment (by label, never name).
async fn find_managed(
    docker: &Docker,
    deployment_id: &str,
) -> Result<Option<(String, String)>, String> {
    let list = docker
        .list_containers(Some(bollard::query_parameters::ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await
        .map_err(|e| format!("container listing failed: {e}"))?;
    Ok(list.into_iter().find_map(|c| {
        let labels = c.labels.as_ref()?;
        if labels.get("foundry.managed").map(String::as_str) != Some("true") {
            return None;
        }
        if labels.get("foundry.deployment_id").map(String::as_str) != Some(deployment_id) {
            return None;
        }
        Some((
            c.id.unwrap_or_default(),
            c.state
                .map(|s| format!("{s:?}").to_lowercase())
                .unwrap_or_default(),
        ))
    }))
}

async fn deploy(p: DeployPayload) -> Result<Option<String>, String> {
    let docker = docker()?;
    let deployment_id = p.deployment_id.to_string();

    // Idempotency: a previous attempt may have gotten partway.
    if let Some((existing, state)) = find_managed(&docker, &deployment_id).await? {
        if state == "running" {
            // Re-delivery after a crash: make sure the vhosts exist too
            // (no-op reload when the conf is already in place).
            crate::vhost::apply(&deployment_id, &crate::vhost::web_ports(&p.ports)).await?;
            return Ok(Some(existing));
        }
        let _ = docker
            .remove_container(
                &existing,
                Some(bollard::query_parameters::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
    }

    // Persistent volume directories (hard-scoped).
    for v in &p.volumes {
        if !v.host_path.starts_with(VOLUME_ROOT) || v.host_path.contains("..") {
            return Err(format!(
                "refusing mount outside {VOLUME_ROOT}: {}",
                v.host_path
            ));
        }
        tokio::fs::create_dir_all(&v.host_path)
            .await
            .map_err(|e| format!("creating {} failed: {e}", v.host_path))?;
    }

    // Pull (credential stays in memory; never logged).
    let credentials = p.registry_auth.as_ref().map(|auth| match auth {
        RegistryAuth::RegistryToken { token } => DockerCredentials {
            registrytoken: Some(token.clone()),
            ..Default::default()
        },
        RegistryAuth::UserPassword { username, password } => DockerCredentials {
            username: Some(username.clone()),
            password: Some(password.clone()),
            ..Default::default()
        },
    });
    let mut pull = docker.create_image(
        Some(bollard::query_parameters::CreateImageOptions {
            from_image: Some(p.image_ref.clone()),
            ..Default::default()
        }),
        None,
        credentials,
    );
    while let Some(msg) = pull.next().await {
        let info = msg.map_err(|e| format!("image pull failed: {e}"))?;
        // Auth/missing-tag failures arrive as 200-stream messages with
        // an embedded error (review finding) — surface them.
        if let Some(error) = info.error {
            return Err(format!("image pull failed: {error}"));
        }
    }

    // Create with labels, GPU device, ports, env, mounts.
    // gpu_uuid + slot make GPU assignment visible host-side:
    // docker ps --format '{{.Names}} {{.Label \"foundry.gpu_uuid\"}}'
    let labels = HashMap::from([
        ("foundry.managed".to_string(), "true".to_string()),
        ("foundry.deployment_id".to_string(), deployment_id.clone()),
        ("foundry.slot_id".to_string(), p.slot_id.to_string()),
        ("foundry.slot".to_string(), p.slot_name.clone()),
        ("foundry.gpu_uuid".to_string(), p.gpu_device_uuid.clone()),
    ]);
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
    for port in &p.ports {
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
    let binds: Vec<String> = p
        .volumes
        .iter()
        .map(|v| {
            format!(
                "{}:{}{}",
                v.host_path,
                v.container_path,
                if v.read_only { ":ro" } else { "" }
            )
        })
        .collect();

    let body = ContainerCreateBody {
        image: Some(p.image_ref.clone()),
        env: Some(p.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
        labels: Some(labels),
        exposed_ports: Some(exposed),
        host_config: Some(HostConfig {
            port_bindings: Some(port_bindings),
            binds: (!binds.is_empty()).then_some(binds),
            // driver omitted = daemon auto-selects the GPU driver
            // (what `docker run --gpus device=…` sends).
            device_requests: Some(vec![DeviceRequest {
                driver: None,
                count: None,
                device_ids: Some(vec![p.gpu_device_uuid.clone()]),
                capabilities: Some(vec![vec!["gpu".to_string()]]),
                options: None,
            }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let created = docker
        .create_container(
            Some(bollard::query_parameters::CreateContainerOptions {
                name: Some(p.container_name.clone()),
                ..Default::default()
            }),
            body,
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("409") || msg.to_lowercase().contains("conflict") {
                format!(
                    "container name {:?} is already used by a container not managed by \
                     Foundry — pick another name or remove that container on the host",
                    p.container_name
                )
            } else {
                format!("container create failed: {msg}")
            }
        })?;

    docker
        .start_container(
            &created.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await
        .map_err(|e| format!("container start failed: {e}"))?;

    // HTTP/S app publishing: the URL is part of the deployment contract
    // — a container nobody can reach is a failed deploy, so tear it
    // down rather than leave an orphan holding the slot and its ports.
    let web = crate::vhost::web_ports(&p.ports);
    if !web.is_empty() {
        if let Err(err) = crate::vhost::apply(&deployment_id, &web).await {
            let _ = docker
                .remove_container(
                    &created.id,
                    Some(bollard::query_parameters::RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
            return Err(format!("vhost publish failed: {err}"));
        }
    }

    Ok(Some(created.id))
}

async fn stop(t: ContainerTarget) -> Result<(), String> {
    let docker = docker()?;
    let Some((id, state)) = find_managed(&docker, &t.deployment_id.to_string()).await? else {
        return Ok(()); // already gone — idempotent success
    };
    if state != "running" {
        return Ok(());
    }
    docker
        .stop_container(
            &id,
            Some(bollard::query_parameters::StopContainerOptions {
                t: Some(30),
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("container stop failed: {e}"))
}

async fn restart(t: ContainerTarget) -> Result<(), String> {
    let docker = docker()?;
    let Some((id, state)) = find_managed(&docker, &t.deployment_id.to_string()).await? else {
        return Err("managed container not found on host".into());
    };
    if state == "running" {
        docker
            .restart_container(
                &id,
                Some(bollard::query_parameters::RestartContainerOptions {
                    t: Some(30),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| format!("container restart failed: {e}"))
    } else {
        docker
            .start_container(
                &id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .map_err(|e| format!("container start failed: {e}"))
    }
}

async fn remove(t: ContainerTarget) -> Result<(), String> {
    let docker = docker()?;
    // Vhost first (drain traffic before the upstream disappears); also
    // runs on re-delivery when the container is already gone.
    crate::vhost::remove(&t.deployment_id.to_string()).await?;
    let Some((id, _)) = find_managed(&docker, &t.deployment_id.to_string()).await? else {
        return Ok(()); // already gone — idempotent success
    };
    docker
        .remove_container(
            &id,
            Some(bollard::query_parameters::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("container remove failed: {e}"))
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

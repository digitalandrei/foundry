//! Foundry GPU-server agent. Pull-only: every connection is outbound
//! HTTPS to the controller (docs/ARCHITECTURE.md § Pull-Based Agent
//! Model).
//!
//! Modes:
//! - `foundry-agent` — run the heartbeat loop (config required)
//! - `foundry-agent --register --url <controller> --token <token>
//!    [--force]` — enroll this server and install the service
//! - `foundry-agent --version`

mod config;
mod docker;
mod file_system;
mod files;
mod inventory;
mod logs;
mod metrics;
mod register;
mod shell;
mod tasks;
mod vhost;

use std::error::Error;
use std::time::Duration;

use foundry_shared::dto::HeartbeatRequest;

const USAGE: &str = "\
usage: foundry-agent                                   run (enrolled servers)
       foundry-agent --register --url <controller> --token <token> [--force]
       foundry-agent --register --url <controller> --fleet-token <key> [--force]
       foundry-agent --setup-apps                      (re)install binary + nginx app publishing
       foundry-agent --version";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("foundry-agent {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--register") {
        let get = |flag: &str| -> Option<String> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let Some(url) = get("--url") else {
            eprintln!("{USAGE}");
            return Err("--register requires --url".into());
        };
        // Exactly one of --token (server-bound, single-use) or
        // --fleet-token (reusable fleet key; auto-creates the server).
        let (token, fleet) = match (get("--token"), get("--fleet-token")) {
            (Some(t), None) => (t, false),
            (None, Some(t)) => (t, true),
            _ => {
                eprintln!("{USAGE}");
                return Err("--register requires exactly one of --token or --fleet-token".into());
            }
        };
        return register::run(register::RegisterArgs {
            url,
            token,
            fleet,
            force: args.iter().any(|a| a == "--force"),
        })
        .await;
    }
    if args.iter().any(|a| a == "--setup-apps") {
        return register::setup_apps_standalone();
    }
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return Err(format!("unknown arguments: {args:?}").into());
    }

    run_agent().await
}

async fn run_agent() -> Result<(), Box<dyn Error>> {
    let path = config::config_path();
    let config = config::load(&path)?;
    tracing::info!(
        controller = %config.controller_url,
        server = config.server_name.as_deref().unwrap_or("?"),
        version = env!("CARGO_PKG_VERSION"),
        "foundry-agent starting"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // One Docker client for the whole process — the task executor and the
    // inventory/metrics/logs/shell loops share it instead of reconnecting
    // every cycle (the per-cycle reconnect was the FD-churn shape behind
    // the NVML single-handle fix). connect_local() doesn't dial, so a
    // daemon that's down now (or comes up later) is handled per request;
    // only a malformed Docker config yields None, disabling Docker
    // features until a restart.
    let docker = match docker::connect_local() {
        Ok(d) => {
            tracing::info!("docker: client ready (shared across loops)");
            Some(d)
        }
        Err(err) => {
            tracing::warn!(%err, "docker config unusable — container telemetry, tasks, and shell disabled until restart");
            None
        }
    };

    // Heartbeat/inventory/metrics and the task loop run concurrently;
    // each exits on SIGTERM/ctrl-c.
    tokio::join!(
        heartbeat_loop(&client, &config, docker.clone()),
        tasks::run_loop(&client, &config, docker.clone()),
        shell::run_loop(&client, &config, docker.clone()),
        files::run_loop(&client, &config)
    );
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Heartbeat + periodic inventory until SIGTERM/ctrl-c. Logs only on
/// state *transitions* (reachable/unreachable) to keep journald quiet.
/// TLS is always verified — no insecure escape hatch (docs/SECURITY.md).
async fn heartbeat_loop(
    client: &reqwest::Client,
    config: &config::AgentConfig,
    docker: Option<bollard::Docker>,
) {
    let base = config.controller_url.trim_end_matches('/');
    let url = format!("{base}/agent/heartbeat");
    let inventory_url = format!("{base}/agent/inventory");
    let mut interval = tokio::time::interval(Duration::from_secs(config.poll_interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Full snapshot at start, then every minute (docs/GPU-MIG.md).
    let mut inventory_interval = tokio::time::interval(Duration::from_secs(60));
    inventory_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // ONE NVML handle for the whole process, shared by the inventory and
    // metrics ticks. Re-initializing NVML per collection cycle leaks file
    // descriptors against the NVIDIA driver (0.45–0.47 regression: the
    // agent exhausted FDs after ~5h — "Too many open files", then
    // NVML/nvidia-smi/sockets all failed), so we init exactly once and
    // never re-init. A held handle does not observe a MIG layout
    // enabled/reshaped after startup, so that change is picked up only on
    // the next agent restart (documented, docs/GPU-MIG.md).
    let nvml = match nvml_wrapper::Nvml::init() {
        Ok(n) => Some(n),
        Err(err) => {
            tracing::info!(%err, "NVML unavailable — GPU metrics disabled");
            None
        }
    };
    // Telemetry every 30s (plans/phase-05.md § Telemetry extension).
    let metrics_url = format!("{base}/agent/metrics");
    let mut collector = metrics::MetricsCollector::new();
    let mut metrics_interval = tokio::time::interval(Duration::from_secs(30));
    metrics_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Container logs every 10s (incremental — only new output ships).
    let logs_url = format!("{base}/agent/logs");
    let mut log_collector = logs::LogCollector::new();
    let mut logs_interval = tokio::time::interval(Duration::from_secs(10));
    logs_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut healthy: Option<bool> = None;
    // Adopted (externally-created) containers the controller tracks, learned
    // on each heartbeat — short docker id → deployment id, so the log
    // collector can ship their logs (they carry no foundry.managed label).
    let mut adopted: std::collections::HashMap<String, foundry_shared::DeploymentId> =
        std::collections::HashMap::new();

    loop {
        tokio::select! {
            _ = metrics_interval.tick() => {
                let sample = collector.collect(nvml.as_ref(), docker.as_ref()).await;
                let result = client
                    .post(&metrics_url)
                    .header("x-foundry-agent-id", &config.agent_id)
                    .bearer_auth(&config.agent_secret)
                    .json(&sample)
                    .send()
                    .await;
                match result {
                    Ok(resp) if resp.status().is_success() => {}
                    Ok(resp) => tracing::warn!(status = %resp.status(), "metrics upload rejected"),
                    Err(err) => tracing::debug!(%err, "metrics upload failed (controller unreachable)"),
                }
            }
            _ = logs_interval.tick() => {
                let chunks = log_collector.collect(&adopted, docker.as_ref()).await;
                if !chunks.is_empty() {
                    let result = client
                        .post(&logs_url)
                        .header("x-foundry-agent-id", &config.agent_id)
                        .bearer_auth(&config.agent_secret)
                        .json(&chunks)
                        .send()
                        .await;
                    match result {
                        Ok(resp) if resp.status().is_success() => {}
                        Ok(resp) => tracing::warn!(status = %resp.status(), "log upload rejected"),
                        Err(err) => tracing::debug!(%err, "log upload failed (controller unreachable)"),
                    }
                }
            }
            _ = inventory_interval.tick() => {
                let snapshot = inventory::collect(nvml.as_ref(), docker.as_ref()).await;
                tracing::debug!(
                    gpus = snapshot.gpus.len(),
                    containers = snapshot.containers.len(),
                    "uploading inventory"
                );
                let result = client
                    .post(&inventory_url)
                    .header("x-foundry-agent-id", &config.agent_id)
                    .bearer_auth(&config.agent_secret)
                    .json(&snapshot)
                    .send()
                    .await;
                match result {
                    Ok(resp) if resp.status().is_success() => {}
                    Ok(resp) => tracing::warn!(status = %resp.status(), "inventory upload rejected"),
                    Err(err) => tracing::debug!(%err, "inventory upload failed (controller unreachable)"),
                }
            }
            _ = interval.tick() => {
                let result = client
                    .post(&url)
                    .header("x-foundry-agent-id", &config.agent_id)
                    .bearer_auth(&config.agent_secret)
                    .json(&HeartbeatRequest {
                        agent_version: env!("CARGO_PKG_VERSION").to_string(),
                    })
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        if healthy != Some(true) {
                            tracing::info!("heartbeat ok — controller reachable");
                            healthy = Some(true);
                        }
                        // Refresh the adopted-container set for log capture.
                        if let Ok(hb) =
                            resp.json::<foundry_shared::dto::HeartbeatResponse>().await
                        {
                            adopted = hb
                                .adopted_containers
                                .into_iter()
                                .map(|a| (a.container_id, a.deployment_id))
                                .collect();
                        }
                    }
                    Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED => {
                        if healthy != Some(false) {
                            tracing::error!(
                                "controller rejected credentials (rotated or server removed?) — \
                                 re-enroll with a fresh token"
                            );
                            healthy = Some(false);
                        }
                    }
                    Ok(resp) => {
                        if healthy != Some(false) {
                            tracing::warn!(status = %resp.status(), "heartbeat failed");
                            healthy = Some(false);
                        }
                    }
                    Err(err) => {
                        if healthy != Some(false) {
                            tracing::warn!(%err, "controller unreachable");
                            healthy = Some(false);
                        }
                    }
                }
            }
            _ = shutdown_signal() => break,
        }
    }
}

pub(crate) async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => tracing::error!(?err, "failed to install SIGTERM handler"),
        }
    };

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received, shutting down"),
        _ = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if std::env::var("FOUNDRY_LOG_FORMAT").as_deref() == Ok("json") {
        fmt().with_env_filter(filter).json().init();
    } else {
        fmt().with_env_filter(filter).init();
    }
}

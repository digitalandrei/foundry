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
mod inventory;
mod register;

use std::error::Error;
use std::time::Duration;

use foundry_shared::dto::HeartbeatRequest;

const USAGE: &str = "\
usage: foundry-agent                                   run (enrolled servers)
       foundry-agent --register --url <controller> --token <token> [--force]
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
        let (Some(url), Some(token)) = (get("--url"), get("--token")) else {
            eprintln!("{USAGE}");
            return Err("--register requires --url and --token".into());
        };
        return register::run(register::RegisterArgs {
            url,
            token,
            force: args.iter().any(|a| a == "--force"),
        })
        .await;
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

    heartbeat_loop(&client, &config).await;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Heartbeat + periodic inventory until SIGTERM/ctrl-c. Logs only on
/// state *transitions* (reachable/unreachable) to keep journald quiet.
/// TLS is always verified — no insecure escape hatch (docs/SECURITY.md).
async fn heartbeat_loop(client: &reqwest::Client, config: &config::AgentConfig) {
    let base = config.controller_url.trim_end_matches('/');
    let url = format!("{base}/agent/heartbeat");
    let inventory_url = format!("{base}/agent/inventory");
    let mut interval = tokio::time::interval(Duration::from_secs(config.poll_interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Full snapshot at start, then every minute (docs/GPU-MIG.md).
    let mut inventory_interval = tokio::time::interval(Duration::from_secs(60));
    inventory_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut healthy: Option<bool> = None;

    loop {
        tokio::select! {
            _ = inventory_interval.tick() => {
                let snapshot = inventory::collect().await;
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

async fn shutdown_signal() {
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

//! Foundry GPU-server agent. Pull-only: every connection is outbound
//! HTTPS to the controller (docs/ARCHITECTURE.md § Pull-Based Agent
//! Model).
//!
//! Phase 2 scope: config, HTTPS client, and a connectivity loop polling
//! the controller's `/health`. The heartbeat/enrollment protocol
//! replaces the loop body in Phase 4.

mod config;

use std::error::Error;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let path = config::config_path();
    let config = config::load(&path)?;
    tracing::info!(
        controller = %config.controller_url,
        version = env!("CARGO_PKG_VERSION"),
        "foundry-agent starting"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    run_loop(&client, &config).await;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Poll the controller until SIGTERM/ctrl-c. TLS is always verified —
/// there is intentionally no insecure escape hatch (docs/SECURITY.md).
async fn run_loop(client: &reqwest::Client, config: &config::AgentConfig) {
    let health_url = format!("{}/health", config.controller_url.trim_end_matches('/'));
    let mut interval = tokio::time::interval(Duration::from_secs(config.poll_interval_secs));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match client.get(&health_url).send().await {
                    Ok(resp) => tracing::debug!(status = %resp.status(), "controller reachable"),
                    Err(err) => tracing::warn!(%err, "controller unreachable"),
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

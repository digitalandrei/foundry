//! Foundry control plane: axum API, GitLab integration, scheduler state,
//! agent task queue.

mod agent_version;
mod audit;
mod auth;
mod cli;
mod config;
mod crypto;
mod error;
mod files;
mod gitlab;
mod lifecycle;
mod repos;
mod routes;
mod shell;
mod state;

#[cfg(test)]
mod db_tests;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use sqlx::mysql::MySqlPoolOptions;

use crate::config::Config;
use crate::crypto::SecretBox;
use crate::state::AppState;

/// Embedded migrations: applied automatically on startup so a deployed
/// binary is always schema-complete (docs/DEPLOYMENT.md § MySQL).
pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return cli::run(args).await;
    }
    serve().await
}

async fn serve() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    let secrets = SecretBox::from_base64_key(&config.encryption_key)?;

    let pool = MySqlPoolOptions::new()
        .max_connections(config.db_max_connections)
        .connect(&config.database_url)
        .await?;
    MIGRATOR.run(&pool).await?;
    tracing::info!("database connected, migrations up to date");

    auth::session::spawn_sweeper(pool.clone());
    repos::servers::spawn_offline_sweeper(pool.clone());
    repos::metrics::spawn_sweeper(pool.clone());
    repos::logs::spawn_sweeper(pool.clone());
    repos::tasks::spawn_abandon_sweeper(pool.clone());

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    let state = AppState {
        pool,
        secrets,
        http,
        public_url: Arc::from(config.public_url.as_str()),
        admin_emails: Arc::from(config.admin_emails.clone()),
        apps_domain: config.apps_domain.as_deref().map(Arc::from),
        progress: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        shells: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        files: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(bind = %config.bind, public_url = %config.public_url,
        version = env!("CARGO_PKG_VERSION"), "foundry-controller listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Structured JSON in production (systemd/journald); human-readable
    // otherwise. Selected via FOUNDRY_LOG_FORMAT=json.
    if std::env::var("FOUNDRY_LOG_FORMAT").as_deref() == Ok("json") {
        fmt().with_env_filter(filter).json().init();
    } else {
        fmt().with_env_filter(filter).init();
    }
}

/// Resolves on SIGTERM (systemd stop) or ctrl-c, triggering graceful
/// shutdown: in-flight requests finish, then the process exits.
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

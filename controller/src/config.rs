//! Controller configuration, loaded from the environment (and `.env`
//! in development; in production systemd supplies the environment via
//! `EnvironmentFile=` — docs/DEPLOYMENT.md).

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    /// Listen address. Localhost only — Nginx is the public listener.
    pub bind: SocketAddr,
    pub database_url: String,
    pub db_max_connections: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("DATABASE_URL is not set")]
    MissingDatabaseUrl,
    #[error("invalid {name}: {value:?}")]
    Invalid { name: &'static str, value: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        // Best-effort .env for development; absent in production.
        let _ = dotenvy::dotenv();

        let bind = match std::env::var("FOUNDRY_BIND") {
            Ok(v) => v.parse().map_err(|_| ConfigError::Invalid {
                name: "FOUNDRY_BIND",
                value: v,
            })?,
            Err(_) => SocketAddr::from(([127, 0, 0, 1], 8400)),
        };
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)?;
        let db_max_connections = match std::env::var("FOUNDRY_DB_MAX_CONNECTIONS") {
            Ok(v) => v.parse().map_err(|_| ConfigError::Invalid {
                name: "FOUNDRY_DB_MAX_CONNECTIONS",
                value: v,
            })?,
            Err(_) => 10,
        };

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
        })
    }
}

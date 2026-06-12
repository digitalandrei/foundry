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
    /// Browser-facing origin, used to build the OAuth redirect URI
    /// (`{public_url}/auth/callback`). Production default; set to the
    /// Vite dev origin (`http://localhost:5173`) for local dev.
    pub public_url: String,
    /// Base64 of 32 bytes; see crypto::SecretBox.
    pub encryption_key: String,
    /// Emails (lowercased) granted `is_admin` at login.
    pub admin_emails: Vec<String>,
    /// Wildcard apps domain for HTTP/S publishing (`ai.protv.ro` →
    /// apps at `<name>.ai.protv.ro`). Unset → HTTP/S kinds rejected.
    pub apps_domain: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
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
            std::env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;
        let db_max_connections = match std::env::var("FOUNDRY_DB_MAX_CONNECTIONS") {
            Ok(v) => v.parse().map_err(|_| ConfigError::Invalid {
                name: "FOUNDRY_DB_MAX_CONNECTIONS",
                value: v,
            })?,
            Err(_) => 10,
        };
        let public_url = std::env::var("FOUNDRY_PUBLIC_URL")
            .unwrap_or_else(|_| "https://foundry.cloudcraft.ro".to_string())
            .trim_end_matches('/')
            .to_string();
        let encryption_key = std::env::var("FOUNDRY_ENCRYPTION_KEY")
            .map_err(|_| ConfigError::Missing("FOUNDRY_ENCRYPTION_KEY"))?;
        let apps_domain = std::env::var("FOUNDRY_APPS_DOMAIN")
            .ok()
            .map(|d| d.trim().trim_matches('.').to_lowercase())
            .filter(|d| !d.is_empty());
        let admin_emails = std::env::var("FOUNDRY_ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
            public_url,
            encryption_key,
            admin_emails,
            apps_domain,
        })
    }
}

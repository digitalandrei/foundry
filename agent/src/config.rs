//! Agent configuration from `/etc/foundry-agent/config.toml`
//! (override path with `FOUNDRY_AGENT_CONFIG` for development).
//! Written by `foundry-agent --register`; the identity authenticates
//! every controller request.

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/foundry-agent/config.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Base URL of the controller, e.g. `https://foundry.cloudcraft.ro`.
    pub controller_url: String,
    /// Permanent identity issued at enrollment.
    pub agent_id: String,
    pub agent_secret: String,
    /// Display name of this server in the controller (informational).
    #[serde(default)]
    pub server_name: Option<String>,
    /// Seconds between controller polls.
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

fn default_poll_interval_secs() -> u64 {
    15
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "config not found at {0} — this server is not enrolled; run \
         `sudo foundry-agent --register --url <controller> --token <token>` \
         (token from the Foundry UI, Servers page)"
    )]
    NotFound(PathBuf),
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

pub fn config_path() -> PathBuf {
    std::env::var("FOUNDRY_AGENT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH))
}

pub fn load(path: &Path) -> Result<AgentConfig, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.to_path_buf()));
    }
    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

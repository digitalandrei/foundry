//! `GET /health` response (`docs/API.md` § Observability Endpoints).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// `"ok"` when the controller can serve traffic, `"degraded"` otherwise.
    pub status: String,
    pub version: String,
    /// `"up"` / `"down"` — current database connectivity.
    pub database: String,
}

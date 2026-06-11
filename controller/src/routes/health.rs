//! `GET /health` — liveness plus database connectivity
//! (docs/API.md § Observability Endpoints). No auth: it exposes no data
//! beyond up/down and the version.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use foundry_shared::dto::HealthResponse;

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let db_up = sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .inspect_err(|err| tracing::warn!(?err, "health: database check failed"))
        .is_ok();

    let (status, body_status, db) = if db_up {
        (StatusCode::OK, "ok", "up")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "degraded", "down")
    };

    (
        status,
        Json(HealthResponse {
            status: body_status.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            database: db.to_string(),
        }),
    )
}

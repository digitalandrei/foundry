//! Route registration, grouped by resource (docs/API.md). One module
//! per resource; this file only assembles the router.

mod health;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .with_state(state)
}

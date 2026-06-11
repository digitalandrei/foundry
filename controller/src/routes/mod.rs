//! Route registration, grouped by resource (docs/API.md). One module
//! per resource; this file only assembles the router.

mod health;
mod instances;
mod me;
mod projects;
mod registry;

use axum::routing::{get, post};
use axum::Router;

use crate::auth;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        // OAuth flow (docs/GITLAB-INTEGRATION.md § OAuth)
        .route("/auth/login/{instance_id}", get(auth::routes::login))
        .route("/auth/callback", get(auth::routes::callback))
        .route("/auth/logout", post(auth::routes::logout))
        // Frontend API — session-authenticated except the login picker.
        .route("/api/instances", get(instances::list_public))
        .route("/api/instances/full", get(instances::list_admin))
        .route("/api/instances", post(instances::create))
        .route("/api/me", get(me::me))
        .route("/api/projects", get(projects::list))
        .route("/api/registry/{project_id}", get(registry::browse))
        .with_state(state)
}

//! Route registration, grouped by resource (docs/API.md). One module
//! per resource; this file only assembles the router.

mod agent;
mod audit;
mod deployments;
mod health;
mod instances;
mod me;
mod projects;
mod registry;
mod servers;

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
        .route("/auth/local", post(auth::routes::local_login))
        .route("/auth/logout", post(auth::routes::logout))
        // Frontend API — session-authenticated except the login picker.
        .route("/api/instances", get(instances::list_public))
        .route("/api/instances/full", get(instances::list_admin))
        .route("/api/instances", post(instances::create))
        .route("/api/instances/{id}", axum::routing::put(instances::update))
        .route(
            "/api/instances/{id}",
            axum::routing::delete(instances::delete),
        )
        .route("/api/me", get(me::me))
        .route("/api/projects", get(projects::list))
        .route("/api/registry/{project_id}", get(registry::browse))
        .route(
            "/api/registry/tags/{tag_id}/exposed-ports",
            get(registry::exposed_ports),
        )
        .route("/api/servers", get(servers::list))
        .route("/api/servers", post(servers::create))
        .route("/api/metrics/latest", get(servers::metrics_latest))
        .route("/api/servers/{server_id}", get(servers::detail))
        .route("/api/servers/{server_id}/metrics", get(servers::metrics))
        .route(
            "/api/servers/{server_id}/volumes",
            get(deployments::list_volumes),
        )
        .route(
            "/api/volumes/{volume_id}",
            axum::routing::delete(deployments::delete_volume),
        )
        .route("/api/deployments", get(deployments::list))
        .route("/api/deployments", post(deployments::create))
        .route("/api/deployments/{id}", get(deployments::detail))
        .route("/api/deployments/{id}/logs", get(deployments::logs))
        // Interactive shell (WebSocket; docs/API.md § Shell)
        .route("/api/deployments/{id}/shell", get(crate::shell::browser))
        .route("/api/deployments/{id}/stop", post(deployments::stop))
        .route("/api/deployments/{id}/restart", post(deployments::restart))
        .route("/api/deployments/{id}/dismiss", post(deployments::dismiss))
        .route(
            "/api/deployments/{id}",
            axum::routing::delete(deployments::remove),
        )
        .route("/api/deployments/{id}/replace", post(deployments::replace))
        .route("/api/audit", get(audit::list))
        .route(
            "/api/servers/{server_id}/enrollment-token",
            post(servers::regenerate_token),
        )
        // Agent protocol (docs/API.md § Agent API)
        .route("/agent/enroll", post(agent::enroll))
        .route("/agent/heartbeat", post(agent::heartbeat))
        .route("/agent/inventory", post(agent::inventory))
        .route("/agent/metrics", post(agent::metrics))
        .route("/agent/logs", post(agent::logs))
        .route("/agent/tasks/next", get(agent::tasks_next))
        .route("/agent/tasks/result", post(agent::tasks_result))
        .route("/agent/tasks/progress", post(agent::tasks_progress))
        .route("/agent/shell/next", get(crate::shell::agent_next))
        .route(
            "/agent/shell/attach/{session_id}",
            get(crate::shell::agent_attach),
        )
        .with_state(state)
}

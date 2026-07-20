//! Route registration, grouped by resource (docs/API.md). One module
//! per resource; this file only assembles the router.

mod agent;
mod audit;
mod deployments;
mod gpu_groups;
mod health;
mod instances;
mod me;
mod projects;
mod registry;
mod servers;
mod volumes;

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
            "/api/registry/tags/{tag_id}/metadata",
            get(registry::image_metadata),
        )
        // Backward-compatible alias for pre-0.53 frontends. The expanded
        // JSON keeps the original `ports` field.
        .route(
            "/api/registry/tags/{tag_id}/exposed-ports",
            get(registry::image_metadata),
        )
        .route("/api/registry/updates", get(registry::updates))
        .route("/api/servers", get(servers::list))
        .route("/api/servers", post(servers::create))
        .route("/api/metrics/latest", get(servers::metrics_latest))
        .route(
            "/api/servers/{server_id}",
            get(servers::detail).delete(servers::delete),
        )
        .route("/api/servers/{server_id}/metrics", get(servers::metrics))
        // GPU groups + slot use-mode (admin-only; docs/API.md § GPU groups)
        .route(
            "/api/servers/{server_id}/gpu-groups",
            get(gpu_groups::list).post(gpu_groups::create),
        )
        .route(
            "/api/gpu-groups/{group_id}",
            axum::routing::delete(gpu_groups::delete).patch(gpu_groups::set_group_use_mode),
        )
        .route(
            "/api/slots/{slot_id}",
            axum::routing::patch(gpu_groups::set_slot_use_mode),
        )
        .route("/api/servers/{server_id}/volumes", get(volumes::list))
        .route(
            "/api/volumes/{volume_id}",
            axum::routing::delete(volumes::delete),
        )
        .route("/api/volumes/{volume_id}/clean", post(volumes::clean))
        .route(
            "/api/servers/{server_id}/volume-files",
            get(crate::files::browser),
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
        .route(
            "/api/fleet-tokens",
            get(servers::list_fleet_tokens).post(servers::create_fleet_token),
        )
        .route(
            "/api/fleet-tokens/{id}",
            axum::routing::delete(servers::delete_fleet_token),
        )
        .route(
            "/api/servers/{server_id}/containers/{container_id}/adopt",
            post(servers::adopt_container),
        )
        // Agent protocol (docs/API.md § Agent API)
        .route("/agent/enroll", post(agent::enroll))
        .route("/agent/enroll/fleet", post(agent::enroll_fleet))
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
        .route("/agent/volume-files/next", get(crate::files::agent_next))
        .route(
            "/agent/volume-files/attach/{session_id}",
            get(crate::files::agent_attach),
        )
        .with_state(state)
}

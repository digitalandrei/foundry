//! `/agent/*` — the pull-only agent protocol (docs/API.md § Agent API).
//! Enroll authenticates with a single-use token; everything else with
//! the permanent agent identity.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use foundry_shared::dto::{
    AgentEnrollRequest, AgentEnrollResponse, HeartbeatRequest, HeartbeatResponse,
};

use crate::auth::agent::AuthenticatedAgent;
use crate::error::AppError;
use crate::repos::servers;
use crate::state::AppState;

/// Default agent poll cadence handed out at enrollment.
const POLL_INTERVAL_SECS: u64 = 15;

pub async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<AgentEnrollRequest>,
) -> Result<Json<AgentEnrollResponse>, AppError> {
    let hostname = req.hostname.trim();
    if hostname.is_empty() || hostname.len() > 255 {
        return Err(AppError::BadRequest("hostname is required".into()));
    }

    let enrolled = servers::enroll(
        &state.pool,
        req.token.trim(),
        hostname,
        req.agent_version.trim(),
        req.os_version.as_deref(),
    )
    .await?;

    tracing::info!(server = %enrolled.server_name, %hostname, "agent enrolled");

    Ok(Json(enroll_response(enrolled)))
}

/// Fleet auto-enrollment: a reusable, time-limited key (not bound to a
/// pre-created server) enrols the calling host under its hostname
/// (docs/ARCHITECTURE.md § Fleet Enrollment).
pub async fn enroll_fleet(
    State(state): State<AppState>,
    Json(req): Json<AgentEnrollRequest>,
) -> Result<Json<AgentEnrollResponse>, AppError> {
    let hostname = req.hostname.trim();
    if hostname.is_empty() || hostname.len() > 255 {
        return Err(AppError::BadRequest("hostname is required".into()));
    }

    let enrolled = servers::enroll_fleet(
        &state.pool,
        req.token.trim(),
        hostname,
        req.agent_version.trim(),
        req.os_version.as_deref(),
    )
    .await?;

    tracing::info!(server = %enrolled.server_name, %hostname, "agent fleet-enrolled");

    Ok(Json(enroll_response(enrolled)))
}

fn enroll_response(e: servers::EnrolledAgent) -> AgentEnrollResponse {
    AgentEnrollResponse {
        agent_id: e.agent_id.to_string(),
        agent_secret: e.agent_secret,
        server_id: e.server_id,
        server_name: e.server_name,
        poll_interval_secs: POLL_INTERVAL_SECS,
    }
}

pub async fn heartbeat(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, AppError> {
    servers::record_heartbeat(&state.pool, ctx.server_id, req.agent_version.trim()).await?;
    let adopted_containers =
        crate::repos::deployments::adopted_for_server(&state.pool, ctx.server_id).await?;
    Ok(Json(HeartbeatResponse { adopted_containers }))
}

pub async fn app_traffic(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(batch): Json<foundry_shared::dto::AppTrafficBatch>,
) -> Result<StatusCode, AppError> {
    crate::repos::traffic::ingest(&state.pool, ctx.server_id, &batch).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Long-poll for the next task (docs/API.md § Agent API): hold up to
/// 25s checking the queue each second; 204 when idle. DEPLOY payloads
/// are enriched here at dispatch — env decrypted and the registry pull
/// token freshly minted from the deployer's GitLab token — so secrets
/// never sit in the queue table.
pub async fn tasks_next(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;

    for _ in 0..25 {
        if let Some(mut task) = crate::repos::tasks::claim_next(&state.pool, ctx.server_id).await? {
            if let foundry_shared::dto::TaskPayload::Deploy(ref mut p) = task.payload {
                enrich_deploy_payload(&state, p).await?;
                // PULLING_IMAGE + slot DEPLOYING mark "an agent picked
                // it up" (slot machine per docs/ARCHITECTURE.md). On a
                // RE-claim (lost task past the 5-min timeout) the
                // deployment already advanced — skip, don't poison the
                // poll with an illegal-transition error.
                let mut tx = state.pool.begin().await?;
                let current = crate::repos::deployments::get(&state.pool, p.deployment_id)
                    .await?
                    .state;
                if current == foundry_shared::DeploymentState::Validating {
                    crate::lifecycle::transition_deployment(
                        &mut tx,
                        p.deployment_id,
                        foundry_shared::DeploymentState::PullingImage,
                        &crate::lifecycle::Actor::controller(),
                        None,
                    )
                    .await?;
                }
                if task.task_type == foundry_shared::TaskType::DeployContainer {
                    crate::lifecycle::transition_slot(
                        &mut tx,
                        p.slot_id,
                        foundry_shared::SlotState::Deploying,
                    )
                    .await?;
                }
                tx.commit().await?;
            }
            let envelope = foundry_shared::dto::TaskEnvelope {
                id: task.id,
                task_type: task.task_type,
                payload: task.payload,
            };
            return Ok(Json(envelope).into_response());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Decrypt env + mint the pull credential for a DEPLOY payload. `create`
/// already verified the deployer holds a GitLab account on the image's
/// instance, so the no-account branch below is the post-create race
/// (instance disabled or token revoked before dispatch): it degrades to
/// an anonymous pull — a public image still works, a private one fails
/// in the executor with a clear error.
async fn enrich_deploy_payload(
    state: &AppState,
    p: &mut foundry_shared::dto::DeployPayload,
) -> Result<(), AppError> {
    let d = crate::repos::deployments::get(&state.pool, p.deployment_id).await?;
    p.env = crate::repos::deployments::env_for_payload(&state.pool, &state.secrets, d.id).await?;

    // A DEPLOY task always targets a registry image, so the deployment has
    // a GitLab instance (adopted deployments never produce a DEPLOY task).
    let Some(instance_id) = d.instance_id else {
        tracing::warn!(deployment = %d.id, "deploy enrich for a deployment without a GitLab instance — anonymous pull");
        return Ok(());
    };
    let accounts =
        crate::repos::users::account_tokens(&state.pool, &state.secrets, d.created_by).await?;
    let Some(account) = accounts.into_iter().find(|a| a.instance_id == instance_id) else {
        tracing::warn!(deployment = %d.id, "deployer's GitLab account vanished after create (disabled/revoked) — anonymous pull");
        return Ok(());
    };
    let instance =
        crate::repos::instances::fetch_config(&state.pool, &state.secrets, instance_id).await?;
    let access = crate::gitlab::tokens::ensure_fresh(state, &instance, &account).await?;
    // image_ref is normally host/path@sha256:digest (legacy rows may still
    // carry host/path:tag). Registry token scope needs only `path`.
    let repo_path = registry_repository_path(&p.image_ref);
    match crate::gitlab::tokens::registry_pull_token(
        &state.http,
        &instance.base_url,
        &access,
        &repo_path,
    )
    .await
    {
        Ok(token) => {
            p.registry_auth = Some(foundry_shared::dto::RegistryAuth::RegistryToken { token });
        }
        Err(err) => {
            // Variant 2 fallback (docs/GITLAB-INTEGRATION.md): hand the
            // daemon a user/token pair and let it run /jwt/auth itself.
            tracing::warn!(
                ?err,
                "registry token mint failed — falling back to user/password"
            );
            p.registry_auth = Some(foundry_shared::dto::RegistryAuth::UserPassword {
                username: "oauth2".into(),
                password: access,
            });
        }
    }
    Ok(())
}

fn registry_repository_path(image_ref: &str) -> String {
    let image_path = image_ref
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(image_ref);
    image_path
        .split_once('@')
        .map(|(path, _)| path)
        .or_else(|| image_path.rsplit_once(':').map(|(path, _)| path))
        .unwrap_or(image_path)
        .to_string()
}

/// Task result: advance the deployment state machine
/// (docs/ARCHITECTURE.md § Agent Tasks; chains live in repos::tasks).
pub async fn tasks_result(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(report): Json<foundry_shared::dto::TaskResultReport>,
) -> Result<StatusCode, AppError> {
    let deployment = crate::repos::tasks::complete(&state.pool, ctx.server_id, &report).await?;
    if let Some(id) = deployment {
        // The task is done either way — live progress text is stale.
        crate::state::lock_recover(&state.progress).remove(&id.0);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Live DEPLOY progress (docs/API.md § Agent API): pull/create/start
/// stage transitions + human detail, surfaced as `status_detail` in
/// the deployment list. Detail text is in-memory only (transient by
/// definition; the state machine is the durable truth).
pub async fn tasks_progress(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(report): Json<foundry_shared::dto::TaskProgressReport>,
) -> Result<StatusCode, AppError> {
    let deployment = crate::repos::tasks::progress(&state.pool, ctx.server_id, &report).await?;
    if let Some(id) = deployment {
        let mut map = crate::state::lock_recover(&state.progress);
        match &report.detail {
            Some(d) => {
                map.insert(id.0, d.chars().take(256).collect());
            }
            None => {
                map.remove(&id.0);
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Telemetry sample (plans/phase-05.md § Telemetry extension).
pub async fn metrics(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(sample): Json<foundry_shared::dto::MetricsSample>,
) -> Result<StatusCode, AppError> {
    if sample.gpus.len() > 64 || sample.migs.len() > 512 || sample.containers.len() > 1024 {
        return Err(AppError::BadRequest("sample exceeds sane bounds".into()));
    }
    crate::repos::metrics::insert(&state.pool, ctx.server_id, &sample).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Container log chunks (docs/API.md § Logs): incremental stdout+stderr
/// for the server's managed containers. Bounded count; each chunk is
/// authorized against its deployment in the repo. Foreign containers are
/// never uploaded (the agent filters by the `foundry.managed` label).
pub async fn logs(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(chunks): Json<Vec<foundry_shared::dto::DeploymentLogChunk>>,
) -> Result<StatusCode, AppError> {
    if chunks.len() > 256 {
        return Err(AppError::BadRequest("log batch exceeds sane bounds".into()));
    }
    crate::repos::logs::append(&state.pool, ctx.server_id, &chunks).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Full snapshot upload (docs/GPU-MIG.md). Bounds: an authenticated
/// agent is not blindly trusted (docs/SECURITY.md § Input hygiene).
pub async fn inventory(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(snap): Json<foundry_shared::dto::InventorySnapshot>,
) -> Result<StatusCode, AppError> {
    if snap.gpus.len() > 64 || snap.containers.len() > 1024 {
        return Err(AppError::BadRequest("snapshot exceeds sane bounds".into()));
    }
    crate::repos::inventory::apply_snapshot(&state.pool, ctx.server_id, &snap).await?;
    tracing::debug!(
        server = %ctx.server_id,
        gpus = snap.gpus.len(),
        containers = snap.containers.len(),
        "inventory applied"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::registry_repository_path;

    #[test]
    fn registry_scope_strips_tag_or_digest() {
        assert_eq!(
            registry_repository_path("registry.example:5050/team/app@sha256:abcd"),
            "team/app"
        );
        assert_eq!(
            registry_repository_path("registry.example:5050/team/app:latest"),
            "team/app"
        );
    }
}

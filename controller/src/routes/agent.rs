//! `/agent/*` — the pull-only agent protocol (docs/API.md § Agent API).
//! Enroll authenticates with a single-use token; everything else with
//! the permanent agent identity.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use foundry_shared::dto::{AgentEnrollRequest, AgentEnrollResponse, HeartbeatRequest};
use foundry_shared::ActorType;

use crate::audit::{self, AuditEntry};
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

    audit::record(
        &state.pool,
        AuditEntry {
            actor_type: ActorType::Agent,
            actor_id: None,
            action: "AGENT_ENROLLED",
            subject_type: Some("server"),
            subject_id: Some(enrolled.server_id.0),
            detail: Some(serde_json::json!({
                "server": enrolled.server_name,
                "hostname": hostname,
                "agent_version": req.agent_version,
            })),
            ip_address: None,
        },
    )
    .await?;
    tracing::info!(server = %enrolled.server_name, %hostname, "agent enrolled");

    Ok(Json(AgentEnrollResponse {
        agent_id: enrolled.agent_id.to_string(),
        agent_secret: enrolled.agent_secret,
        server_id: enrolled.server_id,
        server_name: enrolled.server_name,
        poll_interval_secs: POLL_INTERVAL_SECS,
    }))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(req): Json<HeartbeatRequest>,
) -> Result<StatusCode, AppError> {
    servers::record_heartbeat(&state.pool, ctx.server_id, req.agent_version.trim()).await?;
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
                // it up" (slot machine per docs/ARCHITECTURE.md).
                let mut tx = state.pool.begin().await?;
                crate::lifecycle::transition_deployment(
                    &mut tx,
                    p.deployment_id,
                    foundry_shared::DeploymentState::PullingImage,
                    &crate::lifecycle::Actor::controller(),
                    None,
                )
                .await?;
                crate::lifecycle::transition_slot(
                    &mut tx,
                    p.slot_id,
                    foundry_shared::SlotState::Deploying,
                )
                .await?;
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

/// Decrypt env + mint the pull credential for a DEPLOY payload. A
/// missing GitLab account (local operator) or mint failure degrades to
/// an anonymous pull — public images still work, private ones fail in
/// the executor with a clear error.
async fn enrich_deploy_payload(
    state: &AppState,
    p: &mut foundry_shared::dto::DeployPayload,
) -> Result<(), AppError> {
    let d = crate::repos::deployments::get(&state.pool, p.deployment_id).await?;
    p.env = crate::repos::deployments::env_for_payload(&state.pool, &state.secrets, d.id).await?;

    let accounts =
        crate::repos::users::account_tokens(&state.pool, &state.secrets, d.created_by).await?;
    let Some(account) = accounts
        .into_iter()
        .find(|a| a.instance_id == d.instance_id)
    else {
        tracing::warn!(deployment = %d.id, "deployer has no GitLab account on the instance — anonymous pull");
        return Ok(());
    };
    let instance =
        crate::repos::instances::fetch_config(&state.pool, &state.secrets, d.instance_id).await?;
    let access = crate::gitlab::tokens::ensure_fresh(state, &instance, &account).await?;
    // image_ref = host/path:tag → repo path for the token scope.
    let repo_path = p
        .image_ref
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(&p.image_ref)
        .rsplit_once(':')
        .map(|(path, _)| path)
        .unwrap_or_default()
        .to_string();
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

/// Task result: advance the deployment state machine
/// (docs/ARCHITECTURE.md § Agent Tasks; chains live in repos::tasks).
pub async fn tasks_result(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(report): Json<foundry_shared::dto::TaskResultReport>,
) -> Result<StatusCode, AppError> {
    crate::repos::tasks::complete(&state.pool, ctx.server_id, &report).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Telemetry sample (plans/phase-05.md § Telemetry extension).
pub async fn metrics(
    State(state): State<AppState>,
    AuthenticatedAgent(ctx): AuthenticatedAgent,
    Json(sample): Json<foundry_shared::dto::MetricsSample>,
) -> Result<StatusCode, AppError> {
    if sample.gpus.len() > 64 || sample.containers.len() > 1024 {
        return Err(AppError::BadRequest("sample exceeds sane bounds".into()));
    }
    crate::repos::metrics::insert(&state.pool, ctx.server_id, &sample).await?;
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

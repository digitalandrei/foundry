//! deployment_logs: bounded container-log capture (Phase 7,
//! docs/API.md § Logs). The agent ships incremental stdout+stderr
//! chunks; we keep a window bounded two ways — by time (7 days) and by
//! count (newest N chunks per deployment) — so neither a long-lived nor
//! a log-spamming container can exhaust the controller. Logs are deleted
//! with their deployment (lifecycle::transition_deployment → REMOVED).

use chrono::{Duration, Utc};
use foundry_shared::dto::{DeploymentLogChunk, DeploymentLogsView};
use foundry_shared::{DeploymentId, ServerId};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::AppError;

/// Time window: at most 7 days of logs (operator rule).
const RETENTION_DAYS: i64 = 7;
/// Count window: newest chunks kept per deployment. With the per-chunk
/// cap below this hard-bounds on-disk size per deployment regardless of
/// how fast a container spams.
const MAX_CHUNKS_PER_DEPLOYMENT: i64 = 600;
/// Per-chunk content cap on intake (the agent already bounds its upload;
/// re-clamp defensively — an authenticated agent is not blindly trusted).
const MAX_CHUNK_BYTES: usize = 32 * 1024;
/// Response budget for the viewer — the newest bytes of the window.
const RESPONSE_BYTES: usize = 256 * 1024;

/// Store a batch of new-output chunks. Each chunk is authorized against
/// the uploading server (a managed container's deployment lives on it)
/// and silently skipped if the deployment is gone, foreign, or already
/// REMOVED — re-delivery and races must never fail the upload.
pub async fn append(
    pool: &MySqlPool,
    server_id: ServerId,
    chunks: &[DeploymentLogChunk],
) -> Result<(), AppError> {
    for chunk in chunks {
        // The deployment must exist, belong to this server, and still be
        // live (a REMOVED deployment's logs were intentionally deleted).
        let owner = sqlx::query!(
            r#"SELECT server_id AS "server_id: Uuid", state
               FROM deployments WHERE id = ?"#,
            chunk.deployment_id.0,
        )
        .fetch_optional(pool)
        .await?;
        let Some(owner) = owner else { continue };
        if ServerId::from(owner.server_id) != server_id || owner.state == "REMOVED" {
            continue;
        }

        let content: String = chunk.content.chars().take(MAX_CHUNK_BYTES).collect();
        if content.is_empty() {
            continue;
        }
        sqlx::query!(
            r#"INSERT INTO deployment_logs
               (id, deployment_id, server_id, container_id, logged_at, content)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            chunk.deployment_id.0,
            server_id.0,
            chunk.container_id,
            chunk.through.naive_utc(),
            content,
        )
        .execute(pool)
        .await?;

        trim(pool, chunk.deployment_id).await?;
    }
    Ok(())
}

/// Keep only the newest MAX_CHUNKS_PER_DEPLOYMENT chunks for a deployment
/// (the count bound — runs on every append so a spamming container is
/// capped within one interval).
async fn trim(pool: &MySqlPool, deployment_id: DeploymentId) -> Result<(), AppError> {
    sqlx::query!(
        r#"DELETE FROM deployment_logs
           WHERE deployment_id = ?
             AND id NOT IN (
               SELECT id FROM (
                 SELECT id FROM deployment_logs
                 WHERE deployment_id = ?
                 ORDER BY logged_at DESC, id DESC
                 LIMIT ?
               ) keep
             )"#,
        deployment_id.0,
        deployment_id.0,
        MAX_CHUNKS_PER_DEPLOYMENT,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The recent log window for the viewer: all stored chunks concatenated
/// oldest→newest, then trimmed to the newest RESPONSE_BYTES so the
/// response stays bounded for a chatty container.
pub async fn recent(pool: &MySqlPool, id: DeploymentId) -> Result<DeploymentLogsView, AppError> {
    let rows = sqlx::query!(
        r#"SELECT logged_at, content FROM deployment_logs
           WHERE deployment_id = ? ORDER BY logged_at, id"#,
        id.0,
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(DeploymentLogsView {
            content: String::new(),
            collected_at: None,
            available: false,
        });
    }

    let collected_at = rows.last().map(|r| r.logged_at.and_utc());
    let mut content = String::new();
    for r in &rows {
        content.push_str(&r.content);
        if !content.ends_with('\n') {
            content.push('\n');
        }
    }
    // Keep the newest RESPONSE_BYTES, snapped to a line boundary.
    if content.len() > RESPONSE_BYTES {
        let cut = content.len() - RESPONSE_BYTES;
        let start = content[cut..]
            .find('\n')
            .map(|i| cut + i + 1)
            .unwrap_or(cut);
        content = content[start..].to_string();
    }

    Ok(DeploymentLogsView {
        content,
        collected_at,
        available: true,
    })
}

/// Hard-delete every chunk of a deployment. Called inside the same
/// transaction that retires the deployment (REMOVED) so logs never
/// outlive the thing they describe.
pub async fn delete_for(tx: &mut sqlx::MySqlConnection, id: DeploymentId) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM deployment_logs WHERE deployment_id = ?", id.0)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Time-window sweep + orphan backstop; spawned at startup. Drops chunks
/// older than 7 days and any that outlived a REMOVED deployment (the
/// transactional delete is the primary path; this catches edge cases).
pub fn spawn_sweeper(pool: MySqlPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
        loop {
            interval.tick().await;
            let cutoff = (Utc::now() - Duration::days(RETENTION_DAYS)).naive_utc();
            match sqlx::query!("DELETE FROM deployment_logs WHERE logged_at < ?", cutoff)
                .execute(&pool)
                .await
            {
                Ok(res) if res.rows_affected() > 0 => {
                    tracing::info!(deleted = res.rows_affected(), "old log chunks swept");
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(?err, "log sweep failed"),
            }
            if let Err(err) = sqlx::query!(
                "DELETE FROM deployment_logs WHERE deployment_id IN
                   (SELECT id FROM deployments WHERE state = 'REMOVED')"
            )
            .execute(&pool)
            .await
            {
                tracing::warn!(?err, "removed-deployment log cleanup failed");
            }
        }
    });
}

//! Agent task queue (docs/ARCHITECTURE.md § Agent Tasks): enqueue,
//! dispatch (long-polled by agents), and result handling — which is
//! where deployment state advances and replacement chains continue.
//! Agents report; the controller decides.

use foundry_shared::dto::{TaskPayload, TaskProgressReport, TaskResultReport};
use foundry_shared::{
    DeploymentId, DeploymentState, ServerId, SlotState, TaskId, TaskType, UserId,
};
use sqlx::{MySqlConnection, MySqlPool};
use uuid::Uuid;

use crate::error::AppError;
use crate::lifecycle::{self, Actor};

/// A DISPATCHED task with no result for this long is considered lost
/// and re-queued (agent crash mid-execution; executors are idempotent).
const REDISPATCH_AFTER_SECS: i64 = 300;

pub async fn enqueue(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    deployment_id: Option<DeploymentId>,
    task_type: TaskType,
    payload: &TaskPayload,
) -> Result<TaskId, AppError> {
    let id = TaskId::new();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query!(
        r#"INSERT INTO agent_tasks
           (id, server_id, deployment_id, task_type, payload, state, attempts, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, 'QUEUED', 0, ?, ?)"#,
        id.0,
        server_id.0,
        deployment_id.map(|d| d.0),
        task_type.as_str(),
        serde_json::to_string(payload).map_err(AppError::internal)?,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;
    Ok(id)
}

pub struct DispatchedTask {
    pub id: TaskId,
    pub task_type: TaskType,
    pub payload: TaskPayload,
}

/// Claim the next task for a server (oldest first; lost DISPATCHED
/// tasks are re-claimed after the timeout). Returns None when idle.
pub async fn claim_next(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Option<DispatchedTask>, AppError> {
    let now = chrono::Utc::now();
    let stale = (now - chrono::Duration::seconds(REDISPATCH_AFTER_SECS)).naive_utc();
    let mut tx = pool.begin().await?;

    let row = sqlx::query!(
        r#"SELECT id AS "id: Uuid", task_type, payload
           FROM agent_tasks
           WHERE server_id = ?
             AND (state = 'QUEUED' OR (state = 'DISPATCHED' AND dispatched_at < ?))
           ORDER BY created_at
           LIMIT 1
           FOR UPDATE SKIP LOCKED"#,
        server_id.0,
        stale,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    sqlx::query!(
        "UPDATE agent_tasks SET state = 'DISPATCHED', dispatched_at = ?,
             attempts = attempts + 1, updated_at = ? WHERE id = ?",
        now.naive_utc(),
        now.naive_utc(),
        row.id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(DispatchedTask {
        id: row.id.into(),
        task_type: row.task_type.parse().map_err(AppError::internal)?,
        payload: serde_json::from_slice(&row.payload).map_err(AppError::internal)?,
    }))
}

struct TaskRow {
    task_type: TaskType,
    deployment_id: Option<DeploymentId>,
    server_id: ServerId,
}

/// Live DEPLOY progress (best-effort, agent-throttled): advance the
/// deployment through PULLING_IMAGE → CREATING_CONTAINER → STARTING.
/// Returns the deployment id when the report is current (the caller
/// keeps the transient detail text in AppState — in-memory by design);
/// None when stale (drop silently — never poison the agent loop).
pub async fn progress(
    pool: &MySqlPool,
    reporting_server: ServerId,
    report: &TaskProgressReport,
) -> Result<Option<DeploymentId>, AppError> {
    if !matches!(
        report.state,
        DeploymentState::PullingImage
            | DeploymentState::CreatingContainer
            | DeploymentState::Starting
    ) {
        return Err(AppError::BadRequest(
            "progress may only report PULLING_IMAGE/CREATING_CONTAINER/STARTING".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let task = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid", deployment_id AS "deployment_id: Uuid", state
           FROM agent_tasks WHERE id = ? FOR UPDATE"#,
        report.task_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("task not found"))?;
    if task.server_id != reporting_server.0 {
        return Err(AppError::Forbidden);
    }
    let Some(deployment_id) = task.deployment_id else {
        return Err(AppError::BadRequest("task carries no deployment".into()));
    };
    if task.state != "DISPATCHED" {
        // Completed/requeued in the meantime — stale report.
        tx.commit().await?;
        return Ok(None);
    }

    let current = sqlx::query!(
        "SELECT state FROM deployments WHERE id = ? FOR UPDATE",
        deployment_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment vanished"))?;
    let current: DeploymentState = current.state.parse().map_err(AppError::internal)?;

    if current != report.state {
        if !lifecycle::is_legal(current, report.state) {
            // Out-of-order/duplicate report (e.g. after re-dispatch).
            tx.commit().await?;
            return Ok(None);
        }
        lifecycle::transition_deployment(
            &mut tx,
            deployment_id.into(),
            report.state,
            &Actor::agent(),
            report
                .detail
                .as_ref()
                .map(|d| serde_json::json!({ "progress": d })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Some(deployment_id.into()))
}

/// Apply an agent's result: mark the task, advance the deployment
/// state machine, free/flag the slot, and continue replacement chains.
/// Returns the task's deployment id (if any) so the caller can drop
/// its transient progress entry.
pub async fn complete(
    pool: &MySqlPool,
    reporting_server: ServerId,
    report: &TaskResultReport,
) -> Result<Option<DeploymentId>, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    let row = sqlx::query!(
        r#"SELECT task_type, deployment_id AS "deployment_id: Uuid",
                  server_id AS "server_id: Uuid", state
           FROM agent_tasks WHERE id = ? FOR UPDATE"#,
        report.task_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("task not found"))?;

    // Scope: an agent may only report its own server's tasks.
    if row.server_id != reporting_server.0 {
        return Err(AppError::Forbidden);
    }
    if row.state == "SUCCEEDED" || row.state == "FAILED" {
        // Duplicate report after re-dispatch — idempotent no-op.
        tx.commit().await?;
        return Ok(row.deployment_id.map(Into::into));
    }

    sqlx::query!(
        "UPDATE agent_tasks SET state = ?, completed_at = ?, updated_at = ? WHERE id = ?",
        if report.success {
            "SUCCEEDED"
        } else {
            "FAILED"
        },
        now,
        now,
        report.task_id.0,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO agent_task_results (id, agent_task_id, success, detail, reported_at)
           VALUES (?, ?, ?, ?, ?)"#,
        Uuid::now_v7(),
        report.task_id.0,
        report.success,
        serde_json::to_string(&serde_json::json!({
            "container_id": report.container_id,
            "error": report.error,
        }))
        .map_err(AppError::internal)?,
        now,
    )
    .execute(&mut *tx)
    .await?;

    let task = TaskRow {
        task_type: row.task_type.parse().map_err(AppError::internal)?,
        deployment_id: row.deployment_id.map(Into::into),
        server_id: row.server_id.into(),
    };
    advance_deployment(&mut tx, &task, report).await?;
    tx.commit().await?;
    Ok(task.deployment_id)
}

/// Result → state-machine mapping, including the replacement chain
/// (stop old → remove old → REPLACED → deploy new on the same slot).
async fn advance_deployment(
    tx: &mut MySqlConnection,
    task: &TaskRow,
    report: &TaskResultReport,
) -> Result<(), AppError> {
    let Some(deployment_id) = task.deployment_id else {
        return Ok(()); // REMOVE_VOLUME / inventory tasks carry no deployment
    };
    let actor = Actor::agent();
    let d = sqlx::query!(
        r#"SELECT gpu_slot_id AS "slot_id: Uuid",
                  replaced_by_deployment_id AS "replaced_by: Uuid", state
           FROM deployments WHERE id = ? FOR UPDATE"#,
        deployment_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment vanished"))?;
    let slot_id: foundry_shared::SlotId = d.slot_id.into();
    let detail = report
        .error
        .as_ref()
        .map(|e| serde_json::json!({ "error": e }));

    match (task.task_type, report.success) {
        (TaskType::DeployContainer, true) => {
            if let Some(cid) = &report.container_id {
                sqlx::query!(
                    "UPDATE deployments SET container_id = ?, updated_at = ? WHERE id = ?",
                    cid.chars().take(64).collect::<String>(),
                    chrono::Utc::now().naive_utc(),
                    deployment_id.0,
                )
                .execute(&mut *tx)
                .await?;
            }
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Running,
                &actor,
                detail,
            )
            .await?;
            lifecycle::transition_slot(tx, slot_id, SlotState::Running).await?;
        }
        (TaskType::DeployContainer, false) => {
            // Nothing got deployed — free the slot so it auto-heals.
            fail_deployment(tx, deployment_id, slot_id, report, &actor, true).await?;
        }
        (TaskType::StopContainer, true) => {
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Stopped,
                &actor,
                detail,
            )
            .await?;
            // Stopped container still holds the slot.
            lifecycle::transition_slot(tx, slot_id, SlotState::Reserved).await?;
            // Replacement chain: stopped because a successor waits →
            // remove the old container next (chain continues at REMOVE
            // success).
            if d.replaced_by.is_some() {
                let payload =
                    TaskPayload::Container(foundry_shared::dto::ContainerTarget { deployment_id });
                enqueue(
                    tx,
                    task.server_id,
                    Some(deployment_id),
                    TaskType::RemoveContainer,
                    &payload,
                )
                .await?;
            }
        }
        (TaskType::StopContainer, false) => {
            // The container may still be running — keep the slot FAILED.
            fail_deployment(tx, deployment_id, slot_id, report, &actor, false).await?;
        }
        (TaskType::RestartContainer, success) => {
            let to = if success {
                DeploymentState::Running
            } else {
                DeploymentState::Failed
            };
            lifecycle::transition_deployment(tx, deployment_id, to, &actor, detail).await?;
            lifecycle::transition_slot(
                tx,
                slot_id,
                if success {
                    SlotState::Running
                } else {
                    SlotState::Failed
                },
            )
            .await?;
        }
        (TaskType::RemoveContainer, true) => {
            match d.replaced_by {
                Some(new_id) => {
                    // Old side of a replacement: terminal REPLACED,
                    // slot reserved for the successor, deploy it now.
                    lifecycle::transition_deployment(
                        tx,
                        deployment_id,
                        DeploymentState::Replaced,
                        &actor,
                        Some(serde_json::json!({ "replaced_by": new_id.to_string() })),
                    )
                    .await?;
                    lifecycle::transition_slot(tx, slot_id, SlotState::Reserved).await?;
                    enqueue_deploy(tx, new_id.into()).await?;
                }
                None => {
                    lifecycle::transition_deployment(
                        tx,
                        deployment_id,
                        DeploymentState::Removed,
                        &actor,
                        detail,
                    )
                    .await?;
                    lifecycle::transition_slot(tx, slot_id, SlotState::Free).await?;
                }
            }
        }
        (TaskType::RemoveContainer, false) => {
            // The container may still be present — keep the slot FAILED.
            fail_deployment(tx, deployment_id, slot_id, report, &actor, false).await?;
            // Replacement chain: don't leave the successor wedged in
            // VALIDATING forever (review finding) — fail it too with a
            // clear, actionable error.
            if let Some(new_id) = d.replaced_by {
                let new_id: DeploymentId = new_id.into();
                sqlx::query!(
                    "UPDATE deployments SET error_message = ?, updated_at = ? WHERE id = ?",
                    "replacement aborted: the previous container could not be removed",
                    chrono::Utc::now().naive_utc(),
                    new_id.0,
                )
                .execute(&mut *tx)
                .await?;
                lifecycle::transition_deployment(
                    tx,
                    new_id,
                    DeploymentState::Failed,
                    &actor,
                    Some(serde_json::json!({ "reason": "predecessor removal failed" })),
                )
                .await?;
            }
        }
        (TaskType::RemoveVolume | TaskType::RefreshInventory | TaskType::UploadLogs, _) => {}
    }
    Ok(())
}

/// Mark a deployment FAILED with its error logged. `free_slot` releases
/// the slot to FREE — used when nothing is actually deployed on it (a
/// DEPLOY failure: the agent's executor guarantees no leftover
/// container), so the slot auto-heals and stays usable instead of
/// getting stuck (operator requirement, 0.11.0). STOP/REMOVE failures
/// keep the slot FAILED — a container may still be present; the admin
/// clears it explicitly (dismiss).
async fn fail_deployment(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
    slot_id: foundry_shared::SlotId,
    report: &TaskResultReport,
    actor: &Actor,
    free_slot: bool,
) -> Result<(), AppError> {
    let error = report.error.clone().unwrap_or_else(|| "task failed".into());
    sqlx::query!(
        "UPDATE deployments SET error_message = ?, updated_at = ? WHERE id = ?",
        error.chars().take(2000).collect::<String>(),
        chrono::Utc::now().naive_utc(),
        deployment_id.0,
    )
    .execute(&mut *tx)
    .await?;
    lifecycle::transition_deployment(
        tx,
        deployment_id,
        DeploymentState::Failed,
        actor,
        Some(serde_json::json!({ "error": error })),
    )
    .await?;
    lifecycle::transition_slot(
        tx,
        slot_id,
        if free_slot {
            SlotState::Free
        } else {
            SlotState::Failed
        },
    )
    .await?;
    Ok(())
}

/// Build + enqueue the DEPLOY task for a VALIDATING deployment. The
/// payload carries everything static; env (decrypted) and the registry
/// pull credential are injected at dispatch time so secrets stay out
/// of the queue table and the token is freshly minted.
pub async fn enqueue_deploy(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
) -> Result<TaskId, AppError> {
    let d = sqlx::query!(
        r#"SELECT d.server_id AS "server_id: Uuid", d.image_ref, d.container_name,
                  d.gpu_slot_id AS "slot_id: Uuid", gs.name AS slot_name,
                  d.mem_limit_mb AS "mem_limit_mb?: u32",
                  COALESCE(gs.mig_uuid, g.gpu_uuid) AS "gpu_device_uuid!"
           FROM deployments d
           JOIN gpu_slots gs ON gs.id = d.gpu_slot_id
           JOIN gpus g ON g.id = gs.gpu_id
           WHERE d.id = ?"#,
        deployment_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;

    let ports = sqlx::query!(
        "SELECT container_port, host_port, protocol, kind, hostname FROM deployment_ports
         WHERE deployment_id = ?",
        deployment_id.0
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| {
        Ok(foundry_shared::dto::PortBinding {
            container_port: r.container_port,
            host_port: r.host_port,
            protocol: r.protocol,
            kind: r.kind.parse().map_err(AppError::internal)?,
            hostname: r.hostname,
        })
    })
    .collect::<Result<Vec<_>, AppError>>()?;

    let volumes = sqlx::query!(
        r#"SELECT host_path, container_path, read_only AS "read_only: bool"
           FROM deployment_volumes WHERE deployment_id = ?"#,
        deployment_id.0
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| foundry_shared::dto::VolumeBinding {
        host_path: r.host_path,
        container_path: r.container_path,
        read_only: r.read_only,
    })
    .collect();

    let payload = TaskPayload::Deploy(Box::new(foundry_shared::dto::DeployPayload {
        deployment_id,
        image_ref: d.image_ref,
        container_name: d.container_name.unwrap_or_default(),
        gpu_device_uuid: d.gpu_device_uuid,
        slot_id: d.slot_id.into(),
        slot_name: d.slot_name,
        ports,
        env: Vec::new(), // injected at dispatch
        volumes,
        registry_auth: None, // minted at dispatch
        mem_limit_mb: d.mem_limit_mb,
    }));
    enqueue(
        tx,
        d.server_id.into(),
        Some(deployment_id),
        TaskType::DeployContainer,
        &payload,
    )
    .await
}

/// User-facing lifecycle actions → queued tasks (stop/restart/remove).
pub async fn enqueue_lifecycle(
    pool: &MySqlPool,
    deployment: &super::deployments::DeploymentRow,
    task_type: TaskType,
    from_to: (DeploymentState, DeploymentState),
    user: UserId,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    lifecycle::transition_deployment(&mut tx, deployment.id, from_to.1, &Actor::user(user), None)
        .await?;
    // Stop and remove both put the slot in the transitional STOPPING
    // state ("Freeing" in the UI) so the chip shows work in progress
    // rather than sitting on its prior state until the agent reports.
    if matches!(
        task_type,
        TaskType::StopContainer | TaskType::RemoveContainer
    ) {
        lifecycle::transition_slot(&mut tx, deployment.slot_id, SlotState::Stopping).await?;
    }
    let payload = TaskPayload::Container(foundry_shared::dto::ContainerTarget {
        deployment_id: deployment.id,
    });
    enqueue(
        &mut tx,
        deployment.server_id,
        Some(deployment.id),
        task_type,
        &payload,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Restart = re-deploy. Stop (and a failed deploy) tear the container and
/// image off the host so nothing piles up in `docker ps -a` / `docker
/// images`; that leaves nothing to "start", so restart re-pulls and
/// recreates from the stored spec. The DEPLOY_CONTAINER result drives
/// Restarting → Running (or, on failure, → Failed and frees the slot).
pub async fn enqueue_restart(
    pool: &MySqlPool,
    deployment: &super::deployments::DeploymentRow,
    user: UserId,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    lifecycle::transition_deployment(
        &mut tx,
        deployment.id,
        DeploymentState::Restarting,
        &Actor::user(user),
        None,
    )
    .await?;
    enqueue_deploy(&mut tx, deployment.id).await?;
    tx.commit().await?;
    Ok(())
}

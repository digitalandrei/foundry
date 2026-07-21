//! Agent task queue (docs/ARCHITECTURE.md § Agent Tasks): enqueue,
//! dispatch (long-polled by agents), and result handling — which is
//! where deployment state advances and replacement chains continue.
//! Agents report; the controller decides.

use foundry_shared::dto::{TaskPayload, TaskProgressReport, TaskResultReport};
use foundry_shared::{
    DeploymentId, DeploymentState, ServerId, SlotId, SlotState, TaskId, TaskType, UserId,
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

pub async fn enqueue_server_command(
    pool: &MySqlPool,
    server_id: ServerId,
    task_type: TaskType,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<TaskId, AppError> {
    if !matches!(
        task_type,
        TaskType::RefreshInventory | TaskType::UpgradeAgent
    ) {
        return Err(AppError::BadRequest("unsupported server command".into()));
    }
    let mut tx = pool.begin().await?;
    let server = sqlx::query!(
        "SELECT s.id AS `id: Uuid`, a.agent_version FROM servers s
         LEFT JOIN server_agents a ON a.server_id = s.id WHERE s.id = ? FOR UPDATE",
        server_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("server not found"))?;
    if task_type == TaskType::UpgradeAgent
        && !crate::agent_version::supports(
            server.agent_version.as_deref(),
            crate::agent_version::OPERATIONAL_MIN_AGENT_VERSION,
        )
    {
        return Err(AppError::BadRequest(format!(
            "remote upgrade requires foundry-agent 0.59.0 or newer (reported {}); bootstrap this host once with `sudo foundry-agent --setup-apps`",
            server.agent_version.as_deref().unwrap_or("unknown"),
        )));
    }
    let pending = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM agent_tasks WHERE server_id = ? AND task_type = ? AND state IN ('QUEUED','DISPATCHED') FOR UPDATE",
        server_id.0,
        task_type.as_str(),
    )
    .fetch_one(&mut *tx)
    .await?;
    if pending > 0 {
        return Err(AppError::BadRequest(
            "the server command is already queued".into(),
        ));
    }
    let task_id = enqueue(&mut tx, server_id, None, task_type, &TaskPayload::None).await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: if task_type == TaskType::UpgradeAgent {
                "AGENT_UPGRADE_REQUESTED"
            } else {
                "SERVER_DIAGNOSTICS_REQUESTED"
            },
            subject_type: Some("server"),
            subject_id: Some(server_id.0),
            detail: None,
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(task_id)
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
            | DeploymentState::WaitingHealth
            | DeploymentState::Publishing
    ) {
        return Err(AppError::BadRequest(
            "unsupported deployment progress state".into(),
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
            "failure_stage": report.failure_stage,
            "health_status": report.health_status,
            "health_detail": report.health_detail,
            "readiness": report.readiness,
            "storage": report.storage,
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
    if task.task_type == TaskType::RefreshInventory && report.success {
        let readiness_json = report
            .readiness
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(AppError::internal)?;
        let setup_revision = report
            .readiness
            .as_ref()
            .and_then(|readiness| readiness.setup_revision);
        let checked_at = report
            .readiness
            .as_ref()
            .map(|readiness| readiness.checked_at.naive_utc());
        let storage_total = report.storage.as_ref().map(|storage| storage.total_bytes);
        let storage_available = report
            .storage
            .as_ref()
            .map(|storage| storage.available_bytes);
        sqlx::query!(
            "UPDATE servers SET setup_revision = ?, readiness_json = ?, readiness_checked_at = ?,
             storage_total_bytes = ?, storage_available_bytes = ?, updated_at = ? WHERE id = ?",
            setup_revision,
            readiness_json,
            checked_at,
            storage_total,
            storage_available,
            now,
            task.server_id.0,
        )
        .execute(&mut *tx)
        .await?;
        if let Some(storage) = &report.storage {
            for volume in &storage.volumes {
                sqlx::query!(
                    "UPDATE server_volumes SET used_bytes = ?, usage_measured_at = ?, updated_at = ?
                     WHERE id = ? AND server_id = ?",
                    volume.used_bytes,
                    now,
                    now,
                    volume.volume_id.0,
                    task.server_id.0,
                )
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    advance_deployment(&mut tx, &task, report).await?;
    tx.commit().await?;
    Ok(task.deployment_id)
}

/// Result → state-machine mapping, including the replacement chain:
/// prepare the immutable successor, quiesce and retain the predecessor,
/// publish the healthy successor, then remove/mark the predecessor replaced.
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
        r#"SELECT replaced_by_deployment_id AS "replaced_by: Uuid", state,
                  adopted_container_id
           FROM deployments WHERE id = ? FOR UPDATE"#,
        deployment_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment vanished"))?;
    let detail = report
        .error
        .as_ref()
        .map(|e| serde_json::json!({ "error": e }));

    match (task.task_type, report.success) {
        (TaskType::PrepareDeploy, true) => {
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Prepared,
                &actor,
                None,
            )
            .await?;
            let predecessor = sqlx::query!(
                "SELECT id AS `id: Uuid`, state, adopted_container_id FROM deployments WHERE replaced_by_deployment_id = ? FOR UPDATE",
                deployment_id.0,
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("replacement predecessor not found"))?;
            let old_id: DeploymentId = predecessor.id.into();
            let old_state: DeploymentState =
                predecessor.state.parse().map_err(AppError::internal)?;
            if old_state == DeploymentState::Running {
                lifecycle::transition_deployment(
                    tx,
                    old_id,
                    DeploymentState::Stopping,
                    &actor,
                    None,
                )
                .await?;
                lifecycle::transition_member_slots(tx, old_id, SlotState::Stopping).await?;
                let old_ports = load_port_bindings(tx, old_id).await?;
                enqueue(
                    tx,
                    task.server_id,
                    Some(old_id),
                    TaskType::QuiesceContainer,
                    &TaskPayload::Replacement(foundry_shared::dto::ReplacementTarget {
                        container: foundry_shared::dto::ContainerTarget {
                            deployment_id: old_id,
                            container_id: predecessor.adopted_container_id,
                        },
                        ports: old_ports,
                    }),
                )
                .await?;
            } else {
                if !enqueue_purge_volumes(tx, deployment_id).await? {
                    enqueue_deploy(tx, deployment_id).await?;
                }
            }
        }
        (TaskType::PrepareDeploy, false) => {
            fail_replacement_successor(tx, deployment_id, report, &actor).await?;
        }
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
            sqlx::query!(
                "UPDATE deployments SET health_status = ?, health_detail = ?, error_message = NULL, updated_at = ? WHERE id = ?",
                report.health_status,
                report.health_detail,
                chrono::Utc::now().naive_utc(),
                deployment_id.0,
            )
            .execute(&mut *tx)
            .await?;
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Running,
                &actor,
                detail,
            )
            .await?;
            lifecycle::transition_member_slots(tx, deployment_id, SlotState::Running).await?;
            finish_replacement(tx, deployment_id, task.server_id).await?;
        }
        (TaskType::DeployContainer, false) => {
            if report.failure_stage.as_deref() == Some("PUBLISH") && report.container_id.is_some() {
                sqlx::query!(
                    "UPDATE deployments SET container_id = ?, error_message = ?, health_status = ?, health_detail = ?, updated_at = ? WHERE id = ?",
                    report.container_id,
                    report.error,
                    report.health_status,
                    report.health_detail,
                    chrono::Utc::now().naive_utc(),
                    deployment_id.0,
                )
                .execute(&mut *tx)
                .await?;
                lifecycle::transition_deployment(
                    tx,
                    deployment_id,
                    DeploymentState::PublishFailed,
                    &actor,
                    detail,
                )
                .await?;
                lifecycle::transition_member_slots(tx, deployment_id, SlotState::Running).await?;
                if retained_predecessor(tx, deployment_id).await?.is_some() {
                    lifecycle::transition_deployment(tx, deployment_id, DeploymentState::Stopping, &actor, Some(serde_json::json!({ "reason": "replacement publication failed; restoring predecessor" }))).await?;
                    enqueue(
                        tx,
                        task.server_id,
                        Some(deployment_id),
                        TaskType::StopContainer,
                        &TaskPayload::Container(foundry_shared::dto::ContainerTarget {
                            deployment_id,
                            container_id: None,
                        }),
                    )
                    .await?;
                }
            } else {
                // Preflight/pull/create/health failures leave no workload.
                fail_deployment(tx, deployment_id, report, &actor, true).await?;
                rollback_replacement(tx, deployment_id, task.server_id, &actor).await?;
            }
        }
        (TaskType::PublishVhost, true) => {
            sqlx::query!(
                "UPDATE deployments SET error_message = NULL, updated_at = ? WHERE id = ?",
                chrono::Utc::now().naive_utc(),
                deployment_id.0,
            )
            .execute(&mut *tx)
            .await?;
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Running,
                &actor,
                None,
            )
            .await?;
            lifecycle::transition_member_slots(tx, deployment_id, SlotState::Running).await?;
            finish_replacement(tx, deployment_id, task.server_id).await?;
        }
        (TaskType::PublishVhost, false) => {
            sqlx::query!(
                "UPDATE deployments SET error_message = ?, updated_at = ? WHERE id = ?",
                report.error,
                chrono::Utc::now().naive_utc(),
                deployment_id.0,
            )
            .execute(&mut *tx)
            .await?;
        }
        (TaskType::QuiesceContainer, true) => {
            let Some(new_id) = d.replaced_by else {
                return Err(AppError::BadRequest(
                    "quiesced deployment has no successor".into(),
                ));
            };
            lifecycle::transition_deployment(
                tx,
                deployment_id,
                DeploymentState::Stopped,
                &actor,
                Some(serde_json::json!({ "retained_for": new_id.to_string() })),
            )
            .await?;
            lifecycle::transition_member_slots(tx, deployment_id, SlotState::Reserved).await?;
            let new_id: DeploymentId = new_id.into();
            if !enqueue_purge_volumes(tx, new_id).await? {
                enqueue_deploy(tx, new_id).await?;
            }
        }
        (TaskType::QuiesceContainer, false) => {
            fail_deployment(tx, deployment_id, report, &actor, false).await?;
            if let Some(new_id) = d.replaced_by {
                fail_replacement_successor(tx, new_id.into(), report, &actor).await?;
            }
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
            // Stopped container still holds its slot(s).
            lifecycle::transition_member_slots(tx, deployment_id, SlotState::Reserved).await?;
            let stopped_predecessor = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM deployments WHERE replaced_by_deployment_id = ? AND state = 'STOPPED' FOR UPDATE",
                deployment_id.0,
            )
            .fetch_one(&mut *tx)
            .await? > 0;
            if stopped_predecessor {
                lifecycle::transition_deployment(
                    tx,
                    deployment_id,
                    DeploymentState::Failed,
                    &actor,
                    Some(serde_json::json!({ "reason": "replacement rolled back" })),
                )
                .await?;
                lifecycle::transition_member_slots(tx, deployment_id, SlotState::Free).await?;
                rollback_replacement(tx, deployment_id, task.server_id, &actor).await?;
                return Ok(());
            }
            // Replacement chain: stopped because a successor waits →
            // remove the old container next (chain continues at REMOVE
            // success).
            if d.replaced_by.is_some() {
                let payload = TaskPayload::Container(foundry_shared::dto::ContainerTarget {
                    deployment_id,
                    container_id: d.adopted_container_id.clone(),
                });
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
            // The container may still be running — keep the slot(s) FAILED.
            fail_deployment(tx, deployment_id, report, &actor, false).await?;
        }
        (TaskType::RestartContainer | TaskType::RollbackContainer, success) => {
            let to = if success {
                DeploymentState::Running
            } else {
                DeploymentState::Failed
            };
            lifecycle::transition_deployment(tx, deployment_id, to, &actor, detail).await?;
            lifecycle::transition_member_slots(
                tx,
                deployment_id,
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
                    // The successor is already healthy and published. Keep
                    // the physical slot RUNNING when finalizing the retained
                    // predecessor; updating through the old row would
                    // otherwise overwrite the successor's slot state.
                    lifecycle::transition_deployment(
                        tx,
                        deployment_id,
                        DeploymentState::Replaced,
                        &actor,
                        Some(serde_json::json!({ "replaced_by": new_id.to_string() })),
                    )
                    .await?;
                    lifecycle::transition_member_slots(tx, new_id.into(), SlotState::Running)
                        .await?;
                    // The successor is already healthy/published. The old
                    // retained rollback container can now be forgotten.
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
                    lifecycle::transition_member_slots(tx, deployment_id, SlotState::Free).await?;
                }
            }
        }
        (TaskType::RemoveContainer, false) => {
            // The container may still be present — keep the slot(s) FAILED.
            fail_deployment(tx, deployment_id, report, &actor, false).await?;
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
        (TaskType::PurgeVolumes, true) => {
            enqueue_deploy(tx, deployment_id).await?;
        }
        (TaskType::PurgeVolumes, false) => {
            // Purge happens before container creation, so no workload can
            // be left behind and the slot is safe to release.
            fail_deployment(tx, deployment_id, report, &actor, true).await?;
        }
        (
            TaskType::RemoveVolume
            | TaskType::RefreshInventory
            | TaskType::UploadLogs
            | TaskType::UpgradeAgent,
            _,
        ) => {}
    }
    Ok(())
}

async fn finish_replacement(
    tx: &mut MySqlConnection,
    successor_id: DeploymentId,
    server_id: ServerId,
) -> Result<(), AppError> {
    let predecessor = sqlx::query!(
        "SELECT id AS `id: Uuid`, state, adopted_container_id FROM deployments
         WHERE replaced_by_deployment_id = ? FOR UPDATE",
        successor_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(predecessor) = predecessor else {
        return Ok(());
    };
    let predecessor_id: DeploymentId = predecessor.id.into();
    let state: DeploymentState = predecessor.state.parse().map_err(AppError::internal)?;
    if state == DeploymentState::Stopped || state == DeploymentState::Failed {
        lifecycle::transition_deployment(
            tx,
            predecessor_id,
            DeploymentState::Removing,
            &Actor::controller(),
            Some(serde_json::json!({ "successor": successor_id.to_string() })),
        )
        .await?;
        enqueue(
            tx,
            server_id,
            Some(predecessor_id),
            TaskType::RemoveContainer,
            &TaskPayload::Container(foundry_shared::dto::ContainerTarget {
                deployment_id: predecessor_id,
                container_id: predecessor.adopted_container_id,
            }),
        )
        .await?;
    }
    Ok(())
}

async fn rollback_replacement(
    tx: &mut MySqlConnection,
    successor_id: DeploymentId,
    server_id: ServerId,
    actor: &Actor,
) -> Result<(), AppError> {
    let predecessor = sqlx::query!(
        "SELECT id AS `id: Uuid`, state, adopted_container_id FROM deployments
         WHERE replaced_by_deployment_id = ? FOR UPDATE",
        successor_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(predecessor) = predecessor else {
        return Ok(());
    };
    let predecessor_id: DeploymentId = predecessor.id.into();
    let state: DeploymentState = predecessor.state.parse().map_err(AppError::internal)?;
    sqlx::query!(
        "UPDATE deployments SET replaced_by_deployment_id = NULL, updated_at = ? WHERE id = ?",
        chrono::Utc::now().naive_utc(),
        predecessor_id.0,
    )
    .execute(&mut *tx)
    .await?;
    match state {
        DeploymentState::Stopped
            if predecessor_was_quiesced(tx, predecessor_id, successor_id).await? =>
        {
            lifecycle::transition_deployment(
                tx,
                predecessor_id,
                DeploymentState::Restarting,
                actor,
                Some(serde_json::json!({ "rollback_from": successor_id.to_string() })),
            )
            .await?;
            lifecycle::transition_member_slots(tx, predecessor_id, SlotState::Reserved).await?;
            let predecessor_ports = load_port_bindings(tx, predecessor_id).await?;
            enqueue(
                tx,
                server_id,
                Some(predecessor_id),
                TaskType::RollbackContainer,
                &TaskPayload::Replacement(foundry_shared::dto::ReplacementTarget {
                    container: foundry_shared::dto::ContainerTarget {
                        deployment_id: predecessor_id,
                        container_id: predecessor.adopted_container_id,
                    },
                    ports: predecessor_ports,
                }),
            )
            .await?;
        }
        DeploymentState::Stopped => {
            // It was already stopped before replacement began, so there is
            // no retained live workload to restart. Preserve that state and
            // its reservation exactly as the user left it.
            lifecycle::transition_member_slots(tx, predecessor_id, SlotState::Reserved).await?;
        }
        DeploymentState::Failed => {
            lifecycle::transition_member_slots(tx, predecessor_id, SlotState::Failed).await?;
        }
        DeploymentState::Running => {
            lifecycle::transition_member_slots(tx, predecessor_id, SlotState::Running).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn retained_predecessor(
    tx: &mut MySqlConnection,
    successor_id: DeploymentId,
) -> Result<Option<DeploymentId>, AppError> {
    let predecessor = sqlx::query_scalar!(
        "SELECT p.id AS `id: Uuid` FROM deployments p
         WHERE p.replaced_by_deployment_id = ? AND p.state = 'STOPPED'
           AND EXISTS (
             SELECT 1 FROM deployment_events e
             WHERE e.deployment_id = p.id AND e.from_state = 'STOPPING'
               AND e.to_state = 'STOPPED'
               AND JSON_UNQUOTE(JSON_EXTRACT(e.detail, '$.retained_for')) = ?
           )
         FOR UPDATE",
        successor_id.0,
        successor_id.to_string(),
    )
    .fetch_optional(&mut *tx)
    .await?;
    Ok(predecessor.map(Into::into))
}

async fn predecessor_was_quiesced(
    tx: &mut MySqlConnection,
    predecessor_id: DeploymentId,
    successor_id: DeploymentId,
) -> Result<bool, AppError> {
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM deployment_events e
         WHERE e.deployment_id = ? AND e.from_state = 'STOPPING'
           AND e.to_state = 'STOPPED'
           AND JSON_UNQUOTE(JSON_EXTRACT(e.detail, '$.retained_for')) = ?",
        predecessor_id.0,
        successor_id.to_string(),
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(count > 0)
}

async fn fail_replacement_successor(
    tx: &mut MySqlConnection,
    successor_id: DeploymentId,
    report: &TaskResultReport,
    actor: &Actor,
) -> Result<(), AppError> {
    fail_deployment(tx, successor_id, report, actor, true).await?;
    let predecessor = sqlx::query!(
        "SELECT id AS `id: Uuid`, state FROM deployments WHERE replaced_by_deployment_id = ? FOR UPDATE",
        successor_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(predecessor) = predecessor {
        let predecessor_id: DeploymentId = predecessor.id.into();
        if predecessor.state == DeploymentState::Running.as_str() {
            lifecycle::transition_member_slots(tx, predecessor_id, SlotState::Running).await?;
            sqlx::query!(
                "UPDATE deployments SET replaced_by_deployment_id = NULL, updated_at = ? WHERE id = ?",
                chrono::Utc::now().naive_utc(),
                predecessor_id.0,
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

async fn load_port_bindings(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
) -> Result<Vec<foundry_shared::dto::PortBinding>, AppError> {
    sqlx::query!(
        "SELECT container_port, host_port, protocol, kind, hostname, is_primary,
                health_path, max_body_size_bytes, proxy_timeout_seconds
         FROM deployment_ports WHERE deployment_id = ? ORDER BY container_port",
        deployment_id.0,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|port| {
        Ok(foundry_shared::dto::PortBinding {
            container_port: port.container_port,
            host_port: port.host_port,
            protocol: port.protocol,
            kind: port.kind.parse().map_err(AppError::internal)?,
            hostname: port.hostname,
            primary: port.is_primary != 0,
            health_path: port.health_path,
            max_body_size_bytes: port.max_body_size_bytes,
            proxy_timeout_seconds: port.proxy_timeout_seconds,
        })
    })
    .collect()
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
    // Fan out over every member slot (1 individual, N group).
    lifecycle::transition_member_slots(
        tx,
        deployment_id,
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
                  d.gpu_group_id AS "gpu_group_id: Uuid",
                  d.mem_limit_mb AS "mem_limit_mb?: u32", t.size_bytes,
                  COALESCE(gs.mig_uuid, g.gpu_uuid) AS "gpu_device_uuid!"
           FROM deployments d
           JOIN gpu_slots gs ON gs.id = d.gpu_slot_id
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN registry_tags t ON t.id = d.registry_tag_id
           WHERE d.id = ?"#,
        deployment_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;

    // All member slots + their NVML device UUIDs, GPU-index ordered so
    // `nvidia-smi` lists them predictably (1 for an individual deploy,
    // N for a group → one DeviceRequest over the whole set).
    let members = sqlx::query!(
        r#"SELECT gs.id AS "slot_id: Uuid",
                  COALESCE(gs.mig_uuid, g.gpu_uuid) AS "device_uuid!"
           FROM deployment_slots ds
           JOIN gpu_slots gs ON gs.id = ds.gpu_slot_id
           JOIN gpus g ON g.id = gs.gpu_id
           WHERE ds.deployment_id = ?
           ORDER BY g.display_index, gs.id"#,
        deployment_id.0
    )
    .fetch_all(&mut *tx)
    .await?;
    let gpu_device_uuids: Vec<String> = members.iter().map(|m| m.device_uuid.clone()).collect();
    let slot_ids: Vec<SlotId> = members.iter().map(|m| m.slot_id.into()).collect();

    let ports = sqlx::query!(
        "SELECT container_port, host_port, protocol, kind, hostname, is_primary,
                health_path, max_body_size_bytes, proxy_timeout_seconds FROM deployment_ports
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
            primary: r.is_primary != 0,
            health_path: r.health_path,
            max_body_size_bytes: r.max_body_size_bytes,
            proxy_timeout_seconds: r.proxy_timeout_seconds,
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
        gpu_device_uuids,
        slot_id: d.slot_id.into(),
        slot_ids,
        gpu_group_id: d.gpu_group_id.map(Into::into),
        slot_name: d.slot_name,
        ports,
        env: Vec::new(), // injected at dispatch
        volumes,
        registry_auth: None, // minted at dispatch
        mem_limit_mb: d.mem_limit_mb,
        image_size_bytes: d.size_bytes.and_then(|size| u64::try_from(size).ok()),
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

pub async fn enqueue_prepare(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
) -> Result<TaskId, AppError> {
    let task_id = enqueue_deploy(tx, deployment_id).await?;
    sqlx::query!(
        "UPDATE agent_tasks SET task_type = 'PREPARE_DEPLOY' WHERE id = ?",
        task_id.0,
    )
    .execute(&mut *tx)
    .await?;
    Ok(task_id)
}

/// Retry only nginx publication for a healthy container retained after a
/// recoverable publish failure. No image pull/container recreation occurs.
pub async fn enqueue_publish(
    pool: &MySqlPool,
    deployment_id: DeploymentId,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        "SELECT server_id AS `server_id: Uuid`, state FROM deployments WHERE id = ? FOR UPDATE",
        deployment_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;
    super::deployments::require_server_ready(&mut tx, row.server_id.into(), true).await?;
    if row.state != DeploymentState::PublishFailed.as_str() {
        return Err(AppError::BadRequest(
            "deployment is not waiting for publication retry".into(),
        ));
    }
    let pending = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM agent_tasks WHERE deployment_id = ? AND task_type = 'PUBLISH_VHOST' AND state IN ('QUEUED','DISPATCHED') FOR UPDATE",
        deployment_id.0,
    )
    .fetch_one(&mut *tx)
    .await?;
    if pending > 0 {
        return Err(AppError::BadRequest(
            "publication retry is already queued".into(),
        ));
    }
    let ports = sqlx::query!(
        "SELECT container_port, host_port, protocol, kind, hostname, is_primary,
                health_path, max_body_size_bytes, proxy_timeout_seconds
         FROM deployment_ports WHERE deployment_id = ?",
        deployment_id.0,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|port| {
        Ok(foundry_shared::dto::PortBinding {
            container_port: port.container_port,
            host_port: port.host_port,
            protocol: port.protocol,
            kind: port.kind.parse().map_err(AppError::internal)?,
            hostname: port.hostname,
            primary: port.is_primary != 0,
            health_path: port.health_path,
            max_body_size_bytes: port.max_body_size_bytes,
            proxy_timeout_seconds: port.proxy_timeout_seconds,
        })
    })
    .collect::<Result<Vec<_>, AppError>>()?;
    enqueue(
        &mut tx,
        row.server_id.into(),
        Some(deployment_id),
        TaskType::PublishVhost,
        &TaskPayload::Publish(foundry_shared::dto::PublishPayload {
            deployment_id,
            ports,
        }),
    )
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "DEPLOYMENT_PUBLISH_RETRIED",
            subject_type: Some("deployment"),
            subject_id: Some(deployment_id.0),
            detail: None,
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Queue one atomic purge task for every mount marked purge-on-redeploy.
/// A single task preserves the ordering guarantee: all selected directories
/// are clean before the following DEPLOY_CONTAINER can be claimed.
async fn enqueue_purge_volumes(
    tx: &mut MySqlConnection,
    deployment_id: DeploymentId,
) -> Result<bool, AppError> {
    let row = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid"
           FROM deployments WHERE id = ?"#,
        deployment_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;
    let volumes = sqlx::query!(
        r#"SELECT server_volume_id AS "volume_id!: Uuid", host_path
           FROM deployment_volumes
           WHERE deployment_id = ? AND purge_on_redeploy = 1
             AND server_volume_id IS NOT NULL
           ORDER BY container_path"#,
        deployment_id.0,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|volume| foundry_shared::dto::VolumeTarget {
        volume_id: volume.volume_id.into(),
        path: volume.host_path,
    })
    .collect::<Vec<_>>();
    if volumes.is_empty() {
        return Ok(false);
    }
    super::volumes::require_purge_support(tx, row.server_id.into()).await?;
    enqueue(
        tx,
        row.server_id.into(),
        Some(deployment_id),
        TaskType::PurgeVolumes,
        &TaskPayload::VolumeBatch(foundry_shared::dto::VolumeBatchTarget { volumes }),
    )
    .await?;
    Ok(true)
}

/// User-facing lifecycle actions → queued tasks (stop/restart/remove).
pub async fn enqueue_lifecycle(
    pool: &MySqlPool,
    deployment: &super::deployments::DeploymentRow,
    task_type: TaskType,
    from_to: (DeploymentState, DeploymentState),
    user: UserId,
    ip_address: Option<&str>,
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
        lifecycle::transition_member_slots(&mut tx, deployment.id, SlotState::Stopping).await?;
    }
    let payload = TaskPayload::Container(foundry_shared::dto::ContainerTarget {
        deployment_id: deployment.id,
        container_id: deployment.adopted_container_id.clone(),
    });
    enqueue(
        &mut tx,
        deployment.server_id,
        Some(deployment.id),
        task_type,
        &payload,
    )
    .await?;
    let action = match task_type {
        TaskType::StopContainer => "DEPLOYMENT_STOP_REQUESTED",
        TaskType::RemoveContainer => "DEPLOYMENT_REMOVE_REQUESTED",
        _ => "DEPLOYMENT_ACTION_REQUESTED",
    };
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action,
            subject_type: Some("deployment"),
            subject_id: Some(deployment.id.0),
            detail: Some(serde_json::json!({
                "task_type": task_type.as_str(),
                "from_state": from_to.0.as_str(),
                "to_state": from_to.1.as_str(),
            })),
            ip_address,
        },
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
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let wants_web = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM deployment_ports
         WHERE deployment_id = ? AND kind IN ('HTTP','HTTPS')",
        deployment.id.0,
    )
    .fetch_one(&mut *tx)
    .await?
        > 0;
    super::deployments::require_server_ready(&mut tx, deployment.server_id, wants_web).await?;
    lifecycle::transition_deployment(
        &mut tx,
        deployment.id,
        DeploymentState::Restarting,
        &Actor::user(user),
        None,
    )
    .await?;
    if !enqueue_purge_volumes(&mut tx, deployment.id).await? {
        enqueue_deploy(&mut tx, deployment.id).await?;
    }
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "DEPLOYMENT_RESTART_REQUESTED",
            subject_type: Some("deployment"),
            subject_id: Some(deployment.id.0),
            detail: None,
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

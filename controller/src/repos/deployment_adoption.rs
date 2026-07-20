//! Atomic adoption of a running unmanaged Docker container into Foundry's
//! deployment lifecycle.

use foundry_shared::{DeploymentId, ServerId, SlotId, SlotState, UserId};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::lifecycle;

pub async fn adopt(
    pool: &MySqlPool,
    server_id: ServerId,
    container_id: &str,
    created_by: UserId,
    ip_address: Option<&str>,
) -> Result<DeploymentId, AppError> {
    let now = chrono::Utc::now().naive_utc();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "SELECT container_id FROM server_containers \
         WHERE server_id = ? AND container_id = ? FOR UPDATE",
    )
    .bind(server_id.0)
    .bind(container_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound(
        "container not found in the latest snapshot",
    ))?;
    let c = sqlx::query!(
        r#"SELECT name, image, gpu_uuids, managed AS "managed: bool"
           FROM server_containers WHERE server_id = ? AND container_id = ?"#,
        server_id.0,
        container_id,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound(
        "container not found in the latest snapshot",
    ))?;
    if c.managed {
        return Err(AppError::BadRequest(
            "this container is already managed by Foundry".into(),
        ));
    }
    let container_state: String = sqlx::query_scalar(
        "SELECT state FROM server_containers WHERE server_id = ? AND container_id = ?",
    )
    .bind(server_id.0)
    .bind(container_id)
    .fetch_one(&mut *tx)
    .await?;
    if container_state != "running" {
        return Err(AppError::BadRequest(
            "only a currently running container can be adopted".into(),
        ));
    }
    let already = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployments
           WHERE server_id = ? AND adopted_container_id = ?
             AND state NOT IN ('REMOVED','REPLACED','FAILED','STOPPED')"#,
        server_id.0,
        container_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if already > 0 {
        return Err(AppError::BadRequest(
            "this container is already adopted".into(),
        ));
    }
    let uuids: Vec<String> = c
        .gpu_uuids
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    if uuids.is_empty() {
        return Err(AppError::BadRequest(
            "only a container occupying a GPU can be adopted".into(),
        ));
    }
    let mut member_slot_ids = Vec::new();
    for u in &uuids {
        if let Some(s) = sqlx::query!(
            r#"SELECT gs.id AS "id: Uuid"
               FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id
               WHERE g.server_id = ? AND COALESCE(gs.mig_uuid, g.gpu_uuid) = ?
               FOR UPDATE"#,
            server_id.0,
            u,
        )
        .fetch_optional(&mut *tx)
        .await?
        {
            member_slot_ids.push(SlotId::from(s.id));
        }
    }
    if member_slot_ids.is_empty() {
        return Err(AppError::BadRequest(
            "the container's GPU does not map to any known slot on this server".into(),
        ));
    }
    let id = DeploymentId::new();
    sqlx::query!(
        r#"INSERT INTO deployments
           (id, gpu_slot_id, server_id, image_ref, created_by, state, container_name,
            container_id, adopted_container_id, started_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, 'RUNNING', ?, ?, ?, ?, ?, ?)"#,
        id.0,
        member_slot_ids[0].0,
        server_id.0,
        c.image.chars().take(1024).collect::<String>(),
        created_by.0,
        c.name.chars().take(255).collect::<String>(),
        container_id,
        container_id,
        now,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(created_by),
            action: "CONTAINER_ADOPTED",
            subject_type: Some("deployment"),
            subject_id: Some(id.0),
            detail: Some(serde_json::json!({
                "server_id": server_id,
                "container_id": container_id,
            })),
            ip_address,
        },
    )
    .await?;
    for slot_id in &member_slot_ids {
        sqlx::query!(
            "INSERT INTO deployment_slots (deployment_id, gpu_slot_id) VALUES (?, ?)",
            id.0,
            slot_id.0,
        )
        .execute(&mut *tx)
        .await?;
    }
    lifecycle::transition_member_slots(&mut tx, id, SlotState::Running).await?;
    sqlx::query!(
        r#"INSERT INTO deployment_events
           (id, deployment_id, from_state, to_state, actor_type, actor_id, detail, created_at)
           VALUES (?, ?, NULL, 'RUNNING', 'User', ?, ?, ?)"#,
        Uuid::now_v7(),
        id.0,
        created_by.0,
        serde_json::to_string(&serde_json::json!({ "adopted_container_id": container_id }))
            .map_err(AppError::internal)?,
        now,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

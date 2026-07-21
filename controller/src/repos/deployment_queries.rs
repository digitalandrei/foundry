//! Read side of deployments. List polling is deliberately batched: one base
//! query plus one ports query and one member-slot query, independent of the
//! number of deployments returned.

use foundry_shared::dto::{DeploymentPort, DeploymentSummary};
use foundry_shared::{DeploymentId, DeploymentState, GpuGroupId, ServerId, SlotId, UserId};
use sqlx::{MySql, MySqlPool, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::AppError;

pub struct DeploymentRow {
    pub id: DeploymentId,
    pub state: DeploymentState,
    pub server_id: ServerId,
    pub slot_id: SlotId,
    pub gpu_group_id: Option<GpuGroupId>,
    pub instance_id: Option<foundry_shared::GitlabInstanceId>,
    pub project_id: Option<foundry_shared::GitlabProjectId>,
    pub registry_tag_id: Option<foundry_shared::RegistryTagId>,
    pub image_digest: Option<String>,
    pub container_name: Option<String>,
    pub created_by: UserId,
    pub adopted_container_id: Option<String>,
}

pub async fn get(pool: &MySqlPool, id: DeploymentId) -> Result<DeploymentRow, AppError> {
    let r = sqlx::query!(
        r#"SELECT d.id AS "id: Uuid", d.state, d.server_id AS "server_id: Uuid", d.container_name,
                  gpu_slot_id AS "slot_id: Uuid",
                  gpu_group_id AS "gpu_group_id: Uuid",
                  gitlab_instance_id AS "instance_id: Uuid",
                  registry_tag_id AS "registry_tag_id: Uuid", image_digest,
                  adopted_container_id,
                  d.created_by AS "created_by: Uuid",
                  r.gitlab_project_id AS "project_id: Uuid"
           FROM deployments d
           LEFT JOIN registry_tags t ON t.id = d.registry_tag_id
           LEFT JOIN registry_repositories r ON r.id = t.registry_repository_id
           WHERE d.id = ?"#,
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;
    Ok(DeploymentRow {
        id: r.id.into(),
        state: r.state.parse().map_err(AppError::internal)?,
        server_id: r.server_id.into(),
        slot_id: r.slot_id.into(),
        gpu_group_id: r.gpu_group_id.map(Into::into),
        instance_id: r.instance_id.map(Into::into),
        project_id: r.project_id.map(Into::into),
        registry_tag_id: r.registry_tag_id.map(Into::into),
        image_digest: r.image_digest,
        container_name: r.container_name,
        created_by: r.created_by.into(),
        adopted_container_id: r.adopted_container_id,
    })
}

pub async fn list(pool: &MySqlPool) -> Result<Vec<DeploymentSummary>, AppError> {
    summaries(pool, None).await
}

pub async fn adopted_for_server(
    pool: &MySqlPool,
    server_id: ServerId,
) -> Result<Vec<foundry_shared::dto::AdoptedContainerRef>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id: Uuid", adopted_container_id AS "cid!"
           FROM deployments
           WHERE server_id = ? AND adopted_container_id IS NOT NULL
             AND state NOT IN ('REMOVED','REPLACED','FAILED','STOPPED')"#,
        server_id.0
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| foundry_shared::dto::AdoptedContainerRef {
            container_id: r.cid,
            deployment_id: r.id.into(),
        })
        .collect())
}

async fn summaries(
    pool: &MySqlPool,
    filter_id: Option<DeploymentId>,
) -> Result<Vec<DeploymentSummary>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT d.id AS "id: Uuid", d.container_name, d.image_ref, d.image_digest, d.state,
                  d.error_message, d.health_status, d.health_detail, d.container_id,
                  (d.adopted_container_id IS NOT NULL) AS "adopted: bool",
                  d.created_at, d.started_at,
                  d.server_id AS "server_id: Uuid", s.name AS server_name,
                  d.gpu_slot_id AS "slot_id: Uuid", gs.name AS slot_name,
                  d.gpu_group_id AS "gpu_group_id: Uuid", gg.name AS "group_name?",
                  g.display_index AS gpu_index, g.model AS gpu_model,
                  u.display_name AS created_by_name
           FROM deployments d
           JOIN servers s ON s.id = d.server_id
           JOIN gpu_slots gs ON gs.id = d.gpu_slot_id
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN users u ON u.id = d.created_by
           LEFT JOIN gpu_groups gg ON gg.id = d.gpu_group_id
           WHERE (? IS NULL AND d.state <> 'REMOVED') OR d.id = ?
           ORDER BY d.created_at DESC
           LIMIT 200"#,
        filter_id.map(|i| i.0),
        filter_id.map(|i| i.0),
    )
    .fetch_all(pool)
    .await?;

    let ids: Vec<DeploymentId> = rows.iter().map(|r| r.id.into()).collect();
    let mut ports_by_deployment = std::collections::HashMap::new();
    let mut slots_by_deployment = std::collections::HashMap::new();
    if !ids.is_empty() {
        let mut ports = QueryBuilder::<MySql>::new(
            "SELECT deployment_id, container_port, host_port, protocol, kind, hostname, \
                    is_primary, health_path, max_body_size_bytes, proxy_timeout_seconds \
             FROM deployment_ports WHERE deployment_id IN (",
        );
        {
            let mut separated = ports.separated(", ");
            for id in &ids {
                separated.push_bind(id.0);
            }
        }
        ports.push(") ORDER BY deployment_id, container_port");
        for p in ports.build().fetch_all(pool).await? {
            let deployment_id: Uuid = p.try_get("deployment_id").map_err(AppError::internal)?;
            let kind: String = p.try_get("kind").map_err(AppError::internal)?;
            ports_by_deployment
                .entry(DeploymentId::from(deployment_id))
                .or_insert_with(Vec::new)
                .push(DeploymentPort {
                    container_port: p.try_get("container_port").map_err(AppError::internal)?,
                    host_port: p.try_get("host_port").map_err(AppError::internal)?,
                    protocol: p.try_get("protocol").map_err(AppError::internal)?,
                    kind: kind.parse().map_err(AppError::internal)?,
                    hostname: p.try_get("hostname").map_err(AppError::internal)?,
                    primary: p.try_get("is_primary").map_err(AppError::internal)?,
                    health_path: p.try_get("health_path").map_err(AppError::internal)?,
                    max_body_size_bytes: p
                        .try_get("max_body_size_bytes")
                        .map_err(AppError::internal)?,
                    proxy_timeout_seconds: p
                        .try_get("proxy_timeout_seconds")
                        .map_err(AppError::internal)?,
                });
        }

        let mut slots = QueryBuilder::<MySql>::new(
            "SELECT deployment_id, gpu_slot_id FROM deployment_slots WHERE deployment_id IN (",
        );
        {
            let mut separated = slots.separated(", ");
            for id in &ids {
                separated.push_bind(id.0);
            }
        }
        slots.push(") ORDER BY deployment_id, gpu_slot_id");
        for s in slots.build().fetch_all(pool).await? {
            let deployment_id: Uuid = s.try_get("deployment_id").map_err(AppError::internal)?;
            let slot_id: Uuid = s.try_get("gpu_slot_id").map_err(AppError::internal)?;
            slots_by_deployment
                .entry(DeploymentId::from(deployment_id))
                .or_insert_with(Vec::new)
                .push(SlotId::from(slot_id));
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: DeploymentId = r.id.into();
        let slot_ids = slots_by_deployment.remove(&id).unwrap_or_default();
        out.push(DeploymentSummary {
            id,
            name: r.container_name.unwrap_or_default(),
            image_ref: r.image_ref,
            image_digest: r.image_digest,
            state: r.state.parse().map_err(AppError::internal)?,
            status_detail: None,
            container_id: r.container_id,
            error_message: r.error_message,
            health_status: r.health_status,
            health_detail: r.health_detail,
            server_id: r.server_id.into(),
            server_name: r.server_name,
            slot_id: r.slot_id.into(),
            slot_name: r.slot_name,
            slot_ids: if slot_ids.is_empty() {
                vec![r.slot_id.into()]
            } else {
                slot_ids
            },
            gpu_group_id: r.gpu_group_id.map(Into::into),
            group_name: r.group_name,
            gpu_label: format!(
                "GPU {}{}",
                r.gpu_index,
                r.gpu_model.map(|m| format!(" ({m})")).unwrap_or_default()
            ),
            created_by_name: r.created_by_name,
            adopted: r.adopted,
            ports: ports_by_deployment.remove(&id).unwrap_or_default(),
            created_at: r.created_at.and_utc(),
            started_at: r.started_at.map(|t| t.and_utc()),
        });
    }
    Ok(out)
}

pub async fn detail(
    pool: &MySqlPool,
    id: DeploymentId,
) -> Result<foundry_shared::dto::DeploymentDetail, AppError> {
    let summary = summaries(pool, Some(id))
        .await?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound("deployment not found"))?;
    let mounts = sqlx::query!(
        r#"SELECT sv.id AS "volume_id: Uuid", sv.name AS "volume_name?: String",
                  sv.placement AS "placement?",
                  dv.host_path, dv.container_path,
                  dv.read_only AS "read_only: bool",
                  dv.purge_on_redeploy AS "purge_on_redeploy: bool"
           FROM deployment_volumes dv
           LEFT JOIN server_volumes sv ON sv.id = dv.server_volume_id
           WHERE dv.deployment_id = ?
           ORDER BY dv.container_path"#,
        id.0
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| {
        Ok(foundry_shared::dto::DeploymentMount {
            volume_id: r.volume_id.map(Into::into),
            volume_name: r.volume_name,
            host_path: r.host_path,
            container_path: r.container_path,
            read_only: r.read_only,
            placement: r
                .placement
                .map(|value| value.parse().map_err(AppError::internal))
                .transpose()?,
            purge_on_redeploy: r.purge_on_redeploy,
        })
    })
    .collect::<Result<Vec<_>, AppError>>()?;
    let env = sqlx::query!(
        r#"SELECT env_key, is_secret AS "is_secret: bool"
           FROM deployment_env WHERE deployment_id = ? ORDER BY env_key"#,
        id.0
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| foundry_shared::dto::DeploymentEnvKey {
        key: r.env_key,
        is_secret: r.is_secret,
    })
    .collect();
    Ok(foundry_shared::dto::DeploymentDetail {
        summary,
        mounts,
        env,
    })
}

//! Locked deployment target resolution and server prechecks.

use foundry_shared::dto::DeployTarget;
use foundry_shared::{DeploymentId, GpuGroupId, ServerId, SlotId, SlotState};
use sqlx::MySqlConnection;
use uuid::Uuid;

use crate::error::AppError;

pub(super) struct ResolvedTarget {
    pub server_id: ServerId,
    pub primary_slot_id: SlotId,
    pub primary_slot_name: String,
    pub member_slot_ids: Vec<SlotId>,
    pub group_id: Option<GpuGroupId>,
}

pub(super) async fn resolve_target(
    tx: &mut MySqlConnection,
    target: &DeployTarget,
    replaces: Option<DeploymentId>,
) -> Result<ResolvedTarget, AppError> {
    let exclude = replaces.map(|d| d.0).unwrap_or_else(Uuid::nil);
    match target {
        DeployTarget::Slot { slot_id } => {
            let slot = sqlx::query!(
                r#"SELECT gs.name AS slot_name, gs.state,
                          gs.max_occupants AS "max_occupants: u32",
                          g.server_id AS "server_id: Uuid"
                   FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id
                   WHERE gs.id = ? FOR UPDATE"#,
                slot_id.0
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("slot not found"))?;
            let slot_state: SlotState = slot.state.parse().map_err(AppError::internal)?;
            if slot_state == SlotState::Offline {
                return Err(AppError::BadRequest("slot is offline".into()));
            }
            let device_uuid: String = sqlx::query_scalar(
                "SELECT COALESCE(gs.mig_uuid, g.gpu_uuid) \
                 FROM gpu_slots gs JOIN gpus g ON g.id = gs.gpu_id WHERE gs.id = ?",
            )
            .bind(slot_id.0)
            .fetch_one(&mut *tx)
            .await?;
            reject_running_external(&mut *tx, slot.server_id.into(), &device_uuid).await?;
            let occupants: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM deployment_slots ds
                   JOIN deployments d ON d.id = ds.deployment_id
                   WHERE ds.gpu_slot_id = ? AND d.id <> ?
                     AND d.gpu_group_id IS NULL
                     AND d.state NOT IN ('REMOVED','REPLACED','FAILED')"#,
                slot_id.0,
                exclude,
            )
            .fetch_one(&mut *tx)
            .await?;
            if occupants as u32 >= slot.max_occupants {
                return Err(AppError::BadRequest(format!(
                    "slot is at capacity ({}/{})",
                    occupants, slot.max_occupants
                )));
            }
            Ok(ResolvedTarget {
                server_id: slot.server_id.into(),
                primary_slot_id: *slot_id,
                primary_slot_name: slot.slot_name,
                member_slot_ids: vec![*slot_id],
                group_id: None,
            })
        }
        DeployTarget::Group { gpu_group_id } => {
            let ctx =
                super::gpu_groups::member_slots_for_deploy(tx, *gpu_group_id, replaces).await?;
            if ctx.group_occupants as u32 >= ctx.max_occupants {
                return Err(AppError::BadRequest(format!(
                    "group is at capacity ({}/{})",
                    ctx.group_occupants, ctx.max_occupants
                )));
            }
            let mut busy = Vec::new();
            for m in &ctx.members {
                if m.slot_state == "OFFLINE" {
                    busy.push(format!("GPU {} (offline)", m.gpu_index));
                } else if m.mig_enabled {
                    busy.push(format!("GPU {} (MIG enabled)", m.gpu_index));
                } else if m.foreign_occupants > 0 {
                    busy.push(format!("GPU {} (in individual use)", m.gpu_index));
                } else {
                    match reject_running_external(&mut *tx, ctx.server_id, &m.device_uuid).await {
                        Ok(()) => {}
                        Err(AppError::BadRequest(_)) => {
                            busy.push(format!("GPU {} (external container running)", m.gpu_index))
                        }
                        Err(err) => return Err(err),
                    }
                }
            }
            if !busy.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "group not deployable — {}",
                    busy.join(", ")
                )));
            }
            Ok(ResolvedTarget {
                server_id: ctx.server_id,
                primary_slot_id: ctx.members[0].slot_id,
                primary_slot_name: ctx.members[0].gpu_index.to_string(),
                member_slot_ids: ctx.members.iter().map(|m| m.slot_id).collect(),
                group_id: Some(*gpu_group_id),
            })
        }
    }
}

async fn reject_running_external(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    device_uuid: &str,
) -> Result<(), AppError> {
    let external: Option<String> = sqlx::query_scalar(
        r#"SELECT name FROM server_containers
           WHERE server_id = ? AND managed = 0 AND state = 'running'
             AND JSON_CONTAINS(COALESCE(gpu_uuids, '[]'), JSON_QUOTE(?))
             AND NOT EXISTS (
                 SELECT 1 FROM deployments d
                 WHERE d.server_id = server_containers.server_id
                   AND d.adopted_container_id = server_containers.container_id
                   AND d.state NOT IN ('REMOVED','REPLACED','FAILED','STOPPED')
             )
           ORDER BY name LIMIT 1 FOR UPDATE"#,
    )
    .bind(server_id.0)
    .bind(device_uuid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(name) = external {
        return Err(AppError::BadRequest(format!(
            "GPU is occupied by external container {name:?}; stop or adopt it before deploying"
        )));
    }
    Ok(())
}

pub(super) struct ServerPrecheck {
    pub status: String,
    pub name: String,
    pub app_publishing_ready: Option<bool>,
    pub nginx_status: Option<String>,
    pub docker_ok: Option<bool>,
}

pub(super) async fn fetch_server_precheck(
    tx: &mut MySqlConnection,
    server_id: ServerId,
) -> Result<ServerPrecheck, AppError> {
    let r = sqlx::query!(
        r#"SELECT status, name,
                  app_publishing_ready AS "app_publishing_ready: bool", nginx_status,
                  docker_ok AS "docker_ok: bool"
           FROM servers WHERE id = ?"#,
        server_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("server not found"))?;
    Ok(ServerPrecheck {
        status: r.status,
        name: r.name,
        app_publishing_ready: r.app_publishing_ready,
        nginx_status: r.nginx_status,
        docker_ok: r.docker_ok,
    })
}

pub(super) fn nginx_status_hint(status: Option<&str>) -> &'static str {
    match status {
        Some("NGINX_MISSING") => {
            "nginx is not installed — install it and run `sudo foundry-agent --setup-apps`"
        }
        Some("NGINX_OUTDATED") => {
            "nginx on the server is too old — Foundry needs ≥ 1.25.1 (the `http2` directive); upgrade nginx"
        }
        Some("NGINX_INACTIVE") => {
            "nginx is installed but not running — start it (`sudo systemctl enable --now nginx`)"
        }
        Some("NOT_CONFIGURED") => {
            "nginx is running but not set up for Foundry — run `sudo foundry-agent --setup-apps`"
        }
        Some("TLS_MISSING") => {
            "the server's wildcard TLS certificate is missing — install fullchain.pem + privkey.pem under /etc/foundry-agent/tls/"
        }
        _ => "the agent reports app publishing is unavailable",
    }
}

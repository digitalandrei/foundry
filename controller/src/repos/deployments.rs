//! Deployment creation, listing, and the per-server host-port
//! allocator (plans/phase-06.md § Networking conditions 1–2, 6).

use foundry_shared::dto::{
    CreateDeploymentRequest, DeploymentPort, DeploymentSummary, EnvSpec, PortSpec,
};
use foundry_shared::{
    DeploymentId, DeploymentState, PortKind, ServerId, SlotId, SlotState, TaskType, UserId,
};
use sqlx::{MySqlConnection, MySqlPool};
use uuid::Uuid;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::lifecycle::{self, Actor};

/// Controller-allocated host-port pool (per server) and the ports we
/// never hand out even if requested.
pub const PORT_POOL: std::ops::RangeInclusive<u16> = 20000..=29999;
const RESERVED_HOST_PORTS: &[u16] = &[22];

pub struct NewDeployment {
    pub id: DeploymentId,
    pub container_name: String,
}

/// Validate + insert a deployment in one transaction: slot must be
/// FREE (locked), ports allocated conflict-free, env stored (secrets
/// encrypted), slot → RESERVED, state PENDING → VALIDATING.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &MySqlPool,
    secrets: &SecretBox,
    req: &CreateDeploymentRequest,
    image_ref: &str,
    instance_id: foundry_shared::GitlabInstanceId,
    created_by: UserId,
    replaces: Option<DeploymentId>,
    apps_domain: Option<&str>,
) -> Result<NewDeployment, AppError> {
    validate_ports(&req.ports, apps_domain)?;
    let owner_slug_in_create = &super::volumes::owner_slug(pool, created_by).await?;
    let now = chrono::Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    // Slot: lock and check.
    let slot = sqlx::query!(
        r#"SELECT gs.id AS "id: Uuid", gs.state, gs.name AS slot_name,
                  g.server_id AS "server_id: Uuid",
                  s.status AS server_status, s.name AS server_name,
                  s.app_publishing_ready AS "app_publishing_ready: bool", s.nginx_status
           FROM gpu_slots gs
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN servers s ON s.id = g.server_id
           WHERE gs.id = ? FOR UPDATE"#,
        req.slot_id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("slot not found"))?;

    if slot.server_status != "ONLINE" {
        return Err(AppError::BadRequest("server is not online".into()));
    }
    // HTTP/S deploys need app publishing on the target server. Fail fast
    // with the agent-reported reason rather than dispatching a deploy
    // that the agent can only fail on (operator request). Only blocks
    // when the agent has explicitly reported not-ready.
    let wants_web = req
        .ports
        .iter()
        .any(|p| matches!(p.kind, PortKind::Http | PortKind::Https));
    if wants_web && slot.app_publishing_ready == Some(false) {
        return Err(AppError::BadRequest(format!(
            "HTTP/S publishing isn't ready on {}: {}. Fix it on the server, then redeploy.",
            slot.server_name,
            nginx_status_hint(slot.nginx_status.as_deref()),
        )));
    }
    let slot_state: SlotState = slot.state.parse().map_err(AppError::internal)?;
    // Replacements deploy onto a slot still occupied by the outgoing
    // deployment (RUNNING/RESERVED); fresh deploys need FREE.
    let slot_ok = match replaces {
        None => slot_state == SlotState::Free,
        Some(_) => matches!(slot_state, SlotState::Running | SlotState::Reserved),
    };
    if !slot_ok {
        return Err(AppError::BadRequest(format!(
            "slot is {slot_state}, not available for this operation"
        )));
    }
    let server_id: ServerId = slot.server_id.into();

    // Name: sanitize or generate.
    let container_name = match req.name.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() => sanitize_name(n)?,
        _ => generate_name(image_ref, &slot.slot_name),
    };

    let mut allocated = allocate_ports(&mut tx, server_id, &req.ports).await?;
    assign_hostnames(
        &mut tx,
        &mut allocated,
        &container_name,
        &slot.server_name,
        apps_domain,
        replaces,
    )
    .await?;

    let id = DeploymentId::new();
    sqlx::query!(
        r#"INSERT INTO deployments
           (id, gpu_slot_id, server_id, registry_tag_id, gitlab_instance_id, image_ref,
            created_by, state, container_name, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, 'PENDING', ?, ?, ?)"#,
        id.0,
        req.slot_id.0,
        server_id.0,
        req.registry_tag_id.0,
        instance_id.0,
        image_ref,
        created_by.0,
        container_name,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    for p in &allocated {
        sqlx::query!(
            r#"INSERT INTO deployment_ports
               (id, deployment_id, container_port, host_port, protocol, kind, hostname, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            id.0,
            p.container_port,
            p.host_port,
            p.protocol,
            p.kind.as_str(),
            p.hostname,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }
    // Persistent volumes: create-or-reuse the requester's named volumes
    // (per-user namespace) and bind them.
    if !req.volumes.is_empty() {
        let slug = owner_slug_in_create;
        let mut seen_paths = std::collections::HashSet::new();
        for v in &req.volumes {
            let container_path = v.container_path.trim();
            if !container_path.starts_with('/') || container_path.len() > 255 {
                return Err(AppError::BadRequest(format!(
                    "mount path {container_path:?} must be absolute"
                )));
            }
            if !seen_paths.insert(container_path.to_string()) {
                return Err(AppError::BadRequest(format!(
                    "duplicate mount path {container_path}"
                )));
            }
            let (volume_id, host_path) =
                super::volumes::ensure(&mut tx, server_id, &v.volume_name, created_by, slug)
                    .await?;
            sqlx::query!(
                r#"INSERT INTO deployment_volumes
                   (id, deployment_id, server_volume_id, host_path, container_path, read_only, created_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#,
                Uuid::now_v7(),
                id.0,
                volume_id.0,
                host_path,
                container_path,
                v.read_only,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    for e in &req.env {
        validate_env(e)?;
        let value: Vec<u8> = if e.is_secret {
            secrets.encrypt_str(&e.value)
        } else {
            e.value.clone().into_bytes()
        };
        sqlx::query!(
            r#"INSERT INTO deployment_env
               (id, deployment_id, env_key, env_value, is_secret, created_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            id.0,
            e.key,
            value,
            e.is_secret,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }

    match replaces {
        None => {
            lifecycle::transition_slot(&mut tx, req.slot_id, SlotState::Reserved).await?;
        }
        Some(old_id) => {
            // Replacement orchestration is atomic with the successor's
            // creation (review finding): lock the old row, validate,
            // link, transition, and enqueue its stop/remove here — a
            // crash can no longer strand a linked successor without a
            // queued task.
            let old = sqlx::query!(
                "SELECT state FROM deployments WHERE id = ? FOR UPDATE",
                old_id.0
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("deployment to replace not found"))?;
            let old_state: DeploymentState = old.state.parse().map_err(AppError::internal)?;
            if !matches!(
                old_state,
                DeploymentState::Running | DeploymentState::Stopped | DeploymentState::Failed
            ) {
                return Err(AppError::BadRequest(format!(
                    "deployment is {old_state}, not replaceable"
                )));
            }
            sqlx::query!(
                "UPDATE deployments SET replaced_by_deployment_id = ?, updated_at = ? WHERE id = ?",
                id.0,
                now,
                old_id.0,
            )
            .execute(&mut *tx)
            .await?;
            let (task_type, to) = if old_state == DeploymentState::Running {
                (TaskType::StopContainer, DeploymentState::Stopping)
            } else {
                (TaskType::RemoveContainer, DeploymentState::Removing)
            };
            lifecycle::transition_deployment(&mut tx, old_id, to, &Actor::user(created_by), None)
                .await?;
            if task_type == TaskType::StopContainer {
                lifecycle::transition_slot(&mut tx, req.slot_id, SlotState::Stopping).await?;
            }
            super::tasks::enqueue(
                &mut tx,
                server_id,
                Some(old_id),
                task_type,
                &foundry_shared::dto::TaskPayload::Container(
                    foundry_shared::dto::ContainerTarget {
                        deployment_id: old_id,
                    },
                ),
            )
            .await?;
        }
    }
    // PENDING row exists; record the validation step (slot+ports+image
    // checked synchronously right here).
    lifecycle::transition_deployment(
        &mut tx,
        id,
        DeploymentState::Validating,
        &Actor::user(created_by),
        Some(serde_json::json!({ "image_ref": image_ref, "replaces": replaces.map(|r| r.to_string()) })),
    )
    .await?;

    tx.commit().await?;
    Ok(NewDeployment { id, container_name })
}

/// Operator-readable reason behind a not-ready app-publishing server.
fn nginx_status_hint(status: Option<&str>) -> &'static str {
    match status {
        Some("NGINX_MISSING") => {
            "nginx is not installed — install it and run `sudo foundry-agent --setup-apps`"
        }
        Some("NGINX_INACTIVE") => {
            "nginx is installed but not running — start it (`sudo systemctl enable --now nginx`)"
        }
        Some("NOT_CONFIGURED") => {
            "nginx is running but not set up for Foundry — run `sudo foundry-agent --setup-apps`"
        }
        _ => "the agent reports app publishing is unavailable",
    }
}

fn validate_ports(specs: &[PortSpec], apps_domain: Option<&str>) -> Result<(), AppError> {
    if specs.len() > 32 {
        return Err(AppError::BadRequest("too many ports (max 32)".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for p in specs {
        if p.container_port == 0 {
            return Err(AppError::BadRequest("container port 0 is invalid".into()));
        }
        if !seen.insert((p.container_port, p.kind.protocol())) {
            return Err(AppError::BadRequest(format!(
                "duplicate container port {}",
                p.container_port
            )));
        }
        if let Some(host) = p.host_port {
            if matches!(p.kind, PortKind::Http | PortKind::Https) {
                return Err(AppError::BadRequest(
                    "HTTP/HTTPS ports are proxy-published; host port cannot be pinned".into(),
                ));
            }
            if !PORT_POOL.contains(&host) || RESERVED_HOST_PORTS.contains(&host) {
                return Err(AppError::BadRequest(format!(
                    "host port {host} is outside the allowed pool ({}–{})",
                    PORT_POOL.start(),
                    PORT_POOL.end()
                )));
            }
        }
        if matches!(p.kind, PortKind::Http | PortKind::Https) && apps_domain.is_none() {
            return Err(AppError::BadRequest(
                "HTTP/HTTPS publishing is disabled: FOUNDRY_APPS_DOMAIN is not configured".into(),
            ));
        }
    }
    Ok(())
}

fn validate_env(e: &EnvSpec) -> Result<(), AppError> {
    let ok = !e.key.is_empty()
        && e.key.len() <= 128
        && e.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !e.key.starts_with(|c: char| c.is_ascii_digit());
    if !ok {
        return Err(AppError::BadRequest(format!("invalid env key {:?}", e.key)));
    }
    if e.value.len() > 4096 {
        return Err(AppError::BadRequest("env value too long".into()));
    }
    Ok(())
}

fn sanitize_name(name: &str) -> Result<String, AppError> {
    let ok = name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if !ok {
        return Err(AppError::BadRequest(
            "name must be alphanumeric/dash/underscore, ≤63 chars".into(),
        ));
    }
    Ok(name.to_string())
}

/// Lowercase LDH DNS label (no leading/trailing `-`); None when nothing
/// usable remains. Shared by the container-name and server-name slugs
/// that build an app hostname.
fn dns_label(raw: &str) -> Option<String> {
    let label = raw
        .to_lowercase()
        .chars()
        .map(|c| if c == '_' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!label.is_empty()).then_some(label)
}

/// Generated names embed the GPU slot hint (operator request):
/// `procms-g0-x7f2`, `procms-g0-3-x7f2` for MIG slice 0:3. The
/// authoritative GPU assignment is the foundry.gpu_uuid/slot labels.
fn generate_name(image_ref: &str, slot_name: &str) -> String {
    let base = image_ref
        .rsplit('/')
        .next()
        .unwrap_or("app")
        .split(':')
        .next()
        .unwrap_or("app")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(20)
        .collect::<String>();
    let gpu_hint: String = slot_name
        .chars()
        .map(|c| if c == ':' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();
    let suffix: String = uuid::Uuid::now_v7().simple().to_string()[27..32].to_string();
    format!("{base}-g{gpu_hint}-{suffix}")
}

/// Pick conflict-free host ports against every active claim on the
/// server (rows locked — condition 1 in plans/phase-06.md).
async fn allocate_ports(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    specs: &[PortSpec],
) -> Result<Vec<DeploymentPort>, AppError> {
    // Serialize allocation per server: FOR UPDATE on existing port
    // rows alone locks nothing when the claim set is empty (review
    // finding) — the server row is the allocation mutex.
    sqlx::query!(
        "SELECT id AS `id: Uuid` FROM servers WHERE id = ? FOR UPDATE",
        server_id.0
    )
    .fetch_one(&mut *tx)
    .await?;
    // A FAILED deploy that never created a container released its slot
    // (auto-heal); it must release its host ports too, so it is excluded
    // here. A FAILED deployment that still has a container keeps them.
    let rows = sqlx::query!(
        "SELECT dp.host_port FROM deployment_ports dp
         JOIN deployments d ON d.id = dp.deployment_id
         WHERE d.server_id = ?
           AND (d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                            'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
                OR (d.state = 'FAILED' AND d.container_id IS NOT NULL))
         FOR UPDATE",
        server_id.0
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut used: std::collections::HashSet<u16> = rows.into_iter().map(|r| r.host_port).collect();
    used.extend(RESERVED_HOST_PORTS);

    let mut out = Vec::with_capacity(specs.len());
    let mut cursor = *PORT_POOL.start();
    for spec in specs {
        let host = match spec.host_port {
            Some(h) => {
                if !used.insert(h) {
                    return Err(AppError::BadRequest(format!(
                        "host port {h} is already in use on this server"
                    )));
                }
                h
            }
            None => loop {
                let candidate = cursor;
                if candidate > *PORT_POOL.end() {
                    return Err(AppError::BadRequest(
                        "no free host ports left in the pool".into(),
                    ));
                }
                cursor += 1;
                if used.insert(candidate) {
                    break candidate;
                }
            },
        };
        out.push(DeploymentPort {
            container_port: spec.container_port,
            host_port: host,
            protocol: spec.kind.protocol().to_string(),
            kind: spec.kind,
            hostname: None,
        });
    }
    Ok(out)
}

/// Hostname per HTTP/S port: `<name>.<server>.<domain>` — a per-server
/// subdomain so DNS and the wildcard cert (`*.<server>.<domain>`,
/// operator-issued per server) map predictably (operator design,
/// 0.11.0). Several web ports disambiguate with `<name>-<port>`. The
/// per-server nginx (agent-managed) serves it. Hostnames are globally
/// unique across active deployments — the URL routes to exactly one
/// container; the deployment being replaced is exempt so its successor
/// keeps the URL (the chain removes the old vhost first).
async fn assign_hostnames(
    tx: &mut MySqlConnection,
    ports: &mut [DeploymentPort],
    container_name: &str,
    server_name: &str,
    apps_domain: Option<&str>,
    replaces: Option<DeploymentId>,
) -> Result<(), AppError> {
    let web_count = ports
        .iter()
        .filter(|p| matches!(p.kind, PortKind::Http | PortKind::Https))
        .count();
    if web_count == 0 {
        return Ok(());
    }
    let domain = apps_domain
        .ok_or_else(|| AppError::BadRequest("FOUNDRY_APPS_DOMAIN is not configured".into()))?;

    let slug = dns_label(container_name).ok_or_else(|| {
        AppError::BadRequest("name reduces to an empty app hostname — use letters or digits".into())
    })?;
    // The server name is admin-set and DNS-safe already, but slugify
    // defensively — it is the cert's wildcard label (`*.<server>.…`).
    let server_slug = dns_label(server_name).ok_or_else(|| {
        AppError::BadRequest(format!(
            "server name {server_name:?} has no DNS-safe form — rename the server to publish apps"
        ))
    })?;

    // Never matches a real row when this is not a replacement.
    let exempt = replaces.map(|d| d.0).unwrap_or_else(Uuid::nil);
    for p in ports.iter_mut() {
        if !matches!(p.kind, PortKind::Http | PortKind::Https) {
            continue;
        }
        let label = if web_count == 1 {
            slug.clone()
        } else {
            format!("{slug}-{}", p.container_port)
        };
        if label.len() > 63 {
            return Err(AppError::BadRequest(format!(
                "app hostname label {label:?} exceeds 63 characters — pick a shorter name"
            )));
        }
        let hostname = format!("{label}.{server_slug}.{domain}");
        // Same rule as port allocation: a FAILED deploy with no
        // container freed its slot, so it no longer holds its hostname.
        let taken = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM deployment_ports dp
               JOIN deployments d ON d.id = dp.deployment_id
               WHERE dp.hostname = ?
                 AND d.id <> ?
                 AND (d.state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                                  'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
                      OR (d.state = 'FAILED' AND d.container_id IS NOT NULL))
               FOR UPDATE"#,
            hostname,
            exempt,
        )
        .fetch_one(&mut *tx)
        .await?;
        if taken > 0 {
            return Err(AppError::BadRequest(format!(
                "https://{hostname} is already published by another deployment — pick a different name"
            )));
        }
        p.hostname = Some(hostname);
    }
    Ok(())
}

/// Decrypted env for the task payload (secrets in memory only).
pub async fn env_for_payload(
    pool: &MySqlPool,
    secrets: &SecretBox,
    id: DeploymentId,
) -> Result<Vec<(String, String)>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT env_key, env_value, is_secret AS "is_secret: bool"
           FROM deployment_env WHERE deployment_id = ?"#,
        id.0
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            let value = if r.is_secret {
                secrets
                    .decrypt_str(&r.env_value)
                    .map_err(AppError::internal)?
            } else {
                String::from_utf8(r.env_value).map_err(AppError::internal)?
            };
            Ok((r.env_key, value))
        })
        .collect()
}

pub struct DeploymentRow {
    pub id: DeploymentId,
    pub state: DeploymentState,
    pub server_id: ServerId,
    pub slot_id: SlotId,
    pub instance_id: foundry_shared::GitlabInstanceId,
    pub created_by: UserId,
}

/// Dismiss a FAILED deployment: mark it REMOVED (clears it from the
/// active list — it stays as an audit/event log) and free the slot if
/// it is still stuck FAILED. Controller-side only — a failed deploy
/// left no container, so no agent round-trip is needed (0.11.0).
pub async fn dismiss(pool: &MySqlPool, id: DeploymentId) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        r#"SELECT state, gpu_slot_id AS "slot_id: Uuid" FROM deployments
           WHERE id = ? FOR UPDATE"#,
        id.0
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("deployment not found"))?;
    let state: DeploymentState = row.state.parse().map_err(AppError::internal)?;
    if state != DeploymentState::Failed {
        return Err(AppError::BadRequest(
            "only a failed deployment can be dismissed".into(),
        ));
    }
    let slot_id: SlotId = row.slot_id.into();
    lifecycle::transition_deployment(
        &mut tx,
        id,
        DeploymentState::Removed,
        &Actor::controller(),
        Some(serde_json::json!({ "reason": "dismissed by operator" })),
    )
    .await?;
    // Only a still-FAILED slot is freed — never steal one another
    // deployment has since taken (it would be RUNNING/RESERVED/etc).
    sqlx::query!(
        "UPDATE gpu_slots SET state = 'FREE', updated_at = ?
         WHERE id = ? AND state = 'FAILED'",
        chrono::Utc::now().naive_utc(),
        slot_id.0,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get(pool: &MySqlPool, id: DeploymentId) -> Result<DeploymentRow, AppError> {
    let r = sqlx::query!(
        r#"SELECT id AS "id: Uuid", state, server_id AS "server_id: Uuid",
                  gpu_slot_id AS "slot_id: Uuid",
                  gitlab_instance_id AS "instance_id: Uuid",
                  created_by AS "created_by: Uuid"
           FROM deployments WHERE id = ?"#,
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
        instance_id: r.instance_id.into(),
        created_by: r.created_by.into(),
    })
}

pub async fn list(pool: &MySqlPool) -> Result<Vec<DeploymentSummary>, AppError> {
    summaries(pool, None).await
}

/// One query serves both the list (recent, REMOVED filtered) and the
/// detail lookup (any state — history stays inspectable).
async fn summaries(
    pool: &MySqlPool,
    filter_id: Option<DeploymentId>,
) -> Result<Vec<DeploymentSummary>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT d.id AS "id: Uuid", d.container_name, d.image_ref, d.state,
                  d.error_message, d.container_id,
                  d.created_at, d.started_at,
                  d.server_id AS "server_id: Uuid", s.name AS server_name,
                  d.gpu_slot_id AS "slot_id: Uuid", gs.name AS slot_name,
                  g.display_index AS gpu_index, g.model AS gpu_model,
                  u.display_name AS created_by_name
           FROM deployments d
           JOIN servers s ON s.id = d.server_id
           JOIN gpu_slots gs ON gs.id = d.gpu_slot_id
           JOIN gpus g ON g.id = gs.gpu_id
           JOIN users u ON u.id = d.created_by
           WHERE (? IS NULL AND d.state <> 'REMOVED') OR d.id = ?
           ORDER BY d.created_at DESC
           LIMIT 200"#,
        filter_id.map(|i| i.0),
        filter_id.map(|i| i.0),
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: DeploymentId = r.id.into();
        let port_rows = sqlx::query!(
            "SELECT container_port, host_port, protocol, kind, hostname FROM deployment_ports
             WHERE deployment_id = ? ORDER BY container_port",
            id.0
        )
        .fetch_all(pool)
        .await?;
        out.push(DeploymentSummary {
            id,
            name: r.container_name.unwrap_or_default(),
            image_ref: r.image_ref,
            state: r.state.parse().map_err(AppError::internal)?,
            // Transient text lives in AppState.progress; the route
            // layer overlays it (in-memory by design).
            status_detail: None,
            container_id: r.container_id,
            error_message: r.error_message,
            server_id: r.server_id.into(),
            server_name: r.server_name,
            slot_id: r.slot_id.into(),
            slot_name: r.slot_name,
            gpu_label: format!(
                "GPU {}{}",
                r.gpu_index,
                r.gpu_model.map(|m| format!(" ({m})")).unwrap_or_default()
            ),
            created_by_name: r.created_by_name,
            ports: port_rows
                .into_iter()
                .map(|p| {
                    Ok(DeploymentPort {
                        container_port: p.container_port,
                        host_port: p.host_port,
                        protocol: p.protocol,
                        kind: p.kind.parse().map_err(AppError::internal)?,
                        hostname: p.hostname,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?,
            created_at: r.created_at.and_utc(),
            started_at: r.started_at.map(|t| t.and_utc()),
        });
    }
    Ok(out)
}

/// `GET /api/deployments/{id}` — summary + mounts + env *names* (values
/// never leave the server; docs/SECURITY.md).
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
        r#"SELECT sv.name AS "volume_name?", dv.host_path, dv.container_path,
                  dv.read_only AS "read_only: bool"
           FROM deployment_volumes dv
           LEFT JOIN server_volumes sv ON sv.id = dv.server_volume_id
           WHERE dv.deployment_id = ?
           ORDER BY dv.container_path"#,
        id.0
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| foundry_shared::dto::DeploymentMount {
        volume_name: r.volume_name,
        host_path: r.host_path,
        container_path: r.container_path,
        read_only: r.read_only,
    })
    .collect();

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

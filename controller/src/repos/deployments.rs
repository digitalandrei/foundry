//! Deployment command orchestration and the per-server host-port allocator
//! (plans/phase-06.md § Networking conditions 1–2, 6). Read/adoption/target
//! concerns live in the sibling `deployment_*` modules.

use foundry_shared::dto::{
    CreateDeploymentRequest, DeploymentPort, EnvSpec, PortSpec, VolumeSpec, MEM_LIMIT_MAX_MB,
    MEM_LIMIT_MIN_MB,
};
use foundry_shared::{DeploymentId, DeploymentState, PortKind, ServerId, SlotState, UserId};
use sqlx::{MySqlConnection, MySqlPool};
use uuid::Uuid;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::lifecycle::{self, Actor};

pub use super::deployment_adoption::adopt;
pub use super::deployment_queries::{adopted_for_server, detail, get, list, DeploymentRow};
use super::deployment_targets::{fetch_server_precheck, nginx_status_hint, resolve_target};

/// Controller-allocated host-port pool (per server) and the ports we
/// never hand out even if requested.
pub const PORT_POOL: std::ops::RangeInclusive<u16> = 20000..=29999;
const RESERVED_HOST_PORTS: &[u16] = &[22];
const MAX_DEPLOYMENT_VOLUMES: usize = 16;

#[derive(Debug)]
pub struct NewDeployment {
    pub id: DeploymentId,
}

/// Validate + insert a deployment in one transaction: the target's
/// slot(s) are locked and checked for occupancy (count < cap), ports
/// allocated conflict-free, env stored (secrets encrypted), member slots
/// → RESERVED, state PENDING → VALIDATING.
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
    ip_address: Option<&str>,
) -> Result<NewDeployment, AppError> {
    validate_ports(&req.ports, apps_domain)?;
    let normalized_volume_paths = normalized_volume_paths(&req.volumes)?;
    let now = chrono::Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    // Resolve the deploy target into its member slots (locked FOR UPDATE)
    // and enforce occupancy: an individual deploy needs the slot below its
    // `max_occupants`; a group deploy needs the group below its cap and
    // every member free of non-group holders (single-use = exclusive). A
    // replacement excludes its own outgoing deployment from these counts.
    let target = resolve_target(&mut tx, &req.target, replaces).await?;
    let server_id = target.server_id;
    let replacement_name = if let Some(old_id) = replaces {
        let old_name = sqlx::query_scalar!(
            "SELECT container_name FROM deployments WHERE id = ? FOR UPDATE",
            old_id.0,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("deployment to replace not found"))?;
        Some(replacement_container_name(
            old_name.as_deref(),
            req.name.as_deref(),
        )?)
    } else {
        None
    };

    let wants_web = req
        .ports
        .iter()
        .any(|p| matches!(p.kind, PortKind::Http | PortKind::Https));
    // Controller-side host evidence is stage one; the agent repeats live
    // preflight immediately before touching Docker as stage two.
    let server = require_server_ready(&mut tx, server_id, wants_web).await?;
    if replaces.is_some()
        && !crate::agent_version::supports(
            server.agent_version.as_deref(),
            crate::agent_version::REPLACEMENT_MIN_AGENT_VERSION,
        )
    {
        return Err(AppError::BadRequest(format!(
            "{} reports foundry-agent {}; install 0.64.0 or newer before replacing so the retained container can release and restore its stable name",
            server.name,
            server.agent_version.as_deref().unwrap_or("unknown"),
        )));
    }
    if req.volumes.iter().any(|volume| volume.purge_on_redeploy) {
        super::volumes::require_purge_support(&mut tx, server_id).await?;
    }

    // A replacement retains its deployment name: it is the persistent-volume
    // namespace as well as the container name and app URL.
    let container_name = match replacement_name {
        Some(name) => name,
        None => match req.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => sanitize_name(name)?,
            _ => generate_name(image_ref, &target.primary_slot_name),
        },
    };

    let mut allocated = allocate_ports(&mut tx, server_id, &req.ports).await?;
    require_unique_active_name(&mut tx, server_id, &container_name, replaces).await?;
    assign_hostnames(
        &mut tx,
        &mut allocated,
        &container_name,
        &server.name,
        apps_domain,
        replaces,
    )
    .await?;

    // Memory cap: None → unlimited (default). A set value is clamped to
    // the slider's [32, 256] GB so a hand-crafted request can't escape
    // the bounds.
    let mem_limit_mb = req
        .mem_limit_mb
        .map(|v| v.clamp(MEM_LIMIT_MIN_MB, MEM_LIMIT_MAX_MB));

    let id = DeploymentId::new();
    sqlx::query!(
        r#"INSERT INTO deployments
           (id, gpu_slot_id, gpu_group_id, server_id, registry_tag_id, gitlab_instance_id, image_ref, image_digest,
            created_by, state, container_name, mem_limit_mb, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', ?, ?, ?, ?)"#,
        id.0,
        target.primary_slot_id.0,
        target.group_id.map(|g| g.0),
        server_id.0,
        req.registry_tag_id.0,
        instance_id.0,
        image_ref,
        image_ref.split('@').nth(1),
        created_by.0,
        container_name,
        mem_limit_mb,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    // Occupancy is the count of active rows here — one per member slot
    // (1 for an individual deploy, N for a group). Authoritative for both
    // the multi-use cap and the group atomic lock.
    for slot_id in &target.member_slot_ids {
        sqlx::query!(
            "INSERT INTO deployment_slots (deployment_id, gpu_slot_id) VALUES (?, ?)",
            id.0,
            slot_id.0,
        )
        .execute(&mut *tx)
        .await?;
    }

    for p in &allocated {
        sqlx::query!(
            r#"INSERT INTO deployment_ports
               (id, deployment_id, container_port, host_port, protocol, kind, hostname,
                is_primary, health_path, max_body_size_bytes, proxy_timeout_seconds, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            Uuid::now_v7(),
            id.0,
            p.container_port,
            p.host_port,
            p.protocol,
            p.kind.as_str(),
            p.hostname,
            p.primary,
            p.health_path,
            p.max_body_size_bytes,
            p.proxy_timeout_seconds,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }
    // Persistent volumes: resolve explicit IDs or canonical
    // slot/server placement names and bind them.
    let mut audit_mounts = Vec::with_capacity(req.volumes.len());
    if !req.volumes.is_empty() {
        for (v, container_path) in req.volumes.iter().zip(&normalized_volume_paths) {
            let volume = super::volumes::ensure(
                &mut tx,
                server_id,
                target.primary_slot_id,
                target.group_id,
                &container_name,
                v,
                created_by,
                replaces,
            )
            .await?;
            super::volumes::require_safe_purge_mapping(
                &mut tx,
                volume.id,
                id,
                replaces,
                v.purge_on_redeploy,
            )
            .await?;
            sqlx::query!(
                r#"INSERT INTO deployment_volumes
                   (id, deployment_id, server_volume_id, host_path, container_path,
                    read_only, purge_on_redeploy, created_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
                Uuid::now_v7(),
                id.0,
                volume.id.0,
                &volume.path,
                container_path,
                v.read_only,
                v.purge_on_redeploy,
                now,
            )
            .execute(&mut *tx)
            .await?;
            audit_mounts.push(serde_json::json!({
                "selection": if v.volume_id.is_some() { "existing" } else { "automatic" },
                "volume_id": volume.id.to_string(),
                "source": {
                    "project_name": volume.project_name,
                    "mount_name": volume.name,
                    "placement": volume.placement.as_str(),
                },
                "container_path": container_path,
                "read_only": v.read_only,
                "purge_on_redeploy": v.purge_on_redeploy,
            }));
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
            lifecycle::transition_member_slots(&mut tx, id, SlotState::Reserved).await?;
        }
        Some(old_id) => {
            // Replacement orchestration is atomic with the successor's
            // creation (review finding): lock the old row, validate,
            // link, transition, and enqueue its stop/remove here — a
            // crash can no longer strand a linked successor without a
            // queued task.
            let old = sqlx::query!(
                "SELECT state, adopted_container_id FROM deployments WHERE id = ? FOR UPDATE",
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
            // The successor's immutable image and all host prerequisites are
            // prepared while this predecessor remains untouched. The result
            // handler quiesces the old container only after preparation.
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

    // Creation is a command, not a collection of best-effort writes: the
    // deployment, its reservation, its first task, and its business audit
    // record either all commit or all roll back. Replacement tasks are
    // enqueued above as part of the same transaction.
    if replaces.is_none() {
        super::tasks::enqueue_deploy(&mut tx, id).await?;
    } else {
        super::tasks::enqueue_prepare(&mut tx, id).await?;
    }
    let (action, subject_id, detail) = match replaces {
        Some(old_id) => (
            "DEPLOYMENT_REPLACED",
            old_id.0,
            serde_json::json!({
                "replaced_by": id.to_string(),
                "image_ref": image_ref,
                "mounts": audit_mounts,
            }),
        ),
        None => (
            "DEPLOYMENT_CREATED",
            id.0,
            serde_json::json!({
                "image_ref": image_ref,
                "name": container_name,
                "target": serde_json::to_value(&req.target).ok(),
                "mounts": audit_mounts,
            }),
        ),
    };
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(created_by),
            action,
            subject_type: Some("deployment"),
            subject_id: Some(subject_id),
            detail: Some(detail),
            ip_address,
        },
    )
    .await?;

    tx.commit().await?;
    Ok(NewDeployment { id })
}

/// Stage-one deployment preflight from the latest host evidence. The agent
/// repeats live checks at execution time so a stale snapshot can never be the
/// sole authority.
pub(super) async fn require_server_ready(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    wants_web: bool,
) -> Result<super::deployment_targets::ServerPrecheck, AppError> {
    let server = fetch_server_precheck(tx, server_id).await?;
    if server.status != "ONLINE" {
        return Err(AppError::BadRequest("server is not online".into()));
    }
    if !crate::agent_version::supports(
        server.agent_version.as_deref(),
        crate::agent_version::DEPLOYMENT_MIN_AGENT_VERSION,
    ) {
        return Err(AppError::BadRequest(format!(
            "{} reports foundry-agent {}; install 0.63.0 or newer with `sudo foundry-agent --setup-apps` before deploying",
            server.name,
            server.agent_version.as_deref().unwrap_or("unknown"),
        )));
    }
    if server.setup_revision != Some(foundry_shared::dto::REQUIRED_SETUP_REVISION) {
        return Err(AppError::BadRequest(format!(
            "{} has host setup revision {} (required {}); run `sudo foundry-agent --setup-apps`, then refresh diagnostics",
            server.name,
            server
                .setup_revision
                .map_or_else(|| "missing".into(), |revision| revision.to_string()),
            foundry_shared::dto::REQUIRED_SETUP_REVISION,
        )));
    }
    let readiness = server.readiness.as_ref().ok_or_else(|| {
        AppError::BadRequest(format!(
            "{} has not reported live readiness yet; run diagnostics and try again",
            server.name,
        ))
    })?;
    let required_checks: &[&str] = if wants_web {
        &[
            "docker",
            "docker_gpu",
            "storage_write",
            "capabilities",
            "nginx_config",
            "tls_certificate",
        ]
    } else {
        &["docker", "docker_gpu", "storage_write", "capabilities"]
    };
    if !readiness.checks_ready(required_checks) {
        let detail = readiness
            .checks
            .iter()
            .find(|check| {
                required_checks.contains(&check.code.as_str())
                    && !matches!(
                        check.status,
                        foundry_shared::dto::CheckStatus::Ready
                            | foundry_shared::dto::CheckStatus::Warning
                    )
            })
            .map(|check| format!("{}: {}", check.code, check.detail))
            .unwrap_or_else(|| "host readiness is incomplete".into());
        return Err(AppError::BadRequest(format!(
            "{} is not deployment-ready: {detail}",
            server.name,
        )));
    }
    if server.docker_ok == Some(false) {
        return Err(AppError::BadRequest(format!(
            "Docker isn't running on {} — start the Docker daemon on the server, then redeploy.",
            server.name,
        )));
    }
    if wants_web && server.app_publishing_ready == Some(false) {
        return Err(AppError::BadRequest(format!(
            "HTTP/S publishing isn't ready on {}: {}. Fix it on the server, then redeploy.",
            server.name,
            nginx_status_hint(server.nginx_status.as_deref()),
        )));
    }
    Ok(server)
}

pub async fn pin_image(
    pool: &MySqlPool,
    deployment_id: DeploymentId,
    image_ref: &str,
    digest: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE deployments SET image_ref = ?, image_digest = ?, updated_at = ? WHERE id = ?",
        image_ref,
        digest,
        chrono::Utc::now().naive_utc(),
        deployment_id.0,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Docker names and the first app hostname both derive from the deployment
/// name. Creation is already serialized by `allocate_ports`' server-row
/// lock, so this active-name probe cannot race another deployment on the
/// same server. Removed/replaced history releases the name; a replacement
/// may intentionally keep its predecessor's stable URL.
async fn require_unique_active_name(
    tx: &mut MySqlConnection,
    server_id: ServerId,
    container_name: &str,
    replaces: Option<DeploymentId>,
) -> Result<(), AppError> {
    let exempt = replaces
        .map(|deployment| deployment.0)
        .unwrap_or_else(Uuid::nil);
    let taken = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM deployments
           WHERE server_id = ? AND container_name = ? AND id <> ?
             AND (state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                            'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
                  OR (state = 'FAILED' AND container_id IS NOT NULL))
           FOR UPDATE"#,
        server_id.0,
        container_name,
        exempt,
    )
    .fetch_one(&mut *tx)
    .await?;
    if taken > 0 {
        return Err(AppError::BadRequest(format!(
            "deployment name {container_name:?} is already in use on this server"
        )));
    }
    Ok(())
}

/// Bound and canonicalize every requested mount before any deployment row is
/// written. The image metadata endpoint also caps declarations at 16, but a
/// hand-crafted create request must not bypass that limit.
fn normalized_volume_paths(volumes: &[VolumeSpec]) -> Result<Vec<String>, AppError> {
    if volumes.len() > MAX_DEPLOYMENT_VOLUMES {
        return Err(AppError::BadRequest(format!(
            "too many persistent mounts (max {MAX_DEPLOYMENT_VOLUMES})"
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(volumes.len());
    volumes
        .iter()
        .map(|volume| {
            super::volumes::validate_volume_name(&volume.volume_name)?;
            super::volumes::validate_container_path(&volume.container_path)?;
            let path = super::volumes::normalize_container_path(&volume.container_path)?;
            if !seen.insert(path.clone()) {
                return Err(AppError::BadRequest(format!("duplicate mount path {path}")));
            }
            Ok(path)
        })
        .collect()
}

fn validate_ports(specs: &[PortSpec], apps_domain: Option<&str>) -> Result<(), AppError> {
    if specs.len() > 32 {
        return Err(AppError::BadRequest("too many ports (max 32)".into()));
    }
    let mut seen = std::collections::HashSet::new();
    let mut primary_apps = 0usize;
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
        if p.primary {
            if !matches!(p.kind, PortKind::Http | PortKind::Https) {
                return Err(AppError::BadRequest(
                    "only an HTTP/HTTPS port can be the primary application".into(),
                ));
            }
            primary_apps += 1;
        }
        if primary_apps > 1 {
            return Err(AppError::BadRequest(
                "only one published application port can be primary".into(),
            ));
        }
        if p.health_path
            .as_deref()
            .is_some_and(|path| !path.starts_with('/') || path.len() > 1024)
        {
            return Err(AppError::BadRequest(
                "health_path must start with / and be at most 1024 characters".into(),
            ));
        }
        if p.max_body_size_bytes
            .is_some_and(|bytes| !(1024..=8 * 1024 * 1024 * 1024).contains(&bytes))
        {
            return Err(AppError::BadRequest(
                "max_body_size_bytes must be between 1 KiB and 8 GiB".into(),
            ));
        }
        if p.proxy_timeout_seconds
            .is_some_and(|seconds| !(1..=86_400).contains(&seconds))
        {
            return Err(AppError::BadRequest(
                "proxy_timeout_seconds must be between 1 and 86400".into(),
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

/// Replacements retain their name because volume identity includes it. This
/// remains a repository-level check so non-HTTP callers cannot fork a
/// persistent namespace by passing a different request name.
pub fn replacement_container_name(
    existing: Option<&str>,
    requested: Option<&str>,
) -> Result<String, AppError> {
    let existing = existing
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            AppError::BadRequest(
                "this legacy deployment has no stable name, so it cannot be replaced while preserving its persistent-volume namespace; create a new deployment instead"
                    .into(),
            )
        })?;
    match requested.map(str::trim).filter(|name| !name.is_empty()) {
        None => Ok(existing.to_string()),
        Some(name) if name == existing => Ok(existing.to_string()),
        Some(_) => Err(AppError::BadRequest(format!(
            "replacement must keep deployment name {existing:?}; that name is its persistent-volume namespace. Omit name or use {existing:?}"
        ))),
    }
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
           AND (d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                            'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
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
            primary: spec.primary,
            health_path: spec.health_path.clone(),
            max_body_size_bytes: spec.max_body_size_bytes.unwrap_or(2 * 1024 * 1024 * 1024),
            proxy_timeout_seconds: spec.proxy_timeout_seconds.unwrap_or(300),
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
                 AND (d.state IN ('PENDING','VALIDATING','PREPARED','PULLING_IMAGE','CREATING_CONTAINER',
                                  'STARTING','WAITING_HEALTH','PUBLISHING','PUBLISH_FAILED','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
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

/// Dismiss a FAILED deployment: mark it REMOVED (clears it from the
/// active list — it stays as an audit/event log) and free the slot if
/// it is still stuck FAILED. Controller-side only — a failed deploy
/// left no container, so no agent round-trip is needed (0.11.0).
pub async fn dismiss(
    pool: &MySqlPool,
    id: DeploymentId,
    user: UserId,
    ip_address: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        "SELECT state FROM deployments WHERE id = ? FOR UPDATE",
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
    lifecycle::transition_deployment(
        &mut tx,
        id,
        DeploymentState::Removed,
        &Actor::user(user),
        Some(serde_json::json!({ "reason": "dismissed by operator" })),
    )
    .await?;
    // Free every member slot that is still FAILED — never steal one
    // another deployment has since taken (RUNNING/RESERVED/etc). A
    // multi-use slot a co-tenant still holds won't be FAILED, so it is
    // left alone.
    sqlx::query!(
        "UPDATE gpu_slots gs JOIN deployment_slots ds ON ds.gpu_slot_id = gs.id
         SET gs.state = 'FREE', gs.updated_at = ?
         WHERE ds.deployment_id = ? AND gs.state = 'FAILED'",
        chrono::Utc::now().naive_utc(),
        id.0,
    )
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        crate::audit::AuditEntry {
            actor_type: foundry_shared::ActorType::User,
            actor_id: Some(user),
            action: "DEPLOYMENT_DISMISSED",
            subject_type: Some("deployment"),
            subject_id: Some(id.0),
            detail: None,
            ip_address,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{normalized_volume_paths, replacement_container_name};
    use crate::error::AppError;
    use foundry_shared::dto::VolumeSpec;
    use foundry_shared::VolumePlacement;

    fn volume(path: &str) -> VolumeSpec {
        VolumeSpec {
            volume_id: None,
            volume_name: "models".into(),
            container_path: path.into(),
            read_only: false,
            placement: VolumePlacement::Server,
            purge_on_redeploy: false,
        }
    }

    #[test]
    fn replacement_keeps_existing_name_when_request_omits_it() {
        assert_eq!(
            replacement_container_name(Some("model-a"), None).expect("name is retained"),
            "model-a"
        );
    }

    #[test]
    fn replacement_rejects_a_different_persistent_namespace() {
        let error = replacement_container_name(Some("model-a"), Some("model-b"))
            .expect_err("replacement must retain its name");
        assert!(matches!(
            error,
            AppError::BadRequest(message)
                if message.contains("persistent-volume namespace") && message.contains("model-a")
        ));
    }

    #[test]
    fn nameless_legacy_deployment_cannot_fork_a_namespace() {
        let error = replacement_container_name(None, None)
            .expect_err("legacy deployment cannot provide a stable namespace");
        assert!(matches!(
            error,
            AppError::BadRequest(message) if message.contains("create a new deployment")
        ));
    }

    #[test]
    fn mount_requests_are_bounded_and_destinations_are_unique_after_normalization() {
        let duplicate = normalized_volume_paths(&[volume("/data/"), volume("/data")])
            .expect_err("equivalent Docker destinations cannot be mapped twice");
        assert!(
            matches!(duplicate, AppError::BadRequest(message) if message.contains("duplicate"))
        );

        let too_many = vec![volume("/models"); super::MAX_DEPLOYMENT_VOLUMES + 1];
        let error = normalized_volume_paths(&too_many)
            .expect_err("hand-crafted deployment requests cannot exceed the mount cap");
        assert!(matches!(error, AppError::BadRequest(message) if message.contains("max 16")));
    }
}

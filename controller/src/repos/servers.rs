//! servers + server_agents + enrollment_tokens access
//! (docs/ARCHITECTURE.md § Server Enrollment; skill:
//! https-mtls-agent-transport).

use chrono::{Duration, Utc};
use foundry_shared::dto::ServerSummary;
use foundry_shared::{ServerId, ServerStatus};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::crypto::{random_token, token_hash};
use crate::error::AppError;

pub const TOKEN_TTL_HOURS: i64 = 72;
/// A server is OFFLINE when no heartbeat arrived in this window.
pub const HEARTBEAT_STALE_SECS: i64 = 90;

pub async fn list(pool: &MySqlPool) -> Result<Vec<ServerSummary>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT s.id AS "id: Uuid", s.name, s.hostname, s.status,
                  s.last_heartbeat_at, s.os_version,
                  s.app_publishing_ready AS "app_publishing_ready: bool", s.nginx_status,
                  s.docker_ok AS "docker_ok: bool",
                  a.agent_version, a.id AS "agent_id: Uuid"
           FROM servers s
           LEFT JOIN server_agents a ON a.server_id = s.id
           ORDER BY s.name"#
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let status: ServerStatus = r.status.parse().map_err(AppError::internal)?;
        let id: ServerId = r.id.into();
        out.push(ServerSummary {
            id,
            name: r.name,
            hostname: r.hostname.filter(|h| !h.is_empty()),
            status,
            last_heartbeat_at: r.last_heartbeat_at.map(|t| t.and_utc()),
            agent_version: r.agent_version,
            os_version: r.os_version,
            app_publishing_ready: r.app_publishing_ready,
            nginx_status: r.nginx_status,
            docker_ok: r.docker_ok,
            enrolled: r.agent_id.is_some(),
            gpus: super::inventory::gpus_for_server(pool, id).await?,
            containers_running: super::inventory::running_count(pool, id).await?,
        });
    }
    Ok(out)
}

/// docker/driver versions for the detail view.
pub async fn runtime_versions(
    pool: &MySqlPool,
    id: ServerId,
) -> Result<(Option<String>, Option<String>), AppError> {
    let row = sqlx::query!(
        "SELECT docker_version, nvidia_driver_version FROM servers WHERE id = ?",
        id.0
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("server not found"))?;
    Ok((row.docker_version, row.nvidia_driver_version))
}

pub async fn get_summary(pool: &MySqlPool, id: ServerId) -> Result<ServerSummary, AppError> {
    list(pool)
        .await?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or(AppError::NotFound("server not found"))
}

/// Create a named server (GitLab-agent style: name first, enroll later).
pub async fn create(pool: &MySqlPool, name: &str) -> Result<ServerId, AppError> {
    let id = Uuid::now_v7();
    let now = Utc::now().naive_utc();
    // hostname is left NULL until the agent enrolls (it is the fleet
    // identity and carries a UNIQUE index, so empty strings can't share it).
    sqlx::query!(
        r#"INSERT INTO servers (id, name, status, created_at, updated_at)
           VALUES (?, ?, 'OFFLINE', ?, ?)"#,
        id,
        name,
        now,
        now,
    )
    .execute(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::BadRequest("a server with this name already exists".into())
        }
        _ => AppError::Db(e),
    })?;
    Ok(id.into())
}

/// Mint a fresh enrollment token for a server, expiring older unused
/// ones (one live token per server). Returns the raw token — shown
/// once, stored hashed.
pub async fn issue_enrollment_token(
    pool: &MySqlPool,
    server_id: ServerId,
    created_by: foundry_shared::UserId,
) -> Result<(String, chrono::DateTime<Utc>), AppError> {
    let token = random_token();
    let now = Utc::now();
    let expires_at = now + Duration::hours(TOKEN_TTL_HOURS);

    let mut tx = pool.begin().await?;
    // Revoke older unused tokens for this server.
    sqlx::query!(
        "UPDATE enrollment_tokens SET expires_at = ?, updated_at = ?
         WHERE server_id = ? AND used_at IS NULL AND expires_at > ?",
        now.naive_utc(),
        now.naive_utc(),
        server_id.0,
        now.naive_utc(),
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO enrollment_tokens
           (id, token_hash, server_id, created_by, expires_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        Uuid::now_v7(),
        token_hash(&token),
        server_id.0,
        created_by.0,
        expires_at.naive_utc(),
        now.naive_utc(),
        now.naive_utc(),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((token, expires_at))
}

/// Mint a reusable, time-limited FLEET enrollment key (docs/ARCHITECTURE.md
/// § Fleet Enrollment). Unlike a server-bound token it is not tied to a
/// pre-created server: any agent presenting it within the TTL and use
/// budget auto-enrolls under its own hostname. `max_uses = None` means
/// unlimited within the TTL. Returns the raw key — shown once, stored
/// hashed.
pub async fn issue_fleet_token(
    pool: &MySqlPool,
    ttl_hours: i64,
    max_uses: Option<u32>,
    created_by: foundry_shared::UserId,
) -> Result<(String, chrono::DateTime<Utc>), AppError> {
    let token = random_token();
    let now = Utc::now();
    let expires_at = now + Duration::hours(ttl_hours);
    sqlx::query!(
        r#"INSERT INTO enrollment_tokens
           (id, token_hash, server_id, kind, max_uses, uses, created_by,
            expires_at, created_at, updated_at)
           VALUES (?, ?, NULL, 'FLEET', ?, 0, ?, ?, ?, ?)"#,
        Uuid::now_v7(),
        token_hash(&token),
        max_uses,
        created_by.0,
        expires_at.naive_utc(),
        now.naive_utc(),
        now.naive_utc(),
    )
    .execute(pool)
    .await?;
    Ok((token, expires_at))
}

pub struct EnrolledAgent {
    pub agent_id: Uuid,
    pub agent_secret: String,
    pub server_id: ServerId,
    pub server_name: String,
}

/// Consume an enrollment token (single use) and issue the permanent
/// agent identity. Re-enrollment of the same server replaces the
/// credential (old one stops working immediately).
pub async fn enroll(
    pool: &MySqlPool,
    token: &str,
    hostname: &str,
    agent_version: &str,
    os_version: Option<&str>,
) -> Result<EnrolledAgent, AppError> {
    let now = Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    // The JOIN guarantees server_id is non-null (tokens without a
    // target server simply never match).
    let row = sqlx::query!(
        r#"SELECT t.id AS "id: Uuid", t.server_id AS "server_id!: Uuid", s.name
           FROM enrollment_tokens t
           JOIN servers s ON s.id = t.server_id
           WHERE t.token_hash = ? AND t.used_at IS NULL AND t.expires_at > ?
           FOR UPDATE"#,
        token_hash(token),
        now,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Unauthorized)?;
    let server_id = row.server_id;

    sqlx::query!(
        "UPDATE enrollment_tokens SET used_at = ?, used_by_server_id = ?, updated_at = ? WHERE id = ?",
        now,
        server_id,
        now,
        row.id,
    )
    .execute(&mut *tx)
    .await?;

    let secret = random_token();
    let secret_hash = token_hash(&secret);
    // One agent row per server: replace the credential on re-enroll.
    sqlx::query!("DELETE FROM server_agents WHERE server_id = ?", server_id)
        .execute(&mut *tx)
        .await?;
    let agent_id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO server_agents
           (id, server_id, agent_version, token_hash, enrolled_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        agent_id,
        server_id,
        agent_version,
        secret_hash,
        now,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE servers SET hostname = ?, os_version = ?, updated_at = ? WHERE id = ?",
        hostname,
        os_version,
        now,
        server_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(EnrolledAgent {
        agent_id,
        agent_secret: secret,
        server_id: server_id.into(),
        server_name: row.name,
    })
}

/// Consume one use of a FLEET key and enrol the calling host. The agent's
/// hostname is the identity: an existing server with that hostname is
/// re-enrolled (credential replaced, as with server-bound re-enrollment);
/// otherwise a new server is created named after the hostname. The unique
/// index on `servers.hostname` makes a concurrent first-enroll race fail
/// safely.
pub async fn enroll_fleet(
    pool: &MySqlPool,
    token: &str,
    hostname: &str,
    agent_version: &str,
    os_version: Option<&str>,
) -> Result<EnrolledAgent, AppError> {
    let now = Utc::now().naive_utc();
    let mut tx = pool.begin().await?;

    let tok = sqlx::query!(
        r#"SELECT id AS "id: Uuid", max_uses, uses
           FROM enrollment_tokens
           WHERE token_hash = ? AND kind = 'FLEET'
                 AND used_at IS NULL AND expires_at > ?
           FOR UPDATE"#,
        token_hash(token),
        now,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Unauthorized)?;

    // Use budget (None = unlimited within TTL). The FOR UPDATE above
    // serialises concurrent enrollments on the same key.
    if let Some(max) = tok.max_uses {
        if tok.uses >= max {
            return Err(AppError::Unauthorized);
        }
    }
    sqlx::query!(
        "UPDATE enrollment_tokens SET uses = uses + 1, updated_at = ? WHERE id = ?",
        now,
        tok.id,
    )
    .execute(&mut *tx)
    .await?;

    // Hostname is the identity. Find-or-create the server row.
    let existing = sqlx::query!(
        r#"SELECT id AS "id: Uuid", name FROM servers WHERE hostname = ? FOR UPDATE"#,
        hostname,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let (server_id, server_name) = match existing {
        Some(r) => (r.id, r.name),
        None => {
            let id = Uuid::now_v7();
            // name defaults to the hostname; servers.name is unique, so a
            // hostname clashing with an existing server *name* is rejected.
            sqlx::query!(
                r#"INSERT INTO servers (id, name, hostname, status, created_at, updated_at)
                   VALUES (?, ?, ?, 'OFFLINE', ?, ?)"#,
                id,
                hostname,
                hostname,
                now,
                now,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(db) if db.is_unique_violation() => AppError::BadRequest(
                    "a server with this hostname or name already exists".into(),
                ),
                _ => AppError::Db(e),
            })?;
            (id, hostname.to_string())
        }
    };

    // One agent row per server: replace the credential on re-enroll.
    let secret = random_token();
    let secret_hash = token_hash(&secret);
    sqlx::query!("DELETE FROM server_agents WHERE server_id = ?", server_id)
        .execute(&mut *tx)
        .await?;
    let agent_id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO server_agents
           (id, server_id, agent_version, token_hash, enrolled_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        agent_id,
        server_id,
        agent_version,
        secret_hash,
        now,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE servers SET hostname = ?, os_version = ?, updated_at = ? WHERE id = ?",
        hostname,
        os_version,
        now,
        server_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(EnrolledAgent {
        agent_id,
        agent_secret: secret,
        server_id: server_id.into(),
        server_name,
    })
}

/// The authenticated agent on a request (docs/SECURITY.md § Agent
/// Authentication).
pub struct AgentContext {
    pub server_id: ServerId,
}

pub async fn authenticate_agent(
    pool: &MySqlPool,
    agent_id: Uuid,
    secret: &str,
) -> Result<AgentContext, AppError> {
    let row = sqlx::query!(
        r#"SELECT server_id AS "server_id: Uuid", token_hash FROM server_agents WHERE id = ?"#,
        agent_id
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    use subtle::ConstantTimeEq;
    let presented = token_hash(secret);
    if presented.ct_eq(&row.token_hash).unwrap_u8() != 1 {
        return Err(AppError::Unauthorized);
    }
    Ok(AgentContext {
        server_id: row.server_id.into(),
    })
}

pub async fn record_heartbeat(
    pool: &MySqlPool,
    server_id: ServerId,
    agent_version: &str,
) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    sqlx::query!(
        "UPDATE servers SET status = 'ONLINE', last_heartbeat_at = ?, updated_at = ? WHERE id = ?",
        now,
        now,
        server_id.0,
    )
    .execute(pool)
    .await?;
    sqlx::query!(
        "UPDATE server_agents SET agent_version = ?, updated_at = ? WHERE server_id = ?",
        agent_version,
        now,
        server_id.0,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Periodic: ONLINE servers without a recent heartbeat go OFFLINE.
pub fn spawn_offline_sweeper(pool: MySqlPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let cutoff = (Utc::now() - Duration::seconds(HEARTBEAT_STALE_SECS)).naive_utc();
            match sqlx::query!(
                "UPDATE servers SET status = 'OFFLINE', updated_at = ?
                 WHERE status = 'ONLINE' AND (last_heartbeat_at IS NULL OR last_heartbeat_at < ?)",
                Utc::now().naive_utc(),
                cutoff,
            )
            .execute(&pool)
            .await
            {
                Ok(res) if res.rows_affected() > 0 => {
                    tracing::info!(
                        count = res.rows_affected(),
                        "servers marked OFFLINE (stale heartbeat)"
                    );
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(?err, "offline sweep failed"),
            }
        }
    });
}

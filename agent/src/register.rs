//! `foundry-agent --register --url <controller> --token <token>` —
//! GitLab-agent-style one-shot registration (docs/ARCHITECTURE.md
//! § Server Enrollment). Idempotent pieces, root required:
//!
//! 1. preflight and install this binary to /usr/local/bin/foundry-agent,
//! 2. create the foundry-agent system user (+ docker/video/render
//!    groups where they exist),
//! 3. set up HTTP/S app publishing (nginx include + vhost dir, TLS
//!    drop point, narrow sudoers rule — docs/SECURITY.md),
//! 4. write the systemd unit and daemon-reload,
//! 5. enroll against the controller (token is single-use),
//! 6. atomically replace /etc/foundry-agent/config.toml (0600), then
//!    enable --now. Fallible host prerequisites therefore do not burn a
//!    token, and a previous config remains until the new one is durable.
//!
//! `foundry-agent --setup-apps` refreshes the binary, service user, app
//! publishing, and unit on an already-enrolled server — the upgrade path.

use std::path::Path;
use std::process::Command;

use foundry_shared::dto::{AgentEnrollRequest, AgentEnrollResponse};

pub const INSTALL_PATH: &str = "/usr/local/bin/foundry-agent";
const UNIT_PATH: &str = "/etc/systemd/system/foundry-agent.service";
const SERVICE_USER: &str = "foundry-agent";
const TLS_DIR: &str = "/etc/foundry-agent/tls";
const SUDOERS_PATH: &str = "/etc/sudoers.d/foundry-agent";
const NGINX_BOOTSTRAP: &str = "/etc/nginx/conf.d/foundry-apps.conf";
const CAPABILITY_BOUNDING_SET: &str = "CAP_DAC_OVERRIDE CAP_SETUID CAP_SETGID CAP_AUDIT_WRITE";
const AMBIENT_CAPABILITIES: &str = "CAP_DAC_OVERRIDE";

pub struct RegisterArgs {
    pub url: String,
    pub token: String,
    /// `true` when the token is a reusable fleet key (`--fleet-token`):
    /// enroll via `/agent/enroll/fleet`, which auto-creates the server
    /// from this host's hostname instead of consuming a server-bound token.
    pub fleet: bool,
    pub force: bool,
}

pub async fn run(args: RegisterArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Test escape hatch: skip root/system mutations (config to a tmp
    // path via FOUNDRY_AGENT_CONFIG, no systemd) — used by smoke tests.
    let system_mode = std::env::var("FOUNDRY_AGENT_SKIP_SYSTEM").is_err();

    let config_path = crate::config::config_path();
    if system_mode && unsafe { libc_geteuid() } != 0 {
        return Err(
            "--register must run as root (sudo): it installs the binary, \
                    config, and systemd unit"
                .into(),
        );
    }
    if config_path.exists() && !args.force {
        return Err(format!(
            "already enrolled ({} exists) — pass --force to re-enroll with a new token",
            config_path.display()
        )
        .into());
    }

    // Complete every fallible host prerequisite before consuming the
    // single-use token or rotating an existing controller credential.
    if system_mode {
        install_self()?;
        create_service_user()?;
        prepare_config_directory(&config_path)?;
        setup_apps()?;
        write_unit()?;
        systemctl(&["daemon-reload"])?;
    }

    // Enroll only after the host is ready to persist and run the identity.
    let url = args.url.trim_end_matches('/').to_string();
    let endpoint = if args.fleet {
        "/agent/enroll/fleet"
    } else {
        "/agent/enroll"
    };
    let hostname = read_hostname();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .post(format!("{url}{endpoint}"))
        .json(&AgentEnrollRequest {
            token: args.token.clone(),
            hostname: hostname.clone(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            os_version: read_os_version(),
        })
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let hint = if args.fleet {
            "fleet keys expire and have a use budget — mint a fresh one in the Foundry UI"
        } else {
            "tokens are single-use; generate a fresh one in the Foundry UI"
        };
        return Err(format!("enrollment rejected ({status}): {body} — {hint}").into());
    }
    let enrolled: AgentEnrollResponse = resp.json().await?;
    println!(
        "enrolled as server \"{}\" ({})",
        enrolled.server_name, enrolled.server_id
    );

    write_config(&config_path, &url, &enrolled, system_mode)?;
    println!("config written: {}", config_path.display());
    if system_mode {
        systemctl(&["enable", "--now", "foundry-agent"])?;
        println!("service enabled and started: systemctl status foundry-agent");
    }
    Ok(())
}

/// `--setup-apps`: the upgrade/repair path for enrolled servers —
/// installs this binary, refreshes the app-publishing host pieces and
/// the systemd unit, and restarts the service.
pub fn setup_apps_standalone() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc_geteuid() } != 0 {
        return Err("--setup-apps must run as root (sudo)".into());
    }
    install_self()?;
    create_service_user()?;
    setup_apps()?;
    write_unit()?;
    systemctl(&["daemon-reload"])?;
    if crate::config::config_path().exists() {
        systemctl(&["restart", "foundry-agent"])?;
        println!("service restarted: systemctl status foundry-agent");
    } else {
        println!("not enrolled yet — run --register next");
    }
    let status = crate::vhost::app_publishing_status();
    println!("app publishing: {status}");
    if status == "NGINX_OUTDATED" {
        let (maj, min, pat) = crate::vhost::MIN_NGINX_VERSION;
        println!("  → upgrade nginx to ≥ {maj}.{min}.{pat} — the vhost template uses the `http2` directive");
    }
    Ok(())
}

/// HTTP/S app publishing prerequisites (docs/ARCHITECTURE.md § App
/// Publishing; docs/SECURITY.md). All idempotent:
///
/// - /etc/nginx/foundry-apps/ — per-deployment vhosts, owned by the
///   service user (the agent writes confs directly),
/// - /etc/nginx/conf.d/foundry-apps.conf — include + websocket map,
/// - /etc/foundry-agent/tls/ — operator drops the wildcard cert here
///   (fullchain.pem + privkey.pem); keys never travel through Foundry,
/// - /etc/sudoers.d/foundry-agent — exactly `nginx -t` + `nginx -s
///   reload`, nothing else.
fn setup_apps() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    // Persistent-volume root: the agent (service user) creates volume
    // dirs under it at deploy time — a missing or root-owned
    // /storage/containers was the first real-deploy failure (EROFS).
    std::fs::create_dir_all("/storage/containers")?;
    let status = Command::new("chown")
        .args([
            &format!("{SERVICE_USER}:{SERVICE_USER}"),
            "/storage/containers",
        ])
        .status()?;
    if !status.success() {
        return Err("chown of /storage/containers failed".into());
    }

    // Vhost dir is created even without nginx so the unit's
    // ReadWritePaths never points at a missing path.
    std::fs::create_dir_all(crate::vhost::VHOST_DIR)?;
    let status = Command::new("chown")
        .args([
            &format!("{SERVICE_USER}:{SERVICE_USER}"),
            crate::vhost::VHOST_DIR,
        ])
        .status()?;
    if !status.success() {
        return Err(format!("chown of {} failed", crate::vhost::VHOST_DIR).into());
    }

    std::fs::create_dir_all(TLS_DIR)?;
    std::fs::set_permissions(TLS_DIR, std::fs::Permissions::from_mode(0o755))?;

    let sudoers = format!(
        "# Managed by foundry-agent --setup-apps: vhost reload only.\n\
         {SERVICE_USER} ALL=(root) NOPASSWD: /usr/sbin/nginx -t, /usr/sbin/nginx -s reload\n"
    );
    std::fs::write(SUDOERS_PATH, &sudoers)?;
    std::fs::set_permissions(SUDOERS_PATH, std::fs::Permissions::from_mode(0o440))?;
    let visudo = Command::new("visudo").args(["-cf", SUDOERS_PATH]).status();
    if let Ok(s) = visudo {
        if !s.success() {
            std::fs::remove_file(SUDOERS_PATH)?;
            return Err("visudo rejected the foundry-agent sudoers rule".into());
        }
    }

    if Path::new("/etc/nginx/conf.d").is_dir() {
        // Ubuntu's nginx.conf includes /etc/nginx/conf.d/*.conf in the
        // http block. The map feeds `Connection:` for websocket
        // upgrades; the name is prefixed to avoid colliding with an
        // operator-defined $connection_upgrade.
        let bootstrap = format!(
            "# Managed by foundry-agent --setup-apps (Foundry HTTP/S app publishing).\n\
             map $http_upgrade $foundry_connection_upgrade {{\n\
             \x20   default upgrade;\n\
             \x20   ''      close;\n\
             }}\n\
             include {}/*.conf;\n",
            crate::vhost::VHOST_DIR
        );
        std::fs::write(NGINX_BOOTSTRAP, bootstrap)?;
        println!("nginx app-publishing include written: {NGINX_BOOTSTRAP}");
        println!(
            "wildcard certificate goes to {TLS_DIR}/fullchain.pem + privkey.pem \
             (operator-managed; required before the first HTTP/S deploy)"
        );
    } else {
        println!(
            "WARNING: /etc/nginx/conf.d not found — nginx is not installed. \
             HTTP/S app publishing stays disabled on this server; install nginx \
             and re-run `sudo foundry-agent --setup-apps`."
        );
    }
    Ok(())
}

fn read_hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn read_os_version() -> Option<String> {
    let raw = std::fs::read_to_string("/etc/os-release").ok()?;
    raw.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
}

fn write_config(
    path: &Path,
    url: &str,
    enrolled: &AgentEnrollResponse,
    system_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    #[derive(serde::Serialize)]
    struct StoredConfig<'a> {
        controller_url: &'a str,
        agent_id: &'a str,
        agent_secret: &'a str,
        server_name: &'a str,
        poll_interval_secs: u64,
    }
    let body = format!(
        "# Written by `foundry-agent --register` — identity for this server.\n{}",
        toml::to_string(&StoredConfig {
            controller_url: url,
            agent_id: &enrolled.agent_id,
            agent_secret: &enrolled.agent_secret,
            server_name: &enrolled.server_name,
            poll_interval_secs: enrolled.poll_interval_secs,
        })?
    );

    // Write, chmod/chown, and fsync a sibling before the atomic rename. The
    // previous working config remains intact until every preparation step
    // succeeds, which makes `--force` credential rotation recoverable.
    let temp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config"),
        uuid::Uuid::now_v7()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temp)?;
    if let Err(err) = (|| -> Result<(), Box<dyn std::error::Error>> {
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
        if system_mode {
            let status = Command::new("chown")
                .arg(format!("{SERVICE_USER}:{SERVICE_USER}"))
                .arg(&temp)
                .status()?;
            if !status.success() {
                return Err("chown of the staged agent config failed".into());
            }
        }
        std::fs::rename(&temp, path)?;
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })() {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    Ok(())
}

/// Make directory creation, ownership, and permissions a pre-enrollment
/// prerequisite. After the controller issues a credential, only the staged
/// file write/chown/fsync/rename remains.
fn prepare_config_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let dir = path.parent().ok_or("agent config path has no parent")?;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o750))?;
    let status = Command::new("chown")
        .arg(format!("{SERVICE_USER}:{SERVICE_USER}"))
        .arg(dir)
        .status()?;
    if !status.success() {
        return Err("chown of /etc/foundry-agent failed".into());
    }
    Ok(())
}

fn install_self() -> Result<(), Box<dyn std::error::Error>> {
    let current = std::env::current_exe()?;
    if current == Path::new(INSTALL_PATH) {
        return Ok(());
    }
    std::fs::copy(&current, INSTALL_PATH)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(INSTALL_PATH, std::fs::Permissions::from_mode(0o755))?;
    println!("binary installed: {INSTALL_PATH}");
    Ok(())
}

fn group_exists(name: &str) -> bool {
    Command::new("getent")
        .args(["group", name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn create_service_user() -> Result<(), Box<dyn std::error::Error>> {
    let exists = Command::new("getent")
        .args(["passwd", SERVICE_USER])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        let status = Command::new("useradd")
            .args([
                "--system",
                "--home",
                "/etc/foundry-agent",
                "--shell",
                "/usr/sbin/nologin",
                SERVICE_USER,
            ])
            .status()?;
        if !status.success() {
            return Err("useradd foundry-agent failed".into());
        }
        println!("service user created: {SERVICE_USER}");
    }
    Ok(())
}

fn write_unit() -> Result<(), Box<dyn std::error::Error>> {
    // Only reference groups that exist on this host (systemd fails the
    // unit on unknown supplementary groups). docker → Engine API;
    // video/render → NVML device access.
    let groups: Vec<&str> = ["docker", "video", "render"]
        .into_iter()
        .filter(|g| group_exists(g))
        .collect();
    let supplementary = if groups.is_empty() {
        String::new()
    } else {
        format!("SupplementaryGroups={}\n", groups.join(" "))
    };

    let unit = render_unit(&supplementary);
    std::fs::write(UNIT_PATH, unit)?;
    println!("systemd unit written: {UNIT_PATH}");
    Ok(())
}

fn render_unit(supplementary: &str) -> String {
    // Hardening notes (docs/SECURITY.md § App Publishing):
    // - CAP_DAC_OVERRIDE is the only capability ambient in the agent and is
    //   narrowly required for project-authorized file sessions: container
    //   UIDs commonly own bind-mounted contents;
    // - CAP_SETUID, CAP_SETGID, and CAP_AUDIT_WRITE stay out of the ambient
    //   set but must remain in the bounding set so the setuid-root sudo child
    //   can initialize and run the two sudoers-scoped nginx commands;
    // - no NoNewPrivileges — it blocks the setuid transition the
    //   sudoers-scoped `sudo nginx -s reload` needs;
    // - ProtectSystem=full (not strict) — `nginx -t` runs inside this
    //   unit's mount namespace and must write its logs/temp under /var.
    format!(
        "# Written by `foundry-agent --register` / `--setup-apps` (source of truth:\n\
         # deployment/systemd/foundry-agent.service in the foundry repo).\n\
         [Unit]\n\
         Description=Foundry GPU server agent\n\
         After=network-online.target docker.service\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={SERVICE_USER}\n\
         Group={SERVICE_USER}\n\
         {supplementary}\
         ExecStart={INSTALL_PATH}\n\
         Environment=FOUNDRY_LOG_FORMAT=json\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         TimeoutStopSec=45\n\
         CapabilityBoundingSet={CAPABILITY_BOUNDING_SET}\n\
         AmbientCapabilities={AMBIENT_CAPABILITIES}\n\
         ProtectSystem=full\n\
         ProtectHome=yes\n\
         PrivateTmp=yes\n\
         ReadWritePaths=/etc/foundry-agent /etc/nginx/foundry-apps /storage/containers\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

fn systemctl(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("systemctl").args(args).status()?;
    if !status.success() {
        return Err(format!("systemctl {} failed", args.join(" ")).into());
    }
    Ok(())
}

// Minimal geteuid without adding the libc crate for one call.
unsafe fn libc_geteuid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn enrolled() -> AgentEnrollResponse {
        AgentEnrollResponse {
            agent_id: "agent-id".into(),
            agent_secret: "secret-with-\"quote".into(),
            server_id: foundry_shared::ServerId::new(),
            server_name: "GPU \"west\"".into(),
            poll_interval_secs: 17,
        }
    }

    fn test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("foundry-register-{}", uuid::Uuid::now_v7()))
    }

    #[test]
    fn config_write_is_parseable_private_and_atomic() {
        let dir = test_dir();
        let path = dir.join("config.toml");
        write_config(&path, "https://controller.example", &enrolled(), false).unwrap();

        let loaded = crate::config::load(&path).unwrap();
        assert_eq!(loaded.controller_url, "https://controller.example");
        assert_eq!(loaded.agent_secret, "secret-with-\"quote");
        assert_eq!(loaded.server_name.as_deref(), Some("GPU \"west\""));
        assert_eq!(loaded.poll_interval_secs, 17);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reenrollment_replaces_previous_config_without_staging_debris() {
        let dir = test_dir();
        let path = dir.join("config.toml");
        write_config(&path, "https://old.example", &enrolled(), false).unwrap();
        write_config(&path, "https://new.example", &enrolled(), false).unwrap();

        let loaded = crate::config::load(&path).unwrap();
        assert_eq!(loaded.controller_url, "https://new.example");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sudo_child_capabilities_are_bounded_but_not_ambient() {
        let unit = render_unit("SupplementaryGroups=docker\n");

        assert!(unit.contains(&format!(
            "CapabilityBoundingSet={CAPABILITY_BOUNDING_SET}\n"
        )));
        assert!(unit.contains(&format!("AmbientCapabilities={AMBIENT_CAPABILITIES}\n")));
        assert!(!AMBIENT_CAPABILITIES.contains("CAP_SETUID"));
        assert!(!AMBIENT_CAPABILITIES.contains("CAP_SETGID"));
        assert!(!AMBIENT_CAPABILITIES.contains("CAP_AUDIT_WRITE"));
    }
}

//! `foundry-agent --register --url <controller> --token <token>` —
//! GitLab-agent-style one-shot registration (docs/ARCHITECTURE.md
//! § Server Enrollment). Idempotent pieces, root required:
//!
//! 1. enroll against the controller (token is single-use),
//! 2. install this binary to /usr/local/bin/foundry-agent,
//! 3. create the foundry-agent system user (+ docker/video/render
//!    groups where they exist),
//! 4. write /etc/foundry-agent/config.toml (0600),
//! 5. set up HTTP/S app publishing (nginx include + vhost dir, TLS
//!    drop point, narrow sudoers rule — docs/SECURITY.md),
//! 6. write the systemd unit, daemon-reload, enable --now.
//!
//! `foundry-agent --setup-apps` re-runs 2/3/5/6 on an already-enrolled
//! server — the agent upgrade path.

use std::path::Path;
use std::process::Command;

use foundry_shared::dto::{AgentEnrollRequest, AgentEnrollResponse};

pub const INSTALL_PATH: &str = "/usr/local/bin/foundry-agent";
const UNIT_PATH: &str = "/etc/systemd/system/foundry-agent.service";
const SERVICE_USER: &str = "foundry-agent";
const TLS_DIR: &str = "/etc/foundry-agent/tls";
const SUDOERS_PATH: &str = "/etc/sudoers.d/foundry-agent";
const NGINX_BOOTSTRAP: &str = "/etc/nginx/conf.d/foundry-apps.conf";

pub struct RegisterArgs {
    pub url: String,
    pub token: String,
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

    // 1. Enroll.
    let url = args.url.trim_end_matches('/').to_string();
    let hostname = read_hostname();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .post(format!("{url}/agent/enroll"))
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
        return Err(format!(
            "enrollment rejected ({status}): {body} — tokens are single-use; \
             generate a fresh one in the Foundry UI"
        )
        .into());
    }
    let enrolled: AgentEnrollResponse = resp.json().await?;
    println!(
        "enrolled as server \"{}\" ({})",
        enrolled.server_name, enrolled.server_id
    );

    // 2..6 — system integration.
    if system_mode {
        install_self()?;
        create_service_user()?;
    }
    write_config(&config_path, &url, &enrolled, system_mode)?;
    println!("config written: {}", config_path.display());
    if system_mode {
        setup_apps()?;
        write_unit()?;
        systemctl(&["daemon-reload"])?;
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
    use std::os::unix::fs::PermissionsExt;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = format!(
        "# Written by `foundry-agent --register` — identity for this server.\n\
         controller_url = \"{url}\"\n\
         agent_id = \"{}\"\n\
         agent_secret = \"{}\"\n\
         server_name = \"{}\"\n\
         poll_interval_secs = {}\n",
        enrolled.agent_id, enrolled.agent_secret, enrolled.server_name, enrolled.poll_interval_secs,
    );
    std::fs::write(path, body)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    if system_mode {
        // The service user must be able to read its identity.
        let status = Command::new("chown")
            .args(["-R", &format!("{SERVICE_USER}:{SERVICE_USER}")])
            .arg(path.parent().unwrap_or(path))
            .status()?;
        if !status.success() {
            return Err("chown of /etc/foundry-agent failed".into());
        }
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

    // Hardening notes (docs/SECURITY.md § App Publishing):
    // - no NoNewPrivileges — it blocks the setuid transition the
    //   sudoers-scoped `sudo nginx -s reload` needs;
    // - ProtectSystem=full (not strict) — `nginx -t` runs inside this
    //   unit's mount namespace and must write its logs/temp under /var.
    let unit = format!(
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
         ProtectSystem=full\n\
         ProtectHome=yes\n\
         PrivateTmp=yes\n\
         ReadWritePaths=/etc/foundry-agent /etc/nginx/foundry-apps /storage/containers\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    );
    std::fs::write(UNIT_PATH, unit)?;
    println!("systemd unit written: {UNIT_PATH}");
    Ok(())
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

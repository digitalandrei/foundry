//! Agent-managed nginx vhosts — HTTP/S app publishing under the
//! wildcard apps domain (docs/ARCHITECTURE.md § App Publishing).
//!
//! One conf file per deployment at /etc/nginx/foundry-apps/<id>.conf;
//! every HTTP/S port gets a `server` pair: port-80 redirect plus a TLS
//! proxy to 127.0.0.1:<host_port>. The wildcard certificate is
//! operator-managed at /etc/foundry-agent/tls/ — private keys never
//! travel through Foundry (docs/SECURITY.md § App Publishing). Reloads
//! go through a sudoers rule restricted to `nginx -t` / `nginx -s
//! reload`; a failed config test rolls the file back so one bad vhost
//! can never wedge the rest of the server.

use std::path::PathBuf;

use foundry_shared::dto::PortBinding;
use foundry_shared::PortKind;

pub const VHOST_DIR: &str = "/etc/nginx/foundry-apps";
pub const TLS_CERT: &str = "/etc/foundry-agent/tls/fullchain.pem";
pub const TLS_KEY: &str = "/etc/foundry-agent/tls/privkey.pem";
const NGINX_BIN: &str = "/usr/sbin/nginx";

/// Smoke tests redirect the conf dir (`FOUNDRY_VHOST_DIR`) and skip the
/// nginx test/reload (`FOUNDRY_VHOST_NO_RELOAD`).
fn vhost_dir() -> PathBuf {
    std::env::var("FOUNDRY_VHOST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(VHOST_DIR))
}

fn reload_enabled() -> bool {
    std::env::var("FOUNDRY_VHOST_NO_RELOAD").is_err()
}

const FOUNDRY_INCLUDE: &str = "/etc/nginx/conf.d/foundry-apps.conf";

/// Granular HTTP/S app-publishing status for the inventory snapshot, so
/// the UI shows exactly what (if anything) is wrong rather than a vague
/// "nginx missing":
/// - `READY` — nginx installed, the service is active, and the Foundry
///   include (`--setup-apps`) is in place.
/// - `NGINX_MISSING` — the nginx binary isn't installed.
/// - `NGINX_INACTIVE` — installed but the service isn't running.
/// - `NOT_CONFIGURED` — installed + running, but `--setup-apps` hasn't
///   written the Foundry include yet.
pub fn app_publishing_status() -> &'static str {
    if !nginx_installed() {
        return "NGINX_MISSING";
    }
    // `systemctl is-active` is a read-only query (works as the service
    // user). Unknown (no systemctl) → don't claim it's down.
    if nginx_active() == Some(false) {
        return "NGINX_INACTIVE";
    }
    if !std::path::Path::new(FOUNDRY_INCLUDE).exists() {
        return "NOT_CONFIGURED";
    }
    "READY"
}

/// nginx binary present at any of the usual locations (Ubuntu installs
/// it at `/usr/sbin/nginx`, which is also what the reload sudoers rule
/// targets).
fn nginx_installed() -> bool {
    [
        NGINX_BIN,
        "/usr/bin/nginx",
        "/usr/local/sbin/nginx",
        "/usr/local/bin/nginx",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).exists())
}

/// `systemctl is-active nginx` → Some(true/false); None when systemctl
/// can't be queried (status then falls through as "not inactive").
fn nginx_active() -> Option<bool> {
    std::process::Command::new("systemctl")
        .args(["is-active", "nginx"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
}

/// The ports of a deploy payload that publish a vhost.
pub fn web_ports(ports: &[PortBinding]) -> Vec<&PortBinding> {
    ports
        .iter()
        .filter(|p| p.hostname.is_some() && matches!(p.kind, PortKind::Http | PortKind::Https))
        .collect()
}

/// Write (or rewrite) the deployment's vhost file and reload nginx.
/// Idempotent: identical content short-circuits without a reload, so
/// task re-delivery is cheap. Empty `ports` removes the file instead.
pub async fn apply(deployment_id: &str, ports: &[&PortBinding]) -> Result<(), String> {
    if ports.is_empty() {
        return remove(deployment_id).await;
    }
    validate_id(deployment_id)?;
    for p in ports {
        let hostname = p.hostname.as_deref().unwrap_or_default();
        if !valid_hostname(hostname) {
            return Err(format!("refusing invalid vhost hostname {hostname:?}"));
        }
    }

    let path = vhost_dir().join(format!("{deployment_id}.conf"));
    let content = render(deployment_id, ports);
    let previous = tokio::fs::read_to_string(&path).await.ok();
    if previous.as_deref() == Some(content.as_str()) {
        return Ok(()); // unchanged — nothing to reload
    }
    tokio::fs::write(&path, &content).await.map_err(|e| {
        format!(
            "writing vhost {} failed: {e} — run `sudo foundry-agent --setup-apps` on this server",
            path.display()
        )
    })?;

    if let Err(test_err) = nginx(&["-t"]).await {
        // Roll back so the broken file can't block every later reload.
        match previous {
            Some(old) => {
                let _ = tokio::fs::write(&path, old).await;
            }
            None => {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
        return Err(format!(
            "vhost rejected by nginx -t (rolled back): {test_err}"
        ));
    }
    nginx(&["-s", "reload"]).await
}

/// Remove the deployment's vhost file (if any) and reload. Absent file
/// → idempotent success without touching nginx.
pub async fn remove(deployment_id: &str) -> Result<(), String> {
    validate_id(deployment_id)?;
    let path = vhost_dir().join(format!("{deployment_id}.conf"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("removing vhost {} failed: {e}", path.display())),
    }
    nginx(&["-s", "reload"]).await
}

/// `sudo -n nginx …` — the sudoers rule installed by `--setup-apps`
/// allows exactly `-t` and `-s reload`, nothing else.
async fn nginx(args: &[&str]) -> Result<(), String> {
    if !reload_enabled() {
        return Ok(());
    }
    let output = tokio::process::Command::new("sudo")
        .arg("-n")
        .arg(NGINX_BIN)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("running sudo nginx failed: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "nginx {} failed: {} — is nginx installed and `--setup-apps` done?",
        args.join(" "),
        stderr.chars().take(400).collect::<String>().trim()
    ))
}

fn validate_id(deployment_id: &str) -> Result<(), String> {
    let ok = !deployment_id.is_empty()
        && deployment_id.len() <= 64
        && deployment_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!("invalid deployment id {deployment_id:?}"))
    }
}

fn valid_hostname(hostname: &str) -> bool {
    !hostname.is_empty()
        && hostname.len() <= 253
        && hostname
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

/// Pure conf rendering (unit-tested). `$foundry_connection_upgrade` is
/// the websocket map defined once in /etc/nginx/conf.d/foundry-apps.conf
/// by `--setup-apps`.
fn render(deployment_id: &str, ports: &[&PortBinding]) -> String {
    let mut out = format!(
        "# Managed by foundry-agent — deployment {deployment_id}.\n\
         # Do not edit: rewritten on deploy, removed with the container.\n"
    );
    for p in ports {
        let hostname = p.hostname.as_deref().unwrap_or_default();
        let scheme = if p.kind == PortKind::Https {
            "https"
        } else {
            "http"
        };
        // HTTPS upstreams present a self-signed/container cert; the hop
        // is loopback-only so verification adds nothing.
        let ssl_verify = if p.kind == PortKind::Https {
            "        proxy_ssl_verify off;\n"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n\
             server {{\n\
             \x20   listen 80;\n\
             \x20   listen [::]:80;\n\
             \x20   server_name {hostname};\n\
             \x20   return 301 https://$host$request_uri;\n\
             }}\n\
             \n\
             server {{\n\
             \x20   listen 443 ssl;\n\
             \x20   listen [::]:443 ssl;\n\
             \x20   http2 on;\n\
             \x20   server_name {hostname};\n\
             \n\
             \x20   ssl_certificate     {TLS_CERT};\n\
             \x20   ssl_certificate_key {TLS_KEY};\n\
             \n\
             \x20   client_max_body_size 100m;\n\
             \n\
             \x20   location / {{\n\
             \x20       proxy_pass {scheme}://127.0.0.1:{port};\n\
             {ssl_verify}\
             \x20       proxy_http_version 1.1;\n\
             \x20       proxy_set_header Host $host;\n\
             \x20       proxy_set_header X-Real-IP $remote_addr;\n\
             \x20       proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n\
             \x20       proxy_set_header X-Forwarded-Proto https;\n\
             \x20       proxy_set_header Upgrade $http_upgrade;\n\
             \x20       proxy_set_header Connection $foundry_connection_upgrade;\n\
             \x20       proxy_read_timeout 300s;\n\
             \x20       proxy_send_timeout 300s;\n\
             \x20       proxy_buffering off;\n\
             \x20   }}\n\
             }}\n",
            port = p.host_port,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(kind: PortKind, host_port: u16, hostname: &str) -> PortBinding {
        PortBinding {
            container_port: 8080,
            host_port,
            protocol: kind.protocol().to_string(),
            kind,
            hostname: Some(hostname.to_string()),
        }
    }

    #[test]
    fn render_http_vhost_redirects_and_proxies() {
        let p = binding(PortKind::Http, 20001, "demo.ai.protv.ro");
        let conf = render("0192-dep", &[&p]);
        assert!(conf.contains("server_name demo.ai.protv.ro;"));
        assert!(conf.contains("return 301 https://$host$request_uri;"));
        assert!(conf.contains("proxy_pass http://127.0.0.1:20001;"));
        assert!(conf.contains("ssl_certificate     /etc/foundry-agent/tls/fullchain.pem;"));
        assert!(conf.contains("proxy_set_header Connection $foundry_connection_upgrade;"));
        assert!(!conf.contains("proxy_ssl_verify"));
    }

    #[test]
    fn render_https_upstream_disables_verify() {
        let p = binding(PortKind::Https, 20002, "tls-app.ai.protv.ro");
        let conf = render("dep", &[&p]);
        assert!(conf.contains("proxy_pass https://127.0.0.1:20002;"));
        assert!(conf.contains("proxy_ssl_verify off;"));
    }

    #[test]
    fn render_multi_port_emits_one_pair_per_hostname() {
        let a = binding(PortKind::Http, 20001, "app-8080.ai.protv.ro");
        let b = binding(PortKind::Http, 20002, "app-9090.ai.protv.ro");
        let conf = render("dep", &[&a, &b]);
        assert_eq!(conf.matches("listen 443 ssl;").count(), 2);
        assert_eq!(conf.matches("return 301").count(), 2);
    }

    #[test]
    fn hostname_and_id_validation() {
        assert!(valid_hostname("a-1.ai.protv.ro"));
        assert!(!valid_hostname("bad host"));
        assert!(!valid_hostname("semi;colon"));
        assert!(!valid_hostname("UPPER.ai.protv.ro"));
        assert!(validate_id("0192aef0-1-b").is_ok());
        assert!(validate_id("../../etc/passwd").is_err());
        assert!(validate_id("").is_err());
    }

    /// apply/remove file lifecycle against a temp dir, reload skipped
    /// (the env vars are the documented smoke-test escape hatch; no
    /// other test reads them).
    #[tokio::test]
    async fn apply_and_remove_roundtrip() {
        let dir = std::env::temp_dir().join(format!("foundry-vhost-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("FOUNDRY_VHOST_DIR", &dir);
        std::env::set_var("FOUNDRY_VHOST_NO_RELOAD", "1");

        let p = binding(PortKind::Http, 20007, "round.ai.protv.ro");
        apply("dep-rt", &[&p]).await.unwrap();
        let conf_path = dir.join("dep-rt.conf");
        let written = std::fs::read_to_string(&conf_path).unwrap();
        assert!(written.contains("server_name round.ai.protv.ro;"));

        // Idempotent re-apply, then a port change rewrites the file.
        apply("dep-rt", &[&p]).await.unwrap();
        let changed = binding(PortKind::Http, 20008, "round.ai.protv.ro");
        apply("dep-rt", &[&changed]).await.unwrap();
        assert!(std::fs::read_to_string(&conf_path)
            .unwrap()
            .contains("proxy_pass http://127.0.0.1:20008;"));

        // Empty port set behaves like remove; double-remove is Ok.
        apply("dep-rt", &[]).await.unwrap();
        assert!(!conf_path.exists());
        remove("dep-rt").await.unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

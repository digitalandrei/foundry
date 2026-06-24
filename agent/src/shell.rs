//! Interactive shell executor (operator: web terminal, no SSH). Pull-only
//! by construction: the agent long-polls `/agent/shell/next`; on a request
//! it dials a WebSocket *back* to the controller
//! (`/agent/shell/attach/{id}`) and bridges it to a `docker exec` TTY on
//! the managed container — bash if present, else sh. Only
//! `foundry.managed=true` containers are ever exec'd into.

use std::time::Duration;

use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions, StartExecResults};
use bollard::query_parameters::ListContainersOptions;
use bollard::Docker;
use foundry_shared::dto::ShellRequest;
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::config::AgentConfig;

/// The shell prompt: prefer bash, fall back to sh — both via one exec so
/// there's no two-attempt round trip (operator: "try bash and sh").
const SHELL_CMD: &str = "if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi";

pub async fn run_loop(
    client: &reqwest::Client,
    config: &AgentConfig,
    docker: Option<bollard::Docker>,
) {
    let base = config.controller_url.trim_end_matches('/');
    let next_url = format!("{base}/agent/shell/next");
    loop {
        tokio::select! {
            _ = crate::shutdown_signal() => break,
            req = poll_next(client, config, &next_url) => {
                let Some(req) = req else { continue };
                tracing::info!(deployment = %req.deployment_id, "opening container shell");
                let config = config.clone();
                let docker = docker.clone();
                tokio::spawn(async move {
                    let Some(docker) = docker else {
                        tracing::warn!("shell requested but Docker is unavailable");
                        return;
                    };
                    if let Err(err) = handle(&config, &docker, req).await {
                        tracing::warn!(%err, "shell session ended with error");
                    }
                });
            }
        }
    }
}

/// One long-poll for a pending shell; None on idle/error (caller loops).
async fn poll_next(
    client: &reqwest::Client,
    config: &AgentConfig,
    url: &str,
) -> Option<ShellRequest> {
    let resp = client
        .get(url)
        .header("x-foundry-agent-id", &config.agent_id)
        .bearer_auth(&config.agent_secret)
        .timeout(Duration::from_secs(40))
        .send()
        .await;
    match resp {
        Ok(r) if r.status() == reqwest::StatusCode::OK => r.json::<ShellRequest>().await.ok(),
        Ok(r) if r.status() == reqwest::StatusCode::NO_CONTENT => None,
        Ok(r) => {
            tracing::warn!(status = %r.status(), "shell poll rejected");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
        Err(err) => {
            tracing::debug!(%err, "shell poll failed (controller unreachable)");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
    }
}

async fn handle(config: &AgentConfig, docker: &Docker, req: ShellRequest) -> Result<(), String> {
    let container = match &req.container_id {
        // Adopted (externally-created) container — exec by docker id.
        Some(cid) => find_running_by_id(docker, cid)
            .await
            .ok_or_else(|| "adopted container is not running".to_string())?,
        None => find_running_managed(docker, &req.deployment_id.to_string())
            .await
            .ok_or_else(|| "no running managed container for this deployment".to_string())?,
    };

    // Create + start the exec with a PTY attached to all three streams.
    let exec = docker
        .create_exec(
            &container,
            CreateExecOptions {
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                tty: Some(true),
                env: Some(vec!["TERM=xterm-256color".to_string()]),
                cmd: Some(vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    SHELL_CMD.to_string(),
                ]),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("create exec: {e}"))?;
    let started = docker
        .start_exec(
            &exec.id,
            Some(StartExecOptions {
                detach: false,
                tty: true,
                output_capacity: None,
            }),
        )
        .await
        .map_err(|e| format!("start exec: {e}"))?;
    let StartExecResults::Attached {
        mut output,
        mut input,
    } = started
    else {
        return Err("exec did not attach".into());
    };

    // Dial the bridge back to the controller (WSS) with agent auth.
    let url = ws_url(&config.controller_url, req.session_id);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| format!("ws request: {e}"))?;
    let headers = request.headers_mut();
    headers.insert(
        "x-foundry-agent-id",
        config
            .agent_id
            .parse()
            .map_err(|_| "bad agent id".to_string())?,
    );
    headers.insert(
        "authorization",
        format!("Bearer {}", config.agent_secret)
            .parse()
            .map_err(|_| "bad agent secret".to_string())?,
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("ws connect: {e}"))?;
    let (mut ws_tx, mut ws_rx) = ws.split();

    loop {
        tokio::select! {
            // Browser → container: keystrokes (binary) + resize (text).
            msg = ws_rx.next() => match msg {
                Some(Ok(Message::Binary(b))) => {
                    if input.write_all(&b).await.is_err() { break }
                    let _ = input.flush().await;
                }
                Some(Ok(Message::Text(t))) => {
                    if let Ok(r) = serde_json::from_str::<Resize>(&t) {
                        let _ = docker
                            .resize_exec(&exec.id, ResizeExecOptions { height: r.rows, width: r.cols })
                            .await;
                    }
                }
                Some(Ok(Message::Ping(p))) => {
                    let _ = ws_tx.send(Message::Pong(p)).await;
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(err)) => { tracing::debug!(%err, "shell ws closed"); break; }
                _ => {}
            },
            // Container → browser: stdout/stderr (interleaved by the TTY).
            out = output.next() => match out {
                Some(Ok(log)) => {
                    if ws_tx.send(Message::Binary(log.into_bytes())).await.is_err() { break }
                }
                Some(Err(err)) => {
                    let _ = ws_tx
                        .send(Message::Binary(format!("\r\n[exec error: {err}]\r\n").into_bytes().into()))
                        .await;
                    break;
                }
                None => break, // shell exited
            },
        }
    }
    let _ = ws_tx.send(Message::Close(None)).await;
    Ok(())
}

#[derive(serde::Deserialize)]
struct Resize {
    cols: u16,
    rows: u16,
}

/// `https://host` → `wss://host/agent/shell/attach/{id}` (http → ws).
fn ws_url(controller_url: &str, session_id: uuid::Uuid) -> String {
    let base = controller_url.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{base}/agent/shell/attach/{session_id}")
}

/// A running container by (short) docker id — the adopted-container path.
async fn find_running_by_id(docker: &Docker, short_id: &str) -> Option<String> {
    let list = docker
        .list_containers(Some(ListContainersOptions::default())) // running only
        .await
        .ok()?;
    list.into_iter().find_map(|c| {
        let id = c.id?;
        id.starts_with(short_id).then_some(id)
    })
}

/// The running managed container for a deployment (by label, never name).
async fn find_running_managed(docker: &Docker, deployment_id: &str) -> Option<String> {
    let list = docker
        .list_containers(Some(ListContainersOptions::default())) // running only
        .await
        .ok()?;
    list.into_iter().find_map(|c| {
        let labels = c.labels.as_ref()?;
        if labels.get("foundry.managed").map(String::as_str) != Some("true") {
            return None;
        }
        if labels.get("foundry.deployment_id").map(String::as_str) != Some(deployment_id) {
            return None;
        }
        c.id
    })
}

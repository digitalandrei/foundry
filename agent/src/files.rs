//! Persistent-volume file browser executor. The agent long-polls for an
//! approved session, dials a WebSocket back to the controller, and performs
//! operations only beneath the supplied `/storage/containers/...` roots.
//! It never listens and never accepts an arbitrary host path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use foundry_shared::dto::{FileClientMessage, FileServerMessage, FileSessionRequest};
use foundry_shared::ServerVolumeId;
use futures_util::{Sink, SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::config::AgentConfig;
use crate::file_system::{
    approved_roots, atomic_write, copy_entry, delete_entry, fs_error, list_directory, move_entry,
    non_root_existing, resolve_destination, resolve_existing, resolve_for_delete,
    resolve_new_destination, root, temporary_sibling,
};

const TEXT_LIMIT: u64 = 2 * 1024 * 1024;
const TRANSFER_CHUNK: usize = 128 * 1024;

pub async fn run_loop(client: &reqwest::Client, config: &AgentConfig) {
    let base = config.controller_url.trim_end_matches('/');
    let next_url = format!("{base}/agent/volume-files/next");
    loop {
        tokio::select! {
            _ = crate::shutdown_signal() => break,
            request = poll_next(client, config, &next_url) => {
                let Some(request) = request else { continue };
                tracing::info!(
                    project = %request.project_id,
                    volumes = request.volumes.len(),
                    "opening volume file session"
                );
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle(&config, request).await {
                        tracing::warn!(%error, "volume file session ended with error");
                    }
                });
            }
        }
    }
}

async fn poll_next(
    client: &reqwest::Client,
    config: &AgentConfig,
    url: &str,
) -> Option<FileSessionRequest> {
    let response = client
        .get(url)
        .header("x-foundry-agent-id", &config.agent_id)
        .bearer_auth(&config.agent_secret)
        .timeout(Duration::from_secs(40))
        .send()
        .await;
    match response {
        Ok(response) if response.status() == reqwest::StatusCode::OK => {
            response.json::<FileSessionRequest>().await.ok()
        }
        Ok(response) if response.status() == reqwest::StatusCode::NO_CONTENT => None,
        Ok(response) => {
            tracing::warn!(status = %response.status(), "volume files poll rejected");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
        Err(error) => {
            tracing::debug!(%error, "volume files poll failed (controller unreachable)");
            tokio::time::sleep(Duration::from_secs(5)).await;
            None
        }
    }
}

struct PendingUpload {
    destination: PathBuf,
    temporary: PathBuf,
    size: u64,
    written: u64,
    file: tokio::fs::File,
}

async fn handle(config: &AgentConfig, request: FileSessionRequest) -> Result<(), String> {
    let roots = approved_roots(&request).await?;
    let quotas: HashMap<ServerVolumeId, Option<u64>> = request
        .volumes
        .iter()
        .map(|volume| (volume.volume_id, volume.quota_bytes))
        .collect();
    let url = ws_url(&config.controller_url, request.session_id);
    let mut ws_request = url
        .as_str()
        .into_client_request()
        .map_err(|error| format!("ws request: {error}"))?;
    let headers = ws_request.headers_mut();
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
    let (ws, _) = tokio_tungstenite::connect_async(ws_request)
        .await
        .map_err(|error| format!("ws connect: {error}"))?;
    let (mut sender, mut receiver) = ws.split();
    let mut uploads: HashMap<Uuid, PendingUpload> = HashMap::new();

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let operation = match serde_json::from_str::<FileClientMessage>(&text) {
                    Ok(operation) => operation,
                    Err(error) => {
                        tracing::debug!(%error, "invalid volume file request");
                        continue;
                    }
                };
                let request_id = operation.request_id();
                if let Err(error) =
                    process(&mut sender, &roots, &quotas, &mut uploads, operation).await
                {
                    send(
                        &mut sender,
                        &FileServerMessage::Error {
                            request_id,
                            message: error,
                        },
                    )
                    .await?;
                }
            }
            Ok(Message::Ping(payload)) => {
                sender
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    // Partial uploads intentionally survive disconnects; a browser can
    // reconnect and continue from the offset returned by UploadReady.
    let _ = sender.send(Message::Close(None)).await;
    Ok(())
}

async fn process<S>(
    sender: &mut S,
    roots: &HashMap<ServerVolumeId, PathBuf>,
    quotas: &HashMap<ServerVolumeId, Option<u64>>,
    uploads: &mut HashMap<Uuid, PendingUpload>,
    operation: FileClientMessage,
) -> Result<(), String>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    match operation {
        FileClientMessage::List {
            request_id,
            volume_id,
            path,
        } => {
            let root = root(roots, volume_id)?.clone();
            let requested = path.clone();
            let entries = tokio::task::spawn_blocking(move || list_directory(&root, &requested))
                .await
                .map_err(|error| format!("list task: {error}"))??;
            send(
                sender,
                &FileServerMessage::Listing {
                    request_id,
                    path,
                    entries,
                },
            )
            .await
        }
        FileClientMessage::ReadText {
            request_id,
            volume_id,
            path,
        } => {
            let target = resolve_existing(root(roots, volume_id)?, &path)?;
            let metadata = tokio::fs::metadata(&target)
                .await
                .map_err(|error| fs_error("read metadata", error))?;
            if !metadata.is_file() {
                return Err("only regular files can be edited".into());
            }
            if metadata.len() > TEXT_LIMIT {
                return Err("file is too large for the text editor (maximum 2 MiB)".into());
            }
            let bytes = tokio::fs::read(target)
                .await
                .map_err(|error| fs_error("read file", error))?;
            if bytes.contains(&0) {
                return Err("binary files cannot be opened in the text editor".into());
            }
            let content = String::from_utf8(bytes)
                .map_err(|_| "file is not valid UTF-8; use Download instead".to_string())?;
            send(
                sender,
                &FileServerMessage::Text {
                    request_id,
                    content,
                },
            )
            .await
        }
        FileClientMessage::WriteText {
            request_id,
            volume_id,
            path,
            content,
        } => {
            if content.len() as u64 > TEXT_LIMIT {
                return Err("text is too large to save (maximum 2 MiB)".into());
            }
            let destination = resolve_destination(root(roots, volume_id)?, &path)?;
            if destination.is_dir() {
                return Err("cannot replace a directory with text".into());
            }
            atomic_write(&destination, content.as_bytes()).await?;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Mkdir {
            request_id,
            volume_id,
            path,
        } => {
            let destination = resolve_destination(root(roots, volume_id)?, &path)?;
            tokio::fs::create_dir(&destination)
                .await
                .map_err(|error| fs_error("create directory", error))?;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Rename {
            request_id,
            volume_id,
            from,
            to,
        } => {
            let source = non_root_existing(root(roots, volume_id)?, &from)?;
            let destination = resolve_new_destination(root(roots, volume_id)?, &to)?;
            tokio::fs::rename(source, destination)
                .await
                .map_err(|error| fs_error("rename", error))?;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Copy {
            request_id,
            from_volume_id,
            from,
            to_volume_id,
            to,
        } => {
            let source = non_root_existing(root(roots, from_volume_id)?, &from)?;
            let destination = resolve_new_destination(root(roots, to_volume_id)?, &to)?;
            tokio::task::spawn_blocking(move || copy_entry(&source, &destination))
                .await
                .map_err(|error| format!("copy task: {error}"))??;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Move {
            request_id,
            from_volume_id,
            from,
            to_volume_id,
            to,
        } => {
            let source = non_root_existing(root(roots, from_volume_id)?, &from)?;
            let destination = resolve_new_destination(root(roots, to_volume_id)?, &to)?;
            tokio::task::spawn_blocking(move || move_entry(&source, &destination))
                .await
                .map_err(|error| format!("move task: {error}"))??;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Delete {
            request_id,
            volume_id,
            path,
        } => {
            let target = resolve_for_delete(root(roots, volume_id)?, &path)?;
            tokio::task::spawn_blocking(move || delete_entry(&target))
                .await
                .map_err(|error| format!("delete task: {error}"))??;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
        FileClientMessage::Download {
            request_id,
            volume_id,
            path,
        } => download(sender, root(roots, volume_id)?, request_id, &path).await,
        FileClientMessage::UploadStart {
            request_id,
            volume_id,
            path,
            size,
        } => {
            if uploads.contains_key(&request_id) {
                return Err("upload already exists".into());
            }
            let destination = resolve_destination(root(roots, volume_id)?, &path)?;
            if destination.is_dir() {
                return Err("cannot replace a directory with an uploaded file".into());
            }
            let temporary = temporary_sibling(&destination, request_id)?;
            let written = tokio::fs::metadata(&temporary)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if written > size {
                return Err("partial upload is larger than the declared file".into());
            }
            if let Some(quota) = quotas.get(&volume_id).copied().flatten() {
                let root = root(roots, volume_id)?.clone();
                let existing = tokio::fs::metadata(&destination)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                let used =
                    tokio::task::spawn_blocking(move || crate::host::directory_size_for(&root))
                        .await
                        .map_err(|error| format!("quota task: {error}"))?;
                let final_used = used
                    .saturating_sub(existing)
                    .saturating_add(size.saturating_sub(written));
                if final_used > quota {
                    return Err(format!(
                        "upload would exceed the volume quota ({final_used} > {quota} bytes)"
                    ));
                }
            }
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .write(true)
                .open(&temporary)
                .await
                .map_err(|error| fs_error("start upload", error))?;
            uploads.insert(
                request_id,
                PendingUpload {
                    destination,
                    temporary,
                    size,
                    written,
                    file,
                },
            );
            send(
                sender,
                &FileServerMessage::UploadReady {
                    request_id,
                    offset: written,
                },
            )
            .await
        }
        FileClientMessage::UploadChunk {
            request_id,
            offset,
            data,
        } => {
            if data.len() > TRANSFER_CHUNK * 2 {
                return Err("upload chunk is too large".into());
            }
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "upload chunk is not valid base64".to_string())?;
            let upload = uploads
                .get_mut(&request_id)
                .ok_or_else(|| "upload is not active".to_string())?;
            if offset != upload.written {
                return Err(format!(
                    "upload offset mismatch: expected {}, received {offset}",
                    upload.written
                ));
            }
            if upload.written.saturating_add(decoded.len() as u64) > upload.size {
                return Err("upload exceeds its declared size".into());
            }
            upload
                .file
                .write_all(&decoded)
                .await
                .map_err(|error| fs_error("write upload", error))?;
            upload.written += decoded.len() as u64;
            Ok(())
        }
        FileClientMessage::UploadFinish { request_id } => {
            let mut upload = uploads
                .remove(&request_id)
                .ok_or_else(|| "upload is not active".to_string())?;
            if upload.written != upload.size {
                return Err(format!(
                    "upload ended {} bytes before its declared size",
                    upload.size.saturating_sub(upload.written)
                ));
            }
            upload
                .file
                .flush()
                .await
                .map_err(|error| fs_error("flush upload", error))?;
            upload
                .file
                .sync_all()
                .await
                .map_err(|error| fs_error("sync upload", error))?;
            drop(upload.file);
            tokio::fs::rename(&upload.temporary, &upload.destination)
                .await
                .map_err(|error| fs_error("finish upload", error))?;
            send(sender, &FileServerMessage::Ack { request_id }).await
        }
    }
}

async fn download<S>(sender: &mut S, root: &Path, request_id: Uuid, raw: &str) -> Result<(), String>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let target = resolve_existing(root, raw)?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|error| fs_error("read download metadata", error))?;
    if !metadata.is_file() {
        return Err("only regular files can be downloaded".into());
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "download name is not valid UTF-8".to_string())?
        .to_string();
    send(
        sender,
        &FileServerMessage::DownloadStart {
            request_id,
            name,
            size: metadata.len(),
        },
    )
    .await?;
    let mut file = tokio::fs::File::open(target)
        .await
        .map_err(|error| fs_error("open download", error))?;
    let mut buffer = vec![0_u8; TRANSFER_CHUNK];
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|error| fs_error("read download", error))?;
        if count == 0 {
            break;
        }
        send(
            sender,
            &FileServerMessage::DownloadChunk {
                request_id,
                data: base64::engine::general_purpose::STANDARD.encode(&buffer[..count]),
            },
        )
        .await?;
    }
    send(sender, &FileServerMessage::DownloadFinish { request_id }).await
}

async fn send<S>(sender: &mut S, response: &FileServerMessage) -> Result<(), String>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let json = serde_json::to_string(response).map_err(|error| error.to_string())?;
    sender
        .send(Message::Text(json.into()))
        .await
        .map_err(|error| error.to_string())
}

fn ws_url(controller_url: &str, session_id: Uuid) -> String {
    let base = controller_url.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{base}/agent/volume-files/attach/{session_id}")
}

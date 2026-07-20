//! Persistent-volume file browser protocol (docs/API.md § Volume files).
//! Like the interactive shell, the browser connects to the controller and
//! the pull-only server agent dials back. Every operation is relative to a
//! controller-approved volume root; host paths are never accepted from the
//! browser.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{GitlabProjectId, ServerVolumeId};

/// One controller-approved root exposed to a file-browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVolumeRoot {
    pub volume_id: ServerVolumeId,
    pub name: String,
    pub path: String,
}

/// A pending file-browser session the target agent should attach to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSessionRequest {
    pub session_id: uuid::Uuid,
    pub project_id: GitlabProjectId,
    pub volumes: Vec<FileVolumeRoot>,
}

/// Browser-to-agent operations. Paths are UTF-8, slash-separated and
/// relative to their `volume_id`; the empty string denotes the root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileClientMessage {
    List {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
    },
    ReadText {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
    },
    WriteText {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
        content: String,
    },
    Mkdir {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
    },
    Rename {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        from: String,
        to: String,
    },
    Copy {
        request_id: uuid::Uuid,
        from_volume_id: ServerVolumeId,
        from: String,
        to_volume_id: ServerVolumeId,
        to: String,
    },
    Move {
        request_id: uuid::Uuid,
        from_volume_id: ServerVolumeId,
        from: String,
        to_volume_id: ServerVolumeId,
        to: String,
    },
    Delete {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
    },
    Download {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
    },
    UploadStart {
        request_id: uuid::Uuid,
        volume_id: ServerVolumeId,
        path: String,
        size: u64,
    },
    UploadChunk {
        request_id: uuid::Uuid,
        data: String,
    },
    UploadFinish {
        request_id: uuid::Uuid,
    },
}

impl FileClientMessage {
    pub fn request_id(&self) -> uuid::Uuid {
        match self {
            Self::List { request_id, .. }
            | Self::ReadText { request_id, .. }
            | Self::WriteText { request_id, .. }
            | Self::Mkdir { request_id, .. }
            | Self::Rename { request_id, .. }
            | Self::Copy { request_id, .. }
            | Self::Move { request_id, .. }
            | Self::Delete { request_id, .. }
            | Self::Download { request_id, .. }
            | Self::UploadStart { request_id, .. }
            | Self::UploadChunk { request_id, .. }
            | Self::UploadFinish { request_id, .. } => *request_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEntryKind {
    Directory,
    File,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileEntryKind,
    pub size: u64,
    pub modified_at: Option<DateTime<Utc>>,
}

/// Agent-to-browser responses. Upload and download chunks are base64 so
/// every frame stays self-describing JSON through the controller bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileServerMessage {
    Listing {
        request_id: uuid::Uuid,
        path: String,
        entries: Vec<FileEntry>,
    },
    Text {
        request_id: uuid::Uuid,
        content: String,
    },
    UploadReady {
        request_id: uuid::Uuid,
    },
    DownloadStart {
        request_id: uuid::Uuid,
        name: String,
        size: u64,
    },
    DownloadChunk {
        request_id: uuid::Uuid,
        data: String,
    },
    DownloadFinish {
        request_id: uuid::Uuid,
    },
    Ack {
        request_id: uuid::Uuid,
    },
    Error {
        request_id: uuid::Uuid,
        message: String,
    },
}

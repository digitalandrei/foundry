//! Interactive container shell handshake (docs/API.md § Shell). The
//! browser opens a WebSocket to the controller; the controller registers
//! a pending session and the target server's agent learns of it via
//! `GET /agent/shell/next` (this type), then dials
//! `GET /agent/shell/attach/{session_id}` back as a WebSocket. From there
//! it's raw TTY bytes (binary) + resize control (text) both ways.

use serde::{Deserialize, Serialize};

use crate::DeploymentId;

/// A pending shell the agent should attach to. `session_id` is the
/// controller-side correlation id; `deployment_id` tells the agent which
/// managed container to `docker exec` into.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRequest {
    pub session_id: uuid::Uuid,
    pub deployment_id: DeploymentId,
}

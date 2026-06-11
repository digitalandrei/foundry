//! `POST /auth/local` (docs/API.md) — local operator sign-in.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLoginRequest {
    pub username: String,
    pub password: String,
}

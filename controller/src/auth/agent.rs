//! Agent authentication extractor: every `/agent/*` request (except
//! enroll) presents `X-Foundry-Agent-Id` + `Authorization: Bearer
//! <secret>`; the secret is verified against its stored SHA-256 in
//! constant time. The resulting context is scoped to that server only
//! (docs/SECURITY.md § Agent Authentication).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::AppError;
use crate::repos::servers::{self, AgentContext};
use crate::state::AppState;

pub struct AuthenticatedAgent(pub AgentContext);

impl FromRequestParts<AppState> for AuthenticatedAgent {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let agent_id = parts
            .headers
            .get("x-foundry-agent-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or(AppError::Unauthorized)?;
        let secret = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        let ctx = servers::authenticate_agent(&state.pool, agent_id, secret).await?;
        Ok(AuthenticatedAgent(ctx))
    }
}

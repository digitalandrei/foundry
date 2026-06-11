//! Controller-wide error type. Handlers return `Result<_, AppError>`;
//! the `IntoResponse` impl emits the envelope from docs/API.md and
//! never leaks internals or secrets.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use foundry_shared::dto::ErrorEnvelope;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    NotFound(&'static str),
    #[error("{0}")]
    BadRequest(String),
    #[error("GitLab instance is unreachable or returned an error")]
    GitlabUpstream(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl AppError {
    pub fn internal<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        AppError::Internal(Box::new(err))
    }

    pub fn gitlab<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        AppError::GitlabUpstream(Box::new(err))
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "validation"),
            AppError::GitlabUpstream(_) => (StatusCode::BAD_GATEWAY, "gitlab_upstream"),
            AppError::Db(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if status.is_server_error() || matches!(self, AppError::GitlabUpstream(_)) {
            tracing::error!(error = ?self, %status, "request failed");
        }
        // 5xx details stay in the log; everything else is safe to echo.
        let message = if status.is_server_error() {
            "internal error".to_string()
        } else {
            self.to_string()
        };
        (status, Json(ErrorEnvelope::new(code, message))).into_response()
    }
}

//! Sessions, auth extractors, and the OAuth login/callback/logout
//! routes (docs/SECURITY.md § Identity & Sessions).

pub mod agent;
pub mod cookies;
pub mod routes;
pub mod session;

use axum::http::HeaderMap;

/// Real client IP for audit rows. Nginx restores it from
/// CF-Connecting-IP and forwards it as X-Real-IP
/// (deployment/nginx/...). The controller is localhost-bound, so the
/// header is trustworthy.
pub fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

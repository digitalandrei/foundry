//! GitLab OAuth authorization-code flow with PKCE
//! (docs/GITLAAB-INTEGRATION.md § OAuth — scopes are fixed:
//! openid profile email read_api read_registry).
//!
//! Flow state (CSRF token + PKCE verifier + instance) crosses the
//! redirect inside an encrypted, short-lived cookie — nothing stored
//! server-side.

use chrono::{DateTime, Duration, Utc};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

use foundry_shared::GitlabInstanceId;

use super::InstanceConfig;
use crate::error::AppError;

pub const SCOPES: [&str; 5] = ["openid", "profile", "email", "read_api", "read_registry"];

/// Contents of the encrypted `foundry_oauth` cookie.
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingLogin {
    pub instance_id: GitlabInstanceId,
    pub csrf: String,
    pub pkce_verifier: String,
    pub started_at: DateTime<Utc>,
}

impl PendingLogin {
    pub fn is_expired(&self) -> bool {
        Utc::now() - self.started_at > Duration::minutes(10)
    }
}

/// Exchanged tokens, normalized for storage.
pub struct ExchangedTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

fn oauth_client(
    instance: &InstanceConfig,
    public_url: &str,
) -> Result<
    BasicClient<
        oauth2::EndpointSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointSet,
    >,
    AppError,
> {
    let auth_url = AuthUrl::new(format!("{}/oauth/authorize", instance.base_url))
        .map_err(AppError::internal)?;
    let token_url =
        TokenUrl::new(format!("{}/oauth/token", instance.base_url)).map_err(AppError::internal)?;
    let redirect_url =
        RedirectUrl::new(format!("{public_url}/auth/callback")).map_err(AppError::internal)?;
    Ok(
        BasicClient::new(ClientId::new(instance.oauth_client_id.clone()))
            .set_client_secret(ClientSecret::new(instance.oauth_client_secret.clone()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url),
    )
}

/// Build the GitLab authorize URL plus the state to stash in the
/// encrypted cookie.
pub fn begin_login(
    instance: &InstanceConfig,
    public_url: &str,
) -> Result<(String, PendingLogin), AppError> {
    let client = oauth_client(instance, public_url)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    for scope in SCOPES {
        auth = auth.add_scope(Scope::new(scope.to_string()));
    }
    let (url, csrf) = auth.url();
    Ok((
        url.to_string(),
        PendingLogin {
            instance_id: instance.id,
            csrf: csrf.secret().clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
            started_at: Utc::now(),
        },
    ))
}

fn normalize(token: impl TokenResponse) -> ExchangedTokens {
    ExchangedTokens {
        access_token: token.access_token().secret().clone(),
        refresh_token: token.refresh_token().map(|r| r.secret().clone()),
        expires_at: token
            .expires_in()
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .map(|d| Utc::now() + d),
    }
}

/// Exchange the callback code (PKCE-verified) for tokens.
pub async fn exchange_code(
    http: &reqwest::Client,
    instance: &InstanceConfig,
    public_url: &str,
    code: String,
    pkce_verifier: String,
) -> Result<ExchangedTokens, AppError> {
    let client = oauth_client(instance, public_url)?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(http)
        .await
        .map_err(AppError::gitlab)?;
    Ok(normalize(token))
}

/// Refresh an expired access token.
pub async fn refresh_tokens(
    http: &reqwest::Client,
    instance: &InstanceConfig,
    public_url: &str,
    refresh_token: String,
) -> Result<ExchangedTokens, AppError> {
    let client = oauth_client(instance, public_url)?;
    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token))
        .request_async(http)
        .await
        .map_err(AppError::gitlab)?;
    Ok(normalize(token))
}

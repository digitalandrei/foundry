//! `GET /api/projects` — live permission resolution against every
//! instance the user has an account on; mirror rows updated as cache
//! (docs/GITLAB-INTEGRATION.md § Authorization Resolution).

use axum::extract::State;
use axum::Json;
use foundry_shared::dto::ProjectSummary;

use crate::auth::session::CurrentUser;
use crate::error::AppError;
use crate::gitlab::client::GitlabApi;
use crate::gitlab::tokens;
use crate::repos::{instances, mirror, users};
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<ProjectSummary>>, AppError> {
    let accounts = users::account_tokens(&state.pool, &state.secrets, user.id).await?;
    let mut out = Vec::new();

    for account in accounts {
        let instance =
            match instances::fetch_config(&state.pool, &state.secrets, account.instance_id).await {
                Ok(i) => i,
                Err(_) => continue,
            };
        // One unreachable instance degrades that account, not the
        // whole request (docs/GITLAB-INTEGRATION.md § Failure Modes).
        let result: Result<(), AppError> = async {
            let token = tokens::ensure_fresh(&state, &instance, &account).await?;
            let api = GitlabApi {
                http: &state.http,
                base_url: &instance.base_url,
                access_token: &token,
            };
            for p in api.projects().await? {
                let id = mirror::upsert_project(&state.pool, instance.id, &p).await?;
                out.push(ProjectSummary {
                    id,
                    instance_id: instance.id,
                    gitlab_project_id: p.id,
                    name: p.name,
                    path_with_namespace: p.path_with_namespace,
                    avatar_url: p.avatar_url,
                });
            }
            Ok(())
        }
        .await;
        if let Err(err) = result {
            tracing::warn!(instance = %instance.name, ?err, "project listing degraded");
        }
    }

    out.sort_by(|a, b| a.path_with_namespace.cmp(&b.path_with_namespace));
    Ok(Json(out))
}

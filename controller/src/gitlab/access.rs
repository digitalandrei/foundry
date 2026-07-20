//! Live GitLab authorization helpers. Mirror rows identify resources and
//! cache browse results; a successful upstream read is the access decision.

use foundry_shared::{GitlabInstanceId, UserId};

use super::client::GitlabApi;
use super::tokens;
use crate::error::AppError;
use crate::repos::{instances, users};
use crate::state::AppState;

pub async fn authorize_project(
    state: &AppState,
    user_id: UserId,
    instance_id: GitlabInstanceId,
    gitlab_project_id: i64,
) -> Result<(), AppError> {
    let accounts = users::account_tokens(&state.pool, &state.secrets, user_id).await?;
    let account = accounts
        .into_iter()
        .find(|account| account.instance_id == instance_id)
        .ok_or(AppError::Forbidden)?;
    let instance = instances::fetch_config(&state.pool, &state.secrets, instance_id).await?;
    let token = tokens::ensure_fresh(state, &instance, &account).await?;
    let api = GitlabApi {
        http: &state.http,
        base_url: &instance.base_url,
        access_token: &token,
    };
    api.project(gitlab_project_id)
        .await
        .map(|_| ())
        .map_err(|_| AppError::Forbidden)
}

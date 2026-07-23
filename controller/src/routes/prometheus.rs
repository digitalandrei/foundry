//! `GET /metrics` — Prometheus text exposition (docs/API.md
//! § Observability Endpoints). No auth: nginx keeps the path
//! unreachable from outside; scrapers hit the controller directly on
//! 127.0.0.1:8400 (docs/DEPLOYMENT.md § Observability). Exposes only
//! aggregate counts and sync ages — no names, images, or identifiers.

use std::fmt::Write as _;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;

const CONTENT_TYPE: &str = "text/plain; version=0.0.4";

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let mut body = String::with_capacity(2048);
    let _ = writeln!(
        body,
        "# HELP foundry_build_info Controller build metadata.\n\
         # TYPE foundry_build_info gauge\n\
         foundry_build_info{{version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION"),
    );

    match gather(&state, &mut body).await {
        Ok(()) => {
            body.push_str(
                "# HELP foundry_database_up Whether the controller can query MySQL.\n\
                 # TYPE foundry_database_up gauge\nfoundry_database_up 1\n",
            );
        }
        Err(err) => {
            tracing::warn!(?err, "metrics: database scrape failed");
            body.push_str(
                "# HELP foundry_database_up Whether the controller can query MySQL.\n\
                 # TYPE foundry_database_up gauge\nfoundry_database_up 0\n",
            );
        }
    }
    (StatusCode::OK, [(header::CONTENT_TYPE, CONTENT_TYPE)], body)
}

/// Append every DB-derived metric family; any error leaves the partial
/// families out and flips `foundry_database_up` to 0 in the caller.
async fn gather(state: &AppState, body: &mut String) -> Result<(), sqlx::Error> {
    grouped_gauge(
        state,
        body,
        "foundry_servers",
        "Enrolled GPU servers by liveness status.",
        "status",
        "SELECT status, COUNT(*) FROM servers GROUP BY status",
    )
    .await?;
    grouped_gauge(
        state,
        body,
        "foundry_slots",
        "GPU slots by scheduling state.",
        "state",
        "SELECT state, COUNT(*) FROM gpu_slots GROUP BY state",
    )
    .await?;
    grouped_gauge(
        state,
        body,
        "foundry_deployments",
        "Deployments by lifecycle state.",
        "state",
        "SELECT state, COUNT(*) FROM deployments GROUP BY state",
    )
    .await?;
    grouped_gauge(
        state,
        body,
        "foundry_agent_tasks",
        "Agent tasks by queue state (QUEUED+DISPATCHED = queue depth).",
        "state",
        "SELECT state, COUNT(*) FROM agent_tasks GROUP BY state",
    )
    .await?;

    // GitLab API health proxy: age of the freshest successful mirror
    // sync per instance (the mirror only advances on live API calls).
    let rows: Vec<(String, Option<chrono::NaiveDateTime>)> = sqlx::query_as(
        "SELECT i.name, MAX(p.last_synced_at)
         FROM gitlab_instances i
         LEFT JOIN gitlab_projects p ON p.gitlab_instance_id = i.id
         GROUP BY i.id, i.name",
    )
    .fetch_all(&state.pool)
    .await?;
    body.push_str(
        "# HELP foundry_gitlab_mirror_age_seconds Seconds since the newest successful mirror sync per GitLab instance (-1 = never synced).\n\
         # TYPE foundry_gitlab_mirror_age_seconds gauge\n",
    );
    let now = chrono::Utc::now().naive_utc();
    for (instance, last) in rows {
        let age = last.map_or(-1, |t| (now - t).num_seconds().max(0));
        let _ = writeln!(
            body,
            "foundry_gitlab_mirror_age_seconds{{instance=\"{}\"}} {age}",
            escape_label(&instance),
        );
    }
    Ok(())
}

/// Dynamic SQL (not `query!`): one helper serves four fixed
/// `GROUP BY`-count statements over different tables; every string is a
/// compile-time literal above, never user input.
async fn grouped_gauge(
    state: &AppState,
    body: &mut String,
    name: &str,
    help: &str,
    label: &str,
    sql: &str,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(String, i64)> = sqlx::query_as(sql).fetch_all(&state.pool).await?;
    let _ = writeln!(body, "# HELP {name} {help}\n# TYPE {name} gauge");
    for (value, count) in rows {
        let _ = writeln!(
            body,
            "{name}{{{label}=\"{}\"}} {count}",
            escape_label(&value)
        );
    }
    Ok(())
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::escape_label;

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape_label("plain"), "plain");
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}

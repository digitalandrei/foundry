//! server_metrics: rolling telemetry series (24h retention, JSON
//! payload — the sample shape lives in foundry-shared, not the schema).

use chrono::{Duration, Utc};
use foundry_shared::dto::{MetricsPoint, MetricsSample};
use foundry_shared::ServerId;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::AppError;

const RETENTION_HOURS: i64 = 24;

pub async fn insert(
    pool: &MySqlPool,
    server_id: ServerId,
    sample: &MetricsSample,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO server_metrics (id, server_id, sampled_at, sample) VALUES (?, ?, ?, ?)",
        Uuid::now_v7(),
        server_id.0,
        Utc::now().naive_utc(),
        serde_json::to_string(sample).map_err(AppError::internal)?,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Series for the detail page, oldest → newest.
pub async fn range(
    pool: &MySqlPool,
    server_id: ServerId,
    minutes: i64,
) -> Result<Vec<MetricsPoint>, AppError> {
    let since = (Utc::now() - Duration::minutes(minutes)).naive_utc();
    let rows = sqlx::query!(
        r#"SELECT sampled_at, sample FROM server_metrics
           WHERE server_id = ? AND sampled_at >= ?
           ORDER BY sampled_at"#,
        server_id.0,
        since,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(MetricsPoint {
                sampled_at: r.sampled_at.and_utc(),
                sample: serde_json::from_slice(&r.sample).map_err(AppError::internal)?,
            })
        })
        .collect()
}

/// Hourly retention sweep; spawned at startup.
pub fn spawn_sweeper(pool: MySqlPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let cutoff = (Utc::now() - Duration::hours(RETENTION_HOURS)).naive_utc();
            match sqlx::query!("DELETE FROM server_metrics WHERE sampled_at < ?", cutoff)
                .execute(&pool)
                .await
            {
                Ok(res) if res.rows_affected() > 0 => {
                    tracing::info!(deleted = res.rows_affected(), "metric samples swept");
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(?err, "metrics sweep failed"),
            }
        }
    });
}

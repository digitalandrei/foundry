use foundry_shared::dto::{AppRequestMetrics, AppTrafficBatch, AppTrafficRecord, StatusCount};
use foundry_shared::{DeploymentId, ServerId};
use sqlx::MySqlPool;

use crate::error::AppError;

pub async fn ingest(
    pool: &MySqlPool,
    server_id: ServerId,
    batch: &AppTrafficBatch,
) -> Result<(), AppError> {
    if batch.records.len() > 2_000 {
        return Err(AppError::BadRequest(
            "application traffic batch too large".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    for record in &batch.records {
        if record.method.is_empty()
            || record.method.len() > 16
            || record.path.len() > 2048
            || record
                .request_id
                .as_deref()
                .is_some_and(|request_id| request_id.len() > 64)
            || !(100..=599).contains(&record.status)
        {
            return Err(AppError::BadRequest(
                "application traffic record is invalid".into(),
            ));
        }
        let belongs = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM deployments WHERE id = ? AND server_id = ?",
            record.deployment_id.0,
            server_id.0,
        )
        .fetch_one(&mut *tx)
        .await?;
        if belongs == 0 {
            continue;
        }
        sqlx::query!(
            "INSERT IGNORE INTO app_access_logs
             (deployment_id, occurred_at, method, path, status, request_time_ms, response_bytes, request_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            record.deployment_id.0,
            record.occurred_at.naive_utc(),
            record.method,
            record.path,
            record.status,
            record.request_time_ms,
            record.response_bytes,
            record.request_id,
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "DELETE FROM app_access_logs WHERE occurred_at < UTC_TIMESTAMP() - INTERVAL 7 DAY"
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn recent(
    pool: &MySqlPool,
    deployment_id: DeploymentId,
) -> Result<Vec<AppTrafficRecord>, AppError> {
    let rows = sqlx::query!(
        "SELECT occurred_at, method, path, status, request_time_ms, response_bytes, request_id
         FROM app_access_logs WHERE deployment_id = ? ORDER BY occurred_at DESC LIMIT 500",
        deployment_id.0,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AppTrafficRecord {
            deployment_id,
            occurred_at: row.occurred_at.and_utc(),
            method: row.method,
            path: row.path,
            status: row.status,
            request_time_ms: row.request_time_ms,
            response_bytes: row.response_bytes,
            request_id: row.request_id,
        })
        .collect())
}

pub async fn metrics(
    pool: &MySqlPool,
    deployment_id: DeploymentId,
) -> Result<AppRequestMetrics, AppError> {
    let rows = sqlx::query!(
        "SELECT status, request_time_ms, response_bytes FROM app_access_logs
         WHERE deployment_id = ? AND occurred_at >= UTC_TIMESTAMP() - INTERVAL 24 HOUR",
        deployment_id.0,
    )
    .fetch_all(pool)
    .await?;
    let mut times: Vec<u32> = rows.iter().map(|row| row.request_time_ms).collect();
    times.sort_unstable();
    let requests = rows.len() as u64;
    let average_request_time_ms = times
        .iter()
        .map(|time| u64::from(*time))
        .sum::<u64>()
        .checked_div(requests)
        .unwrap_or_default();
    let p95_request_time_ms = percentile(&times, 95) as u64;
    let mut counts = std::collections::BTreeMap::new();
    for row in &rows {
        *counts.entry(row.status).or_insert(0u64) += 1;
    }
    Ok(AppRequestMetrics {
        requests,
        errors: rows.iter().filter(|row| row.status >= 500).count() as u64,
        response_bytes: rows.iter().map(|row| row.response_bytes).sum(),
        average_request_time_ms,
        p95_request_time_ms,
        by_status: counts
            .into_iter()
            .map(|(status, count)| StatusCount { status, count })
            .collect(),
    })
}

pub async fn delete_for(
    tx: &mut sqlx::MySqlConnection,
    deployment_id: DeploymentId,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM app_access_logs WHERE deployment_id = ?",
        deployment_id.0,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

fn percentile(sorted: &[u32], percentage: usize) -> u32 {
    let index = ((sorted.len() * percentage).div_ceil(100)).saturating_sub(1);
    sorted.get(index).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(percentile(&[], 95), 0);
        assert_eq!(percentile(&[7], 95), 7);
        assert_eq!(percentile(&(1..=100).collect::<Vec<_>>(), 95), 95);
    }
}

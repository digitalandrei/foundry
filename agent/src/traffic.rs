//! Incremental reader for Foundry's per-application nginx JSON logs.

use std::collections::HashMap;
use std::io::{BufRead, Seek};
use std::path::PathBuf;

use foundry_shared::dto::{AppTrafficBatch, AppTrafficRecord};

#[derive(Default)]
pub struct TrafficCollector {
    offsets: HashMap<PathBuf, u64>,
    pending: Option<(AppTrafficBatch, HashMap<PathBuf, u64>)>,
}

impl TrafficCollector {
    /// Stage a batch without advancing durable in-memory cursors. Until the
    /// controller accepts it, later ticks return the exact same records so a
    /// transient outage cannot silently discard access data.
    pub async fn collect(&mut self) -> AppTrafficBatch {
        if let Some((batch, _)) = &self.pending {
            return batch.clone();
        }
        let offsets = self.offsets.clone();
        let (records, next_offsets) = tokio::task::spawn_blocking(move || collect_sync(offsets))
            .await
            .unwrap_or_default();
        let batch = AppTrafficBatch { records };
        if !batch.records.is_empty() {
            self.pending = Some((batch.clone(), next_offsets));
        } else {
            self.offsets = next_offsets;
        }
        batch
    }

    pub fn commit(&mut self) {
        if let Some((_, offsets)) = self.pending.take() {
            self.offsets = offsets;
        }
    }
}

fn collect_sync(
    mut offsets: HashMap<PathBuf, u64>,
) -> (Vec<AppTrafficRecord>, HashMap<PathBuf, u64>) {
    let mut records = Vec::new();
    let Ok(entries) = std::fs::read_dir(crate::vhost::ACCESS_LOG_DIR) else {
        return (records, offsets);
    };
    for entry in entries.flatten().take(2_000) {
        if records.len() >= 2_000 {
            break;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".access.log") else {
            continue;
        };
        let Ok(deployment_id) = id.parse::<uuid::Uuid>() else {
            continue;
        };
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        let len = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let offset = offsets.get(&path).copied().unwrap_or(0).min(len);
        if file.seek(std::io::SeekFrom::Start(offset)).is_err() {
            continue;
        }
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        while records.len() < 2_000 && reader.read_line(&mut line).unwrap_or(0) > 0 {
            if let Some(record) = parse_line(deployment_id.into(), line.trim()) {
                records.push(record);
            }
            line.clear();
        }
        if let Ok(position) = reader.stream_position() {
            offsets.insert(path, position);
        }
    }
    (records, offsets)
}

fn parse_line(deployment_id: foundry_shared::DeploymentId, line: &str) -> Option<AppTrafficRecord> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let occurred_at = chrono::DateTime::parse_from_rfc3339(value.get("ts")?.as_str()?)
        .ok()?
        .with_timezone(&chrono::Utc);
    Some(AppTrafficRecord {
        deployment_id,
        occurred_at,
        method: value.get("method")?.as_str()?.chars().take(16).collect(),
        path: value.get("path")?.as_str()?.chars().take(2048).collect(),
        status: value.get("status")?.as_u64()?.try_into().ok()?,
        request_time_ms: (value.get("request_time")?.as_f64()? * 1000.0).max(0.0) as u32,
        response_bytes: value.get("bytes")?.as_u64()?,
        request_id: value
            .get("request_id")
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
            .map(|id| id.chars().take(64).collect()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nginx_json_without_query_or_headers() {
        let deployment_id = foundry_shared::DeploymentId::new();
        let record = parse_line(
            deployment_id,
            r#"{"ts":"2026-07-21T10:00:00+03:00","request_id":"abc","method":"POST","path":"/upload","status":201,"request_time":1.25,"bytes":42}"#,
        )
        .unwrap();
        assert_eq!(record.deployment_id, deployment_id);
        assert_eq!(record.path, "/upload");
        assert_eq!(record.request_time_ms, 1_250);
        assert_eq!(record.response_bytes, 42);
    }

    #[tokio::test]
    async fn pending_batch_repeats_until_ack_commit() {
        let deployment_id = foundry_shared::DeploymentId::new();
        let record = AppTrafficRecord {
            deployment_id,
            occurred_at: chrono::Utc::now(),
            method: "GET".into(),
            path: "/".into(),
            status: 200,
            request_time_ms: 1,
            response_bytes: 2,
            request_id: Some("request".into()),
        };
        let next_offsets = HashMap::from([(PathBuf::from("access.log"), 123)]);
        let mut collector = TrafficCollector {
            offsets: HashMap::new(),
            pending: Some((
                AppTrafficBatch {
                    records: vec![record],
                },
                next_offsets.clone(),
            )),
        };

        assert_eq!(collector.collect().await.records.len(), 1);
        assert!(collector.offsets.is_empty());
        collector.commit();
        assert_eq!(collector.offsets, next_offsets);
    }
}

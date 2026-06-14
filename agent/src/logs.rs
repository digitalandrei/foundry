//! Container log capture (Phase 7, docs/ARCHITECTURE.md § Agent Tasks).
//! Periodically ships *new* stdout+stderr for each managed running
//! container to `POST /agent/logs`. Only `foundry.managed=true`
//! containers are read — foreign containers are detected for slot
//! visibility but never have their logs collected.
//!
//! Incremental, not full-tail: a per-deployment cursor (the newest log
//! timestamp already shipped) drives a `docker logs --since` fetch each
//! round, and lines at or before the cursor are dropped, so a chunk
//! carries only genuinely new output. The controller bounds storage; the
//! agent bounds each upload.

use std::collections::{HashMap, HashSet};

use bollard::container::LogOutput;
use bollard::query_parameters::{ListContainersOptions, LogsOptionsBuilder};
use bollard::Docker;
use chrono::{DateTime, Utc};
use foundry_shared::dto::DeploymentLogChunk;
use foundry_shared::DeploymentId;
use futures_util::StreamExt;

/// First sight of a container: trailing backlog to capture before
/// switching to incremental `since` fetches.
const INITIAL_TAIL: &str = "500";
/// Bound a single chunk so one chatty container can't balloon an upload
/// (the controller re-clamps; this keeps the wire payload small).
const MAX_CHUNK_BYTES: usize = 32 * 1024;

#[derive(Default)]
pub struct LogCollector {
    /// deployment_id (label) → newest log timestamp already shipped.
    cursors: HashMap<String, DateTime<Utc>>,
}

impl LogCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// One capture round: a chunk per managed running container that
    /// produced new output. Best-effort — a docker hiccup yields an
    /// empty batch, never an error (logs must never disrupt the agent).
    pub async fn collect(&mut self) -> Vec<DeploymentLogChunk> {
        let Ok(docker) = Docker::connect_with_local_defaults() else {
            return Vec::new();
        };
        let Ok(list) = docker
            .list_containers(Some(ListContainersOptions::default())) // running only
            .await
        else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for c in list {
            let labels = c.labels.unwrap_or_default();
            if labels.get("foundry.managed").map(String::as_str) != Some("true") {
                continue;
            }
            let Some(dep_label) = labels.get("foundry.deployment_id").cloned() else {
                continue;
            };
            let Some(container_id) = c.id else { continue };
            let Some(deployment_id) = uuid::Uuid::parse_str(&dep_label)
                .ok()
                .map(DeploymentId::from)
            else {
                continue;
            };
            seen.insert(dep_label.clone());

            if let Some(chunk) = self
                .capture(&docker, &container_id, &dep_label, deployment_id)
                .await
            {
                out.push(chunk);
            }
        }
        // Forget cursors for containers no longer present — bounds memory;
        // removed deployments never reappear (restart re-deploys fresh).
        self.cursors.retain(|k, _| seen.contains(k));
        out
    }

    async fn capture(
        &mut self,
        docker: &Docker,
        container_id: &str,
        dep_label: &str,
        deployment_id: DeploymentId,
    ) -> Option<DeploymentLogChunk> {
        let cursor = self.cursors.get(dep_label).copied();
        // `since` is whole-second (i32); the precise cursor filter below
        // drops anything re-streamed from the cursor's own second.
        let mut opts = LogsOptionsBuilder::new()
            .stdout(true)
            .stderr(true)
            .timestamps(true)
            .follow(false);
        opts = match cursor {
            Some(at) => opts.since(at.timestamp() as i32),
            None => opts.tail(INITIAL_TAIL),
        };

        let mut stream = docker.logs(container_id, Some(opts.build()));
        let mut lines: Vec<(DateTime<Utc>, String)> = Vec::new();
        while let Some(frame) = stream.next().await {
            let Ok(frame) = frame else { break };
            let bytes = match frame {
                LogOutput::StdOut { message }
                | LogOutput::StdErr { message }
                | LogOutput::Console { message } => message,
                LogOutput::StdIn { .. } => continue,
            };
            for line in String::from_utf8_lossy(&bytes).split_inclusive('\n') {
                // `--timestamps` prefixes every line with "<rfc3339> ".
                let Some((ts_str, _)) = line.split_once(' ') else {
                    continue;
                };
                let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) else {
                    continue;
                };
                let ts = ts.with_timezone(&Utc);
                if cursor.is_some_and(|cur| ts <= cur) {
                    continue; // already shipped (same-second re-stream)
                }
                lines.push((ts, line.to_string()));
            }
        }
        if lines.is_empty() {
            return None;
        }
        // stdout and stderr arrive on separate frames — merge by time.
        lines.sort_by_key(|l| l.0);

        // Keep the newest lines within the byte budget.
        let mut total: usize = lines.iter().map(|(_, l)| l.len()).sum();
        let mut start = 0;
        while total > MAX_CHUNK_BYTES && start + 1 < lines.len() {
            total -= lines[start].1.len();
            start += 1;
        }
        let lines = &lines[start..];

        let through = lines.last()?.0;
        self.cursors.insert(dep_label.to_string(), through);
        let content: String = lines.iter().map(|(_, l)| l.as_str()).collect();

        Some(DeploymentLogChunk {
            deployment_id,
            container_id: container_id.chars().take(12).collect(),
            through,
            content,
        })
    }
}

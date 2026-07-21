//! Operational readiness and per-application HTTP traffic DTOs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::DeploymentId;

pub const REQUIRED_SETUP_REVISION: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Ready,
    Warning,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCheck {
    pub code: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostReadiness {
    pub setup_revision: Option<u32>,
    pub required_setup_revision: u32,
    pub checked_at: DateTime<Utc>,
    pub checks: Vec<ReadinessCheck>,
}

impl HostReadiness {
    pub fn ready(&self) -> bool {
        self.setup_revision == Some(self.required_setup_revision)
            && self
                .checks
                .iter()
                .all(|check| matches!(check.status, CheckStatus::Ready | CheckStatus::Warning))
    }

    pub fn checks_ready(&self, codes: &[&str]) -> bool {
        codes.iter().all(|code| {
            self.checks.iter().any(|check| {
                check.code == *code
                    && matches!(check.status, CheckStatus::Ready | CheckStatus::Warning)
            })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
    #[serde(default)]
    pub volumes: Vec<VolumeUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeUsage {
    pub volume_id: crate::ServerVolumeId,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTrafficRecord {
    pub deployment_id: DeploymentId,
    pub occurred_at: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub request_time_ms: u32,
    pub response_bytes: u64,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTrafficBatch {
    pub records: Vec<AppTrafficRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRequestMetrics {
    pub requests: u64,
    pub errors: u64,
    pub response_bytes: u64,
    pub average_request_time_ms: u64,
    pub p95_request_time_ms: u64,
    pub by_status: Vec<StatusCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusCount {
    pub status: u16,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readiness(status: CheckStatus) -> HostReadiness {
        HostReadiness {
            setup_revision: Some(REQUIRED_SETUP_REVISION),
            required_setup_revision: REQUIRED_SETUP_REVISION,
            checked_at: Utc::now(),
            checks: vec![ReadinessCheck {
                code: "docker".into(),
                status,
                detail: "test".into(),
            }],
        }
    }

    #[test]
    fn unknown_is_not_positive_readiness_evidence() {
        assert!(readiness(CheckStatus::Ready).ready());
        assert!(readiness(CheckStatus::Warning).ready());
        assert!(!readiness(CheckStatus::Unknown).ready());
        assert!(!readiness(CheckStatus::Failed).ready());
    }

    #[test]
    fn named_check_sets_require_every_check() {
        let report = readiness(CheckStatus::Ready);
        assert!(report.checks_ready(&["docker"]));
        assert!(!report.checks_ready(&["docker", "storage_write"]));
    }
}

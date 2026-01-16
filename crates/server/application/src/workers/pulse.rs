//! Worker Pulse Service
//!
//! Responsible ONLY for heartbeat monitoring and liveness checks.

use hodei_server_domain::shared_kernel::{Result, WorkerId, WorkerState};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct PulseFilter {
    state: Option<WorkerState>,
    stale_threshold: Option<Duration>,
}

impl PulseFilter {
    pub fn new() -> Self {
        Self {
            state: None,
            stale_threshold: None,
        }
    }

    pub fn with_state(mut self, state: WorkerState) -> Self {
        self.state = Some(state);
        self
    }
}

impl Default for PulseFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct HeartbeatResult {
    pub worker_id: WorkerId,
    pub recorded: bool,
}

#[derive(Debug, Clone)]
pub struct StaleDetectionResult {
    pub stale_workers: Vec<WorkerId>,
    pub check_duration_ms: u64,
}

#[async_trait::async_trait]
pub trait WorkerRegistryPulsePort: Send + Sync {
    async fn record_heartbeat(&self, worker_id: &WorkerId) -> Result<()>;
    async fn find_unhealthy(
        &self,
        timeout: Duration,
    ) -> Result<Vec<hodei_server_domain::workers::Worker>>;
    async fn get_worker(
        &self,
        worker_id: &WorkerId,
    ) -> Result<Option<hodei_server_domain::workers::Worker>>;
    async fn count_workers(&self) -> Result<usize>;
}

pub struct WorkerPulseService {
    registry_port: Arc<dyn WorkerRegistryPulsePort>,
    heartbeat_timeout: Duration,
}

impl WorkerPulseService {
    pub fn new(registry_port: Arc<dyn WorkerRegistryPulsePort>) -> Self {
        Self {
            registry_port,
            heartbeat_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    pub async fn record_heartbeat(&self, worker_id: &WorkerId) -> Result<HeartbeatResult> {
        debug!(%worker_id, "Recording heartbeat");
        self.registry_port.record_heartbeat(worker_id).await?;
        Ok(HeartbeatResult {
            worker_id: worker_id.clone(),
            recorded: true,
        })
    }

    pub async fn detect_stale_workers(
        &self,
        threshold: Option<Duration>,
    ) -> Result<StaleDetectionResult> {
        let timeout = threshold.unwrap_or(self.heartbeat_timeout);
        let unhealthy = self.registry_port.find_unhealthy(timeout).await?;
        let stale_workers: Vec<WorkerId> = unhealthy.into_iter().map(|w| w.id().clone()).collect();

        Ok(StaleDetectionResult {
            stale_workers,
            check_duration_ms: 0,
        })
    }

    pub async fn check_liveness(&self, worker_id: &WorkerId) -> Result<bool> {
        let worker = self.registry_port.get_worker(worker_id).await?;
        match worker {
            Some(w) => {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(w.last_heartbeat())
                    .to_std()
                    .unwrap_or_default();
                Ok(elapsed <= self.heartbeat_timeout)
            }
            None => Ok(false),
        }
    }
}

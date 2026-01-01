//! Worker Reconciliation Service
//!
//! Responsible ONLY for reconciling worker states and detecting stale workers.
//! No infrastructure calls, no provisioning, no cleanup.

use chrono::{DateTime, Utc};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::shared_kernel::{JobId, Result, WorkerId, WorkerState};
use hodei_server_domain::workers::Worker;
use hodei_server_domain::workers::WorkerRegistry;
use hodei_server_domain::workers::health::WorkerHealthService;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Result of a reconciliation run
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    pub stale_workers: Vec<WorkerId>,
    pub affected_jobs: Vec<JobId>,
    pub jobs_requiring_reassignment: Vec<WorkerId>,
    pub workers_marked_unhealthy: Vec<WorkerId>,
    pub check_duration_ms: u64,
}

/// Configuration for reconciliation
#[derive(Debug, Clone)]
pub struct ReconciliationConfig {
    /// Heartbeat timeout for detecting stale workers
    pub heartbeat_timeout: Duration,
    /// Grace period before marking job for reassignment
    pub worker_dead_grace_period: Duration,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(60),
            worker_dead_grace_period: Duration::from_secs(120),
        }
    }
}

/// Ports for worker registry operations (ISP)
#[async_trait::async_trait]
pub trait WorkerRegistryReconciliationPort: Send + Sync {
    async fn find_unhealthy(&self, timeout: Duration) -> Result<Vec<Worker>>;
    async fn update_state(&self, worker_id: &WorkerId, new_state: WorkerState) -> Result<()>;
    async fn count(&self) -> Result<usize>;
}

/// Ports for event publishing (ISP)
#[async_trait::async_trait]
pub trait ReconciliationEventPort: Send + Sync {
    async fn emit_worker_heartbeat_missed(
        &self,
        worker_id: &WorkerId,
        job_id: &JobId,
        last_heartbeat: DateTime<Utc>,
    ) -> Result<()>;
    async fn emit_job_reassignment_required(
        &self,
        worker_id: &WorkerId,
        job_id: &JobId,
    ) -> Result<()>;
    async fn emit_worker_status_changed(
        &self,
        worker_id: &WorkerId,
        old_state: WorkerState,
        new_state: WorkerState,
        reason: &str,
    ) -> Result<()>;
}

/// Worker Reconciliation Service
///
/// Responsibilities:
/// - Detect workers with stale heartbeats
/// - Handle job reassignment for workers that exceed grace period
/// - Emit domain events for stale workers
pub struct WorkerReconciliationService {
    registry_port: Arc<dyn WorkerRegistryReconciliationPort>,
    event_port: Arc<dyn ReconciliationEventPort>,
    health_service: Arc<WorkerHealthService>,
    config: ReconciliationConfig,
}

impl WorkerReconciliationService {
    pub fn new(
        registry_port: Arc<dyn WorkerRegistryReconciliationPort>,
        event_port: Arc<dyn ReconciliationEventPort>,
        health_service: Arc<WorkerHealthService>,
    ) -> Self {
        Self {
            registry_port,
            event_port,
            health_service,
            config: ReconciliationConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ReconciliationConfig) -> Self {
        self.config = config;
        self
    }

    /// Run reconciliation: detect stale workers and emit events
    pub async fn run_reconciliation(&self) -> Result<ReconciliationResult> {
        let start = Utc::now();
        let mut result = ReconciliationResult::default();

        // Find workers with stale heartbeats
        let stale_workers = self
            .registry_port
            .find_unhealthy(self.config.heartbeat_timeout)
            .await?;

        for worker in stale_workers {
            let worker_id = worker.id().clone();
            let heartbeat_age = self.health_service.calculate_heartbeat_age(&worker);
            let stale_seconds = heartbeat_age.as_duration().as_secs();

            info!(
                "🔍 Reconciliation: Worker {} has stale heartbeat ({}s ago)",
                worker_id, stale_seconds
            );

            result.stale_workers.push(worker_id.clone());

            // Check if worker has an assigned job
            if let Some(job_id) = worker.current_job_id() {
                result.affected_jobs.push(job_id.clone());

                // Emit events for job reassignment
                let last_heartbeat = worker.updated_at();
                if let Err(e) = self
                    .event_port
                    .emit_worker_heartbeat_missed(&worker_id, &job_id, last_heartbeat)
                    .await
                {
                    warn!(
                        "Failed to emit heartbeat missed event for worker {}: {:?}",
                        worker_id, e
                    );
                }

                // If beyond grace period, mark for job reassignment
                if stale_seconds > self.config.worker_dead_grace_period.as_secs() {
                    info!(
                        "⚠️ Worker {} exceeded grace period, triggering job reassignment for {}",
                        worker_id, job_id
                    );

                    if let Err(e) = self
                        .event_port
                        .emit_job_reassignment_required(&worker_id, &job_id)
                        .await
                    {
                        warn!(
                            "Failed to emit job reassignment event for job {}: {:?}",
                            job_id, e
                        );
                    }

                    result.jobs_requiring_reassignment.push(worker_id.clone());
                }
            }

            // Update worker state to reflect unhealthy status
            if *worker.state() != WorkerState::Terminated {
                if let Err(e) = self
                    .registry_port
                    .update_state(&worker_id, WorkerState::Terminating)
                    .await
                {
                    error!(
                        "Failed to update worker {} to Terminating state: {}",
                        worker_id, e
                    );
                } else {
                    result.workers_marked_unhealthy.push(worker_id.clone());

                    // Emit WorkerStatusChanged event
                    if let Err(e) = self
                        .event_port
                        .emit_worker_status_changed(
                            &worker_id,
                            worker.state().clone(),
                            WorkerState::Terminating,
                            "heartbeat_timeout",
                        )
                        .await
                    {
                        warn!(
                            "Failed to emit status changed event for worker {}: {:?}",
                            worker_id, e
                        );
                    }
                }
            }
        }

        let duration = Utc::now().signed_duration_since(start);
        result.check_duration_ms = duration.num_milliseconds() as u64;

        if !result.stale_workers.is_empty() {
            info!(
                "📊 Reconciliation complete: {} stale workers, {} affected jobs, {} requiring reassignment ({}ms)",
                result.stale_workers.len(),
                result.affected_jobs.len(),
                result.jobs_requiring_reassignment.len(),
                result.check_duration_ms
            );
        }

        Ok(result)
    }

    /// Get count of live workers
    pub async fn get_live_worker_count(&self) -> Result<usize> {
        self.registry_port.count().await
    }
}

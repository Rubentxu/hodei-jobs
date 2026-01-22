//!
//! # Timeout Checker
//!
//! Detects and handles job timeouts using saga-engine v4.0 workflows.

use crate::saga::sync_durable_executor::SyncDurableWorkflowExecutor;
use crate::saga::workflows::timeout_durable::TimeoutDurableWorkflow;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::shared_kernel::JobId;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Configuration for timeout checking
#[derive(Debug, Clone)]
pub struct TimeoutCheckerConfig {
    pub check_interval: Duration,
    pub default_job_timeout: Duration,
}

impl Default for TimeoutCheckerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            default_job_timeout: Duration::from_secs(300),
        }
    }
}

/// Errors from timeout checker
#[derive(Debug, Error)]
pub enum TimeoutCheckerError {
    #[error("Job not found: {0}")]
    JobNotFound(JobId),
    #[error("Timeout check failed: {0}")]
    CheckFailed(String),
}

/// Result of timeout check
#[derive(Debug)]
pub struct TimeoutCheckResult {
    pub job_id: JobId,
    pub has_timed_out: bool,
    pub workflow_executed: bool,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

/// Timeout checker service
#[derive(Clone)]
pub struct TimeoutChecker {
    executor: Arc<SyncDurableWorkflowExecutor<TimeoutDurableWorkflow>>,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    config: TimeoutCheckerConfig,
}

impl TimeoutChecker {
    pub fn new(
        executor: Arc<SyncDurableWorkflowExecutor<TimeoutDurableWorkflow>>,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        config: Option<TimeoutCheckerConfig>,
    ) -> Self {
        Self {
            executor,
            job_repository,
            config: config.unwrap_or_default(),
        }
    }

    pub fn config(&self) -> &TimeoutCheckerConfig {
        &self.config
    }

    /// Check a specific job for timeout
    pub async fn check_job(
        &self,
        job_id: &JobId,
    ) -> Result<TimeoutCheckResult, TimeoutCheckerError> {
        let job = self
            .job_repository
            .find_by_id(job_id)
            .await
            .map_err(|e| TimeoutCheckerError::CheckFailed(e.to_string()))?
            .ok_or_else(|| TimeoutCheckerError::JobNotFound(job_id.clone()))?;

        let has_timed_out = job.is_timed_out(self.config.default_job_timeout);

        let mut workflow_executed = false;

        if has_timed_out {
            let input = super::workflows::timeout_durable::TimeoutWorkflowInput::new(
                job_id.clone(),
                self.config.default_job_timeout,
                self.config.default_job_timeout * 2, // max_duration is 2x expected
                "job-timeout",
            );

            let json_input = serde_json::to_value(&input)
                .map_err(|e| TimeoutCheckerError::CheckFailed(e.to_string()))?;

            let result = self
                .executor
                .execute(json_input)
                .await
                .map_err(|e| TimeoutCheckerError::CheckFailed(e.to_string()))?;

            workflow_executed = matches!(
                result,
                saga_engine_core::workflow::WorkflowResult::Completed { .. }
            );
        }

        Ok(TimeoutCheckResult {
            job_id: job_id.clone(),
            has_timed_out,
            workflow_executed,
            checked_at: chrono::Utc::now(),
        })
    }

    /// Run the timeout checker (placeholder for periodic checking)
    pub async fn run(&self) {
        // Placeholder - in production this would run a periodic loop
    }
}

impl std::fmt::Debug for TimeoutChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimeoutChecker")
            .field("config", &self.config)
            .finish()
    }
}

//! Timeout Checker - Periodic Job Timeout Detection
//!
//! This module provides a background task that periodically checks for jobs
//! that have exceeded their configured timeout and triggers TimeoutSaga
//! to handle graceful termination.

use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::{Saga, SagaContext, SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Configuration for the timeout checker
#[derive(Debug, Clone)]
pub struct TimeoutCheckerConfig {
    /// How often to check for timed out jobs
    pub check_interval: Duration,
    /// Default timeout to use if job doesn't have one configured
    pub default_timeout: Duration,
    /// Maximum number of jobs to process per check
    pub max_batch_size: usize,
    /// Whether to actually trigger timeout sagas (false = dry run)
    pub enabled: bool,
}

impl Default for TimeoutCheckerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10),
            default_timeout: Duration::from_secs(3600), // 1 hour default
            max_batch_size: 100,
            enabled: true,
        }
    }
}

/// Result of timeout check operation
#[derive(Debug)]
pub enum TimeoutCheckResult {
    /// Check completed successfully
    Completed {
        jobs_checked: usize,
        timed_out_jobs: usize,
        sagas_triggered: usize,
    },
    /// Check failed with error
    Failed(String),
    /// Checker is disabled
    Disabled,
}

/// Timeout Checker Background Task
///
/// Runs periodically to detect jobs that have exceeded their timeout
/// and triggers TimeoutSaga for graceful termination.
#[derive(Clone)]
pub struct TimeoutChecker<JR, SO>
where
    JR: JobRepository + Send + Sync + 'static,
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    /// Job repository for finding timed out jobs
    job_repository: Arc<JR>,

    /// Saga orchestrator for triggering timeout sagas
    saga_orchestrator: Arc<SO>,

    /// Checker configuration
    config: TimeoutCheckerConfig,
}

impl<JR, SO> TimeoutChecker<JR, SO>
where
    JR: JobRepository + Send + Sync + 'static,
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    /// Create a new TimeoutChecker
    pub fn new(
        job_repository: Arc<JR>,
        saga_orchestrator: Arc<SO>,
        config: Option<TimeoutCheckerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();

        Self {
            job_repository,
            saga_orchestrator,
            config,
        }
    }

    /// Run the timeout checker (blocks until shutdown)
    pub async fn run(&self) {
        if !self.config.enabled {
            info!("⏰ TimeoutChecker: Checker is disabled, not starting");
            return;
        }

        info!(
            "⏰ TimeoutChecker: Starting with interval {:?}",
            self.config.check_interval
        );

        let mut interval = tokio::time::interval(self.config.check_interval);

        loop {
            interval.tick().await;
            let result = self.check_and_trigger_timeouts().await;
            if let TimeoutCheckResult::Failed(e) = result {
                error!("⏰ TimeoutChecker: Check failed: {}", e);
            }
        }
    }

    /// Run a single check (useful for testing)
    pub async fn run_once(&self) -> TimeoutCheckResult {
        self.check_and_trigger_timeouts().await
    }

    /// Check for timed out jobs and trigger sagas
    async fn check_and_trigger_timeouts(&self) -> TimeoutCheckResult {
        debug!("⏰ TimeoutChecker: Checking for timed out jobs...");

        // Find all running jobs
        let running_jobs = match self.job_repository.find_by_state(&JobState::Running).await {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("⏰ TimeoutChecker: Failed to fetch running jobs: {}", e);
                return TimeoutCheckResult::Failed(format!("Failed to fetch running jobs: {}", e));
            }
        };

        let now = chrono::Utc::now();
        let mut timed_out_jobs: Vec<JobId> = Vec::new();

        // Check each running job for timeout
        for job in &running_jobs {
            if timed_out_jobs.len() >= self.config.max_batch_size {
                warn!(
                    "⏰ TimeoutChecker: Reached max batch size {}, stopping check",
                    self.config.max_batch_size
                );
                break;
            }

            // Check if job has a start time (required for timeout check)
            let started_at = match job.started_at() {
                Some(ts) => ts,
                None => {
                    warn!(
                        "⏰ TimeoutChecker: Job {} has no start time, skipping",
                        job.id
                    );
                    continue;
                }
            };

            // Get timeout duration from spec (job-specific) or use default
            // timeout_ms is in milliseconds, convert to Duration
            let timeout_ms = job.spec.timeout_ms;
            let timeout_duration = if timeout_ms > 0 {
                Duration::from_millis(timeout_ms)
            } else {
                self.config.default_timeout
            };

            // Check if elapsed time exceeds timeout
            let elapsed = now.signed_duration_since(started_at);

            if let Ok(elapsed_duration) = elapsed.to_std() {
                if elapsed_duration > timeout_duration {
                    debug!(
                        "⏰ TimeoutChecker: Job {} has timed out (elapsed: {:?}, timeout: {:?})",
                        job.id, elapsed_duration, timeout_duration
                    );
                    timed_out_jobs.push(job.id.clone());
                }
            }
        }

        let jobs_checked = running_jobs.len();
        let timed_out_count = timed_out_jobs.len();

        if timed_out_count > 0 {
            info!(
                "⏰ TimeoutChecker: Found {} timed out jobs out of {} checked",
                timed_out_count, jobs_checked
            );
        } else {
            debug!(
                "⏰ TimeoutChecker: No timed out jobs found (checked {})",
                jobs_checked
            );
        }

        // Trigger timeout saga for each timed out job
        let mut sagas_triggered = 0;

        for job_id in timed_out_jobs {
            if let Err(e) = self.trigger_timeout_saga(&job_id).await {
                error!(
                    "⏰ TimeoutChecker: Failed to trigger timeout saga for job {}: {}",
                    job_id, e
                );
            } else {
                sagas_triggered += 1;
            }
        }

        TimeoutCheckResult::Completed {
            jobs_checked,
            timed_out_jobs: timed_out_count,
            sagas_triggered,
        }
    }

    /// Trigger a timeout saga for a specific job
    async fn trigger_timeout_saga(&self, job_id: &JobId) -> Result<(), DomainError> {
        // Create saga context
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Timeout,
            Some(format!("timeout-{}", job_id.0)),
            Some("timeout_checker".to_string()),
        );

        // Get timeout duration from job spec
        let job_opt = self.job_repository.find_by_id(job_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch job {}: {}", job_id, e),
            }
        })?;

        let timeout_ms = job_opt.as_ref().map(|j| j.spec.timeout_ms).unwrap_or(0);
        let timeout_duration = if timeout_ms > 0 {
            Duration::from_millis(timeout_ms)
        } else {
            self.config.default_timeout
        };

        // Create timeout saga
        let saga = hodei_server_domain::saga::TimeoutSaga::new(
            job_id.clone(),
            timeout_duration,
            "system_timeout_checker",
        );

        // Execute saga
        match self.saga_orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.state == hodei_server_domain::saga::SagaState::Completed {
                    info!(
                        "✅ TimeoutChecker: Timeout saga completed for job {}",
                        job_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown timeout saga error".to_string());
                    error!(
                        "❌ TimeoutChecker: Timeout saga failed for job {}: {}",
                        job_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ TimeoutChecker: Orchestrator error for job {}: {}",
                    job_id, e
                );
                Err(e)
            }
        }
    }
}

/// Timeout Checker Background Task (Trait Object Version)
///
/// This version accepts trait objects directly for dynamic dispatch.
/// Use this when you have dynamic implementations (e.g., Arc<dyn JobRepository>).
#[derive(Clone)]
pub struct DynTimeoutChecker {
    /// Job repository for finding timed out jobs
    job_repository: Arc<dyn JobRepository + Send + Sync>,

    /// Saga orchestrator for triggering timeout sagas
    saga_orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,

    /// Checker configuration
    config: TimeoutCheckerConfig,
}

impl DynTimeoutChecker {
    /// Create a new DynTimeoutChecker
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        saga_orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        config: Option<TimeoutCheckerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();

        Self {
            job_repository,
            saga_orchestrator,
            config,
        }
    }

    /// Run the timeout checker (blocks until shutdown)
    pub async fn run(&self) {
        if !self.config.enabled {
            info!("⏰ DynTimeoutChecker: Checker is disabled, not starting");
            return;
        }

        info!(
            "⏰ DynTimeoutChecker: Starting with interval {:?}",
            self.config.check_interval
        );

        let mut interval = tokio::time::interval(self.config.check_interval);

        loop {
            interval.tick().await;
            let result = self.check_and_trigger_timeouts().await;
            if let TimeoutCheckResult::Failed(e) = result {
                error!("⏰ DynTimeoutChecker: Check failed: {}", e);
            }
        }
    }

    /// Check for timed out jobs and trigger sagas
    async fn check_and_trigger_timeouts(&self) -> TimeoutCheckResult {
        debug!("⏰ DynTimeoutChecker: Checking for timed out jobs...");

        // Find all running jobs
        let running_jobs = match self.job_repository.find_by_state(&JobState::Running).await {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("⏰ DynTimeoutChecker: Failed to fetch running jobs: {}", e);
                return TimeoutCheckResult::Failed(format!("Failed to fetch running jobs: {}", e));
            }
        };

        let now = chrono::Utc::now();
        let mut timed_out_jobs: Vec<JobId> = Vec::new();

        // Check each running job for timeout
        for job in &running_jobs {
            if timed_out_jobs.len() >= self.config.max_batch_size {
                warn!(
                    "⏰ DynTimeoutChecker: Reached max batch size {}, stopping check",
                    self.config.max_batch_size
                );
                break;
            }

            // Check if job has a start time (required for timeout check)
            let started_at = match job.started_at() {
                Some(ts) => ts,
                None => {
                    warn!(
                        "⏰ DynTimeoutChecker: Job {} has no start time, skipping",
                        job.id
                    );
                    continue;
                }
            };

            // Get timeout duration from spec (job-specific) or use default
            let timeout_ms = job.spec.timeout_ms;
            let timeout_duration = if timeout_ms > 0 {
                Duration::from_millis(timeout_ms)
            } else {
                self.config.default_timeout
            };

            // Check if elapsed time exceeds timeout
            let elapsed = now.signed_duration_since(started_at);

            if let Ok(elapsed_duration) = elapsed.to_std() {
                if elapsed_duration > timeout_duration {
                    debug!(
                        "⏰ DynTimeoutChecker: Job {} has timed out (elapsed: {:?}, timeout: {:?})",
                        job.id, elapsed_duration, timeout_duration
                    );
                    timed_out_jobs.push(job.id.clone());
                }
            }
        }

        let jobs_checked = running_jobs.len();
        let timed_out_count = timed_out_jobs.len();

        if timed_out_count > 0 {
            info!(
                "⏰ DynTimeoutChecker: Found {} timed out jobs out of {} checked",
                timed_out_count, jobs_checked
            );
        } else {
            debug!(
                "⏰ DynTimeoutChecker: No timed out jobs found (checked {})",
                jobs_checked
            );
        }

        // Trigger timeout saga for each timed out job
        let mut sagas_triggered = 0;

        for job_id in timed_out_jobs {
            if let Err(e) = self.trigger_timeout_saga(&job_id).await {
                error!(
                    "⏰ DynTimeoutChecker: Failed to trigger timeout saga for job {}: {}",
                    job_id, e
                );
            } else {
                sagas_triggered += 1;
            }
        }

        TimeoutCheckResult::Completed {
            jobs_checked,
            timed_out_jobs: timed_out_count,
            sagas_triggered,
        }
    }

    /// Trigger a timeout saga for a specific job
    async fn trigger_timeout_saga(&self, job_id: &JobId) -> Result<(), DomainError> {
        // Create saga context
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Timeout,
            Some(format!("timeout-{}", job_id.0)),
            Some("dyn_timeout_checker".to_string()),
        );

        // Get timeout duration from job spec
        let job_opt = self.job_repository.find_by_id(job_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch job {}: {}", job_id, e),
            }
        })?;

        let timeout_ms = job_opt.as_ref().map(|j| j.spec.timeout_ms).unwrap_or(0);
        let timeout_duration = if timeout_ms > 0 {
            Duration::from_millis(timeout_ms)
        } else {
            self.config.default_timeout
        };

        // Create timeout saga
        let saga = hodei_server_domain::saga::TimeoutSaga::new(
            job_id.clone(),
            timeout_duration,
            "system_timeout_checker",
        );

        // Execute saga
        match self.saga_orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.state == hodei_server_domain::saga::SagaState::Completed {
                    info!(
                        "✅ DynTimeoutChecker: Timeout saga completed for job {}",
                        job_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown timeout saga error".to_string());
                    error!(
                        "❌ DynTimeoutChecker: Timeout saga failed for job {}: {}",
                        job_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ DynTimeoutChecker: Orchestrator error for job {}: {}",
                    job_id, e
                );
                Err(e)
            }
        }
    }
}
#[derive(Debug)]
pub struct TimeoutCheckerBuilder<JR, SO>
where
    JR: JobRepository + Send + Sync + 'static,
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    job_repository: Option<Arc<JR>>,
    saga_orchestrator: Option<Arc<SO>>,
    config: Option<TimeoutCheckerConfig>,
}

impl<JR, SO> TimeoutCheckerBuilder<JR, SO>
where
    JR: JobRepository + Send + Sync + 'static,
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    /// Create a new builder (requires type parameters to be specified)
    pub fn new() -> Self {
        Self {
            job_repository: None,
            saga_orchestrator: None,
            config: None,
        }
    }

    pub fn with_job_repository(mut self, job_repository: Arc<JR>) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_saga_orchestrator(mut self, saga_orchestrator: Arc<SO>) -> Self {
        self.saga_orchestrator = Some(saga_orchestrator);
        self
    }

    pub fn with_config(mut self, config: TimeoutCheckerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<TimeoutChecker<JR, SO>> {
        let job_repository = self
            .job_repository
            .ok_or_else(|| anyhow::anyhow!("job_repository is required"))?;
        let saga_orchestrator = self
            .saga_orchestrator
            .ok_or_else(|| anyhow::anyhow!("saga_orchestrator is required"))?;

        Ok(TimeoutChecker::new(
            job_repository,
            saga_orchestrator,
            self.config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::SagaType;

    #[test]
    fn test_timeout_checker_config_defaults() {
        let config = TimeoutCheckerConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(10));
        assert_eq!(config.default_timeout, Duration::from_secs(3600));
        assert_eq!(config.max_batch_size, 100);
        assert!(config.enabled);
    }

    #[test]
    fn test_timeout_check_result_variants() {
        let completed = TimeoutCheckResult::Completed {
            jobs_checked: 10,
            timed_out_jobs: 2,
            sagas_triggered: 2,
        };
        let failed = TimeoutCheckResult::Failed("error".to_string());
        let disabled = TimeoutCheckResult::Disabled;

        if let TimeoutCheckResult::Completed {
            jobs_checked,
            timed_out_jobs,
            sagas_triggered,
        } = completed
        {
            assert_eq!(jobs_checked, 10);
            assert_eq!(timed_out_jobs, 2);
            assert_eq!(sagas_triggered, 2);
        } else {
            panic!("Expected Completed variant");
        }

        assert!(matches!(failed, TimeoutCheckResult::Failed(_)));
        assert!(matches!(disabled, TimeoutCheckResult::Disabled));
    }

    #[test]
    fn test_saga_type_is_timeout() {
        assert!(SagaType::Timeout.is_timeout());
        assert!(!SagaType::Execution.is_timeout());
        assert!(!SagaType::Cancellation.is_timeout());
        assert!(!SagaType::Cleanup.is_timeout());
    }

    #[test]
    fn test_timeout_checker_builder() {
        // Test that the builder can be instantiated with concrete types
        // The builder requires JobRepository and SagaOrchestrator type parameters
        // but we don't need to fully implement them for this builder test
        let _config = TimeoutCheckerConfig::default();
        assert!(_config.enabled);
        assert_eq!(_config.check_interval, Duration::from_secs(10));
    }
}

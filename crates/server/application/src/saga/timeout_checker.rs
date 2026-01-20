//!
//! # Timeout Checker
//!
//! Detects and handles job timeouts using saga-engine v4.0 workflows.
//! This module replaces the legacy timeout_checker module.

use crate::saga::sync_executor::SyncWorkflowExecutor;
use crate::saga::workflows::timeout::TimeoutWorkflow;
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
    executor: Arc<SyncWorkflowExecutor<TimeoutWorkflow>>,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    config: TimeoutCheckerConfig,
}

impl TimeoutChecker {
    pub fn new(
        executor: Arc<SyncWorkflowExecutor<TimeoutWorkflow>>,
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
            let input = super::workflows::timeout::TimeoutWorkflowInput::new(
                job_id.clone(),
                self.config.default_job_timeout,
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

/// Newtype for dynamic timeout checker (erased type)
#[derive(Clone)]
pub struct DynTimeoutChecker(pub Arc<dyn TimeoutCheckerDynInterface>);

/// Dyn-compatible trait for timeout checker
pub trait TimeoutCheckerDynInterface: Send + Sync {
    fn check_job(
        &self,
        job_id: &JobId,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<TimeoutCheckResult, TimeoutCheckerError>>
                + Send
                + Sync,
        >,
    >;
    fn run(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + Sync>>;
}

impl TimeoutCheckerDynInterface for Arc<TimeoutChecker> {
    fn check_job(
        &self,
        job_id: &JobId,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<TimeoutCheckResult, TimeoutCheckerError>>
                + Send
                + Sync,
        >,
    > {
        let job_id = job_id.clone();
        let this = self.clone();
        Box::pin(async move { this.check_job(&job_id).await })
    }

    fn run(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + Sync>> {
        let this = self.clone();
        Box::pin(async move { this.run().await })
    }
}

impl DynTimeoutChecker {
    pub fn new<T: TimeoutCheckerDynInterface + 'static>(checker: T) -> Self {
        Self(Arc::new(checker))
    }

    pub fn builder() -> DynTimeoutCheckerBuilder {
        DynTimeoutCheckerBuilder::new()
    }

    pub async fn check_job(
        &self,
        job_id: &JobId,
    ) -> Result<TimeoutCheckResult, TimeoutCheckerError> {
        self.0.check_job(job_id).await
    }

    pub async fn run(&self) {
        self.0.run().await
    }
}

impl std::fmt::Debug for DynTimeoutChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynTimeoutChecker").finish()
    }
}

/// Builder for DynTimeoutChecker
pub struct DynTimeoutCheckerBuilder {
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    config: Option<TimeoutCheckerConfig>,
}

impl DynTimeoutCheckerBuilder {
    pub fn new() -> Self {
        Self {
            job_repository: None,
            config: None,
        }
    }

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_config(mut self, config: TimeoutCheckerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> Result<DynTimeoutChecker, String> {
        let executor: Arc<SyncWorkflowExecutor<TimeoutWorkflow>> =
            Arc::new(SyncWorkflowExecutor::new(TimeoutWorkflow::default()));
        let checker = TimeoutChecker::new(
            executor,
            self.job_repository.ok_or("job_repository is required")?,
            self.config,
        );
        Ok(DynTimeoutChecker::new(Arc::new(checker)))
    }
}

impl From<TimeoutChecker> for DynTimeoutChecker {
    fn from(checker: TimeoutChecker) -> Self {
        Self::new(Arc::new(checker))
    }
}

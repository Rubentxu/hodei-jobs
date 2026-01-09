//! Job State Timeout Monitor
//!
//! Background task that detects and recovers jobs stuck in intermediate states
//! (ASSIGNED, SCHEDULED) by timing them out and transitioning to FAILED state.
//!
//! This module implements US-30.2: Job State Timeout Monitor

use crate::jobs::repository_ext::JobRepositoryExt;
use chrono::{DateTime, Duration, Utc};
use hodei_server_domain::events::{DomainEvent, EventMetadata};
use hodei_server_domain::jobs::events::JobStatusChanged;
use hodei_server_domain::jobs::Job;
use hodei_server_domain::shared_kernel::{JobId, JobState};
use hodei_server_domain::{DomainError, EventPublisher, JobRepository, Result};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{error, info};

/// Configuration for the Job State Timeout Monitor
#[derive(Debug, Clone)]
pub struct JobStateTimeoutConfig {
    /// Timeout for jobs in ASSIGNED state (default: 5 minutes)
    pub assigned_timeout: Duration,
    /// Timeout for jobs in SCHEDULED state (default: 10 minutes)
    pub scheduled_timeout: Duration,
    /// Interval between timeout checks (default: 30 seconds)
    pub check_interval: StdDuration,
    /// Batch size for processing stuck jobs (default: 100)
    pub batch_size: u32,
    /// Whether the monitor is enabled (default: true)
    pub enabled: bool,
}

impl Default for JobStateTimeoutConfig {
    fn default() -> Self {
        Self {
            assigned_timeout: Duration::minutes(5),
            scheduled_timeout: Duration::minutes(10),
            check_interval: StdDuration::from_secs(30),
            batch_size: 100,
            enabled: true,
        }
    }
}

/// Result of a timeout check run
#[derive(Debug, Clone)]
pub struct TimeoutCheckResult {
    /// Jobs that were detected as timed out in ASSIGNED state
    pub timed_out_assigned: Vec<JobId>,
    /// Jobs that were detected as timed out in SCHEDULED state
    pub timed_out_scheduled: Vec<JobId>,
    /// Jobs that could not be processed due to errors
    pub failed_jobs: Vec<(JobId, String)>,
    /// Timestamp when the check was performed
    pub checked_at: DateTime<Utc>,
    /// Total jobs checked in this run
    pub total_checked: usize,
}

impl TimeoutCheckResult {
    /// Creates a new empty result
    pub fn new() -> Self {
        Self {
            timed_out_assigned: Vec::new(),
            timed_out_scheduled: Vec::new(),
            failed_jobs: Vec::new(),
            checked_at: Utc::now(),
            total_checked: 0,
        }
    }

    /// Returns true if any jobs were timed out
    pub fn has_timed_out(&self) -> bool {
        !self.timed_out_assigned.is_empty() || !self.timed_out_scheduled.is_empty()
    }

    /// Returns the total number of timed out jobs
    pub fn total_timed_out(&self) -> usize {
        self.timed_out_assigned.len() + self.timed_out_scheduled.len()
    }
}

impl Default for TimeoutCheckResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Job State Timeout Monitor
///
/// This monitor runs as a background task and periodically checks for jobs
/// that have been stuck in intermediate states for too long.
pub struct JobStateTimeoutMonitor<R, E>
where
    R: JobRepository + JobRepositoryExt + Send + Sync,
    E: EventPublisher<Error = DomainError> + Send + Sync,
{
    /// Job repository for querying and updating jobs
    repository: Arc<R>,
    /// Event publisher for domain events
    event_publisher: Arc<E>,
    /// Configuration for timeouts
    config: JobStateTimeoutConfig,
    /// Shutdown signal receiver
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

impl<R, E> JobStateTimeoutMonitor<R, E>
where
    R: JobRepository + JobRepositoryExt + Send + Sync,
    E: EventPublisher<Error = DomainError> + Send + Sync,
{
    /// Creates a new JobStateTimeoutMonitor
    pub fn new(
        repository: Arc<R>,
        event_publisher: Arc<E>,
        config: JobStateTimeoutConfig,
        shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            repository,
            event_publisher,
            config,
            shutdown,
        }
    }

    /// Runs the timeout monitor loop
    ///
    /// This method runs indefinitely until a shutdown signal is received.
    /// It periodically checks for stuck jobs and transitions them to FAILED state.
    pub async fn run(&mut self) {
        if !self.config.enabled {
            info!("JobStateTimeoutMonitor is disabled");
            return;
        }

        info!(
            "JobStateTimeoutMonitor started with config: assigned_timeout={}s, scheduled_timeout={}s, interval={:?}",
            self.config.assigned_timeout.num_seconds(),
            self.config.scheduled_timeout.num_seconds(),
            self.config.check_interval
        );

        let mut interval = interval(self.config.check_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.run_check().await {
                        error!("Timeout check failed: {}", e);
                    }
                }
                _ = self.shutdown.recv() => {
                    info!("JobStateTimeoutMonitor shutting down");
                    break;
                }
            }
        }
    }

    /// Runs a single timeout check
    ///
    /// This method queries for jobs in ASSIGNED and SCHEDULED states
    /// that have been in those states longer than the configured timeout,
    /// and transitions them to FAILED state with a timeout reason.
    pub async fn run_check(&mut self) -> Result<TimeoutCheckResult> {
        let mut result = TimeoutCheckResult::new();
        let now = Utc::now();

        // Calculate the cutoff timestamps
        let assigned_cutoff = now - self.config.assigned_timeout;
        let scheduled_cutoff = now - self.config.scheduled_timeout;

        // Find and process stuck ASSIGNED jobs
        match self
            .repository
            .find_by_state_older_than(JobState::Assigned, assigned_cutoff)
            .await
        {
            Ok(jobs) => {
                result.total_checked += jobs.len();
                for job in jobs {
                    let job_id = job.id.clone();
                    match self.timeout_job(job).await {
                        Ok(()) => {
                            result.timed_out_assigned.push(job_id);
                        }
                        Err(e) => {
                            result.failed_jobs.push((job_id, e.to_string()));
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to query assigned jobs: {}", e);
            }
        }

        // Find and process stuck SCHEDULED jobs
        match self
            .repository
            .find_by_state_older_than(JobState::Scheduled, scheduled_cutoff)
            .await
        {
            Ok(jobs) => {
                result.total_checked += jobs.len();
                for job in jobs {
                    let job_id = job.id.clone();
                    match self.timeout_job(job).await {
                        Ok(()) => {
                            result.timed_out_scheduled.push(job_id);
                        }
                        Err(e) => {
                            result.failed_jobs.push((job_id, e.to_string()));
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to query scheduled jobs: {}", e);
            }
        }

        // Log results
        if result.has_timed_out() {
            info!(
                "Timeout check completed: {} jobs timed out (assigned: {}, scheduled: {}), {} failed, {} total checked",
                result.total_timed_out(),
                result.timed_out_assigned.len(),
                result.timed_out_scheduled.len(),
                result.failed_jobs.len(),
                result.total_checked
            );
        }

        Ok(result)
    }

    /// Times out a single job
    ///
    /// Transitions the job from its current state to FAILED
    /// and publishes a JobStatusChanged event.
    async fn timeout_job(&mut self, job: Job) -> Result<()> {
        // Extract necessary data before mutating the job
        let old_state = job.state().clone();
        let job_id = job.id.clone();
        let new_state = JobState::Failed;

        // Create timeout reason
        let timeout_reason = format!(
            "Job timed out after being in {} state for {} seconds",
            old_state,
            match old_state {
                JobState::Assigned => self.config.assigned_timeout.num_seconds(),
                JobState::Scheduled => self.config.scheduled_timeout.num_seconds(),
                _ => 0,
            }
        );

        // Note: Job.fail() consumes the job, so we need to use the extracted values
        // Since we can't mutate job after fail(), we need to handle this differently
        // by using a mutable reference

        // Create a mutable clone for updating
        let mut job_mut = job;
        job_mut.fail(timeout_reason.clone())?;

        // Save the updated job
        self.repository.save(&job_mut).await?;

        // Publish domain event
        // EPIC-65 Phase 3: Using modular event type
        let status_changed_event = JobStatusChanged {
            job_id,
            old_state,
            new_state,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("timeout-monitor".to_string()),
        };

        let metadata = EventMetadata::for_system_event(None, "timeout-monitor");

        self.event_publisher
            .publish_enriched(status_changed_event.into(), metadata)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to publish timeout event: {}", e),
            })?;

        info!("Job {} timed out: old_state -> FAILED", job_mut.id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timeout_check_result_has_timed_out() {
        let result = TimeoutCheckResult::new();
        assert!(!result.has_timed_out());

        let mut result_with_timeout = TimeoutCheckResult::new();
        result_with_timeout.timed_out_assigned.push(JobId::new());
        assert!(result_with_timeout.has_timed_out());
    }

    #[tokio::test]
    async fn test_timeout_check_result_total_timed_out() {
        let result = TimeoutCheckResult::new();
        assert_eq!(result.total_timed_out(), 0);

        let mut result = TimeoutCheckResult::new();
        result.timed_out_assigned.push(JobId::new());
        result.timed_out_assigned.push(JobId::new());
        result.timed_out_scheduled.push(JobId::new());
        assert_eq!(result.total_timed_out(), 3);
    }

    #[tokio::test]
    async fn test_job_state_timeout_config_defaults() {
        let config = JobStateTimeoutConfig::default();

        assert_eq!(config.assigned_timeout, Duration::minutes(5));
        assert_eq!(config.scheduled_timeout, Duration::minutes(10));
        assert_eq!(config.check_interval, StdDuration::from_secs(30));
        assert_eq!(config.batch_size, 100);
        assert!(config.enabled);
    }
}

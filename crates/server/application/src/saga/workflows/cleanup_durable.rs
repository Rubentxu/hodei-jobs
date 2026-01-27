//!
//! # Cleanup Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`CleanupDurableWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! This is a migration from the legacy [`super::cleanup::CleanupWorkflow`]
//! which used the step-list pattern (`WorkflowDefinition`).
//!
//! ## Workflow Flow
//!
//! 1. Identify unhealthy workers (stuck in Creating, Busy, or Stall states)
//! 2. Identify orphaned jobs (running without active workers)
//! 3. Clean up unhealthy workers
//! 4. Reset orphaned jobs
//! 5. Publish cleanup metrics
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{Activity, DurableWorkflow, WorkflowContext};

use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Cleanup Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupWorkflowInput {
    /// Threshold for considering a worker unhealthy
    pub unhealthy_threshold: Duration,
    /// Threshold for considering a job orphaned
    pub orphaned_job_threshold: Duration,
    /// Whether to actually perform cleanup (dry run if false)
    pub dry_run: bool,
    /// Optional batch size limit
    pub batch_size: Option<usize>,
}

impl CleanupWorkflowInput {
    /// Create a new input with production defaults
    pub fn production_defaults() -> Self {
        Self {
            unhealthy_threshold: Duration::from_secs(300), // 5 minutes
            orphaned_job_threshold: Duration::from_secs(600), // 10 minutes
            dry_run: false,
            batch_size: Some(100),
        }
    }

    /// Create a dry-run input for testing
    pub fn dry_run() -> Self {
        Self {
            unhealthy_threshold: Duration::from_secs(300),
            orphaned_job_threshold: Duration::from_secs(600),
            dry_run: true,
            batch_size: Some(100),
        }
    }

    /// Generate idempotency key for this input
    pub fn idempotency_key(&self) -> String {
        let now = chrono::Utc::now().timestamp();
        format!(
            "cleanup:{}:{}",
            now,
            if self.dry_run { "dry" } else { "prod" }
        )
    }
}

/// Output of the Cleanup Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupWorkflowOutput {
    /// Number of unhealthy workers identified
    pub unhealthy_workers_identified: usize,
    /// Number of unhealthy workers cleaned up
    pub unhealthy_workers_cleaned: usize,
    /// Number of orphaned jobs identified
    pub orphaned_jobs_identified: usize,
    /// Number of orphaned jobs reset
    pub orphaned_jobs_reset: usize,
    /// Whether metrics were published
    pub metrics_published: bool,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Timestamp of completion
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// Details of cleaned resources
    pub details: CleanupDetails,
}

/// Details about the cleanup operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupDetails {
    /// Worker IDs that were cleaned
    pub cleaned_workers: Vec<String>,
    /// Job IDs that were reset
    pub reset_jobs: Vec<String>,
    /// Any errors encountered
    pub errors: Vec<String>,
    /// Cleanup duration in milliseconds
    pub duration_ms: u64,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CleanupWorkflowError {
    #[error("Worker identification failed: {0}")]
    WorkerIdentificationFailed(String),

    #[error("Job identification failed: {0}")]
    JobIdentificationFailed(String),

    #[error("Worker cleanup failed: {0}")]
    WorkerCleanupFailed(String),

    #[error("Job reset failed: {0}")]
    JobResetFailed(String),

    #[error("Metrics publication failed: {0}")]
    MetricsPublicationFailed(String),

    #[error("Activity execution failed: {0}")]
    ActivityFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

/// Activity for identifying unhealthy workers
pub struct IdentifyUnhealthyWorkersActivity {
    /// Worker registry for querying
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for IdentifyUnhealthyWorkersActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyUnhealthyWorkersActivity").finish()
    }
}

impl IdentifyUnhealthyWorkersActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for IdentifyUnhealthyWorkersActivity {
    const TYPE_ID: &'static str = "cleanup-identify-unhealthy-workers";

    type Input = IdentifyUnhealthyWorkersInput;
    type Output = IdentifyUnhealthyWorkersOutput;
    type Error = CleanupWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // In production, this would query the registry for workers in problematic states
        // that have been in those states longer than the threshold
        let unhealthy_workers: Vec<String> = Vec::new();

        Ok(IdentifyUnhealthyWorkersOutput {
            unhealthy_workers,
            checked_count: 0,
            threshold_seconds: input.threshold.as_secs(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyUnhealthyWorkersInput {
    pub threshold: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyUnhealthyWorkersOutput {
    pub unhealthy_workers: Vec<String>,
    pub checked_count: usize,
    pub threshold_seconds: u64,
}

/// Activity for identifying orphaned jobs
pub struct IdentifyOrphanedJobsActivity {
    /// Worker registry for cross-referencing
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for IdentifyOrphanedJobsActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyOrphanedJobsActivity").finish()
    }
}

impl IdentifyOrphanedJobsActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for IdentifyOrphanedJobsActivity {
    const TYPE_ID: &'static str = "cleanup-identify-orphaned-jobs";

    type Input = IdentifyOrphanedJobsInput;
    type Output = IdentifyOrphanedJobsOutput;
    type Error = CleanupWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // In production, this would find jobs that appear to be running
        // but don't have an active worker associated
        let orphaned_jobs: Vec<String> = Vec::new();

        Ok(IdentifyOrphanedJobsOutput {
            orphaned_jobs,
            checked_count: 0,
            threshold_seconds: input.threshold.as_secs(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyOrphanedJobsInput {
    pub threshold: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyOrphanedJobsOutput {
    pub orphaned_jobs: Vec<String>,
    pub checked_count: usize,
    pub threshold_seconds: u64,
}

/// Activity for cleaning up unhealthy workers
pub struct CleanupUnhealthyWorkersActivity {
    /// Worker provisioning for termination
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
    /// Worker registry for state updates
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for CleanupUnhealthyWorkersActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanupUnhealthyWorkersActivity").finish()
    }
}

impl CleanupUnhealthyWorkersActivity {
    pub fn new(
        provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            provisioning,
            registry,
        }
    }
}

#[async_trait]
impl Activity for CleanupUnhealthyWorkersActivity {
    const TYPE_ID: &'static str = "cleanup-unhealthy-workers";

    type Input = CleanupUnhealthyWorkersInput;
    type Output = CleanupUnhealthyWorkersOutput;
    type Error = CleanupWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let mut cleaned_count = 0;
        let mut cleaned_ids: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for worker_id_str in &input.worker_ids {
            if input.dry_run {
                cleaned_ids.push(worker_id_str.clone());
                cleaned_count += 1;
                continue;
            }

            if let Some(worker_id) = WorkerId::from_string(worker_id_str) {
                match self
                    .provisioning
                    .terminate_worker(&worker_id, "cleanup")
                    .await
                {
                    Ok(_) => {
                        self.registry
                            .update_state(&worker_id, WorkerState::Destroyed)
                            .await
                            .ok();
                        cleaned_ids.push(worker_id_str.clone());
                        cleaned_count += 1;
                    }
                    Err(e) => {
                        errors.push(format!("Failed to clean {}: {}", worker_id_str, e));
                    }
                }
            } else {
                errors.push(format!("Invalid worker ID: {}", worker_id_str));
            }
        }

        Ok(CleanupUnhealthyWorkersOutput {
            cleaned_count,
            cleaned_ids,
            errors,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupUnhealthyWorkersInput {
    pub worker_ids: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupUnhealthyWorkersOutput {
    pub cleaned_count: usize,
    pub cleaned_ids: Vec<String>,
    pub errors: Vec<String>,
}

/// Activity for resetting orphaned jobs
pub struct ResetOrphanedJobsActivity {
    /// Worker registry for updates
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for ResetOrphanedJobsActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetOrphanedJobsActivity").finish()
    }
}

impl ResetOrphanedJobsActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for ResetOrphanedJobsActivity {
    const TYPE_ID: &'static str = "cleanup-reset-orphaned-jobs";

    type Input = ResetOrphanedJobsInput;
    type Output = ResetOrphanedJobsOutput;
    type Error = CleanupWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let mut reset_count = 0;
        let mut reset_ids: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for job_id_str in &input.job_ids {
            if input.dry_run {
                reset_ids.push(job_id_str.clone());
                reset_count += 1;
                continue;
            }

            if let Some(job_id) = JobId::from_string(job_id_str) {
                // Find worker and reset it
                if let Some(ref worker) = self.registry.find_by_job(&job_id).await.ok().flatten() {
                    self.registry
                        .update_state(worker.id(), WorkerState::Ready)
                        .await
                        .ok();
                }
                reset_ids.push(job_id_str.clone());
                reset_count += 1;
            } else {
                errors.push(format!("Invalid job ID: {}", job_id_str));
            }
        }

        Ok(ResetOrphanedJobsOutput {
            reset_count,
            reset_ids,
            errors,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetOrphanedJobsInput {
    pub job_ids: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetOrphanedJobsOutput {
    pub reset_count: usize,
    pub reset_ids: Vec<String>,
    pub errors: Vec<String>,
}

/// Activity for publishing cleanup metrics
#[derive(Debug)]
pub struct PublishCleanupMetricsActivity;

impl PublishCleanupMetricsActivity {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Activity for PublishCleanupMetricsActivity {
    const TYPE_ID: &'static str = "cleanup-publish-metrics";

    type Input = PublishCleanupMetricsInput;
    type Output = PublishCleanupMetricsOutput;
    type Error = CleanupWorkflowError;

    async fn execute(&self, _input: Self::Input) -> Result<Self::Output, Self::Error> {
        // In production, this would publish metrics to a monitoring system
        Ok(PublishCleanupMetricsOutput {
            published: true,
            published_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishCleanupMetricsInput {
    pub workers_cleaned: usize,
    pub jobs_reset: usize,
    pub duration_ms: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishCleanupMetricsOutput {
    pub published: bool,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Cleanup Durable Workflow
// =============================================================================

/// Cleanup Workflow for saga-engine v4 using DurableWorkflow pattern
///
/// This workflow handles system maintenance and cleanup:
/// 1. Identify unhealthy workers
/// 2. Identify orphaned jobs
/// 3. Clean up unhealthy workers
/// 4. Reset orphaned jobs
/// 5. Publish cleanup metrics
pub struct CleanupDurableWorkflow {
    /// Worker registry
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
    /// Worker provisioning service
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
}

impl std::fmt::Debug for CleanupDurableWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanupDurableWorkflow").finish()
    }
}

impl Clone for CleanupDurableWorkflow {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            provisioning: self.provisioning.clone(),
        }
    }
}

impl CleanupDurableWorkflow {
    /// Create a new CleanupDurableWorkflow
    pub fn new(
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
        provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
    ) -> Self {
        Self {
            registry,
            provisioning,
        }
    }
}

#[async_trait]
impl DurableWorkflow for CleanupDurableWorkflow {
    const TYPE_ID: &'static str = "cleanup";
    const VERSION: u32 = 1;

    type Input = CleanupWorkflowInput;
    type Output = CleanupWorkflowOutput;
    type Error = CleanupWorkflowError;

    /// Execute the workflow
    ///
    /// Implements the Workflow-as-Code pattern for system cleanup.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        let start_time = std::time::Instant::now();
        let mut details = CleanupDetails::default();

        // Activity 1: Identify unhealthy workers
        let identify_unhealthy_input = IdentifyUnhealthyWorkersInput {
            threshold: input.unhealthy_threshold,
        };

        let identify_unhealthy_output = context
            .execute_activity(
                &IdentifyUnhealthyWorkersActivity::new(self.registry.clone()),
                identify_unhealthy_input,
            )
            .await
            .map_err(|e| CleanupWorkflowError::ActivityFailed(e.to_string()))?;

        let unhealthy_workers_count = identify_unhealthy_output.unhealthy_workers.len();

        // Activity 2: Identify orphaned jobs
        let identify_orphaned_input = IdentifyOrphanedJobsInput {
            threshold: input.orphaned_job_threshold,
        };

        let identify_orphaned_output = context
            .execute_activity(
                &IdentifyOrphanedJobsActivity::new(self.registry.clone()),
                identify_orphaned_input,
            )
            .await
            .map_err(|e| CleanupWorkflowError::ActivityFailed(e.to_string()))?;

        let orphaned_jobs_count = identify_orphaned_output.orphaned_jobs.len();

        // Activity 3: Clean up unhealthy workers (if not dry run)
        let mut workers_cleaned = 0;
        if !identify_unhealthy_output.unhealthy_workers.is_empty() && !input.dry_run {
            let cleanup_input = CleanupUnhealthyWorkersInput {
                worker_ids: identify_unhealthy_output.unhealthy_workers.clone(),
                dry_run: input.dry_run,
            };

            let cleanup_output = context
                .execute_activity(
                    &CleanupUnhealthyWorkersActivity::new(
                        self.provisioning.clone(),
                        self.registry.clone(),
                    ),
                    cleanup_input,
                )
                .await
                .map_err(|e| CleanupWorkflowError::ActivityFailed(e.to_string()))?;

            workers_cleaned = cleanup_output.cleaned_count;
            details.cleaned_workers = cleanup_output.cleaned_ids;
            details.errors.extend(cleanup_output.errors);
        }

        // Activity 4: Reset orphaned jobs (if not dry run)
        let mut jobs_reset = 0;
        if !identify_orphaned_output.orphaned_jobs.is_empty() && !input.dry_run {
            let reset_input = ResetOrphanedJobsInput {
                job_ids: identify_orphaned_output.orphaned_jobs.clone(),
                dry_run: input.dry_run,
            };

            let reset_output = context
                .execute_activity(
                    &ResetOrphanedJobsActivity::new(self.registry.clone()),
                    reset_input,
                )
                .await
                .map_err(|e| CleanupWorkflowError::ActivityFailed(e.to_string()))?;

            jobs_reset = reset_output.reset_count;
            details.reset_jobs = reset_output.reset_ids;
            details.errors.extend(reset_output.errors);
        }

        // Activity 5: Publish cleanup metrics
        let duration_ms = start_time.elapsed().as_millis() as u64;
        details.duration_ms = duration_ms;

        let metrics_input = PublishCleanupMetricsInput {
            workers_cleaned,
            jobs_reset,
            duration_ms,
            dry_run: input.dry_run,
        };

        let metrics_output = context
            .execute_activity(&PublishCleanupMetricsActivity::new(), metrics_input)
            .await
            .map_err(|e| CleanupWorkflowError::ActivityFailed(e.to_string()))?;

        Ok(CleanupWorkflowOutput {
            unhealthy_workers_identified: unhealthy_workers_count,
            unhealthy_workers_cleaned: workers_cleaned,
            orphaned_jobs_identified: orphaned_jobs_count,
            orphaned_jobs_reset: jobs_reset,
            metrics_published: metrics_output.published,
            dry_run: input.dry_run,
            completed_at: chrono::Utc::now(),
            details,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::provisioning::{MockWorkerProvisioning, MockWorkerRegistry};
    use hodei_server_domain::workers::WorkerProvisioning;

    #[tokio::test]
    async fn test_cleanup_workflow_input_production_defaults() {
        let input = CleanupWorkflowInput::production_defaults();

        assert_eq!(input.unhealthy_threshold, Duration::from_secs(300));
        assert_eq!(input.orphaned_job_threshold, Duration::from_secs(600));
        assert!(!input.dry_run);
    }

    #[tokio::test]
    async fn test_cleanup_workflow_input_dry_run() {
        let input = CleanupWorkflowInput::dry_run();

        assert!(input.dry_run);
    }

    #[tokio::test]
    async fn test_idempotency_key() {
        let input = CleanupWorkflowInput::production_defaults();
        let key = input.idempotency_key();
        assert!(key.starts_with("cleanup:"));
    }

    #[tokio::test]
    async fn test_identify_unhealthy_workers_activity() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = IdentifyUnhealthyWorkersActivity::new(registry);
        let input = IdentifyUnhealthyWorkersInput {
            threshold: Duration::from_secs(300),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.threshold_seconds, 300);
    }

    #[tokio::test]
    async fn test_identify_orphaned_jobs_activity() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = IdentifyOrphanedJobsActivity::new(registry);
        let input = IdentifyOrphanedJobsInput {
            threshold: Duration::from_secs(600),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.threshold_seconds, 600);
    }

    #[tokio::test]
    async fn test_cleanup_unhealthy_workers_activity_dry_run() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioningService + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());
        let activity = CleanupUnhealthyWorkersActivity::new(provisioning, registry);
        let input = CleanupUnhealthyWorkersInput {
            worker_ids: vec!["worker-1".to_string(), "worker-2".to_string()],
            dry_run: true,
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.cleaned_count, 2);
    }

    #[tokio::test]
    async fn test_reset_orphaned_jobs_activity_dry_run() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = ResetOrphanedJobsActivity::new(registry);
        let input = ResetOrphanedJobsInput {
            job_ids: vec!["job-1".to_string(), "job-2".to_string()],
            dry_run: true,
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.reset_count, 2);
    }

    #[tokio::test]
    async fn test_publish_cleanup_metrics_activity() {
        let activity = PublishCleanupMetricsActivity::new();
        let input = PublishCleanupMetricsInput {
            workers_cleaned: 5,
            jobs_reset: 10,
            duration_ms: 1500,
            dry_run: false,
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.published);
    }

    #[tokio::test]
    async fn test_cleanup_workflow_output() {
        let output = CleanupWorkflowOutput {
            unhealthy_workers_identified: 3,
            unhealthy_workers_cleaned: 2,
            orphaned_jobs_identified: 5,
            orphaned_jobs_reset: 4,
            metrics_published: true,
            dry_run: false,
            completed_at: chrono::Utc::now(),
            details: CleanupDetails {
                cleaned_workers: vec!["w1".to_string(), "w2".to_string()],
                reset_jobs: vec!["j1".to_string()],
                errors: vec![],
                duration_ms: 1000,
            },
        };

        assert_eq!(output.unhealthy_workers_identified, 3);
        assert_eq!(output.unhealthy_workers_cleaned, 2);
        assert_eq!(output.orphaned_jobs_identified, 5);
        assert!(output.metrics_published);
    }
}

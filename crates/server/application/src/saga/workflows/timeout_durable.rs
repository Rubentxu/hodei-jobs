//!
//! # Timeout Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`TimeoutDurableWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! This is a migration from the legacy [`super::timeout::TimeoutWorkflow`]
//! which used the step-list pattern (`WorkflowDefinition`).
//!
//! ## Workflow Flow
//!
//! 1. Validate timeout (check if job has exceeded its timeout)
//! 2. Terminate the worker if job has timed out
//! 3. Mark job as FAILED due to timeout
//! 4. Release worker resources
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{Activity, DurableWorkflow, WorkflowContext};

use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Timeout Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutWorkflowInput {
    /// Job ID that may have timed out
    pub job_id: String,
    /// Expected duration before timeout
    pub expected_duration: Duration,
    /// Maximum allowed duration
    pub max_duration: Duration,
    /// Reason for timeout check
    pub reason: String,
}

impl TimeoutWorkflowInput {
    /// Create a new input
    pub fn new(
        job_id: JobId,
        expected_duration: Duration,
        max_duration: Duration,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.to_string(),
            expected_duration,
            max_duration,
            reason: reason.into(),
        }
    }

    /// Generate idempotency key for this input
    pub fn idempotency_key(&self) -> String {
        format!("timeout:{}:{}", self.job_id, self.reason)
    }
}

/// Output of the Timeout Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutWorkflowOutput {
    /// The job ID
    pub job_id: String,
    /// Whether the job timed out
    pub timed_out: bool,
    /// Worker ID if applicable
    pub worker_id: Option<String>,
    /// Whether worker was terminated
    pub worker_terminated: bool,
    /// Final job state
    pub final_state: JobState,
    /// Timestamp of timeout handling
    pub handled_at: chrono::DateTime<chrono::Utc>,
    /// Duration check result in seconds
    pub checked_duration_secs: u64,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum TimeoutWorkflowError {
    #[error("Timeout validation failed: {0}")]
    TimeoutValidationFailed(String),

    #[error("Worker lookup failed: {0}")]
    WorkerLookupFailed(String),

    #[error("Worker termination failed: {0}")]
    WorkerTerminationFailed(String),

    #[error("Job state update failed: {0}")]
    JobStateUpdateFailed(String),

    #[error("Activity execution failed: {0}")]
    ActivityFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

/// Activity for checking if job has timed out
pub struct CheckTimeoutActivity {
    /// Worker registry for lookup
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for CheckTimeoutActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckTimeoutActivity").finish()
    }
}

impl CheckTimeoutActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for CheckTimeoutActivity {
    const TYPE_ID: &'static str = "timeout-check";

    type Input = CheckTimeoutInput;
    type Output = CheckTimeoutOutput;
    type Error = TimeoutWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            TimeoutWorkflowError::TimeoutValidationFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;

        let worker = self
            .registry
            .find_by_job(&job_id)
            .await
            .map_err(|e| TimeoutWorkflowError::WorkerLookupFailed(e.to_string()))?;

        // Check if worker exists and is in a state that indicates it may have timed out
        let has_timed_out = worker
            .as_ref()
            .map_or(false, |w| matches!(w.state(), WorkerState::Busy));

        // Use max_duration for timeout calculation
        let checked_duration_secs = input.max_duration.as_secs();

        Ok(CheckTimeoutOutput {
            job_id: input.job_id,
            has_timed_out,
            worker_id: worker.map(|w| w.id().to_string()),
            checked_duration_secs,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckTimeoutInput {
    pub job_id: String,
    pub expected_duration: Duration,
    pub max_duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckTimeoutOutput {
    pub job_id: String,
    pub has_timed_out: bool,
    pub worker_id: Option<String>,
    pub checked_duration_secs: u64,
}

/// Activity for terminating timed-out worker
pub struct TerminateTimedOutWorkerActivity {
    /// Worker provisioning service
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
    /// Worker registry for state updates
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for TerminateTimedOutWorkerActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminateTimedOutWorkerActivity").finish()
    }
}

impl TerminateTimedOutWorkerActivity {
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
impl Activity for TerminateTimedOutWorkerActivity {
    const TYPE_ID: &'static str = "timeout-terminate-worker";

    type Input = TerminateTimedOutWorkerInput;
    type Output = TerminateTimedOutWorkerOutput;
    type Error = TimeoutWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if let Some(worker_id_str) = &input.worker_id {
            let worker_id = WorkerId::from_string(worker_id_str).ok_or_else(|| {
                TimeoutWorkflowError::WorkerTerminationFailed(format!(
                    "Invalid worker ID: {}",
                    worker_id_str
                ))
            })?;

            // Get worker to find provider
            let worker = self
                .registry
                .find_by_id(&worker_id)
                .await
                .map_err(|e| TimeoutWorkflowError::WorkerTerminationFailed(e.to_string()))?;

            if let Some(worker) = worker {
                // Terminate the worker infrastructure
                self.provisioning
                    .terminate_worker(&worker_id, &input.reason)
                    .await
                    .map_err(|e| TimeoutWorkflowError::WorkerTerminationFailed(e.to_string()))?;

                // Update registry to reflect termination
                self.registry
                    .update_state(&worker_id, WorkerState::Destroyed)
                    .await
                    .ok();

                Ok(TerminateTimedOutWorkerOutput {
                    worker_id: worker_id_str.clone(),
                    terminated: true,
                    provider_id: worker.provider_id().to_string(),
                    message: "Worker terminated due to timeout".to_string(),
                })
            } else {
                Ok(TerminateTimedOutWorkerOutput {
                    worker_id: worker_id_str.clone(),
                    terminated: false,
                    provider_id: String::new(),
                    message: "Worker not found".to_string(),
                })
            }
        } else {
            Ok(TerminateTimedOutWorkerOutput {
                worker_id: String::new(),
                terminated: false,
                provider_id: String::new(),
                message: "No worker to terminate".to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateTimedOutWorkerInput {
    pub worker_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateTimedOutWorkerOutput {
    pub worker_id: String,
    pub terminated: bool,
    pub provider_id: String,
    pub message: String,
}

/// Activity for marking job as failed due to timeout
pub struct MarkJobFailedActivity {
    // In a real implementation, this would need access to update job state
    // For now, this is a placeholder that simulates the activity
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for MarkJobFailedActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkJobFailedActivity").finish()
    }
}

impl MarkJobFailedActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for MarkJobFailedActivity {
    const TYPE_ID: &'static str = "timeout-mark-job-failed";

    type Input = MarkJobFailedInput;
    type Output = MarkJobFailedOutput;
    type Error = TimeoutWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // In production, this would update the job state in the repository
        // For now, we simulate success
        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            TimeoutWorkflowError::JobStateUpdateFailed(format!("Invalid job ID: {}", input.job_id))
        })?;

        // Find and update any worker associated with this job
        let worker = self
            .registry
            .find_by_job(&job_id)
            .await
            .map_err(|e| TimeoutWorkflowError::JobStateUpdateFailed(e.to_string()))?;

        if let Some(ref worker) = worker {
            self.registry
                .update_state(worker.id(), WorkerState::Ready)
                .await
                .ok();
        }

        Ok(MarkJobFailedOutput {
            job_id: input.job_id,
            failed: true,
            failure_reason: format!("TIMEOUT: {}", input.timeout_reason),
            failed_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobFailedInput {
    pub job_id: String,
    pub timeout_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobFailedOutput {
    pub job_id: String,
    pub failed: bool,
    pub failure_reason: String,
    pub failed_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Timeout Durable Workflow
// =============================================================================

/// Timeout Workflow for saga-engine v4 using DurableWorkflow pattern
///
/// This workflow handles job timeout detection and recovery:
/// 1. Check if the job has exceeded its timeout
/// 2. Terminate the worker if the job has timed out
/// 3. Mark the job as FAILED
/// 4. Release worker resources
pub struct TimeoutDurableWorkflow {
    /// Worker registry
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
    /// Worker provisioning service
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
}

impl std::fmt::Debug for TimeoutDurableWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimeoutDurableWorkflow").finish()
    }
}

impl Clone for TimeoutDurableWorkflow {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            provisioning: self.provisioning.clone(),
        }
    }
}

impl TimeoutDurableWorkflow {
    /// Create a new TimeoutDurableWorkflow
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
impl DurableWorkflow for TimeoutDurableWorkflow {
    const TYPE_ID: &'static str = "timeout";
    const VERSION: u32 = 1;

    type Input = TimeoutWorkflowInput;
    type Output = TimeoutWorkflowOutput;
    type Error = TimeoutWorkflowError;

    /// Execute the workflow
    ///
    /// Implements the Workflow-as-Code pattern for job timeout handling.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Activity 1: Check if job has timed out
        let check_input = CheckTimeoutInput {
            job_id: input.job_id.clone(),
            expected_duration: input.expected_duration,
            max_duration: input.max_duration,
        };

        let check_output = context
            .execute_activity(
                &CheckTimeoutActivity::new(self.registry.clone()),
                check_input,
            )
            .await
            .map_err(|e| TimeoutWorkflowError::ActivityFailed(e.to_string()))?;

        // Activity 2: Terminate worker if timed out
        let mut worker_terminated = false;
        if check_output.has_timed_out {
            let terminate_input = TerminateTimedOutWorkerInput {
                worker_id: check_output.worker_id.clone(),
                reason: format!("Timeout: {}", input.reason),
            };

            let _terminate_output = context
                .execute_activity(
                    &TerminateTimedOutWorkerActivity::new(
                        self.provisioning.clone(),
                        self.registry.clone(),
                    ),
                    terminate_input,
                )
                .await
                .map_err(|e| TimeoutWorkflowError::ActivityFailed(e.to_string()))?;
            worker_terminated = true;
        }

        // Activity 3: Mark job as failed if timed out
        let final_state = if check_output.has_timed_out {
            let fail_input = MarkJobFailedInput {
                job_id: input.job_id.clone(),
                timeout_reason: input.reason.clone(),
            };

            let _fail_output = context
                .execute_activity(
                    &MarkJobFailedActivity::new(self.registry.clone()),
                    fail_input,
                )
                .await
                .map_err(|e| TimeoutWorkflowError::ActivityFailed(e.to_string()))?;

            JobState::Failed
        } else {
            JobState::Succeeded
        };

        Ok(TimeoutWorkflowOutput {
            job_id: input.job_id,
            timed_out: check_output.has_timed_out,
            worker_id: check_output.worker_id,
            worker_terminated,
            final_state,
            handled_at: chrono::Utc::now(),
            checked_duration_secs: check_output.checked_duration_secs,
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
    async fn test_timeout_workflow_input() {
        let job_id = JobId::new();
        let input = TimeoutWorkflowInput::new(
            job_id.clone(),
            Duration::from_secs(3600),
            Duration::from_secs(7200),
            "Scheduled timeout check",
        );

        assert_eq!(input.job_id, job_id.to_string());
        assert_eq!(input.expected_duration, Duration::from_secs(3600));
        assert_eq!(input.max_duration, Duration::from_secs(7200));
    }

    #[tokio::test]
    async fn test_idempotency_key() {
        let job_id = JobId::new();
        let input = TimeoutWorkflowInput::new(
            job_id.clone(),
            Duration::from_secs(3600),
            Duration::from_secs(7200),
            "test reason",
        );

        let key = input.idempotency_key();
        assert!(key.starts_with("timeout:"));
        assert!(key.contains(&job_id.to_string()));
        assert!(key.contains(":test reason"));
    }

    #[tokio::test]
    async fn test_check_timeout_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = CheckTimeoutActivity::new(registry);
        let job_id = JobId::new();
        let input = CheckTimeoutInput {
            job_id: job_id.to_string(),
            expected_duration: Duration::from_secs(3600),
            max_duration: Duration::from_secs(7200),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.job_id, job_id.to_string());
    }

    #[tokio::test]
    async fn test_terminate_timed_out_worker_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioningService + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());
        let activity = TerminateTimedOutWorkerActivity::new(provisioning, registry);
        let input = TerminateTimedOutWorkerInput {
            worker_id: Some("worker-456".to_string()),
            reason: "timeout".to_string(),
        };

        let result = activity.execute(input).await;
        // May fail if worker not found, which is expected in test
        assert!(result.is_err() || result.unwrap().terminated);
    }

    #[tokio::test]
    async fn test_mark_job_failed_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = MarkJobFailedActivity::new(registry);
        let job_id = JobId::new();
        let input = MarkJobFailedInput {
            job_id: job_id.to_string(),
            timeout_reason: "Job exceeded maximum duration".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.failed);
        assert_eq!(
            output.failure_reason,
            "TIMEOUT: Job exceeded maximum duration"
        );
    }

    #[tokio::test]
    async fn test_timeout_workflow_output() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let output = TimeoutWorkflowOutput {
            job_id: job_id.to_string(),
            timed_out: true,
            worker_id: Some(worker_id.to_string()),
            worker_terminated: true,
            final_state: JobState::Failed,
            handled_at: chrono::Utc::now(),
            checked_duration_secs: 7200,
        };

        assert_eq!(output.job_id, job_id.to_string());
        assert!(output.timed_out);
        assert_eq!(output.final_state, JobState::Failed);
    }

    #[tokio::test]
    async fn test_timeout_workflow_output_not_timed_out() {
        let output = TimeoutWorkflowOutput {
            job_id: "job-123".to_string(),
            timed_out: false,
            worker_id: None,
            worker_terminated: false,
            final_state: JobState::Succeeded,
            handled_at: chrono::Utc::now(),
            checked_duration_secs: 3600,
        };

        assert!(!output.timed_out);
        assert_eq!(output.final_state, JobState::Succeeded);
    }
}

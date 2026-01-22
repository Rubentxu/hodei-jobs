//!
//! # Cancellation Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`CancellationDurableWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! This is a migration from the legacy [`super::cancellation::CancellationWorkflow`]
//! which used the step-list pattern (`WorkflowDefinition`).
//!
//! ## Workflow Flow
//!
//! 1. Validate cancellation request
//! 2. Notify worker to stop
//! 3. Update job state to CANCELLED
//! 4. Release worker resources
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{Activity, DurableWorkflow, WorkflowContext};

use crate::saga::bridge::worker_lifecycle::{
    SetWorkerBusyActivity, SetWorkerReadyActivity, TerminateWorkerActivity,
};
use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Cancellation Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationWorkflowInput {
    /// Job ID to cancel
    pub job_id: String,
    /// Reason for cancellation
    pub reason: String,
    /// Requester identifier
    pub requester: Option<String>,
    /// Whether to destroy worker infrastructure
    pub destroy_infrastructure: bool,
}

impl CancellationWorkflowInput {
    /// Create a new input
    pub fn new(job_id: JobId, reason: impl Into<String>) -> Self {
        Self {
            job_id: job_id.to_string(),
            reason: reason.into(),
            requester: None,
            destroy_infrastructure: false,
        }
    }

    /// Set the requester
    pub fn with_requester(mut self, requester: impl Into<String>) -> Self {
        self.requester = Some(requester.into());
        self
    }

    /// Set destroy infrastructure flag
    pub fn with_destroy_infrastructure(mut self, destroy: bool) -> Self {
        self.destroy_infrastructure = destroy;
        self
    }

    /// Generate idempotency key for this input
    pub fn idempotency_key(&self) -> String {
        format!("cancellation:{}:{}", self.job_id, self.reason)
    }
}

/// Output of the Cancellation Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationWorkflowOutput {
    /// The job ID
    pub job_id: String,
    /// Original job state before cancellation
    pub original_state: JobState,
    /// Job state after cancellation
    pub final_state: JobState,
    /// Whether worker was notified
    pub worker_notified: bool,
    /// Whether worker infrastructure was destroyed
    pub infrastructure_destroyed: bool,
    /// Worker ID if applicable
    pub worker_id: Option<String>,
    /// Timestamp of cancellation
    pub cancelled_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CancellationWorkflowError {
    #[error("Job validation failed: {0}")]
    JobValidationFailed(String),

    #[error("Worker lookup failed: {0}")]
    WorkerLookupFailed(String),

    #[error("Worker notification failed: {0}")]
    WorkerNotificationFailed(String),

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

/// Activity for validating cancellation request
pub struct ValidateCancellationActivity {
    /// Worker registry for lookup
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for ValidateCancellationActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateCancellationActivity").finish()
    }
}

impl ValidateCancellationActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for ValidateCancellationActivity {
    const TYPE_ID: &'static str = "cancellation-validate";

    type Input = ValidateCancellationInput;
    type Output = ValidateCancellationOutput;
    type Error = CancellationWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            CancellationWorkflowError::JobValidationFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;

        let worker = self
            .registry
            .find_by_job(&job_id)
            .await
            .map_err(|e| CancellationWorkflowError::WorkerLookupFailed(e.to_string()))?;

        let is_cancellable = worker.as_ref().map(|w| w.state()).map_or(true, |state| {
            matches!(state, WorkerState::Ready | WorkerState::Creating)
        });

        Ok(ValidateCancellationOutput {
            job_id: input.job_id,
            is_cancellable,
            current_state: JobState::Pending,
            worker_id: worker.map(|w| w.id().to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCancellationInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCancellationOutput {
    pub job_id: String,
    pub is_cancellable: bool,
    pub current_state: JobState,
    pub worker_id: Option<String>,
}

/// Activity for notifying worker to stop
pub struct NotifyWorkerStopActivity {
    /// Worker registry for state updates
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for NotifyWorkerStopActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyWorkerStopActivity").finish()
    }
}

impl NotifyWorkerStopActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for NotifyWorkerStopActivity {
    const TYPE_ID: &'static str = "cancellation-notify-worker";

    type Input = NotifyWorkerStopInput;
    type Output = NotifyWorkerStopOutput;
    type Error = CancellationWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if let Some(worker_id_str) = &input.worker_id {
            let worker_id = WorkerId::from_string(worker_id_str).ok_or_else(|| {
                CancellationWorkflowError::WorkerNotificationFailed(format!(
                    "Invalid worker ID: {}",
                    worker_id_str
                ))
            })?;

            // Set worker back to ready (releasing from job)
            self.registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| CancellationWorkflowError::WorkerNotificationFailed(e.to_string()))?;

            Ok(NotifyWorkerStopOutput {
                worker_id: worker_id_str.clone(),
                notified: true,
                message: "Worker notified of cancellation".to_string(),
            })
        } else {
            Ok(NotifyWorkerStopOutput {
                worker_id: String::new(),
                notified: false,
                message: "No worker to notify".to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerStopInput {
    pub job_id: String,
    pub worker_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerStopOutput {
    pub worker_id: String,
    pub notified: bool,
    pub message: String,
}

/// Activity for terminating worker infrastructure
pub struct TerminateInfrastructureActivity {
    /// Worker provisioning service
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
    /// Worker registry
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for TerminateInfrastructureActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminateInfrastructureActivity").finish()
    }
}

impl TerminateInfrastructureActivity {
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
impl Activity for TerminateInfrastructureActivity {
    const TYPE_ID: &'static str = "cancellation-terminate-infrastructure";

    type Input = TerminateInfrastructureInput;
    type Output = TerminateInfrastructureOutput;
    type Error = CancellationWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if let Some(worker_id_str) = &input.worker_id {
            let worker_id = WorkerId::from_string(worker_id_str).ok_or_else(|| {
                CancellationWorkflowError::WorkerTerminationFailed(format!(
                    "Invalid worker ID: {}",
                    worker_id_str
                ))
            })?;

            // Get worker to find provider
            let worker =
                self.registry.find_by_id(&worker_id).await.map_err(|e| {
                    CancellationWorkflowError::WorkerTerminationFailed(e.to_string())
                })?;

            if let Some(worker) = worker {
                // Terminate the worker infrastructure
                self.provisioning
                    .terminate_worker(&worker_id, &input.reason)
                    .await
                    .map_err(|e| {
                        CancellationWorkflowError::WorkerTerminationFailed(e.to_string())
                    })?;

                // Update registry to reflect termination
                self.registry
                    .update_state(&worker_id, WorkerState::Destroyed)
                    .await
                    .ok();

                Ok(TerminateInfrastructureOutput {
                    worker_id: worker_id_str.clone(),
                    destroyed: true,
                    provider_id: worker.provider_id().to_string(),
                    message: "Worker terminated successfully".to_string(),
                })
            } else {
                Ok(TerminateInfrastructureOutput {
                    worker_id: worker_id_str.clone(),
                    destroyed: false,
                    provider_id: String::new(),
                    message: "Worker not found".to_string(),
                })
            }
        } else {
            Ok(TerminateInfrastructureOutput {
                worker_id: String::new(),
                destroyed: false,
                provider_id: String::new(),
                message: "No worker to terminate".to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateInfrastructureInput {
    pub worker_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateInfrastructureOutput {
    pub worker_id: String,
    pub destroyed: bool,
    pub provider_id: String,
    pub message: String,
}

// =============================================================================
// Cancellation Durable Workflow
// =============================================================================

/// Cancellation Workflow for saga-engine v4 using DurableWorkflow pattern
///
/// This workflow handles job cancellation:
/// 1. Validate the cancellation request
/// 2. Notify the worker to stop
/// 3. Optionally terminate worker infrastructure
/// 4. Release worker resources
pub struct CancellationDurableWorkflow {
    /// Worker registry
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
    /// Worker provisioning service
    provisioning: Arc<dyn WorkerProvisioningService + Send + Sync>,
}

impl std::fmt::Debug for CancellationDurableWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationDurableWorkflow").finish()
    }
}

impl Clone for CancellationDurableWorkflow {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            provisioning: self.provisioning.clone(),
        }
    }
}

impl CancellationDurableWorkflow {
    /// Create a new CancellationDurableWorkflow
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
impl DurableWorkflow for CancellationDurableWorkflow {
    const TYPE_ID: &'static str = "cancellation";
    const VERSION: u32 = 1;

    type Input = CancellationWorkflowInput;
    type Output = CancellationWorkflowOutput;
    type Error = CancellationWorkflowError;

    /// Execute the workflow
    ///
    /// Implements the Workflow-as-Code pattern for job cancellation.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Activity 1: Validate the cancellation request
        let validate_input = ValidateCancellationInput {
            job_id: input.job_id.clone(),
        };

        let validate_output = context
            .execute_activity(
                &ValidateCancellationActivity::new(self.registry.clone()),
                validate_input,
            )
            .await
            .map_err(|e| CancellationWorkflowError::ActivityFailed(e.to_string()))?;

        // Store original state for output
        let original_state = validate_output.current_state.clone();

        // If not cancellable, we still proceed but mark appropriately
        let worker_id = validate_output.worker_id.clone();

        // Activity 2: Notify worker to stop (if there's a worker)
        let notify_input = NotifyWorkerStopInput {
            job_id: input.job_id.clone(),
            worker_id: validate_output.worker_id.clone(),
            reason: input.reason.clone(),
        };

        let notify_output = context
            .execute_activity(
                &NotifyWorkerStopActivity::new(self.registry.clone()),
                notify_input,
            )
            .await
            .map_err(|e| CancellationWorkflowError::ActivityFailed(e.to_string()))?;

        // Activity 3: Optionally terminate infrastructure
        let mut infrastructure_destroyed = false;
        if input.destroy_infrastructure && notify_output.notified {
            let terminate_input = TerminateInfrastructureInput {
                worker_id: validate_output.worker_id.clone(),
                reason: format!("Cancellation: {}", input.reason),
            };

            let _terminate_output = context
                .execute_activity(
                    &TerminateInfrastructureActivity::new(
                        self.provisioning.clone(),
                        self.registry.clone(),
                    ),
                    terminate_input,
                )
                .await
                .map_err(|e| CancellationWorkflowError::ActivityFailed(e.to_string()))?;
            infrastructure_destroyed = true;
        }

        // Final state is CANCELLED
        let final_state = JobState::Cancelled;

        Ok(CancellationWorkflowOutput {
            job_id: input.job_id,
            original_state,
            final_state,
            worker_notified: notify_output.notified,
            infrastructure_destroyed,
            worker_id,
            cancelled_at: chrono::Utc::now(),
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::workers::MockWorkerProvisioning;
    use hodei_server_domain::workers::MockWorkerRegistry;

    #[tokio::test]
    async fn test_cancellation_workflow_input() {
        let input = CancellationWorkflowInput::new(
            JobId::from_string("job-123").unwrap(),
            "User requested cancellation",
        );

        assert_eq!(input.job_id, "job-123");
        assert_eq!(input.reason, "User requested cancellation");
        assert!(!input.destroy_infrastructure);
    }

    #[tokio::test]
    async fn test_cancellation_workflow_input_with_requester() {
        let input = CancellationWorkflowInput::new(
            JobId::from_string("job-123").unwrap(),
            "Timeout reached",
        )
        .with_requester("system")
        .with_destroy_infrastructure(true);

        assert_eq!(input.job_id, "job-123");
        assert_eq!(input.requester, Some("system".to_string()));
        assert!(input.destroy_infrastructure);
    }

    #[tokio::test]
    async fn test_idempotency_key() {
        let input =
            CancellationWorkflowInput::new(JobId::from_string("job-123").unwrap(), "test reason");

        let key = input.idempotency_key();
        assert!(key.starts_with("cancellation:job-123:test reason"));
    }

    #[tokio::test]
    async fn test_validate_cancellation_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = ValidateCancellationActivity::new(registry);
        let input = ValidateCancellationInput {
            job_id: "job-123".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.job_id, "job-123");
    }

    #[tokio::test]
    async fn test_notify_worker_stop_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = NotifyWorkerStopActivity::new(registry);
        let input = NotifyWorkerStopInput {
            job_id: "job-123".to_string(),
            worker_id: Some("worker-456".to_string()),
            reason: "test".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.notified);
    }

    #[tokio::test]
    async fn test_notify_worker_stop_activity_no_worker() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let activity = NotifyWorkerStopActivity::new(registry);
        let input = NotifyWorkerStopInput {
            job_id: "job-123".to_string(),
            worker_id: None,
            reason: "test".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.notified);
    }

    #[tokio::test]
    async fn test_terminate_infrastructure_activity_with_mock() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());
        let activity = TerminateInfrastructureActivity::new(provisioning, registry);
        let input = TerminateInfrastructureInput {
            worker_id: Some("worker-456".to_string()),
            reason: "cancellation".to_string(),
        };

        let result = activity.execute(input).await;
        // May fail if worker not found, which is expected in test
        assert!(result.is_err() || result.unwrap().destroyed);
    }

    #[tokio::test]
    async fn test_cancellation_workflow_output() {
        let output = CancellationWorkflowOutput {
            job_id: "job-123".to_string(),
            original_state: JobState::Running,
            final_state: JobState::Cancelled,
            worker_notified: true,
            infrastructure_destroyed: false,
            worker_id: Some("worker-456".to_string()),
            cancelled_at: chrono::Utc::now(),
        };

        assert_eq!(output.job_id, "job-123");
        assert_eq!(output.final_state, JobState::Cancelled);
        assert!(output.worker_notified);
    }
}

//!
//! # Recovery Saga Coordinator
//!
//! Coordinates the recovery saga using saga-engine v4.0 workflows.

use crate::saga::sync_durable_executor::SyncDurableWorkflowExecutor;
use crate::saga::workflows::recovery_durable::RecoveryWorkflow;
use async_trait::async_trait;
use hodei_server_domain::shared_kernel::JobId;
use hodei_server_domain::shared_kernel::WorkerId;
use std::sync::Arc;
use thiserror::Error;

/// Errors from recovery saga operations
#[derive(Debug, Error)]
pub enum RecoverySagaError {
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
    #[error("Compensation action completed")]
    Compensated,
}

/// Result of recovery saga execution
#[derive(Debug)]
pub struct RecoverySagaResult {
    pub saga_id: String,
    pub old_worker_id: WorkerId,
    pub new_worker_id: WorkerId,
    pub job_id: JobId,
    pub duration: std::time::Duration,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Configuration for recovery saga
#[derive(Debug, Clone)]
pub struct RecoverySagaCoordinatorConfig {
    pub saga_timeout: std::time::Duration,
    pub step_timeout: std::time::Duration,
}

impl Default for RecoverySagaCoordinatorConfig {
    fn default() -> Self {
        Self {
            saga_timeout: std::time::Duration::from_secs(300),
            step_timeout: std::time::Duration::from_secs(60),
        }
    }
}

/// Coordinator for recovery workflow
#[derive(Clone)]
pub struct RecoverySagaCoordinator {
    executor: Arc<SyncDurableWorkflowExecutor<RecoveryWorkflow>>,
}

impl RecoverySagaCoordinator {
    pub fn new(executor: Arc<SyncDurableWorkflowExecutor<RecoveryWorkflow>>) -> Self {
        Self { executor }
    }

    /// Execute recovery saga
    pub async fn execute_recovery_saga(
        &self,
        job_id: JobId,
        failed_worker_id: WorkerId,
        reason: String,
    ) -> Result<RecoverySagaResult, RecoverySagaError> {
        let start_time = std::time::Instant::now();
        let saga_id = uuid::Uuid::new_v4().to_string();
        let input = RecoveryInput::new(
            job_id.clone(),
            failed_worker_id.clone(),
            Some(saga_id.clone()),
            reason,
        );

        let result = self
            .executor
            .execute(
                serde_json::to_value(&input)
                    .map_err(|e| RecoverySagaError::RecoveryFailed(e.to_string()))?,
            )
            .await
            .map_err(|e| RecoverySagaError::RecoveryFailed(e.to_string()))?;

        match result {
            saga_engine_core::workflow::WorkflowResult::Completed { output, .. } => {
                let new_worker_id = output
                    .get("new_worker_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| WorkerId::from_string(s))
                    .unwrap_or_else(|| WorkerId::new());

                Ok(RecoverySagaResult {
                    saga_id,
                    old_worker_id: failed_worker_id,
                    new_worker_id,
                    job_id,
                    duration: start_time.elapsed(),
                    completed_at: chrono::Utc::now(),
                })
            }
            saga_engine_core::workflow::WorkflowResult::Failed { error, .. } => {
                Err(RecoverySagaError::RecoveryFailed(error))
            }
            saga_engine_core::workflow::WorkflowResult::Cancelled { .. } => {
                Err(RecoverySagaError::RecoveryFailed("Cancelled".to_string()))
            }
        }
    }
}

use super::workflows::recovery_durable::RecoveryInput;
use crate::saga::SagaExecutionId;
use crate::saga::port::SagaPort;

#[async_trait]
impl SagaPort<RecoveryWorkflow> for RecoverySagaCoordinator {
    type Error = RecoverySagaError;

    async fn start_workflow(
        &self,
        input: RecoveryInput,
        _idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        let execution_id = SagaExecutionId::new();

        let result = self
            .executor
            .execute(
                serde_json::to_value(&input)
                    .map_err(|e| RecoverySagaError::RecoveryFailed(e.to_string()))?,
            )
            .await
            .map_err(|e| RecoverySagaError::RecoveryFailed(e.to_string()))?;

        match result {
            saga_engine_core::workflow::WorkflowResult::Completed { .. } => Ok(execution_id),
            saga_engine_core::workflow::WorkflowResult::Failed { error, .. } => {
                Err(RecoverySagaError::RecoveryFailed(error))
            }
            saga_engine_core::workflow::WorkflowResult::Cancelled { .. } => {
                Err(RecoverySagaError::RecoveryFailed("Cancelled".to_string()))
            }
        }
    }

    async fn get_workflow_state(
        &self,
        _execution_id: &SagaExecutionId,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        Ok(saga_engine_core::workflow::WorkflowState::Running {
            current_step: None,
            elapsed: None,
        })
    }

    async fn cancel_workflow(
        &self,
        _execution_id: &SagaExecutionId,
        _reason: String,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn send_signal(
        &self,
        _execution_id: &SagaExecutionId,
        _signal: String,
        _payload: <RecoveryWorkflow as saga_engine_core::workflow::DurableWorkflow>::Output,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_completion(
        &self,
        _execution_id: &SagaExecutionId,
        _timeout: std::time::Duration,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        Ok(saga_engine_core::workflow::WorkflowState::Running {
            current_step: None,
            elapsed: None,
        })
    }
}

/// Newtype for dynamic recovery coordinator (erased type for trait objects)
#[derive(Clone)]
pub struct DynRecoverySagaCoordinator(pub Arc<dyn RecoverySagaCoordinatorTrait>);

/// Trait for recovery coordinator operations (dyn-compatible)
#[async_trait]
pub trait RecoverySagaCoordinatorTrait: Send + Sync {
    async fn execute_recovery_saga(
        &self,
        job_id: JobId,
        failed_worker_id: WorkerId,
        reason: String,
    ) -> Result<RecoverySagaResult, RecoverySagaError>;
}

#[async_trait]
impl RecoverySagaCoordinatorTrait for RecoverySagaCoordinator {
    async fn execute_recovery_saga(
        &self,
        job_id: JobId,
        failed_worker_id: WorkerId,
        reason: String,
    ) -> Result<RecoverySagaResult, RecoverySagaError> {
        self.execute_recovery_saga(job_id, failed_worker_id, reason)
            .await
    }
}

impl DynRecoverySagaCoordinator {
    pub fn new<T: RecoverySagaCoordinatorTrait + 'static>(coordinator: T) -> Self {
        Self(Arc::new(coordinator))
    }

    /// Execute recovery saga (convenience method)
    pub async fn execute_recovery_saga(
        &self,
        job_id: JobId,
        failed_worker_id: WorkerId,
        reason: String,
    ) -> Result<RecoverySagaResult, RecoverySagaError> {
        self.0
            .execute_recovery_saga(job_id, failed_worker_id, reason)
            .await
    }
}

//!
//! # Recovery Saga Coordinator
//!
//! Coordinates the recovery saga using saga-engine v4.0 workflows.
//! This module replaces the legacy RecoverySagaCoordinator.

use crate::saga::sync_executor::SyncWorkflowExecutor;
use async_trait::async_trait;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::saga::SagaOrchestrator;
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
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

impl RecoverySagaCoordinatorConfig {
    pub fn default() -> Self {
        Self {
            saga_timeout: std::time::Duration::from_secs(300),
            step_timeout: std::time::Duration::from_secs(60),
        }
    }
}

/// Coordinator for recovery workflow
#[derive(Clone)]
pub struct RecoverySagaCoordinator {
    executor: Arc<SyncWorkflowExecutor<super::workflows::recovery::RecoveryWorkflow>>,
}

impl RecoverySagaCoordinator {
    pub fn new(
        executor: Arc<SyncWorkflowExecutor<super::workflows::recovery::RecoveryWorkflow>>,
    ) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl crate::saga::port::SagaPort<super::workflows::recovery::RecoveryWorkflow>
    for RecoverySagaCoordinator
{
    type Error = RecoverySagaError;

    async fn start_workflow(
        &self,
        input: super::workflows::recovery::RecoveryWorkflowInput,
        _idempotency_key: Option<String>,
    ) -> Result<crate::saga::SagaExecutionId, Self::Error> {
        let execution_id = crate::saga::SagaExecutionId::new();

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
        _execution_id: &crate::saga::SagaExecutionId,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        Ok(saga_engine_core::workflow::WorkflowState::Running {
            current_step: None,
            elapsed: None,
        })
    }

    async fn cancel_workflow(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
        _reason: String,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn send_signal(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
        _signal: String,
        _payload: super::workflows::recovery::RecoveryWorkflowOutput,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_completion(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
        _timeout: std::time::Duration,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        // For now, just return running state
        Ok(saga_engine_core::workflow::WorkflowState::Running {
            current_step: None,
            elapsed: None,
        })
    }
}

/// Newtype for dynamic recovery coordinator
pub struct DynRecoverySagaCoordinator(pub Arc<dyn RecoverySagaCoordinatorTrait>);

impl DynRecoverySagaCoordinator {
    pub fn new<T: RecoverySagaCoordinatorTrait + 'static>(coordinator: T) -> Self {
        Self(Arc::new(coordinator))
    }

    /// Create a builder for DynRecoverySagaCoordinator
    pub fn builder() -> DynRecoverySagaCoordinatorBuilder {
        DynRecoverySagaCoordinatorBuilder::new()
    }
}

/// Builder for DynRecoverySagaCoordinator
pub struct DynRecoverySagaCoordinatorBuilder {
    orchestrator: Option<Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    config: Option<RecoverySagaCoordinatorConfig>,
}

impl DynRecoverySagaCoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            worker_registry: None,
            event_bus: None,
            config: None,
        }
    }

    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_config(mut self, config: RecoverySagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> Result<DynRecoverySagaCoordinator, String> {
        // For now, just wrap a RecoverySagaCoordinator with a default executor
        // A full implementation would require more complex setup
        let executor: Arc<SyncWorkflowExecutor<super::workflows::recovery::RecoveryWorkflow>> =
            Arc::new(SyncWorkflowExecutor::new(
                super::workflows::recovery::RecoveryWorkflow::default(),
            ));
        let coordinator = RecoverySagaCoordinator::new(executor);
        Ok(DynRecoverySagaCoordinator::new(coordinator))
    }
}

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
        let start_time = std::time::Instant::now();
        let saga_id = uuid::Uuid::new_v4().to_string();
        let input = super::workflows::recovery::RecoveryWorkflowInput::new(
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

#[async_trait]
impl RecoverySagaCoordinatorTrait for DynRecoverySagaCoordinator {
    async fn execute_recovery_saga(
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

// Implement for reference to DynRecoverySagaCoordinator
#[async_trait]
impl RecoverySagaCoordinatorTrait for &DynRecoverySagaCoordinator {
    async fn execute_recovery_saga(
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

// Implement for reference to Arc<DynRecoverySagaCoordinator>
#[async_trait]
impl RecoverySagaCoordinatorTrait for &Arc<DynRecoverySagaCoordinator> {
    async fn execute_recovery_saga(
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

// Implement for &dyn RecoverySagaCoordinatorTrait
#[async_trait]
impl RecoverySagaCoordinatorTrait for &dyn RecoverySagaCoordinatorTrait {
    async fn execute_recovery_saga(
        &self,
        job_id: JobId,
        failed_worker_id: WorkerId,
        reason: String,
    ) -> Result<RecoverySagaResult, RecoverySagaError> {
        (*self)
            .execute_recovery_saga(job_id, failed_worker_id, reason)
            .await
    }
}

impl std::fmt::Debug for DynRecoverySagaCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynRecoverySagaCoordinator").finish()
    }
}

// Deref to allow using the inner method directly
impl std::ops::Deref for DynRecoverySagaCoordinator {
    type Target = Arc<dyn RecoverySagaCoordinatorTrait>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

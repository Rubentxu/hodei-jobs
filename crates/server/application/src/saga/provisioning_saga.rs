//!
//! # Provisioning Saga Coordinator
//!
//! Coordinates the provisioning saga using saga-engine v4.0 workflows.
//! This module replaces the legacy ProvisioningSagaCoordinator.

use crate::saga::sync_durable_executor::SyncDurableWorkflowExecutor;
use async_trait::async_trait;
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::{WorkerRegistry, WorkerSpec};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, instrument, warn};

/// Errors from provisioning saga operations
#[derive(Debug, Error)]
pub enum ProvisioningSagaError {
    #[error("Provider {provider_id} is not available")]
    ProviderNotAvailable { provider_id: ProviderId },
    #[error("Worker provisioning failed: {0}")]
    ProvisioningFailed(String),
    #[error("Saga execution failed: {0}")]
    SagaFailed(String),
    #[error("Compensation action completed")]
    Compensated,
}

/// Configuration for provisioning saga
#[derive(Debug, Clone)]
pub struct ProvisioningSagaCoordinatorConfig {
    pub saga_timeout: std::time::Duration,
    pub step_timeout: std::time::Duration,
}

/// Result of provisioning saga execution
#[derive(Debug)]
pub struct ProvisioningSagaResult {
    pub worker_id: WorkerId,
    pub job_id: Option<JobId>,
    pub duration: std::time::Duration,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

impl ProvisioningSagaCoordinatorConfig {
    pub fn default() -> Self {
        Self {
            saga_timeout: std::time::Duration::from_secs(300),
            step_timeout: std::time::Duration::from_secs(60),
        }
    }
}

/// Coordinator for provisioning workflow
#[derive(Clone)]
pub struct ProvisioningSagaCoordinator {
    executor: Arc<
        SyncDurableWorkflowExecutor<super::workflows::provisioning_durable::ProvisioningWorkflow>,
    >,
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl ProvisioningSagaCoordinator {
    pub fn new(
        executor: Arc<
            SyncDurableWorkflowExecutor<
                super::workflows::provisioning_durable::ProvisioningWorkflow,
            >,
        >,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self { executor, registry }
    }
}

#[async_trait]
impl crate::saga::port::SagaPort<super::workflows::provisioning_durable::ProvisioningWorkflow>
    for ProvisioningSagaCoordinator
{
    type Error = ProvisioningSagaError;

    async fn start_workflow(
        &self,
        input: super::workflows::provisioning_durable::ProvisioningInput,
        _idempotency_key: Option<String>,
    ) -> Result<crate::saga::SagaExecutionId, Self::Error> {
        let execution_id = crate::saga::SagaExecutionId::new();

        let result = self
            .executor
            .execute(
                serde_json::to_value(&input)
                    .map_err(|e| ProvisioningSagaError::SagaFailed(e.to_string()))?,
            )
            .await
            .map_err(|e| ProvisioningSagaError::SagaFailed(e.to_string()))?;

        match result {
            saga_engine_core::workflow::WorkflowResult::Completed { .. } => Ok(execution_id),
            saga_engine_core::workflow::WorkflowResult::Failed { error, .. } => {
                Err(ProvisioningSagaError::SagaFailed(error))
            }
            saga_engine_core::workflow::WorkflowResult::Cancelled { .. } => {
                Err(ProvisioningSagaError::SagaFailed("Cancelled".to_string()))
            }
        }
    }

    async fn get_workflow_state(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        // TODO: Implement state retrieval
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
        // TODO: Implement cancellation
        Ok(())
    }

    async fn send_signal(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
        _signal: String,
        _payload: super::workflows::provisioning_durable::ProvisioningOutput,
    ) -> Result<(), Self::Error> {
        // TODO: Implement signal sending
        Ok(())
    }

    async fn wait_for_completion(
        &self,
        _execution_id: &crate::saga::SagaExecutionId,
        _timeout: std::time::Duration,
    ) -> Result<saga_engine_core::workflow::WorkflowState, Self::Error> {
        Ok(saga_engine_core::workflow::WorkflowState::Running {
            current_step: None,
            elapsed: None,
        })
    }
}

/// Newtype for dynamic provisioning coordinator
pub struct DynProvisioningSagaCoordinator(pub Arc<dyn ProvisioningSagaCoordinatorTrait>);

impl DynProvisioningSagaCoordinator {
    pub fn new<T: ProvisioningSagaCoordinatorTrait + 'static>(coordinator: T) -> Self {
        Self(Arc::new(coordinator))
    }
}

#[async_trait]
pub trait ProvisioningSagaCoordinatorTrait: Send + Sync {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError>;
}

#[async_trait]
impl ProvisioningSagaCoordinatorTrait for ProvisioningSagaCoordinator {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError> {
        let start_time = std::time::Instant::now();
        let input = super::workflows::provisioning_durable::ProvisioningInput::new(
            super::workflows::provisioning_durable::WorkerSpecData::from_spec(spec),
            provider_id.to_string(),
            job_id.as_ref().map(|j| j.to_string()),
        );

        let result = self
            .executor
            .execute(
                serde_json::to_value(&input)
                    .map_err(|e| ProvisioningSagaError::SagaFailed(e.to_string()))?,
            )
            .await
            .map_err(|e| ProvisioningSagaError::SagaFailed(e.to_string()))?;

        match result {
            saga_engine_core::workflow::WorkflowResult::Completed { output, .. } => {
                let output: super::workflows::provisioning_durable::ProvisioningOutput =
                    serde_json::from_value(output)
                        .map_err(|e| ProvisioningSagaError::SagaFailed(e.to_string()))?;
                let worker_id =
                    WorkerId::from_string(&output.worker_id).unwrap_or_else(|| WorkerId::new());
                Ok(ProvisioningSagaResult {
                    worker_id,
                    job_id,
                    duration: start_time.elapsed(),
                    completed_at: chrono::Utc::now(),
                })
            }
            saga_engine_core::workflow::WorkflowResult::Failed { error, .. } => {
                Err(ProvisioningSagaError::SagaFailed(error))
            }
            saga_engine_core::workflow::WorkflowResult::Cancelled { .. } => {
                Err(ProvisioningSagaError::SagaFailed("Cancelled".to_string()))
            }
        }
    }
}

#[async_trait]
impl ProvisioningSagaCoordinatorTrait for DynProvisioningSagaCoordinator {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError> {
        self.0
            .execute_provisioning_saga(provider_id, spec, job_id)
            .await
    }
}

// Implement for reference to DynProvisioningSagaCoordinator
#[async_trait]
impl ProvisioningSagaCoordinatorTrait for &DynProvisioningSagaCoordinator {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError> {
        self.0
            .execute_provisioning_saga(provider_id, spec, job_id)
            .await
    }
}

// Implement for reference to Arc<DynProvisioningSagaCoordinator>
#[async_trait]
impl ProvisioningSagaCoordinatorTrait for &Arc<DynProvisioningSagaCoordinator> {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError> {
        self.0
            .execute_provisioning_saga(provider_id, spec, job_id)
            .await
    }
}

// Implement for &dyn ProvisioningSagaCoordinatorTrait
#[async_trait]
impl ProvisioningSagaCoordinatorTrait for &dyn ProvisioningSagaCoordinatorTrait {
    async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningSagaResult, ProvisioningSagaError> {
        (*self)
            .execute_provisioning_saga(provider_id, spec, job_id)
            .await
    }
}

impl std::fmt::Debug for DynProvisioningSagaCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynProvisioningSagaCoordinator").finish()
    }
}

// Deref to allow using the inner method directly
impl std::ops::Deref for DynProvisioningSagaCoordinator {
    type Target = Arc<dyn ProvisioningSagaCoordinatorTrait>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//!
//! # Execution Saga Dispatcher
//!
//! Coordinates job execution using saga-engine v4.0 DurableWorkflow.
//! This module replaces the legacy dispatcher_saga module.

use crate::saga::bridge::job_execution_port::CommandBusJobExecutionPort;
use crate::saga::sync_durable_executor::SyncDurableWorkflowExecutor;
use crate::saga::workflows::execution_durable::ExecutionWorkflow;
use async_trait::async_trait;
use hodei_server_domain::jobs::Job;
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

/// Concrete type for ExecutionWorkflow with CommandBusJobExecutionPort
type RealExecutionWorkflow = ExecutionWorkflow<CommandBusJobExecutionPort>;

/// Configuration for execution saga
#[derive(Debug, Clone)]
pub struct ExecutionSagaDispatcherConfig {
    pub saga_timeout: std::time::Duration,
    pub step_timeout: std::time::Duration,
}

impl Default for ExecutionSagaDispatcherConfig {
    fn default() -> Self {
        Self {
            saga_timeout: std::time::Duration::from_secs(300),
            step_timeout: std::time::Duration::from_secs(60),
        }
    }
}

/// Errors from execution saga
#[derive(Debug, Error)]
pub enum ExecutionSagaDispatcherError {
    #[error("Job validation failed: {0}")]
    JobValidationFailed(String),
    #[error("No available workers: {0}")]
    NoWorkersAvailable(String),
    #[error("Saga execution failed: {0}")]
    SagaFailed(String),
}

/// Result of execution saga
#[derive(Debug)]
pub struct ExecutionSagaResult {
    pub job_id: String,
    pub worker_id: String,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

/// Dispatcher for execution workflow (v4.0 DurableWorkflow)
#[derive(Clone)]
pub struct ExecutionSagaDispatcher {
    executor: Arc<SyncDurableWorkflowExecutor<RealExecutionWorkflow>>,
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl ExecutionSagaDispatcher {
    pub fn new(
        executor: Arc<SyncDurableWorkflowExecutor<RealExecutionWorkflow>>,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self { executor, registry }
    }
}

/// Newtype for dynamic execution dispatcher
pub struct DynExecutionSagaDispatcher(pub Arc<dyn ExecutionSagaDispatcherTrait>);

impl DynExecutionSagaDispatcher {
    pub fn new<T: ExecutionSagaDispatcherTrait + 'static>(dispatcher: T) -> Self {
        Self(Arc::new(dispatcher))
    }
}

#[async_trait]
pub trait ExecutionSagaDispatcherTrait: Send + Sync {
    async fn dispatch(
        &self,
        job: &Job,
        worker: &Worker,
    ) -> Result<ExecutionSagaResult, ExecutionSagaDispatcherError>;
}

#[async_trait]
impl ExecutionSagaDispatcherTrait for ExecutionSagaDispatcher {
    async fn dispatch(
        &self,
        job: &Job,
        worker: &Worker,
    ) -> Result<ExecutionSagaResult, ExecutionSagaDispatcherError> {
        // Extract command and arguments from job spec
        let command_vec = job.spec().command().to_command_vec();
        let command = command_vec.first().cloned().unwrap_or_default();
        let arguments = command_vec.get(1..).map(|s| s.to_vec()).unwrap_or_default();

        let input = super::workflows::execution_durable::ExecutionWorkflowInput::new(
            job.id.to_string(),
            worker.id().to_string(),
            command,
            arguments,
        );

        let result = self
            .executor
            .execute(
                serde_json::to_value(&input).map_err(|e: serde_json::Error| {
                    ExecutionSagaDispatcherError::SagaFailed(e.to_string())
                })?,
            )
            .await
            .map_err(|e: String| ExecutionSagaDispatcherError::SagaFailed(e))?;

        match result {
            saga_engine_core::workflow::WorkflowResult::Completed { .. } => {
                Ok(ExecutionSagaResult {
                    job_id: job.id.to_string(),
                    worker_id: worker.id().to_string(),
                    assigned_at: chrono::Utc::now(),
                })
            }
            saga_engine_core::workflow::WorkflowResult::Failed { error, .. } => {
                Err(ExecutionSagaDispatcherError::SagaFailed(error))
            }
            saga_engine_core::workflow::WorkflowResult::Cancelled { .. } => Err(
                ExecutionSagaDispatcherError::SagaFailed("Cancelled".to_string()),
            ),
        }
    }
}

#[async_trait]
impl ExecutionSagaDispatcherTrait for DynExecutionSagaDispatcher {
    async fn dispatch(
        &self,
        job: &Job,
        worker: &Worker,
    ) -> Result<ExecutionSagaResult, ExecutionSagaDispatcherError> {
        self.0.dispatch(job, worker).await
    }
}

impl std::fmt::Debug for DynExecutionSagaDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynExecutionSagaDispatcher").finish()
    }
}

// Deref to allow using the inner dispatch method directly
impl std::ops::Deref for DynExecutionSagaDispatcher {
    type Target = Arc<dyn ExecutionSagaDispatcherTrait>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

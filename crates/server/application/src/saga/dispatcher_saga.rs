//!
//! # Execution Saga Dispatcher
//!
//! Coordinates job execution using saga-engine v4.0 DurableWorkflow.
//! This module provides execution dispatch via the SagaEngine for proper
//! durable execution with activities dispatched to NATS task queues.

use crate::saga::bridge::job_execution_port::CommandBusJobExecutionPort;
use crate::saga::saga_engine_executor::SagaEngineExecutionExecutor;
use crate::saga::workflows::execution_durable::ExecutionWorkflow;
use async_trait::async_trait;
use hodei_server_domain::jobs::Job;
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use saga_engine_core::port::{EventStore, TaskQueue, TimerStore};
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

/// Executor trait for execution workflow
///
/// This trait abstracts the execution backend, allowing different implementations:
/// - `SagaEngineExecutionExecutor`: Production-ready with NATS task queues
/// - `SyncDurableWorkflowExecutor`: Testing-only, synchronous execution
#[async_trait]
pub trait ExecutionWorkflowExecutor: Send + Sync {
    async fn execute(
        &self,
        job: &Job,
        worker: &Worker,
    ) -> Result<ExecutionSagaResult, ExecutionSagaDispatcherError>;
}

/// Dispatcher for execution workflow (v4.0 DurableWorkflow) using SagaEngine
///
/// This dispatcher uses the SagaEngine for proper durable execution:
/// - Workflow state persisted in PostgreSQL EventStore
/// - Activities dispatched to NATS TaskQueue
/// - Workflows resume when activities complete
#[derive(Clone)]
pub struct ExecutionSagaDispatcher<E, Q, T>
where
    E: EventStore + Send + Sync + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    executor: Arc<SagaEngineExecutionExecutor<E, Q, T>>,
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl<E, Q, T> ExecutionSagaDispatcher<E, Q, T>
where
    E: EventStore + Send + Sync + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    pub fn new(
        executor: Arc<SagaEngineExecutionExecutor<E, Q, T>>,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self { executor, registry }
    }
}

#[async_trait]
impl<E, Q, T> ExecutionSagaDispatcherTrait for ExecutionSagaDispatcher<E, Q, T>
where
    E: EventStore + Send + Sync + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    async fn dispatch(
        &self,
        job: &Job,
        worker: &Worker,
    ) -> Result<ExecutionSagaResult, ExecutionSagaDispatcherError> {
        self.executor.execute(job, worker).await
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

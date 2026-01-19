//! SagaEngine v4.0 Adapter
//!
//! Adapter that uses the saga-engine v4.0 library for saga execution.

use async_trait::async_trait;
use std::fmt::Debug;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use hodei_server_domain::saga::SagaType;

use crate::saga::port::SagaPort;
use crate::saga::port::types::{SagaExecutionId, WorkflowState};

/// Configuration for the saga-engine v4.0 adapter
#[derive(Debug, Clone, Default)]
pub struct SagaEngineV4Config {
    /// Default workflow timeout
    pub default_timeout: Duration,
    /// Maximum concurrent workflows
    pub max_concurrent_workflows: usize,
    /// Enable automatic snapshotting
    pub enable_snapshots: bool,
    /// Snapshot threshold (number of events)
    pub snapshot_threshold: u64,
}

/// Result of starting a workflow
#[derive(Debug, Clone)]
pub struct WorkflowStartResult {
    /// The execution ID
    pub execution_id: String,
    /// Current state
    pub state: WorkflowExecutionState,
    /// Timestamp when started
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// State of a workflow execution
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowExecutionState {
    /// Workflow is running
    Running,
    /// Workflow completed successfully
    Completed,
    /// Workflow failed
    Failed { error: String },
    /// Workflow was cancelled
    Cancelled { reason: String },
    /// Workflow was compensated
    Compensated { error: String },
}

/// Workflow runtime trait for saga-engine v4.0
#[async_trait]
pub trait WorkflowRuntime: Send + Sync + Debug {
    /// Start a new workflow
    async fn start_workflow(
        &self,
        input: Vec<u8>,
        idempotency_key: Option<String>,
    ) -> Result<WorkflowStartResult, io::Error>;

    /// Get workflow status
    async fn get_workflow_status(
        &self,
        execution_id: &str,
    ) -> Result<Option<WorkflowExecutionState>, io::Error>;

    /// Cancel a workflow
    async fn cancel_workflow(&self, execution_id: &str, reason: String) -> Result<(), io::Error>;

    /// Send a signal to a workflow
    async fn send_signal(
        &self,
        execution_id: &str,
        signal_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), io::Error>;
}

/// Adapter that uses saga-engine v4.0 library for saga execution.
pub struct SagaEngineV4Adapter<R: WorkflowRuntime + Send + Sync + 'static> {
    /// The workflow runtime
    runtime: Arc<R>,
    /// Configuration
    config: SagaEngineV4Config,
}

impl<R: WorkflowRuntime + Send + Sync + 'static> SagaEngineV4Adapter<R> {
    /// Create a new SagaEngineV4Adapter with runtime
    pub fn new(runtime: Arc<R>, config: Option<SagaEngineV4Config>) -> Self {
        Self {
            runtime,
            config: config.unwrap_or_default(),
        }
    }
}

#[async_trait]
impl<R: WorkflowRuntime + Send + Sync + 'static> SagaPort for SagaEngineV4Adapter<R> {
    type Error = io::Error;

    async fn start_workflow(
        &self,
        _saga_type: SagaType,
        _input: (),
        _idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        debug!("Starting workflow via SagaEngineV4Adapter");
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SagaEngineV4Adapter requires saga-specific implementation",
        ))
    }

    async fn get_workflow_state(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<WorkflowState<()>, Self::Error> {
        debug!(execution_id = %execution_id, "Getting workflow state");
        let status = self
            .runtime
            .get_workflow_status(execution_id.as_str())
            .await?;

        match status {
            Some(WorkflowExecutionState::Running) => Ok(WorkflowState::Running {
                current_step: None,
                elapsed: None,
            }),
            Some(WorkflowExecutionState::Completed) => Ok(WorkflowState::Completed {
                output: (),
                steps_executed: 0,
                duration: Duration::ZERO,
            }),
            Some(WorkflowExecutionState::Failed { error }) => Ok(WorkflowState::Failed {
                error_message: error,
                failed_step: None,
                steps_executed: 0,
                compensations_executed: 0,
            }),
            Some(WorkflowExecutionState::Cancelled { reason }) => Ok(WorkflowState::Cancelled {
                reason,
                steps_executed: 0,
            }),
            Some(WorkflowExecutionState::Compensated { error }) => Ok(WorkflowState::Compensated {
                error_message: error,
                failed_step: None,
                steps_executed: 0,
                compensations_executed: 0,
            }),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Workflow not found: {}", execution_id),
            )),
        }
    }

    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error> {
        debug!(execution_id = %execution_id, reason = %reason, "Cancelling workflow");
        self.runtime
            .cancel_workflow(execution_id.as_str(), reason)
            .await?;
        Ok(())
    }
}

/// In-memory implementation of WorkflowRuntime for testing
#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkflowRuntime {
    workflows: Arc<std::sync::Mutex<std::collections::HashMap<String, WorkflowStartResult>>>,
}

#[async_trait::async_trait]
impl WorkflowRuntime for InMemoryWorkflowRuntime {
    async fn start_workflow(
        &self,
        _input: Vec<u8>,
        _idempotency_key: Option<String>,
    ) -> Result<WorkflowStartResult, io::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let result = WorkflowStartResult {
            execution_id: id.clone(),
            state: WorkflowExecutionState::Running,
            started_at: chrono::Utc::now(),
        };

        self.workflows
            .lock()
            .unwrap()
            .insert(id.clone(), result.clone());
        Ok(result)
    }

    async fn get_workflow_status(
        &self,
        execution_id: &str,
    ) -> Result<Option<WorkflowExecutionState>, io::Error> {
        let workflows = self.workflows.lock().unwrap();
        Ok(workflows.get(execution_id).map(|r| r.state.clone()))
    }

    async fn cancel_workflow(&self, execution_id: &str, reason: String) -> Result<(), io::Error> {
        let mut workflows = self.workflows.lock().unwrap();
        if let Some(result) = workflows.get_mut(execution_id) {
            result.state = WorkflowExecutionState::Cancelled { reason };
        }
        Ok(())
    }

    async fn send_signal(
        &self,
        _execution_id: &str,
        _signal_type: &str,
        _payload: Vec<u8>,
    ) -> Result<(), io::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_runtime_start_workflow() {
        let runtime = InMemoryWorkflowRuntime::default();
        let result = runtime.start_workflow(vec![1, 2, 3], None).await.unwrap();
        assert!(!result.execution_id.is_empty());
        assert_eq!(result.state, WorkflowExecutionState::Running);
    }

    #[tokio::test]
    async fn test_in_memory_runtime_cancel_workflow() {
        let runtime = InMemoryWorkflowRuntime::default();
        let start_result = runtime.start_workflow(vec![], None).await.unwrap();
        runtime
            .cancel_workflow(&start_result.execution_id, "Test".to_string())
            .await
            .unwrap();
        let status = runtime
            .get_workflow_status(&start_result.execution_id)
            .await
            .unwrap()
            .unwrap();
        match status {
            WorkflowExecutionState::Cancelled { reason } => assert_eq!(reason, "Test"),
            _ => panic!("Expected Cancelled state"),
        }
    }
}

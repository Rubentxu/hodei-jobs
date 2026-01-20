//! SagaEngine v4.0 Adapter
//!
//! Adapter that uses the saga-engine v4.0 library for saga execution.

use async_trait::async_trait;
use saga_engine_core::workflow::{WorkflowDefinition, WorkflowState};
use serde::Serialize;
use std::fmt::Debug;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use crate::saga::port::SagaPort;
use crate::saga::port::types::SagaExecutionId;

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
    pub execution_id: SagaExecutionId,
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
        input: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<WorkflowStartResult, io::Error>;

    /// Get workflow status
    async fn get_workflow_status(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<Option<WorkflowExecutionState>, io::Error>;

    /// Cancel a workflow
    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), io::Error>;

    /// Send a signal to a workflow
    async fn send_signal(
        &self,
        execution_id: &SagaExecutionId,
        signal: String,
        payload: serde_json::Value,
    ) -> Result<(), io::Error>;
}

/// Adapter that uses saga-engine v4.0 library for saga execution.
pub struct SagaEngineV4Adapter<R, W>
where
    R: WorkflowRuntime + Send + Sync + 'static,
    W: WorkflowDefinition + Send + 'static,
{
    /// The workflow runtime
    runtime: Arc<R>,
    /// Configuration
    config: SagaEngineV4Config,
    /// Marker for workflow type
    _phantom: std::marker::PhantomData<W>,
}

impl<R, W> SagaEngineV4Adapter<R, W>
where
    R: WorkflowRuntime + Send + Sync + 'static,
    W: WorkflowDefinition + Send + 'static,
{
    /// Create a new SagaEngineV4Adapter with runtime
    pub fn new(runtime: Arc<R>, config: Option<SagaEngineV4Config>) -> Self {
        Self {
            runtime,
            config: config.unwrap_or_default(),
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<R, W> SagaPort<W> for SagaEngineV4Adapter<R, W>
where
    R: WorkflowRuntime + Send + Sync + 'static,
    W: WorkflowDefinition + Send + 'static,
{
    type Error = io::Error;

    async fn start_workflow(
        &self,
        input: W::Input,
        idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        debug!(
            "Starting workflow via SagaEngineV4Adapter for workflow type {}",
            W::TYPE_ID
        );

        // Serialize the type-safe input to JSON for the runtime
        let json_input = serde_json::to_value(&input).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to serialize input: {}", e),
            )
        })?;

        let result = self
            .runtime
            .start_workflow(json_input, idempotency_key)
            .await?;
        Ok(result.execution_id)
    }

    async fn get_workflow_state(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<WorkflowState, Self::Error> {
        debug!(execution_id = %execution_id, "Getting workflow state");
        let status = self.runtime.get_workflow_status(execution_id).await?;

        match status {
            Some(WorkflowExecutionState::Running) => Ok(WorkflowState::Running {
                current_step: None,
                elapsed: None,
            }),
            Some(WorkflowExecutionState::Completed) => Ok(WorkflowState::Completed {
                output: serde_json::json!({}),
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
        self.runtime.cancel_workflow(execution_id, reason).await?;
        Ok(())
    }

    async fn send_signal(
        &self,
        execution_id: &SagaExecutionId,
        signal: String,
        payload: W::Output,
    ) -> Result<(), Self::Error> {
        debug!(execution_id = %execution_id, "Sending signal to workflow");
        let json_payload = serde_json::to_value(&payload).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to serialize payload: {}", e),
            )
        })?;
        self.runtime
            .send_signal(execution_id, signal, json_payload)
            .await?;
        Ok(())
    }

    async fn wait_for_completion(
        &self,
        execution_id: &SagaExecutionId,
        timeout: Duration,
    ) -> Result<WorkflowState, Self::Error> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        while start.elapsed() < timeout {
            match self.get_workflow_state(execution_id).await? {
                WorkflowState::Running { .. } => {
                    tokio::time::sleep(poll_interval).await;
                }
                state => return Ok(state),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("Workflow did not complete within timeout: {}", execution_id),
        ))
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
        _input: serde_json::Value,
        _idempotency_key: Option<String>,
    ) -> Result<WorkflowStartResult, io::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let execution_id = SagaExecutionId::from_string(&id);
        let result = WorkflowStartResult {
            execution_id,
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
        execution_id: &SagaExecutionId,
    ) -> Result<Option<WorkflowExecutionState>, io::Error> {
        let workflows = self.workflows.lock().unwrap();
        Ok(workflows
            .get(execution_id.as_str())
            .map(|r| r.state.clone()))
    }

    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), io::Error> {
        let mut workflows = self.workflows.lock().unwrap();
        if let Some(result) = workflows.get_mut(execution_id.as_str()) {
            result.state = WorkflowExecutionState::Cancelled { reason };
        }
        Ok(())
    }

    async fn send_signal(
        &self,
        _execution_id: &SagaExecutionId,
        _signal: String,
        _payload: serde_json::Value,
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
        let result = runtime
            .start_workflow(serde_json::json!({"test": true}), None)
            .await
            .unwrap();
        assert!(!result.execution_id.as_str().is_empty());
        assert_eq!(result.state, WorkflowExecutionState::Running);
    }

    #[tokio::test]
    async fn test_in_memory_runtime_cancel_workflow() {
        let runtime = InMemoryWorkflowRuntime::default();
        let start_result = runtime
            .start_workflow(serde_json::json!({}), None)
            .await
            .unwrap();
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

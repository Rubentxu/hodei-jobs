//! # SagaEngine - Central Workflow Execution Engine
//!
//! This module provides the [`SagaEngine`] which is the core orchestrator for
//! durable workflow execution.

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use crate::event::{EventId, EventType, HistoryEvent, SagaId};
use crate::port::Task;
use crate::workflow::{DurableWorkflow, WorkflowContext, WorkflowTypeId};

/// Configuration for the SagaEngine.
#[derive(Debug, Clone)]
pub struct SagaEngineConfig {
    /// Maximum events before forcing a snapshot.
    pub max_events_before_snapshot: u64,
    /// Default activity timeout.
    pub default_activity_timeout: Duration,
    /// Task queue for workflow tasks.
    pub workflow_task_queue: String,
}

impl Default for SagaEngineConfig {
    fn default() -> Self {
        Self {
            max_events_before_snapshot: 100,
            default_activity_timeout: Duration::from_secs(300),
            workflow_task_queue: "saga-workflows".to_string(),
        }
    }
}

impl SagaEngineConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_events_before_snapshot(mut self, n: u64) -> Self {
        self.max_events_before_snapshot = n;
        self
    }

    pub fn with_activity_timeout(mut self, timeout: Duration) -> Self {
        self.default_activity_timeout = timeout;
        self
    }
}

/// Errors from SagaEngine operations.
#[derive(Debug, Error)]
pub enum SagaEngineError {
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(SagaId),

    #[error("Workflow type not registered: {0}")]
    WorkflowTypeNotFound(&'static str),

    #[error("Invalid workflow state: {0}")]
    InvalidState(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result of a workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SagaExecutionResult {
    Completed {
        output: serde_json::Value,
        event_count: u64,
    },
    Failed {
        error: String,
        event_count: u64,
    },
    Cancelled {
        reason: String,
        event_count: u64,
    },
    Running {
        state: serde_json::Value,
    },
}

/// Workflow task for internal processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTask {
    pub saga_id: SagaId,
    pub workflow_type: String,
    pub event_id: u64,
    pub payload: Vec<u8>,
}

/// The SagaEngine orchestrates durable workflow execution.
#[derive(Debug)]
pub struct SagaEngine<E, Q, T>
where
    E: crate::port::EventStore + 'static,
    Q: crate::port::TaskQueue + 'static,
    T: crate::port::TimerStore + 'static,
{
    /// Engine configuration.
    pub config: SagaEngineConfig,
    /// Event store for persistence.
    pub event_store: Arc<E>,
    /// Task queue for activities.
    pub task_queue: Arc<Q>,
    /// Timer store for delays/timeouts.
    pub timer_store: Arc<T>,
}

impl<E, Q, T> SagaEngine<E, Q, T>
where
    E: crate::port::EventStore + 'static,
    Q: crate::port::TaskQueue + 'static,
    T: crate::port::TimerStore + 'static,
{
    /// Create a new SagaEngine with the given adapters.
    pub fn new(
        config: SagaEngineConfig,
        event_store: Arc<E>,
        task_queue: Arc<Q>,
        timer_store: Arc<T>,
    ) -> Self {
        Self {
            config,
            event_store,
            task_queue,
            timer_store,
        }
    }

    /// Get the event store reference.
    pub fn event_store(&self) -> &E {
        &*self.event_store
    }

    /// Get the task queue reference.
    pub fn task_queue(&self) -> &Q {
        &*self.task_queue
    }

    /// Get the timer store reference.
    pub fn timer_store(&self) -> &T {
        &*self.timer_store
    }

    /// Reconstruct workflow context from history.
    pub async fn reconstruct_context(
        &self,
        history: &[HistoryEvent],
        workflow_type: &str,
    ) -> Result<WorkflowContext, SagaEngineError> {
        let mut context = WorkflowContext::new(
            history.first().unwrap().saga_id.clone(),
            WorkflowTypeId::from_str(workflow_type),
            crate::workflow::WorkflowConfig::default(),
        );

        // Replay all events to reconstruct state
        for event in history {
            context.metadata.replay_count += 1;
            context.metadata.last_activity_at = Some(chrono::Utc::now());

            match event.event_type {
                EventType::ActivityTaskCompleted => {
                    context.advance_step();
                    context.metadata.tasks_executed += 1;
                    if let Some(output) = event.attributes.get("output") {
                        let key = format!("step_{}", context.current_step_index - 1);
                        context.set_step_output(key, output.clone());
                    }
                }
                EventType::ActivityTaskFailed => {
                    context.increment_attempt();
                    context.metadata.total_retries += 1;
                }
                EventType::WorkflowExecutionCanceled => {
                    if let Some(reason) = event.attributes.get("reason") {
                        context.mark_cancelled(reason.as_str().unwrap_or_default().to_string());
                    }
                }
                _ => {}
            }
        }

        Ok(context)
    }

    /// Get the status of a workflow.
    pub async fn get_workflow_status(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<SagaExecutionResult>, E::Error> {
        let history = self.event_store.get_history(saga_id).await?;

        if history.is_empty() {
            return Ok(None);
        }

        let last_event = history.last().unwrap();

        match last_event.event_type {
            EventType::WorkflowExecutionCompleted => {
                let output = last_event
                    .attributes
                    .get("output")
                    .cloned()
                    .unwrap_or_default();
                Ok(Some(SagaExecutionResult::Completed {
                    output,
                    event_count: history.len() as u64,
                }))
            }
            EventType::WorkflowExecutionFailed => {
                let error = last_event
                    .attributes
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                Ok(Some(SagaExecutionResult::Failed {
                    error,
                    event_count: history.len() as u64,
                }))
            }
            EventType::WorkflowExecutionCanceled => {
                let reason = last_event
                    .attributes
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Cancelled")
                    .to_string();
                Ok(Some(SagaExecutionResult::Cancelled {
                    reason,
                    event_count: history.len() as u64,
                }))
            }
            _ => Ok(Some(SagaExecutionResult::Running {
                state: serde_json::json!({
                    "event_count": history.len(),
                    "last_event_type": last_event.event_type.as_str()
                }),
            })),
        }
    }

    /// Start a new workflow execution.
    pub async fn start_workflow<W: DurableWorkflow>(
        &self,
        workflow_id: SagaId,
        input: W::Input,
    ) -> Result<SagaId, SagaEngineError> {
        // 1. Serialize input
        let input_payload = serde_json::to_value(input)
            .map_err(|e| SagaEngineError::Serialization(e.to_string()))?;

        // 2. Create WorkflowExecutionStarted event
        let event = HistoryEvent::builder()
            .event_id(EventId(1))
            .event_type(EventType::WorkflowExecutionStarted)
            .saga_id(workflow_id.clone())
            .payload(serde_json::json!({
                "input": input_payload,
                "workflow_type": W::TYPE_ID
            }))
            .build();

        // 3. Persist event
        self.event_store
            .append_events(&workflow_id, 0, &[event])
            .await
            .map_err(|e| {
                SagaEngineError::InvalidState(format!("Failed to persist start event: {:?}", e))
            })?;

        // 4. Schedule first workflow task
        let task_payload = serde_json::to_vec(&WorkflowTask {
            saga_id: workflow_id.clone(),
            workflow_type: W::TYPE_ID.to_string(),
            event_id: 1,
            payload: vec![],
        })
        .map_err(|e| SagaEngineError::Serialization(e.to_string()))?;

        let task = Task::new(
            "workflow-execute".to_string(),
            workflow_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            task_payload,
        )
        .with_queue(self.config.workflow_task_queue.clone());

        self.task_queue
            .publish(&task, &self.config.workflow_task_queue)
            .await
            .map_err(|e| {
                SagaEngineError::InvalidState(format!("Failed to schedule start task: {:?}", e))
            })?;

        Ok(workflow_id)
    }

    /// Resume a workflow to process a task or replay.
    pub async fn resume_workflow<W: DurableWorkflow>(
        &self,
        workflow: &W,
        saga_id: SagaId,
    ) -> Result<SagaExecutionResult, SagaEngineError> {
        // 1. Fetch history
        let history = self.event_store.get_history(&saga_id).await.map_err(|e| {
            SagaEngineError::InvalidState(format!("Failed to fetch history: {:?}", e))
        })?;

        if history.is_empty() {
            return Err(SagaEngineError::WorkflowNotFound(saga_id));
        }

        // 2. Reconstruct context
        let mut context = self.reconstruct_context(&history, W::TYPE_ID).await?;
        let current_version = history.len() as u64;

        // 3. Extract input from start event
        let start_event = history
            .first()
            .ok_or(SagaEngineError::InvalidState("Empty history".into()))?;
        let input_value =
            start_event
                .attributes
                .get("input")
                .ok_or(SagaEngineError::InvalidState(
                    "Missing input in start event".into(),
                ))?;

        // Deserialize input
        let input: W::Input = serde_json::from_value(input_value.clone()).map_err(|e| {
            SagaEngineError::Serialization(format!("Invalid workflow input: {}", e))
        })?;

        // 4. Run workflow
        context.current_step_index = 0;
        let result = workflow.run(&mut context, input).await;

        // Handle the result
        match result {
            Ok(output) => {
                // Workflow Completed
                let serialized_output = serde_json::to_value(&output)
                    .map_err(|e| SagaEngineError::Serialization(e.to_string()))?;

                let event = HistoryEvent::builder()
                    .event_id(EventId(current_version + 1))
                    .event_type(EventType::WorkflowExecutionCompleted)
                    .saga_id(saga_id.clone())
                    .payload(serde_json::json!({ "output": serialized_output }))
                    .build();

                self.event_store
                    .append_events(&saga_id, current_version, &[event])
                    .await
                    .map_err(|e| {
                        SagaEngineError::InvalidState(format!("Failed to complete workflow: {}", e))
                    })?;

                Ok(SagaExecutionResult::Completed {
                    output: serialized_output,
                    event_count: current_version + 1,
                })
            }
            Err(e) => {
                // Check if paused via context
                if let Some(paused) = context.pending_activity.take() {
                    // Workflow Paused - Schedule Activity
                    let event = HistoryEvent::builder()
                        .event_id(EventId(current_version + 1))
                        .event_type(EventType::ActivityTaskScheduled)
                        .saga_id(saga_id.clone())
                        .payload(serde_json::json!({
                            "activity_type": paused.activity_type,
                            "activity_id": paused.activity_id,
                            "input": paused.input,
                        }))
                        .build();

                    self.event_store
                        .append_events(&saga_id, current_version, &[event])
                        .await
                        .map_err(|e| {
                            SagaEngineError::InvalidState(format!(
                                "Failed to persist pause event: {}",
                                e
                            ))
                        })?;

                    let task_payload = serde_json::to_vec(&WorkflowTask {
                        saga_id: saga_id.clone(),
                        workflow_type: W::TYPE_ID.to_string(),
                        event_id: current_version + 1,
                        payload: serde_json::to_vec(&paused.input)
                            .map_err(|e| SagaEngineError::Serialization(e.to_string()))?,
                    })
                    .map_err(|e| SagaEngineError::Serialization(e.to_string()))?;

                    let task = Task::new(
                        paused.activity_type.clone(),
                        saga_id.clone(),
                        paused.activity_id.clone(),
                        task_payload,
                    )
                    .with_queue(self.config.workflow_task_queue.clone());

                    self.task_queue
                        .publish(&task, &self.config.workflow_task_queue)
                        .await
                        .map_err(|e| {
                            SagaEngineError::InvalidState(format!(
                                "Failed to schedule activity task: {}",
                                e
                            ))
                        })?;

                    Ok(SagaExecutionResult::Running {
                        state: serde_json::json!({
                            "status": "paused",
                            "waiting_for": paused.activity_type
                        }),
                    })
                } else {
                    // Actual failure
                    let event = HistoryEvent::builder()
                        .event_id(EventId(current_version + 1))
                        .event_type(EventType::WorkflowExecutionFailed)
                        .saga_id(saga_id.clone())
                        .payload(serde_json::json!({ "error": e.to_string() }))
                        .build();

                    self.event_store
                        .append_events(&saga_id, current_version, &[event])
                        .await
                        .map_err(|e| {
                            SagaEngineError::InvalidState(format!("Failed to fail workflow: {}", e))
                        })?;

                    Ok(SagaExecutionResult::Failed {
                        error: e.to_string(),
                        event_count: current_version + 1,
                    })
                }
            }
        }
    }

    /// Dynamic resume logic to support DynDurableWorkflow trait objects
    pub async fn resume_workflow_dyn(
        &self,
        workflow: &dyn crate::workflow::registry::DynDurableWorkflow,
        saga_id: SagaId,
    ) -> Result<SagaExecutionResult, SagaEngineError> {
        // 1. Fetch history
        let history = self.event_store.get_history(&saga_id).await.map_err(|e| {
            SagaEngineError::InvalidState(format!("Failed to fetch history: {:?}", e))
        })?;

        if history.is_empty() {
            return Err(SagaEngineError::WorkflowNotFound(saga_id));
        }

        // 2. Extract workflow type from history
        let start_event = history
            .first()
            .ok_or(SagaEngineError::InvalidState("Empty history".into()))?;
        let workflow_type_str = start_event
            .attributes
            .get("workflow_type")
            .and_then(|s| s.as_str())
            .ok_or(SagaEngineError::InvalidState(
                "Missing workflow_type".into(),
            ))?;

        // 3. Reconstruct context
        let mut context = self
            .reconstruct_context(&history, workflow_type_str)
            .await?;
        let current_version = history.len() as u64;

        // 4. Extract input
        let input_value = start_event
            .attributes
            .get("input")
            .ok_or(SagaEngineError::InvalidState(
                "Missing input in start event".into(),
            ))?
            .clone();

        // 5. Run workflow
        context.current_step_index = 0;
        let result = workflow.run_dyn(&mut context, input_value).await;

        // 6. Handle result
        match result {
            Ok(output) => {
                let serialized_output = output;
                let event = HistoryEvent::builder()
                    .event_id(EventId(current_version + 1))
                    .event_type(EventType::WorkflowExecutionCompleted)
                    .saga_id(saga_id.clone())
                    .payload(serde_json::json!({ "output": serialized_output }))
                    .build();

                self.event_store
                    .append_events(&saga_id, current_version, &[event])
                    .await
                    .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;
                Ok(SagaExecutionResult::Completed {
                    output: serialized_output,
                    event_count: current_version + 1,
                })
            }
            Err(e) => {
                if let Some(paused) = context.pending_activity.take() {
                    let event = HistoryEvent::builder()
                        .event_id(EventId(current_version + 1))
                        .event_type(EventType::ActivityTaskScheduled)
                        .saga_id(saga_id.clone())
                        .payload(serde_json::json!({
                            "activity_type": paused.activity_type,
                            "activity_id": paused.activity_id,
                            "input": paused.input,
                        }))
                        .build();

                    self.event_store
                        .append_events(&saga_id, current_version, &[event])
                        .await
                        .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;
                    let task_payload = serde_json::to_vec(&WorkflowTask {
                        saga_id: saga_id.clone(),
                        workflow_type: workflow_type_str.to_string(),
                        event_id: current_version + 1,
                        payload: serde_json::to_vec(&paused.input).unwrap(),
                    })
                    .unwrap();

                    let task = Task::new(
                        paused.activity_type.clone(),
                        saga_id.clone(),
                        paused.activity_id.clone(),
                        task_payload,
                    )
                    .with_queue(self.config.workflow_task_queue.clone());
                    self.task_queue
                        .publish(&task, &self.config.workflow_task_queue)
                        .await
                        .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;

                    Ok(SagaExecutionResult::Running {
                        state: serde_json::json!({ "status": "paused", "waiting_for": paused.activity_type }),
                    })
                } else {
                    let event = HistoryEvent::builder()
                        .event_id(EventId(current_version + 1))
                        .event_type(EventType::WorkflowExecutionFailed)
                        .saga_id(saga_id.clone())
                        .payload(serde_json::json!({ "error": e.to_string() }))
                        .build();
                    self.event_store
                        .append_events(&saga_id, current_version, &[event])
                        .await
                        .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;
                    Ok(SagaExecutionResult::Failed {
                        error: e.to_string(),
                        event_count: current_version + 1,
                    })
                }
            }
        }
    }

    /// Complete an activity task and resume workflow.
    pub async fn complete_activity_task(
        &self,
        saga_id: SagaId,
        workflow_type: String,
        result: Result<serde_json::Value, String>,
    ) -> Result<(), SagaEngineError> {
        let history = self
            .event_store
            .get_history(&saga_id)
            .await
            .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;
        let current_version = history.len() as u64;

        let event = match &result {
            Ok(output) => HistoryEvent::builder()
                .event_id(EventId(current_version + 1))
                .event_type(EventType::ActivityTaskCompleted)
                .saga_id(saga_id.clone())
                .payload(serde_json::json!({ "output": output }))
                .build(),
            Err(e) => HistoryEvent::builder()
                .event_id(EventId(current_version + 1))
                .event_type(EventType::ActivityTaskFailed)
                .saga_id(saga_id.clone())
                .payload(serde_json::json!({ "error": e }))
                .build(),
        };

        self.event_store
            .append_events(&saga_id, current_version, &[event])
            .await
            .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;

        let task_payload = serde_json::to_vec(&WorkflowTask {
            saga_id: saga_id.clone(),
            workflow_type: workflow_type,
            event_id: current_version + 1,
            payload: vec![],
        })
        .map_err(|e| SagaEngineError::Serialization(e.to_string()))?;

        // Schedule workflow task
        let task = Task::new(
            "workflow-execute".to_string(),
            saga_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            task_payload,
        )
        .with_queue(self.config.workflow_task_queue.clone());

        self.task_queue
            .publish(&task, &self.config.workflow_task_queue)
            .await
            .map_err(|e| SagaEngineError::InvalidState(format!("{:?}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::SagaId;
    use std::time::Duration;

    #[test]
    fn test_saga_engine_config_defaults() {
        let config = SagaEngineConfig::default();
        assert_eq!(config.max_events_before_snapshot, 100);
        assert_eq!(config.default_activity_timeout, Duration::from_secs(300));
        assert_eq!(config.workflow_task_queue, "saga-workflows");
    }

    #[test]
    fn test_saga_engine_config_builder() {
        let config = SagaEngineConfig::new()
            .with_max_events_before_snapshot(50)
            .with_activity_timeout(Duration::from_secs(600));

        assert_eq!(config.max_events_before_snapshot, 50);
        assert_eq!(config.default_activity_timeout, Duration::from_secs(600));
    }

    #[test]
    fn test_saga_execution_result_variants() {
        let completed = SagaExecutionResult::Completed {
            output: serde_json::json!({"result": "test"}),
            event_count: 10,
        };
        assert!(matches!(completed, SagaExecutionResult::Completed { .. }));

        let failed = SagaExecutionResult::Failed {
            error: "test error".to_string(),
            event_count: 5,
        };
        assert!(matches!(failed, SagaExecutionResult::Failed { .. }));

        let cancelled = SagaExecutionResult::Cancelled {
            reason: "user request".to_string(),
            event_count: 3,
        };
        assert!(matches!(cancelled, SagaExecutionResult::Cancelled { .. }));

        let running = SagaExecutionResult::Running {
            state: serde_json::json!({"status": "running"}),
        };
        assert!(matches!(running, SagaExecutionResult::Running { .. }));
    }

    #[test]
    fn test_saga_engine_error_variants() {
        let not_found = SagaEngineError::WorkflowNotFound(SagaId("test-123".to_string()));
        assert!(not_found.to_string().contains("test-123"));

        let type_not_found = SagaEngineError::WorkflowTypeNotFound("test-workflow");
        assert!(type_not_found.to_string().contains("test-workflow"));

        let invalid_state = SagaEngineError::InvalidState("invalid".to_string());
        assert!(invalid_state.to_string().contains("invalid"));

        let serialization = SagaEngineError::Serialization("error".to_string());
        assert!(serialization.to_string().contains("error"));
    }
}

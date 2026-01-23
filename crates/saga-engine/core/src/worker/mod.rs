//! # Worker - Task Queue Polling and Execution
//!
//! This module provides the [`Worker`] for polling task queues and executing
//! workflow and activity tasks.

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;

use crate::activity_registry::ActivityRegistry;
use crate::port::{ConsumerConfig, TaskMessage, TaskQueue, TimerStore};
use crate::saga_engine::{SagaEngine, WorkflowTask};
use crate::workflow::registry::WorkflowRegistry;

/// Worker configuration.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Consumer name for task queue.
    pub consumer_name: String,
    /// Queue group for load balancing.
    pub queue_group: String,
    /// Maximum concurrent tasks.
    pub max_concurrent: u64,
    /// Poll interval.
    pub poll_interval: Duration,
    /// Ack wait timeout.
    pub ack_wait: Duration,
    /// Enable graceful shutdown.
    pub graceful_shutdown_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "saga-worker".to_string(),
            queue_group: "saga-workers".to_string(),
            max_concurrent: 10,
            poll_interval: Duration::from_millis(100),
            ack_wait: Duration::from_secs(30),
            graceful_shutdown_timeout: Duration::from_secs(30),
        }
    }
}

impl WorkerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_consumer_name(mut self, name: impl Into<String>) -> Self {
        self.consumer_name = name.into();
        self
    }

    pub fn with_queue_group(mut self, group: impl Into<String>) -> Self {
        self.queue_group = group.into();
        self
    }

    pub fn with_max_concurrent(mut self, n: u64) -> Self {
        self.max_concurrent = n;
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }
}

/// Worker errors.
#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Task processing failed: {0}")]
    TaskFailed(String),

    #[error("Task acknowledgment failed: {0}")]
    AckFailed(String),

    #[error("Worker shutdown timeout")]
    ShutdownTimeout,

    #[error("Task not found: {0}")]
    TaskNotFound(String),
}

/// Result of task processing.
#[derive(Debug, Clone)]
pub enum TaskProcessingResult {
    Completed,
    Failed(String),
    RetryRequired(String),
    Terminated,
}

/// A worker that polls task queues and processes tasks.
pub struct Worker<E, Q, T>
where
    E: crate::port::EventStore + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    /// Worker configuration.
    config: WorkerConfig,
    /// The saga engine for task processing.
    saga_engine: Arc<SagaEngine<E, Q, T>>,
    /// Registry for activities.
    activity_registry: Arc<ActivityRegistry>,
    /// Registry for workflows.
    workflow_registry: Arc<WorkflowRegistry>,
    /// Flag to control worker shutdown.
    running: Arc<AtomicBool>,
    /// Consumer configuration.
    consumer_config: ConsumerConfig,
}

impl<E, Q, T> Debug for Worker<E, Q, T>
where
    E: crate::port::EventStore + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("config", &self.config)
            .field("running", &self.running.load(Ordering::SeqCst))
            .finish()
    }
}

impl<E, Q, T> Worker<E, Q, T>
where
    E: crate::port::EventStore + 'static,
    Q: TaskQueue + 'static,
    T: TimerStore + 'static,
{
    /// Create a new worker.
    pub fn new(
        config: WorkerConfig,
        saga_engine: Arc<SagaEngine<E, Q, T>>,
        activity_registry: Arc<ActivityRegistry>,
        workflow_registry: Arc<WorkflowRegistry>,
    ) -> Self {
        let consumer_config = ConsumerConfig {
            name: config.consumer_name.clone(),
            queue_group: config.queue_group.clone(),
            max_in_flight: config.max_concurrent,
            ack_wait: config.ack_wait,
            max_deliver: 3,
            enable_dlq: true,
        };

        Self {
            config,
            saga_engine,
            activity_registry,
            workflow_registry,
            running: Arc::new(AtomicBool::new(true)),
            consumer_config,
        }
    }

    /// Get the task queue reference.
    pub fn task_queue(&self) -> &Q {
        &*self.saga_engine.task_queue()
    }

    /// Start the worker in the current task.
    pub async fn start(&self) -> Result<(), WorkerError> {
        self.running.store(true, Ordering::SeqCst);

        // Ensure consumer exists
        let queue = self.task_queue();
        if let Err(e) = queue
            .ensure_consumer(&self.config.consumer_name, &self.consumer_config)
            .await
        {
            return Err(WorkerError::TaskFailed(format!(
                "Consumer creation failed: {:?}",
                e
            )));
        }

        // Main polling loop
        while self.running.load(Ordering::SeqCst) {
            let messages = queue
                .fetch(&self.config.consumer_name, 10, self.config.poll_interval)
                .await
                .map_err(|e| WorkerError::TaskFailed(format!("Task fetch failed: {:?}", e)))?;

            for message in messages {
                self.process_message(&message).await;
            }
        }

        Ok(())
    }

    /// Process a single task message.
    async fn process_message(&self, message: &TaskMessage) {
        let task_type = &message.task.task_type;
        let task = &message.task;

        let result = if task_type == "workflow-execute" {
            self.handle_workflow_task(task).await
        } else {
            self.handle_activity_task(task).await
        };

        match result {
            Ok(_) => {
                if let Err(e) = self.task_queue().ack(&message.message_id).await {
                    tracing::error!("Failed to ack message {}: {:?}", message.message_id, e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to process task {}: {:?}", task.task_id, e);
                // Nak logic (simple for now)
                let _ = self.task_queue().nak(&message.message_id, None).await;
            }
        }
    }

    async fn handle_workflow_task(
        &self,
        task: &crate::port::task_queue::Task,
    ) -> Result<(), WorkerError> {
        // Deserialize internal workflow task (clone payload to avoid lifetime issues)
        let payload = task.payload.clone();
        let inner_task: WorkflowTask = serde_json::from_slice(&payload).map_err(|e| {
            WorkerError::TaskFailed(format!("Invalid workflow task payload: {}", e))
        })?;

        // 🔍 DEBUG: Log workflow lookup attempt
        tracing::debug!(
            "🔍 [Worker] Looking for workflow type: '{}' (saga_id: {})",
            inner_task.workflow_type,
            inner_task.saga_id
        );
        tracing::debug!(
            "🔍 [Worker] Registry has {} workflows: {:?}",
            self.workflow_registry.len(),
            self.workflow_registry.workflow_keys()
        );

        // Lookup workflow
        let workflow = self
            .workflow_registry
            .get_workflow(&inner_task.workflow_type)
            .ok_or_else(|| {
                tracing::error!(
                    "❌ [Worker] Workflow '{}' not found in registry! Available workflows: {:?}",
                    inner_task.workflow_type,
                    self.workflow_registry.workflow_keys()
                );
                WorkerError::TaskFailed(format!(
                    "Workflow type not found: {}",
                    inner_task.workflow_type
                ))
            })?;

        tracing::debug!(
            "✅ [Worker] Found workflow '{}', resuming saga {}",
            inner_task.workflow_type,
            inner_task.saga_id
        );

        // Resume the workflow
        self.saga_engine
            .resume_workflow_dyn(workflow.as_ref(), inner_task.saga_id)
            .await
            .map_err(|e| WorkerError::TaskFailed(format!("Saga execution failed: {:?}", e)))?;

        Ok(())
    }

    async fn handle_activity_task(
        &self,
        task: &crate::port::task_queue::Task,
    ) -> Result<(), WorkerError> {
        // Lookup activity
        let activity = self
            .activity_registry
            .get_activity(&task.task_type)
            .ok_or_else(|| {
                WorkerError::TaskNotFound(format!("Activity type not found: {}", task.task_type))
            })?;

        // Inner payload is `WorkflowTask` struct?
        // In `resume_workflow` (Step 155), I created `WorkflowTask` where payload is `paused.input`.
        // Wait, `WorkflowTask` struct fields: `saga_id`, `workflow_type`, `event_id`, `payload`.
        // I re-wrapped the activity input inside `WorkflowTask`'s payload.
        // So `task.payload` -> `WorkflowTask` -> `inner.payload` (Activity Input).

        // Deserialize internal wrapper (clone payload to avoid lifetime issues)
        let payload = task.payload.clone();
        let inner_task: WorkflowTask = serde_json::from_slice(&payload)
            .map_err(|e| WorkerError::TaskFailed(format!("Invalid activity wrapper: {}", e)))?;

        let input_value: serde_json::Value = serde_json::from_slice(&inner_task.payload)
            .map_err(|e| WorkerError::TaskFailed(format!("Invalid activity input: {}", e)))?;

        // Execute Activity
        let result = activity.execute_dyn(input_value).await;

        // Handle Result: we need to append event to history and WAKE UP workflow.
        // How to wake up?
        // Enqueue generic "workflow-execute" task for the saga?
        // Yes.
        // BUT `SagaEngine` should handle "Activity Completion" logic to be consistent (persistence + scheduling).
        //
        // I'll call `self.saga_engine.record_activity_completion(...)` or similar.
        // Since that method doesn't exist yet, I'll inline the logic here or call a future method.
        // Inline logic for now:
        // 1. Append Event (Completed/Failed).
        // 2. Schedule "workflow-execute" task.

        // I will assume `SagaEngine` has `record_activity_result` method.
        /*
        self.saga_engine.record_activity_result(
             &inner_task.saga_id,
             &inner_task.workflow_type,
             result
        ).await...
        */

        // Since I can't change SagaEngine in this tool call, I will implement a placeholder or raw logic.
        // I'll use `record_activity_result` and then implement it in SagaEngine.

        // Wait, `SagaEngine` logic needs `workflow_type` to schedule the next task?
        // Yes, `WorkflowTask` needs `workflow_type`.
        // We have it in `inner_task.workflow_type`.

        // So:
        /*
        let completion_event = match result {
             Ok(out) => HistoryEvent::builder().event_type(EventType::ActivityTaskCompleted)...,
             Err(e) => HistoryEvent::builder().event_type(EventType::ActivityTaskFailed)...
        };
        store.append...

        let resume_task = WorkflowTask { ... };
        queue.publish(resume_task...);
        */

        // This logic is better placed in `SagaEngine`.
        // I'll call `self.saga_engine.complete_activity_task`.

        let output = match result {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        };

        self.saga_engine
            .complete_activity_task(inner_task.saga_id, inner_task.workflow_type, output)
            .await
            .map_err(|e| {
                WorkerError::TaskFailed(format!("Failed to complete activity: {:?}", e))
            })?;

        Ok(())
    }

    /// Signal the worker to stop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Wait for the worker to stop gracefully.
    pub async fn shutdown(&self) -> Result<(), WorkerError> {
        self.stop();

        // Wait for graceful shutdown timeout
        tokio::time::timeout(self.config.graceful_shutdown_timeout, async {
            while self.running.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| WorkerError::ShutdownTimeout)
    }

    /// Check if the worker is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Task handler trait for processing specific task types.
#[async_trait]
pub trait TaskHandler: Send + Sync + Debug + 'static {
    /// Get the task type this handler supports.
    fn task_type(&self) -> &'static str;

    /// Process a task and return the result.
    async fn handle(&self, payload: &[u8]) -> Result<TaskProcessingResult, WorkerError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_worker_config_defaults() {
        let config = WorkerConfig::default();
        assert_eq!(config.consumer_name, "saga-worker");
        assert_eq!(config.queue_group, "saga-workers");
        assert_eq!(config.max_concurrent, 10);
        assert_eq!(config.poll_interval, Duration::from_millis(100));
    }

    #[test]
    fn test_worker_config_builder() {
        let config = WorkerConfig::new()
            .with_consumer_name("my-worker")
            .with_queue_group("my-group")
            .with_max_concurrent(20)
            .with_poll_interval(Duration::from_millis(500));

        assert_eq!(config.consumer_name, "my-worker");
        assert_eq!(config.queue_group, "my-group");
        assert_eq!(config.max_concurrent, 20);
        assert_eq!(config.poll_interval, Duration::from_millis(500));
    }

    #[test]
    fn test_task_processing_result_variants() {
        let completed = TaskProcessingResult::Completed;
        assert!(matches!(completed, TaskProcessingResult::Completed));

        let failed = TaskProcessingResult::Failed("error".to_string());
        assert!(matches!(failed, TaskProcessingResult::Failed(_)));

        let retry = TaskProcessingResult::RetryRequired("retry reason".to_string());
        assert!(matches!(retry, TaskProcessingResult::RetryRequired(_)));

        let terminated = TaskProcessingResult::Terminated;
        assert!(matches!(terminated, TaskProcessingResult::Terminated));
    }

    #[test]
    fn test_worker_error_variants() {
        let task_failed = WorkerError::TaskFailed("error".to_string());
        assert!(task_failed.to_string().contains("error"));

        let ack_failed = WorkerError::AckFailed("ack error".to_string());
        assert!(ack_failed.to_string().contains("ack"));

        let not_found = WorkerError::TaskNotFound("task-id".to_string());
        assert!(not_found.to_string().contains("task-id"));

        let timeout = WorkerError::ShutdownTimeout;
        assert!(timeout.to_string().contains("timeout"));
    }
}

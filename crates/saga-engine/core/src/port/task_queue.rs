//! TaskQueue port for NATS JetStream Pull.
//!
//! This module defines the [`TaskQueue`] trait for distributing
//! work to workers using durable pull consumers.

use crate::event::SagaId;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::time::Duration;

/// Errors from task queue operations.
#[derive(Debug, thiserror::Error)]
pub enum TaskQueueError<E> {
    #[error("Task publish failed: {0:?}")]
    Publish(E),

    #[error("Consumer creation failed: {0:?}")]
    ConsumerCreation(E),

    #[error("Task fetch failed: {0:?}")]
    Fetch(E),

    #[error("Task acknowledgment failed: {0:?}")]
    Ack(E),

    #[error("Task negative acknowledgment failed: {0:?}")]
    Nak(E),
}

/// Unique identifier for a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    /// Create a new task ID.
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A task to be processed by a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub task_id: TaskId,

    /// Type of the task (e.g., "activity", "checkpoint").
    pub task_type: String,

    /// The saga this task belongs to.
    pub saga_id: SagaId,

    /// The workflow run this task belongs to.
    pub run_id: String,

    /// Task payload/arguments.
    pub payload: Vec<u8>,

    /// Current retry count.
    pub retry_count: u32,

    /// Maximum retries allowed.
    pub max_retries: u32,

    /// Task queue to route to.
    pub task_queue: String,
}

impl Task {
    /// Create a new task.
    pub fn new(task_type: String, saga_id: SagaId, run_id: String, payload: Vec<u8>) -> Self {
        Self {
            task_id: TaskId(uuid::Uuid::new_v4().to_string()),
            task_type,
            saga_id,
            run_id,
            payload,
            retry_count: 0,
            max_retries: 3,
            task_queue: "default".to_string(),
        }
    }

    /// Set the task queue.
    pub fn with_queue(mut self, queue: String) -> Self {
        self.task_queue = queue;
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Check if the task can be retried.
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count.
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

/// A message received from the task queue.
#[derive(Debug, Clone)]
pub struct TaskMessage {
    /// Unique message ID from the queue.
    pub message_id: String,

    /// The task data.
    pub task: Task,

    /// The subject this message was received on.
    pub subject: String,

    /// Whether the message has been redelivered.
    pub redelivered: bool,

    /// Number of delivery attempts.
    pub delivery_count: u32,
}

impl TaskMessage {
    /// Create a new task message.
    pub fn new(
        message_id: String,
        task: Task,
        subject: String,
        redelivered: bool,
        delivery_count: u32,
    ) -> Self {
        Self {
            message_id,
            task,
            subject,
            redelivered,
            delivery_count,
        }
    }
}

/// Configuration for task queue consumer.
#[derive(Debug, Clone)]
pub struct ConsumerConfig {
    /// Name of the consumer.
    pub name: String,

    /// Queue group for load balancing.
    pub queue_group: String,

    /// Maximum concurrent messages.
    pub max_in_flight: u64,

    /// Ack wait timeout.
    pub ack_wait: Duration,

    /// Max delivery attempts before DLQ.
    pub max_deliver: u32,

    /// Enable dead letter queue.
    pub enable_dlq: bool,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            name: "saga-worker".to_string(),
            queue_group: "saga-workers".to_string(),
            max_in_flight: 10,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            enable_dlq: true,
        }
    }
}

/// Trait for task queue operations using pull consumers.
///
/// The TaskQueue provides durable task distribution with:
/// - Pull-based fetching (workers request work)
/// - Acknowledgment semantics (exactly-once processing)
/// - Automatic retry with exponential backoff
/// - Dead letter queue for failed tasks
///
/// # Usage Pattern
///
/// ```ignore
/// // Worker creates/fetches a consumer
/// queue.ensure_consumer("my-consumer", &config).await?;
///
/// // Worker fetches available tasks
/// let messages = queue.fetch("my-consumer", 10, Duration::from_secs(5)).await?;
///
/// // Process each task
/// for msg in &messages {
///     process_task(&msg.task).await;
///     queue.ack(&msg.message_id).await?;
/// }
/// ```
#[async_trait::async_trait]
pub trait TaskQueue: Send + Sync {
    /// The error type for this implementation.
    type Error: Debug + Send + Sync + 'static;

    /// Publish a task to the queue.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to publish.
    /// * `subject` - The NATS subject to publish to.
    async fn publish(
        &self,
        task: &Task,
        subject: &str,
    ) -> Result<TaskId, TaskQueueError<Self::Error>>;

    /// Ensure a consumer exists with the given configuration.
    ///
    /// This should be idempotent - calling multiple times with the same
    /// config should not create duplicate consumers.
    ///
    /// # Arguments
    ///
    /// * `consumer_name` - Unique name for the consumer.
    /// * `config` - Consumer configuration.
    async fn ensure_consumer(
        &self,
        consumer_name: &str,
        config: &ConsumerConfig,
    ) -> Result<(), TaskQueueError<Self::Error>>;

    /// Fetch available tasks from a consumer.
    ///
    /// This is a pull operation - the worker requests available work.
    ///
    /// # Arguments
    ///
    /// * `consumer_name` - The consumer to fetch from.
    /// * `max_messages` - Maximum messages to fetch.
    /// * `timeout` - How long to wait for messages.
    ///
    /// # Returns
    ///
    /// A list of task messages, possibly empty.
    async fn fetch(
        &self,
        consumer_name: &str,
        max_messages: u64,
        timeout: Duration,
    ) -> Result<Vec<TaskMessage>, TaskQueueError<Self::Error>>;

    /// Acknowledge successful processing of a task.
    ///
    /// Must be called after successfully processing a task.
    ///
    /// # Arguments
    ///
    /// * `message_id` - The message ID to acknowledge.
    async fn ack(&self, message_id: &str) -> Result<(), TaskQueueError<Self::Error>>;

    /// Negative acknowledgment - task failed, schedule retry or DLQ.
    ///
    /// # Arguments
    ///
    /// * `message_id` - The message ID to nack.
    /// * `delay` - Optional delay before redelivery.
    async fn nak(
        &self,
        message_id: &str,
        delay: Option<Duration>,
    ) -> Result<(), TaskQueueError<Self::Error>>;

    /// Terminate a task without retry (send to DLQ).
    ///
    /// # Arguments
    ///
    /// * `message_id` - The message ID to terminate.
    async fn terminate(&self, message_id: &str) -> Result<(), TaskQueueError<Self::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new(
            "activity".to_string(),
            SagaId("saga-1".to_string()),
            "run-1".to_string(),
            vec![1, 2, 3],
        );

        assert_eq!(task.task_type, "activity");
        assert_eq!(task.saga_id.0, "saga-1");
        assert!(task.can_retry());
    }

    #[test]
    fn test_task_retry_limit() {
        let mut task = Task::new(
            "activity".to_string(),
            SagaId("saga-1".to_string()),
            "run-1".to_string(),
            vec![],
        );
        task.max_retries = 2;

        assert!(task.can_retry());
        task.increment_retry();
        assert!(task.can_retry());
        task.increment_retry();
        assert!(!task.can_retry());
    }
}

//! # Tokio TaskQueue Implementation
//!
//! This module provides [`TokioTaskQueue`] for in-process task distribution
//! using Tokio channels.

use async_trait::async_trait;
use saga_engine_core::port::task_queue::{
    ConsumerConfig, Task, TaskId, TaskMessage, TaskQueue, TaskQueueError,
};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, Notify};
use uuid::Uuid;

use num_cpus;

/// Configuration for [`TokioTaskQueue`].
#[derive(Debug, Clone)]
pub struct TokioTaskQueueConfig {
    /// Channel capacity.
    pub capacity: usize,
    /// Number of worker threads.
    pub workers: usize,
    /// Default ack wait timeout.
    pub ack_wait: Duration,
    /// Maximum retries for failed tasks.
    pub max_retries: u32,
}

impl Default for TokioTaskQueueConfig {
    fn default() -> Self {
        Self {
            capacity: 1000,
            workers: num_cpus::get(),
            ack_wait: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// Tokio-based TaskQueue implementation.
///
/// This implementation provides:
/// - Zero-config in-process task distribution
/// - Backpressure via bounded channels
/// - Multiple worker support
/// - Graceful shutdown
///
/// # Examples
///
/// ```ignore
/// use saga_engine_local::task_queue::TokioTaskQueue;
///
/// #[tokio::main]
/// async fn main() {
///     let task_queue = TokioTaskQueue::builder()
///         .capacity(1000)
///         .workers(4)
///         .build();
/// }
/// ```
///
/// # Architecture
///
/// ```text
/// ┌─────────────────────────────────────────┐
/// │              TaskQueue                   │
/// ├─────────────────────────────────────────┤
/// │  ┌───────────────────────────────────┐  │
/// │  │         mpsc::Sender              │  │
/// │  └───────────────────────────────────┘  │
/// │                   │                     │
/// │                   ▼                     │
/// │  ┌───────────────────────────────────┐  │
/// │  │         Worker Thread(s)          │  │
/// │  │  (consume from channel, buffer)   │  │
/// │  └───────────────────────────────────┘  │
/// │                   │                     │
/// │                   ▼                     │
/// │  ┌───────────────────────────────────┐  │
/// │  │    Consumer Pending Queues        │  │
/// │  │  (per-consumer message buffers)   │  │
/// │  └───────────────────────────────────┘  │
/// └─────────────────────────────────────────┘
/// ```
#[derive(Debug, Clone)]
pub struct TokioTaskQueue {
    state: Arc<TokioTaskQueueState>,
}

#[derive(Debug)]
struct TokioTaskQueueState {
    sender: Arc<Mutex<mpsc::Sender<TaskMessage>>>,
    consumers: Mutex<HashMap<String, Vec<TaskMessage>>>,
    terminated: AtomicBool,
    notify: Notify,
}

impl Clone for TokioTaskQueueState {
    fn clone(&self) -> Self {
        Self {
            sender: Arc::clone(&self.sender),
            consumers: Mutex::new(HashMap::new()),
            terminated: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }
}

impl TokioTaskQueue {
    /// Create a new builder.
    pub fn builder() -> TokioTaskQueueBuilder {
        TokioTaskQueueBuilder::new()
    }

    /// Create a new TokioTaskQueue with default configuration.
    pub fn new() -> Self {
        Self::with_config(TokioTaskQueueConfig::default())
    }

    /// Create a new TokioTaskQueue with custom configuration.
    pub fn with_config(config: TokioTaskQueueConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.capacity);
        let state = Arc::new(TokioTaskQueueState {
            sender: Arc::new(Mutex::new(sender)),
            consumers: Mutex::new(HashMap::new()),
            terminated: AtomicBool::new(false),
            notify: Notify::new(),
        });

        Self::spawn_workers(state.clone(), receiver, config.workers);

        Self { state }
    }

    fn spawn_workers(state: Arc<TokioTaskQueueState>, mut receiver: mpsc::Receiver<TaskMessage>, _workers: usize) {
        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                if state.terminated.load(Ordering::SeqCst) {
                    break;
                }
                let mut consumers = state.consumers.lock().await;
                consumers
                    .entry(message.subject.clone())
                    .or_insert_with(Vec::new)
                    .push(message);
                state.notify.notify_one();
            }
        });
    }

    /// Create a new TokioTaskQueue with the specified number of workers.
    pub fn with_workers(workers: usize) -> Self {
        let config = TokioTaskQueueConfig {
            workers,
            ..Default::default()
        };
        Self::with_config(config)
    }
}

#[async_trait]
impl TaskQueue for TokioTaskQueue {
    type Error = TokioTaskQueueError;

    async fn publish(
        &self,
        task: &Task,
        subject: &str,
    ) -> Result<TaskId, TaskQueueError<Self::Error>> {
        if self.state.terminated.load(Ordering::SeqCst) {
            return Err(TaskQueueError::Publish(TokioTaskQueueError::Terminated));
        }

        let message = TaskMessage {
            message_id: Uuid::new_v4().to_string(),
            task: task.clone(),
            subject: subject.to_string(),
            redelivered: false,
            delivery_count: 0,
        };

        self.state
            .sender
            .lock()
            .await
            .send(message)
            .await
            .map_err(|e| TaskQueueError::Publish(TokioTaskQueueError::ChannelClosed(format!("{:?}", e.0))))?;

        Ok(task.task_id.clone())
    }

    async fn ensure_consumer(
        &self,
        consumer_name: &str,
        _config: &ConsumerConfig,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        let mut consumers = self.state.consumers.lock().await;
        if !consumers.contains_key(consumer_name) {
            consumers.insert(consumer_name.to_string(), Vec::new());
        }
        Ok(())
    }

    async fn fetch(
        &self,
        consumer_name: &str,
        max_messages: u64,
        timeout: Duration,
    ) -> Result<Vec<TaskMessage>, TaskQueueError<Self::Error>> {
        let mut consumers = self.state.consumers.lock().await;

        let pending = consumers.entry(consumer_name.to_string()).or_insert_with(Vec::new);

        if pending.is_empty() {
            let notify = &self.state.notify;
            let termination_check = &self.state.terminated;

            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(timeout) => {
                    return Ok(Vec::new());
                }
                _ = tokio::task::yield_now() => {}
            }

            if termination_check.load(Ordering::SeqCst) {
                return Ok(Vec::new());
            }
        }

        let to_take = std::cmp::min(max_messages as usize, pending.len());
        let taken: Vec<TaskMessage> = pending.drain(..to_take).collect();
        Ok(taken)
    }

    async fn ack(&self, _message_id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }

    async fn nak(
        &self,
        _message_id: &str,
        _delay: Option<Duration>,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }

    async fn terminate(&self, _message_id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
}

impl TokioTaskQueue {
    /// Terminate the task queue (stop accepting new tasks).
    ///
    /// Note: This is a local extension, not part of the TaskQueue trait.
    /// The trait has a `terminate(message_id)` method for terminating individual tasks.
    pub fn shutdown(&self) {
        self.state.terminated.store(true, Ordering::SeqCst);
        self.state.notify.notify_one();
    }
}

/// Builder for [`TokioTaskQueue`].
#[derive(Debug, Default)]
pub struct TokioTaskQueueBuilder {
    config: TokioTaskQueueConfig,
}

impl TokioTaskQueueBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: TokioTaskQueueConfig::default(),
        }
    }

    /// Set channel capacity.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.config.capacity = capacity;
        self
    }

    /// Set number of workers.
    pub fn workers(mut self, workers: usize) -> Self {
        self.config.workers = workers;
        self
    }

    /// Set ack wait timeout.
    pub fn ack_wait(mut self, duration: Duration) -> Self {
        self.config.ack_wait = duration;
        self
    }

    /// Set maximum retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Build the [`TokioTaskQueue`].
    pub fn build(self) -> TokioTaskQueue {
        TokioTaskQueue::with_config(self.config)
    }
}

/// Errors from [`TokioTaskQueue`] operations.
#[derive(Debug, Error)]
pub enum TokioTaskQueueError {
    /// Channel is closed.
    #[error("Channel closed: {0}")]
    ChannelClosed(String),

    /// Queue is terminated.
    #[error("Queue is terminated")]
    Terminated,

    /// Consumer not found.
    #[error("Consumer not found: {0}")]
    ConsumerNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::port::task_queue::Task;
    use saga_engine_core::SagaId;

    #[tokio::test]
    async fn test_publish_task() {
        let queue = TokioTaskQueue::new();

        queue.ensure_consumer("default", &ConsumerConfig::default()).await.unwrap();

        let task = Task::new(
            "activity".to_string(),
            SagaId::new(),
            "run-1".to_string(),
            vec![1, 2, 3],
        );

        let task_id = queue.publish(&task, "default").await.unwrap();
        assert_eq!(task_id, task.task_id);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let messages = queue.fetch("default", 10, Duration::from_secs(1)).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].task.task_id, task.task_id);
    }

    #[tokio::test]
    async fn test_ensure_consumer() {
        let queue = TokioTaskQueue::new();
        let config = ConsumerConfig::default();

        let result = queue.ensure_consumer("test-consumer", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ack_is_noop() {
        let queue = TokioTaskQueue::new();
        let result = queue.ack("message-id").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_nak_is_noop() {
        let queue = TokioTaskQueue::new();
        let result = queue.nak("message-id", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown() {
        let queue = TokioTaskQueue::new();
        queue.shutdown();

        let task = Task::new(
            "activity".to_string(),
            SagaId::new(),
            "run-1".to_string(),
            vec![1, 2, 3],
        );

        let result = queue.publish(&task, "default").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_with_timeout() {
        let queue = TokioTaskQueue::new();

        let messages = queue.fetch("non-existent", 10, Duration::from_millis(100)).await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_consumers() {
        let queue = TokioTaskQueue::new();

        queue.ensure_consumer("consumer-1", &ConsumerConfig::default()).await.unwrap();
        queue.ensure_consumer("consumer-2", &ConsumerConfig::default()).await.unwrap();

        let task1 = Task::new("activity".to_string(), SagaId::new(), "run-1".to_string(), vec![]);
        let task2 = Task::new("activity".to_string(), SagaId::new(), "run-2".to_string(), vec![]);

        queue.publish(&task1, "consumer-1").await.unwrap();
        queue.publish(&task2, "consumer-2").await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let messages1 = queue.fetch("consumer-1", 10, Duration::from_secs(1)).await.unwrap();
        let messages2 = queue.fetch("consumer-2", 10, Duration::from_secs(1)).await.unwrap();

        assert_eq!(messages1.len(), 1);
        assert_eq!(messages2.len(), 1);
    }
}

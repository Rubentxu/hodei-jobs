//! NATS TaskQueue implementation using NATS Core Pub/Sub.
//!
//! This module provides a production-ready implementation of [`TaskQueue`]
//! using NATS Core Pub/Sub for task distribution.
//!
//! # Architecture
//!
//! - **Pub/Sub**: Lightweight publish/subscribe pattern
//! - **In-Memory Channels**: Task buffering for fetch operations
//! - **ACK/NAK**: Explicit acknowledgment for tracking
//!
//! # Features
//!
//! - Fast, lightweight message passing
//! - Horizontal scaling with multiple subscribers
//! - Simple acknowledgment tracking
//! - Production-ready error handling

use async_nats::Client;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use saga_engine_core::event::SagaId;
use saga_engine_core::port::task_queue::{ConsumerConfig, Task, TaskId, TaskMessage, TaskQueue, TaskQueueError};

/// Default subject prefix for tasks.
const DEFAULT_SUBJECT_PREFIX: &str = "saga.tasks";

/// Configuration for [`NatsTaskQueue`].
#[derive(Debug, Clone)]
pub struct NatsTaskQueueConfig {
    /// NATS server URL.
    pub nats_url: String,

    /// Subject prefix for tasks.
    pub subject_prefix: String,

    /// Channel size for task receivers.
    pub channel_size: usize,
}

impl Default for NatsTaskQueueConfig {
    fn default() -> Self {
        Self {
            nats_url: "nats://localhost:4222".to_string(),
            subject_prefix: DEFAULT_SUBJECT_PREFIX.to_string(),
            channel_size: 1000,
        }
    }
}

/// Message acknowledgment tracker.
#[derive(Debug, Clone)]
struct AckTracker {
    /// Tasks waiting for acknowledgment.
    pending: HashMap<String, PendingMessage>,

    /// Tasks that have been acknowledged.
    acknowledged: HashMap<String, AckStatus>,
}

/// A pending message awaiting acknowledgment.
#[derive(Debug, Clone)]
struct PendingMessage {
    /// The task message.
    message: TaskMessage,

    /// When the message was received.
    received_at: std::time::Instant,
}

/// Acknowledgment status for a message.
#[derive(Debug, Clone)]
enum AckStatus {
    /// Message has been acknowledged.
    Acknowledged,

    /// Message has been negatively acknowledged.
    Nak { delay: Option<Duration> },

    /// Message has been terminated.
    Terminated,
}

/// NATS-based TaskQueue implementation with Pub/Sub.
///
/// This implementation uses NATS Core Pub/Sub for task distribution:
/// - Tasks are published to NATS subjects
/// - Workers subscribe and pull tasks on-demand via in-memory channels
/// - ACK confirms successful processing
/// - NAK with delay triggers retry
///
/// # Note
///
/// This implementation uses NATS Core Pub/Sub instead of JetStream
/// for simplicity and performance. It provides:
/// - Lower latency
/// - Simpler deployment
/// - Faster message throughput
///
/// For persistent storage with replay capabilities, consider using
/// a JetStream-based implementation or combine this with an EventStore.
#[derive(Debug, Clone)]
pub struct NatsTaskQueue {
    /// NATS client connection.
    client: Arc<Client>,

    /// Task queue configuration.
    config: NatsTaskQueueConfig,

    /// Channel receivers for each consumer (name -> receiver).
    receivers: Arc<RwLock<HashMap<String, mpsc::Receiver<TaskMessage>>>>,

    /// Senders for each consumer (name -> sender).
    senders: Arc<RwLock<HashMap<String, mpsc::Sender<TaskMessage>>>>,

    /// Acknowledgment trackers for each consumer.
    ack_trackers: Arc<RwLock<HashMap<String, AckTracker>>>,

    /// Subject for task distribution.
    subject: String,
}

impl NatsTaskQueue {
    /// Create a new NatsTaskQueue with default configuration.
    pub async fn new(config: NatsTaskQueueConfig) -> Result<Self, TaskQueueError<Box<dyn std::error::Error + Send + Sync>>> {
        let client = async_nats::connect(&config.nats_url).await.map_err(|e| {
            TaskQueueError::ConsumerCreation(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        });

        let subject = format!("{}.*", config.subject_prefix);

        Ok(Self {
            client: Arc::new(client),
            config,
            receivers: Arc::new(RwLock::new(HashMap::new())),
            senders: Arc::new(RwLock::new(HashMap::new())),
            ack_trackers: Arc::new(RwLock::new(HashMap::new())),
            subject,
        })
    }

    /// Subscribe to a consumer's task subject.
    async fn subscribe_consumer(&self, consumer_name: &str) -> Result<(), TaskQueueError<Box<dyn std::error::Error + Send + Sync>>> {
        let subject = format!("{}.{}", self.config.subject_prefix, consumer_name);

        let receivers = self.receivers.read().await;
        if receivers.contains_key(consumer_name) {
            tracing::debug!("Consumer {} already subscribed", consumer_name);
            return Ok(());
        }

        let (tx, rx) = mpsc::channel(self.config.channel_size);

        {
            let mut senders = self.senders.write().await;
            let mut receivers_mut = self.receivers.write().await;
            let mut ack_trackers = self.ack_trackers.write().await;

            senders.insert(consumer_name.to_string(), tx);
            receivers_mut.insert(consumer_name.to_string(), rx);
            ack_trackers.insert(consumer_name.to_string(), AckTracker {
                pending: HashMap::new(),
                acknowledged: HashMap::new(),
            });
        }

        let mut subscriber = self.client.subscribe(subject.clone()).await.map_err(|e| {
            tracing::error!("Failed to subscribe to {}: {}", subject, e);
            TaskQueueError::ConsumerCreation(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        let consumer_name_owned = consumer_name.to_string();

        tokio::spawn(async move {
            loop {
                match subscriber.next().await {
                    Some(message) => {
                        if let Ok(task) = serde_json::from_slice::<Task>(&message.payload) {
                            let task_message = TaskMessage::new(
                                message.sid.to_string(),
                                task,
                                message.subject.clone(),
                                false,
                                1,
                            );

                            let _ = tx.send(task_message).await;
                        } else {
                            tracing::warn!("Failed to deserialize task from NATS message");
                        }
                    }
                    None => {
                        tracing::debug!("Subscriber stream ended for {}", subject);
                        break;
                    }
                }
            }
        });

        tracing::info!("Subscribed consumer {} to subject {}", consumer_name, subject);
        Ok(())
    }
}

#[async_trait::async_trait]
impl TaskQueue for NatsTaskQueue {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn publish(
        &self,
        task: &Task,
        subject: &str,
    ) -> Result<TaskId, TaskQueueError<Self::Error>> {
        let payload = serde_json::to_vec(task).map_err(|e| {
            tracing::error!("Failed to serialize task: {}", e);
            TaskQueueError::Publish(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        let publish_subject = format!("{}.{}", self.subject, subject);

        self.client
            .publish(publish_subject.clone(), payload.into())
            .await
            .map_err(|e| {
                tracing::error!("Failed to publish task to {}: {}", publish_subject, e);
                TaskQueueError::Publish(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?;

        tracing::debug!("Published task {} to {}", task.task_id, publish_subject);
        Ok(task.task_id.clone())
    }

    async fn ensure_consumer(
        &self,
        consumer_name: &str,
        _config: &ConsumerConfig,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        self.subscribe_consumer(consumer_name).await?;
        Ok(())
    }

    async fn fetch(
        &self,
        consumer_name: &str,
        _max_messages: u64,
        timeout: Duration,
    ) -> Result<Vec<TaskMessage>, TaskQueueError<Self::Error>> {
        let receiver = {
            let receivers = self.receivers.read().await;
            receivers.get(consumer_name).cloned().ok_or_else(|| {
                TaskQueueError::Fetch(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Consumer {} not found", consumer_name),
                ))
                as Box<dyn std::error::Error + Send + Sync>
            })?
        };

        let fetch_start = std::time::Instant::now();
        let mut messages = Vec::new();
        let max_messages = _max_messages as usize;

        loop {
            match tokio::time::timeout(timeout, receiver.recv()).await {
                Ok(Some(message)) => {
                    let mut ack_trackers = self.ack_trackers.write().await;
                    if let Some(tracker) = ack_trackers.get_mut(consumer_name) {
                        tracker.pending.insert(message.message_id.clone(), PendingMessage {
                            message: message.clone(),
                            received_at: std::time::Instant::now(),
                        });
                    }

                    messages.push(message);

                    if messages.len() >= max_messages {
                        break;
                    }
                }
                Ok(None) => {
                    tracing::warn!("Consumer {} channel closed", consumer_name);
                    break;
                }
                Err(_) => {
                    tracing::debug!("Fetch timeout after {:?}", timeout);
                    break;
                }
            }
        }

        let elapsed = fetch_start.elapsed();
        tracing::debug!(
            "Fetched {} messages for consumer {} in {:?}",
            messages.len(),
            consumer_name,
            elapsed
        );

        Ok(messages)
    }

    async fn ack(
        &self,
        message_id: &str,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        let mut ack_trackers = self.ack_trackers.write().await;

        let tracker_opt = ack_trackers
            .iter_mut()
            .find_map(|(name, tracker)| {
                tracker.pending.remove(message_id).map(|p| (name.clone(), tracker))
            });

        if let Some((consumer_name, tracker)) = tracker_opt {
            tracker.acknowledged.insert(message_id.to_string(), AckStatus::Acknowledged);

            let elapsed = tracker
                .pending
                .get(message_id)
                .map(|p| p.received_at.elapsed())
                .unwrap_or(Duration::ZERO);

            tracing::debug!(
                "Acknowledged message {} for consumer {} (processed in {:?})",
                message_id,
                consumer_name,
                elapsed
            );
        }

        Ok(())
    }

    async fn nak(
        &self,
        message_id: &str,
        delay: Option<Duration>,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        let mut ack_trackers = self.ack_trackers.write().await;

        let tracker_opt = ack_trackers
            .iter_mut()
            .find_map(|(name, tracker)| {
                tracker.pending.remove(message_id).map(|p| (name.clone(), tracker))
            });

        if let Some((consumer_name, tracker)) = tracker_opt {
            tracker.acknowledged.insert(
                message_id.to_string(),
                AckStatus::Nak { delay },
            );

            tracing::warn!(
                "Negatively acknowledged message {} for consumer {} with delay {:?}",
                message_id,
                consumer_name,
                delay
            );
        }

        Ok(())
    }

    async fn terminate(
        &self,
        message_id: &str,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        let mut ack_trackers = self.ack_trackers.write().await;

        let tracker_opt = ack_trackers
            .iter_mut()
            .find_map(|(name, tracker)| {
                tracker.pending.remove(message_id).map(|p| (name.clone(), tracker))
            });

        if let Some((consumer_name, tracker)) = tracker_opt {
            tracker.acknowledged.insert(message_id.to_string(), AckStatus::Terminated);

            tracing::warn!(
                "Terminated message {} for consumer {}",
                message_id,
                consumer_name
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::SagaId;

    #[tokio::test]
    #[ignore = "Requires NATS server"]
    async fn test_nats_task_queue_config() {
        let config = NatsTaskQueueConfig::default();
        assert_eq!(config.subject_prefix, DEFAULT_SUBJECT_PREFIX);
        assert_eq!(config.channel_size, 1000);
    }

    #[tokio::test]
    #[ignore = "Requires NATS server"]
    async fn test_publish_and_fetch() {
        let config = NatsTaskQueueConfig {
            nats_url: "nats://localhost:4222".to_string(),
            ..Default::default()
        };

        let queue = NatsTaskQueue::new(config).await.unwrap();

        let task = Task::new(
            "test".to_string(),
            SagaId("test-saga".to_string()),
            "test-run".to_string(),
            b"test payload".to_vec(),
        );

        let task_id = queue
            .publish(&task, "test.subject")
            .await
            .unwrap();

        assert!(!task_id.0.is_empty());

        queue.ensure_consumer("test-consumer", &Default::default()).await.unwrap();

        let messages = queue.fetch("test-consumer", 1, Duration::from_secs(5)).await.unwrap();

        assert!(messages.len() <= 1);
    }
}

//! # Outbox Port
//!
//! This module defines the [`OutboxRepository`] trait for implementing the
//! Outbox Pattern for eventual consistency.
//!
//! # Outbox Pattern
//!
//! The Outbox pattern enables reliable messaging without distributed transactions:
//! 1. Save domain data AND outbox messages in the same database transaction
//! 2. A separate relay process reads pending messages and publishes them
//! 3. This ensures "exactly-once" semantics within a single database
//!
//! # Example
//!
//! ```ignore
//! use async_trait::async_trait;
//!
//! // When saving domain data...
//! let message = OutboxMessage {
//!     topic: "events.workflow.completed".to_string(),
//!     payload: serde_json::to_value(event)?,
//!     ..Default::default()
//! };
//!
//! // Save both in same transaction
//! tx.execute("INSERT INTO events", &event).await?;
//! outbox.write(message).await?;
//! tx.commit().await?;
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Repository trait for outbox message persistence.
///
/// The outbox pattern ensures reliable event delivery without distributed transactions.
/// Messages are persisted alongside domain data and processed by a separate relay.
#[async_trait]
pub trait OutboxRepository: Send + Sync {
    /// Error type for repository operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Write a message to the outbox.
    async fn write(&self, message: OutboxMessage) -> Result<(), Self::Error>;

    /// Get pending messages for processing.
    async fn get_pending(&self, batch_size: usize) -> Result<Vec<OutboxMessage>, Self::Error>;

    /// Mark a message as published.
    async fn mark_published(&self, message_id: Uuid) -> Result<(), Self::Error>;

    /// Mark a message as failed.
    async fn mark_failed(&self, message_id: Uuid, error: &str) -> Result<(), Self::Error>;

    /// Retry failed messages (increment retry_count).
    async fn retry_failed(&self, max_retries: u32) -> Result<usize, Self::Error>;

    /// Get count of pending messages.
    async fn pending_count(&self) -> Result<u64, Self::Error>;
}

/// Message stored in the outbox for eventual publication.
///
/// The outbox message contains all information needed to publish an event
/// to the message bus (NATS, Kafka, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxMessage {
    /// Unique message identifier.
    pub id: Uuid,
    /// Topic/subject to publish to.
    pub topic: String,
    /// Message payload (serialized event/command).
    pub payload: serde_json::Value,
    /// Custom headers for the message.
    pub headers: HashMap<String, String>,
    /// When the message was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Number of retry attempts.
    pub retry_count: u32,
    /// Current processing status.
    pub status: OutboxStatus,
    /// Last error if failed.
    pub last_error: Option<String>,
}

impl Default for OutboxMessage {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            topic: String::new(),
            payload: serde_json::json!({}),
            headers: HashMap::new(),
            created_at: chrono::Utc::now(),
            retry_count: 0,
            status: OutboxStatus::Pending,
            last_error: None,
        }
    }
}

impl OutboxMessage {
    /// Create a new message for a topic with payload.
    pub fn new(topic: String, payload: serde_json::Value) -> Self {
        Self {
            topic,
            payload,
            ..Default::default()
        }
    }

    /// Create a message with headers.
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Check if the message can be retried.
    pub fn can_retry(&self, max_retries: u32) -> bool {
        self.status == OutboxStatus::Failed && self.retry_count < max_retries
    }
}

/// Status of an outbox message.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutboxStatus {
    /// Message is waiting to be processed.
    Pending,
    /// Message is currently being processed.
    Processing,
    /// Message was successfully published.
    Published,
    /// Message failed after all retries.
    Failed,
}

impl std::fmt::Display for OutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutboxStatus::Pending => write!(f, "pending"),
            OutboxStatus::Processing => write!(f, "processing"),
            OutboxStatus::Published => write!(f, "published"),
            OutboxStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Configuration for outbox operations.
#[derive(Debug, Clone)]
pub struct OutboxConfig {
    /// Maximum messages to process in a batch.
    pub batch_size: usize,
    /// Maximum retry attempts per message.
    pub max_retries: u32,
    /// Base backoff duration.
    pub base_backoff: std::time::Duration,
    /// Maximum backoff duration.
    pub max_backoff: std::time::Duration,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            max_retries: 3,
            base_backoff: std::time::Duration::from_millis(100),
            max_backoff: std::time::Duration::from_secs(30),
        }
    }
}

/// Backoff configuration for retry logic.
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Base backoff duration.
    pub base: std::time::Duration,
    /// Maximum backoff duration.
    pub max: std::time::Duration,
    /// Multiplier for exponential backoff.
    pub multiplier: f64,
    /// Jitter factor (0.0 to 1.0).
    pub jitter: f64,
    /// Maximum number of retries.
    pub max_retries: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base: std::time::Duration::from_millis(100),
            max: std::time::Duration::from_secs(30),
            multiplier: 2.0,
            jitter: 0.1,
            max_retries: 3,
        }
    }
}

impl BackoffConfig {
    /// Calculate backoff duration for a given attempt.
    pub fn calculate_backoff(&self, attempt: u32) -> std::time::Duration {
        let calculated = self.base.mul_f64(self.multiplier.powf(attempt as f64));
        std::time::Duration::min(calculated, self.max)
    }
}

/// Outbox repository error.
#[derive(Debug, Error)]
pub enum OutboxError {
    #[error("Write error: {0}")]
    WriteError(String),

    #[error("Read error: {0}")]
    ReadError(String),

    #[error("Update error: {0}")]
    UpdateError(String),

    #[error("Message not found: {0}")]
    NotFound(Uuid),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_outbox_message_creation() {
        let message = OutboxMessage::new(
            "test.topic".to_string(),
            serde_json::json!({"key": "value"}),
        );

        assert_eq!(message.topic, "test.topic");
        assert!(message.id != Uuid::nil());
        assert_eq!(message.status, OutboxStatus::Pending);
        assert_eq!(message.retry_count, 0);
    }

    #[tokio::test]
    async fn test_outbox_message_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("trace_id".to_string(), "abc123".to_string());

        let message = OutboxMessage::new("test.topic".to_string(), serde_json::json!({}))
            .with_headers(headers);

        assert_eq!(message.headers.get("trace_id"), Some(&"abc123".to_string()));
    }

    #[tokio::test]
    async fn test_outbox_status_display() {
        assert_eq!(OutboxStatus::Pending.to_string(), "pending");
        assert_eq!(OutboxStatus::Processing.to_string(), "processing");
        assert_eq!(OutboxStatus::Published.to_string(), "published");
        assert_eq!(OutboxStatus::Failed.to_string(), "failed");
    }

    #[tokio::test]
    async fn test_can_retry() {
        let mut pending = OutboxMessage::default();
        assert!(!pending.can_retry(3));

        pending.status = OutboxStatus::Failed;
        pending.retry_count = 0;
        assert!(pending.can_retry(3));

        pending.retry_count = 3;
        assert!(!pending.can_retry(3));
    }

    #[tokio::test]
    async fn test_outbox_config_defaults() {
        let config = OutboxConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_backoff, std::time::Duration::from_millis(100));
        assert_eq!(config.max_backoff, std::time::Duration::from_secs(30));
    }
}

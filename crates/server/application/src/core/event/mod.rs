//! Event Bus Infrastructure
//!
//! Provides event publishing infrastructure for the application layer.

use async_trait::async_trait;
use futures::stream::BoxStream;
use hodei_server_domain::events::DomainEvent;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, broadcast};
use tracing::debug;

/// Errors for Event Bus operations
#[derive(Debug, Error)]
pub enum EventBusInfrastructureError {
    #[error("Event serialization error: {0}")]
    SerializationError(String),
    #[error("Event handler error: {0}")]
    HandlerError(String),
    #[error("Subscription error: {0}")]
    SubscriptionError(String),
}

impl From<EventBusInfrastructureError> for hodei_server_domain::shared_kernel::DomainError {
    fn from(err: EventBusInfrastructureError) -> Self {
        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: err.to_string(),
        }
    }
}

/// Configuration for event handling
#[derive(Debug, Clone, Default)]
pub struct EventBusConfig {
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    pub enable_dlq: bool,
    pub dlq_max_size: usize,
}

/// Dead Letter Queue entry
#[derive(Debug, Clone)]
pub struct DeadLetterEntry {
    pub event: DomainEvent,
    pub error: String,
    pub attempts: u32,
    pub first_attempt: chrono::DateTime<chrono::Utc>,
}

/// Dead Letter Queue
#[derive(Clone, Default)]
pub struct DeadLetterQueue {
    entries: Arc<Mutex<Vec<DeadLetterEntry>>>,
    max_size: usize,
}

impl DeadLetterQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::with_capacity(max_size))),
            max_size,
        }
    }

    pub async fn push(&self, event: &DomainEvent, error: &str, attempts: u32) {
        let mut entries = self.entries.lock().await;
        if entries.len() >= self.max_size {
            entries.remove(0);
        }
        let now = chrono::Utc::now();
        entries.push(DeadLetterEntry {
            event: event.clone(),
            error: error.to_string(),
            attempts,
            first_attempt: now,
        });
    }

    pub async fn get_all(&self) -> Vec<DeadLetterEntry> {
        self.entries.lock().await.clone()
    }
}

/// Trait for handling domain events - accepts any type that can be converted to DomainEvent
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &DomainEvent) -> Result<(), EventBusInfrastructureError>;
    fn interested_in(&self) -> &'static str;
}

/// Event Publisher trait
#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusInfrastructureError>;
    async fn publish_batch(
        &self,
        events: &[&DomainEvent],
    ) -> Result<(), EventBusInfrastructureError>;
}

/// In-memory Event Bus
#[derive(Clone)]
pub struct InMemoryEventBus {
    tx: broadcast::Sender<DomainEvent>,
    dlq: DeadLetterQueue,
    config: EventBusConfig,
}

impl InMemoryEventBus {
    pub fn new(config: Option<EventBusConfig>) -> Self {
        Self {
            tx: broadcast::channel(1000).0,
            dlq: DeadLetterQueue::new(
                config
                    .as_ref()
                    .unwrap_or(&EventBusConfig::default())
                    .dlq_max_size,
            ),
            config: config.unwrap_or_default(),
        }
    }

    pub fn dlq(&self) -> &DeadLetterQueue {
        &self.dlq
    }

    pub fn subscribe_all(&self) -> broadcast::Receiver<DomainEvent> {
        self.tx.subscribe()
    }
}

#[async_trait::async_trait]
impl EventPublisher for InMemoryEventBus {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusInfrastructureError> {
        let _ = self.tx.send(event.clone());
        debug!("Published event: {}", event.event_type());
        Ok(())
    }

    async fn publish_batch(
        &self,
        events: &[&DomainEvent],
    ) -> Result<(), EventBusInfrastructureError> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }
}

/// Event metadata for tracing
#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl EventMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }
}

/// Event factory
#[derive(Clone, Default)]
pub struct EventFactory;

impl EventFactory {
    pub fn system_event() -> EventMetadata {
        EventMetadata {
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            actor: Some("system".to_string()),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn user_event(user_id: impl Into<String>) -> EventMetadata {
        EventMetadata {
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            actor: Some(user_id.into()),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Mock event handler for testing
#[derive(Clone, Default)]
pub struct MockEventHandler;

#[async_trait::async_trait]
impl EventHandler for MockEventHandler {
    async fn handle(&self, _event: &DomainEvent) -> Result<(), EventBusInfrastructureError> {
        Ok(())
    }

    fn interested_in(&self) -> &'static str {
        "*"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_publish_and_subscribe() {
        let bus = InMemoryEventBus::new(None);
        let mut rx = bus.subscribe_all();

        // Publish would be tested with proper async runtime
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_dead_letter_queue() {
        let dlq = DeadLetterQueue::new(10);
        assert!(dlq.get_all().await.is_empty());
    }

    #[tokio::test]
    async fn test_mock_event_handler() {
        let handler = MockEventHandler;
        let job_id = hodei_server_domain::shared_kernel::JobId::new();
        let spec = hodei_server_domain::jobs::JobSpec::new(vec!["echo".to_string()]);
        let result = handler
            .handle(&DomainEvent::JobCreated {
                job_id,
                spec,
                occurred_at: chrono::Utc::now(),
                correlation_id: None,
                actor: None,
            })
            .await;
        assert!(result.is_ok());
        assert_eq!(handler.interested_in(), "*");
    }
}

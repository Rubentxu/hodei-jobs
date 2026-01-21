//! # EventBus Port
//!
//! This module defines the [`EventBus`] trait for publishing and subscribing
//! to domain events. Events represent something that has happened in the system.
//!
//! # Example
//!
//! ```ignore
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! #[derive(Clone, Debug)]
//! struct UserCreated {
//!     user_id: UserId,
//!     email: String,
//!     timestamp: chrono::DateTime<chrono::Utc>,
//! }
//!
//! impl DomainEvent for UserCreated {
//!     fn event_type(&self) -> &'static str { "user.created" }
//!     fn aggregate_id(&self) -> &str { self.user_id.as_str() }
//! }
//!
//! // Subscribe to events
//! event_bus.subscribe::<UserCreated>(Arc::new(MyHandler)).await?;
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

/// Trait for publishing and subscribing to domain events.
///
/// The EventBus pattern provides:
/// - Event publishing to a message bus
/// - Asynchronous event delivery
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event to the bus.
    async fn publish<E>(&self, event: E) -> Result<(), EventBusError>
    where
        E: DomainEvent + Send + 'static;

    /// Subscribe to events of a specific type.
    async fn subscribe<E>(
        &self,
        handler: Arc<dyn EventHandler<E>>,
    ) -> Result<SubscriptionId, EventBusError>
    where
        E: DomainEvent + Send + 'static;

    /// Subscribe to all events matching a pattern.
    async fn subscribe_with_pattern(
        &self,
        pattern: String,
        handler: Arc<dyn EventHandlerAny>,
    ) -> Result<SubscriptionId, EventBusError>;

    /// Unsubscribe from events.
    async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<(), EventBusError>;
}

/// Marker trait for domain events.
///
/// Events represent something that has already happened in the system.
/// They are immutable and should describe what occurred, not what to do.
///
/// # Characteristics
/// - Immutable: Once an event happens, it cannot be changed
/// - Past tense: Event names should be past tense (UserCreated, not CreateUser)
/// - Descriptive: Events contain all information about what happened
pub trait DomainEvent:
    Send + Sync + Clone + Serialize + for<'de> Deserialize<'de> + 'static
{
    /// Unique type identifier for this event.
    const TYPE_ID: &'static str;

    /// Returns the event type identifier.
    fn event_type(&self) -> &'static str {
        Self::TYPE_ID
    }

    /// Returns the aggregate ID this event belongs to.
    fn aggregate_id(&self) -> &str;

    /// Returns the timestamp when the event occurred.
    fn timestamp(&self) -> chrono::DateTime<chrono::Utc>;
}

/// Handler trait for a specific event type.
///
/// Implement this trait to handle events of a specific type.
#[async_trait]
pub trait EventHandler<E: DomainEvent>: Send + Sync {
    /// Handle the event.
    async fn handle(&self, event: E) -> Result<(), EventBusError>;
}

/// Handler for any event type (used with patterns).
#[async_trait]
pub trait EventHandlerAny: Send + Sync {
    /// Handle any event.
    async fn handle(&self, event_type: &'static str, payload: &[u8]) -> Result<(), EventBusError>;
}

/// Subscription identifier for managing subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub Uuid);

impl Default for SubscriptionId {
    fn default() -> Self {
        SubscriptionId(Uuid::new_v4())
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Errors that can occur during event publishing or subscription.
#[derive(Debug, Error)]
pub enum EventBusError {
    /// Error publishing an event
    #[error("Publish error: {0}")]
    Publish(String),

    /// Error subscribing to events
    #[error("Subscription error: {0}")]
    Subscription(String),

    /// Subscription not found
    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(SubscriptionId),

    /// Connection lost
    #[error("Connection lost: {0}")]
    ConnectionLost(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

/// Configuration for event publishing.
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Event subject prefix
    pub prefix: String,
    /// Enable event persistence
    pub persistent: bool,
    /// Enable compression
    pub compress: bool,
    /// Batch size for publishing
    pub batch_size: usize,
    /// Publish timeout
    pub timeout: std::time::Duration,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            prefix: "saga".to_string(),
            persistent: true,
            compress: false,
            batch_size: 100,
            timeout: std::time::Duration::from_secs(5),
        }
    }
}

/// Metadata for an event subscription.
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub event_type: &'static str,
    pub pattern: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub handler_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestEvent {
        id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    }

    impl DomainEvent for TestEvent {
        const TYPE_ID: &'static str = "test.event";

        fn aggregate_id(&self) -> &str {
            &self.id
        }

        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
            self.timestamp
        }
    }

    #[tokio::test]
    async fn test_event_bus_error_variants() {
        let error = EventBusError::Publish("connection lost".to_string());
        assert!(error.to_string().contains("Publish"));

        let error = EventBusError::Subscription("handler error".to_string());
        assert!(error.to_string().contains("Subscription"));

        let sub_id = SubscriptionId::default();
        let error = EventBusError::SubscriptionNotFound(sub_id);
        assert!(error.to_string().contains("not found"));

        let error = EventBusError::ConnectionLost("nats".to_string());
        assert!(error.to_string().contains("Connection"));
    }

    #[tokio::test]
    async fn test_subscription_id() {
        let id1 = SubscriptionId::default();
        let id2 = SubscriptionId::default();
        assert_ne!(id1, id2);

        let id3 = id1;
        assert_eq!(id1, id3);
    }

    #[tokio::test]
    async fn test_event_bus_config_defaults() {
        let config = EventBusConfig::default();
        assert_eq!(config.prefix, "saga");
        assert!(config.persistent);
        assert!(!config.compress);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.timeout, std::time::Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_domain_event() {
        let event = TestEvent {
            id: "test-123".to_string(),
            timestamp: chrono::Utc::now(),
        };

        assert_eq!(event.event_type(), "test.event");
        assert_eq!(event.aggregate_id(), "test-123");
        assert!(event.timestamp() <= chrono::Utc::now());
    }
}

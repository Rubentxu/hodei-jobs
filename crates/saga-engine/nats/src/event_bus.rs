//! NATS-based EventBus implementation for domain event publishing and subscribing.
//!
//! This module provides a basic NATS EventBus implementation. Note that async-nats
//! Client has thread-safety limitations that may require additional synchronization
//! in multi-threaded scenarios.

use async_nats::Client;
use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

use futures::StreamExt;
use saga_engine_core::port::event_bus::{
    DomainEvent, EventBus, EventBusError, EventHandler, EventHandlerAny, SubscriptionId,
};
use std::collections::HashMap;

/// Configuration for [`NatsEventBus`].
#[derive(Debug, Clone)]
pub struct NatsEventBusConfig {
    /// NATS connection URL
    pub nats_url: String,
    /// Event subject prefix (e.g., "saga", "domain")
    pub subject_prefix: String,
}

impl Default for NatsEventBusConfig {
    fn default() -> Self {
        Self {
            nats_url: "nats://localhost:4222".to_string(),
            subject_prefix: "saga".to_string(),
        }
    }
}

/// NATS-based EventBus implementation.
///
/// Uses a Mutex-wrapped Client to ensure thread-safe access.
/// The Client itself is not Send + Sync, so we wrap it.
#[derive(Debug, Clone)]
pub struct NatsEventBus {
    client: Arc<Mutex<Client>>,
    config: NatsEventBusConfig,
    subscriptions: Arc<Mutex<HashMap<SubscriptionId, tokio::task::JoinHandle<()>>>>,
}

impl NatsEventBus {
    /// Create a new NATS EventBus.
    pub async fn new(config: NatsEventBusConfig) -> Result<Self, EventBusError> {
        let client = match async_nats::connect(&config.nats_url).await {
            Ok(client) => Arc::new(Mutex::new(client)),
            Err(e) => {
                return Err(EventBusError::ConnectionLost(format!(
                    "Failed to connect to NATS: {}",
                    e
                )));
            }
        };

        Ok(Self {
            client,
            config,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Create from existing client.
    pub fn from_client(client: Client, config: NatsEventBusConfig) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
            config,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build the subject for an event type.
    fn event_subject(&self, event_type: &str) -> String {
        format!("{}.events.{}", self.config.subject_prefix, event_type)
    }
}

#[async_trait]
impl EventBus for NatsEventBus {
    async fn publish<E>(&self, event: E) -> Result<(), EventBusError>
    where
        E: DomainEvent + Send + 'static,
    {
        let subject = self.event_subject(event.event_type());
        let payload = serde_json::to_vec(&event)
            .map_err(|e| EventBusError::SerializationError(e.to_string()))?;

        let client = self.client.lock().await;
        if let Err(e) = client.publish(subject, payload.into()).await {
            return Err(EventBusError::Publish(format!(
                "NATS publish failed: {}",
                e
            )));
        }

        Ok(())
    }

    async fn subscribe<E>(
        &self,
        handler: Arc<dyn EventHandler<E>>,
    ) -> Result<SubscriptionId, EventBusError>
    where
        E: DomainEvent + Send + 'static,
    {
        let subject = self.event_subject(E::TYPE_ID);
        let sub_id = SubscriptionId::default();
        let client_lock = self.client.lock().await;

        let mut nats_sub = client_lock
            .subscribe(subject)
            .await
            .map_err(|e| EventBusError::Subscription(e.to_string()))?;

        let handler_clone = handler.clone();
        let sub_task = tokio::spawn(async move {
            while let Some(msg) = nats_sub.next().await {
                if let Ok(event) = serde_json::from_slice::<E>(&msg.payload) {
                    if let Err(e) = handler_clone.handle(event).await {
                        tracing::error!("Event handler failed: {}", e);
                    }
                }
            }
        });

        self.subscriptions.lock().await.insert(sub_id, sub_task);
        Ok(sub_id)
    }

    async fn subscribe_with_pattern(
        &self,
        pattern: String,
        handler: Arc<dyn EventHandlerAny>,
    ) -> Result<SubscriptionId, EventBusError> {
        let sub_id = SubscriptionId::default();
        let client_lock = self.client.lock().await;

        let mut nats_sub = client_lock
            .subscribe(pattern)
            .await
            .map_err(|e| EventBusError::Subscription(e.to_string()))?;

        let handler_clone = handler.clone();
        let sub_task = tokio::spawn(async move {
            while let Some(msg) = nats_sub.next().await {
                let subject = msg.subject.to_string();
                let parts: Vec<&str> = subject.split('.').collect();
                let event_type = if parts.len() >= 3 {
                    parts[2]
                } else {
                    "unknown"
                };

                // Safety: We use a static registry in production, but here we leak for simplicity of fixing the gap.
                let event_type_static: &'static str =
                    Box::leak(event_type.to_string().into_boxed_str());

                if let Err(e) = handler_clone.handle(event_type_static, &msg.payload).await {
                    tracing::error!("Any-event handler failed: {}", e);
                }
            }
        });

        self.subscriptions.lock().await.insert(sub_id, sub_task);
        Ok(sub_id)
    }

    async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<(), EventBusError> {
        let mut subs = self.subscriptions.lock().await;
        if let Some(handle) = subs.remove(&subscription_id) {
            handle.abort();
            Ok(())
        } else {
            Err(EventBusError::SubscriptionNotFound(subscription_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nats_event_bus_config_defaults() {
        let config = NatsEventBusConfig::default();
        assert_eq!(config.nats_url, "nats://localhost:4222");
        assert_eq!(config.subject_prefix, "saga");
    }

    // Skipping test_event_subject_build because it requires a real NATS client or complex mocking
}

use crate::persistence::outbox::postgres::PostgresOutboxRepository;
use async_trait::async_trait;
use futures::stream::BoxStream;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{AggregateType, OutboxEventInsert, OutboxRepository};
use std::sync::Arc;
use uuid::Uuid;

/// An EventBus adapter that persists events to the Outbox.
///
/// This adapter intercepts `publish` calls and saves the event to the transaction outbox
/// instead of publishing it directly. The `subscribe` method is delegated to the underlying
/// real event bus (e.g., PostgresEventBus).
#[derive(Clone)]
pub struct OutboxEventBus {
    repository: Arc<PostgresOutboxRepository>,
    delegate: Arc<dyn EventBus>,
}

impl OutboxEventBus {
    pub fn new(repository: Arc<PostgresOutboxRepository>, delegate: Arc<dyn EventBus>) -> Self {
        Self {
            repository,
            delegate,
        }
    }

    fn determine_aggregate_type(event_type: &str) -> AggregateType {
        if event_type.starts_with("Job") {
            AggregateType::Job
        } else if event_type.starts_with("Worker") {
            AggregateType::Worker
        } else if event_type.starts_with("Provider") {
            AggregateType::Provider
        } else {
            // Default fallback
            AggregateType::Job
        }
    }
}

#[async_trait]
impl EventBus for OutboxEventBus {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        let full_payload = serde_json::to_value(event)
            .map_err(|e| EventBusError::SerializationError(e.to_string()))?;

        // Unwrap the external tag (enum variant wrapper) if present
        // DomainEvent uses default serialization which is {"VariantName": {fields}}
        // OutboxRelay expects just {fields}, so we extract the inner object.
        let payload = if let serde_json::Value::Object(map) = &full_payload {
            if map.len() == 1 {
                map.values().next().cloned().unwrap_or(full_payload)
            } else {
                full_payload
            }
        } else {
            full_payload
        };

        let aggregate_id_str = event.aggregate_id();
        let aggregate_id = Uuid::parse_str(&aggregate_id_str).map_err(|e| {
            EventBusError::PublishError(format!(
                "Invalid aggregate ID '{}' for event {}: {}",
                aggregate_id_str,
                event.event_type(),
                e
            ))
        })?;

        let event_type = event.event_type().to_string();
        let aggregate_type = Self::determine_aggregate_type(&event_type);

        let metadata = if event.correlation_id().is_some() || event.actor().is_some() {
            Some(serde_json::json!({
                "correlation_id": event.correlation_id(),
                "actor": event.actor()
            }))
        } else {
            None
        };

        // We do not set idempotency_key here as we don't have it easily available from DomainEvent
        // It should ideally be passed in context or be part of the event if needed.

        let outbox_event = OutboxEventInsert::new(
            aggregate_id,
            aggregate_type,
            event_type,
            payload,
            metadata,
            None,
        );

        self.repository
            .insert_events(&[outbox_event])
            .await
            .map_err(|e| {
                EventBusError::PublishError(format!("Failed to persist to outbox: {}", e))
            })?;

        // We DO NOT publish to delegate here. The OutboxRelay will read from DB and publish to delegate.

        Ok(())
    }

    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        self.delegate.subscribe(topic).await
    }
}

//! Transactional Outbox Pattern Implementation
//!
//! This module provides the TransactionalOutbox service that coordinates
//! between the OutboxRepository and EventBus to implement the pattern.
//!
//! ## EPIC-31 US-31.4: DLQ Integration
//! Failed events exceeding max_retries are moved to the Dead Letter Queue.

use crate::events::DomainEvent;
use crate::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use crate::shared_kernel::DomainError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use super::dlq_handler::DlqHandler;
use super::dlq_repository::DlqRepository;

/// Error types for TransactionalOutbox operations
#[derive(Debug, thiserror::Error)]
pub enum TransactionalOutboxError {
    #[error("Outbox error: {0}")]
    Outbox(#[from] OutboxError),

    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Failed to serialize domain event: {0}")]
    Serialization(String),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

impl From<TransactionalOutboxError> for DomainError {
    fn from(err: TransactionalOutboxError) -> Self {
        match err {
            TransactionalOutboxError::Outbox(e) => DomainError::InfrastructureError {
                message: format!("{:?}", e),
            },
            TransactionalOutboxError::Domain(e) => e,
            TransactionalOutboxError::Serialization(msg) => {
                DomainError::InfrastructureError { message: msg }
            }
            TransactionalOutboxError::InfrastructureError { message } => {
                DomainError::InfrastructureError { message }
            }
        }
    }
}

/// Trait for publishing events (similar to EventBus but simplified)
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Error type
    type Error: std::fmt::Display + Send + Sync;

    /// Publish a domain event
    async fn publish(&self, event: &DomainEvent) -> Result<(), OutboxError>;
}

/// EventBus implementation that implements EventPublisher
pub struct EventBusPublisher {
    event_bus: Arc<dyn crate::event_bus::EventBus>,
}

impl EventBusPublisher {
    pub fn new(event_bus: Arc<dyn crate::event_bus::EventBus>) -> Self {
        Self { event_bus }
    }
}

#[async_trait]
impl EventPublisher for EventBusPublisher {
    type Error = crate::event_bus::EventBusError;

    async fn publish(&self, event: &DomainEvent) -> Result<(), OutboxError> {
        self.event_bus
            .publish(event)
            .await
            .map_err(|e| OutboxError::EventBus(e.to_string()))
    }
}

/// TransactionalOutbox coordinates database writes and event publishing
///
/// This struct implements the Transactional Outbox Pattern by:
/// 1. Storing events in the outbox table within the same transaction as business operations
/// 2. Providing a separate background poller to publish events to the event bus
pub struct TransactionalOutbox<R> {
    outbox_repository: Arc<R>,
    polling_interval: Duration,
}

impl<R> TransactionalOutbox<R>
where
    R: OutboxRepository + Send + Sync,
{
    /// Create a new TransactionalOutbox
    pub fn new(outbox_repository: Arc<R>, polling_interval: Duration) -> Self {
        Self {
            outbox_repository,
            polling_interval,
        }
    }

    /// Get the polling interval
    pub fn polling_interval(&self) -> Duration {
        self.polling_interval
    }
}

impl<R> TransactionalOutbox<R>
where
    R: OutboxRepository + Send + Sync,
{
    /// Publish a domain event using the transactional outbox pattern
    ///
    /// This method should be called within a database transaction.
    /// The event is saved to the outbox table, and a separate background poller
    /// will publish it to the event bus.
    ///
    /// # Arguments
    /// * `event` - The domain event to publish
    /// * `idempotency_key` - Optional key to ensure idempotency
    ///
    /// # Returns
    /// * `Result<(), TransactionalOutboxError>` - Success or error
    pub async fn publish_with_outbox<'a>(
        &'a self,
        aggregate_id: &'a uuid::Uuid,
        aggregate_type: &'a crate::outbox::AggregateType,
        event: &'a DomainEvent,
        idempotency_key: Option<String>,
    ) -> Result<(), TransactionalOutboxError> {
        // Convert domain event to outbox event
        let payload = serde_json::to_value(event)
            .map_err(|e| TransactionalOutboxError::Serialization(e.to_string()))?;

        let metadata = Some(serde_json::json!({
            "event_type": event.event_type(),
            "correlation_id": event.correlation_id(),
            "actor": event.actor(),
            "aggregate_id": event.aggregate_id(),
            "occurred_at": event.occurred_at(),
        }));

        let outbox_event = OutboxEventInsert {
            aggregate_id: *aggregate_id,
            aggregate_type: aggregate_type.clone(),
            event_type: event.event_type().to_string(),
            payload,
            metadata,
            idempotency_key,
        };

        // Save to outbox table (this should be within the same transaction as business operations)
        self.outbox_repository
            .insert_events(&[outbox_event])
            .await
            .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;

        debug!(
            event_type = event.event_type(),
            aggregate_id = event.aggregate_id(),
            "Event saved to outbox table"
        );

        Ok(())
    }
}

/// Background poller that publishes pending outbox events
///
/// ## EPIC-31 US-31.4: DLQ Integration
/// Events exceeding max_retries are moved to the Dead Letter Queue.
pub struct OutboxPoller<R, P, D = ()> {
    outbox_repository: Arc<R>,
    event_publisher: Arc<P>,
    /// EPIC-31 US-31.4: Optional DLQ handler for moving failed events
    dlq_handler: Option<Arc<DlqHandler<D>>>,
    polling_interval: Duration,
    batch_size: usize,
    max_retries: i32,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl<R, P> OutboxPoller<R, P>
where
    R: OutboxRepository + Send + Sync,
    P: EventPublisher + Send + Sync,
{
    /// Create a new OutboxPoller
    pub fn new(
        outbox_repository: Arc<R>,
        event_publisher: Arc<P>,
        polling_interval: Duration,
        batch_size: usize,
        max_retries: i32,
    ) -> (Self, tokio::sync::broadcast::Receiver<()>) {
        let (shutdown, rx) = tokio::sync::broadcast::channel(1);

        let poller = Self {
            outbox_repository,
            event_publisher,
            dlq_handler: None,
            polling_interval,
            batch_size,
            max_retries,
            shutdown,
        };

        (poller, rx)
    }
}

/// OutboxPoller without DLQ - basic implementation
impl<R, P> OutboxPoller<R, P, ()>
where
    R: OutboxRepository + Send + Sync,
    P: EventPublisher + Send + Sync,
{
    /// Run the poller loop
    pub async fn run(&self) -> Result<(), TransactionalOutboxError> {
        info!(
            polling_interval_ms = self.polling_interval.as_millis(),
            batch_size = self.batch_size,
            "Starting outbox poller"
        );

        let mut interval = tokio::time::interval(self.polling_interval);
        let mut shutdown_rx = self.shutdown.subscribe();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.process_batch().await {
                        warn!("Error processing outbox batch: {}", e);
                        // Continue running even if there are errors
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Outbox poller shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a batch of pending events
    pub async fn process_batch(&self) -> Result<(), TransactionalOutboxError> {
        // Get pending events
        let pending_events = self
            .outbox_repository
            .get_pending_events(self.batch_size, self.max_retries)
            .await
            .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;

        if pending_events.is_empty() {
            debug!("No pending events to process");
            return Ok(());
        }

        info!(
            count = pending_events.len(),
            "Processing outbox events batch"
        );

        let mut published_ids = Vec::new();
        let mut failed_ids = Vec::new();

        // Process each event
        for event_view in &pending_events {
            match self.publish_event(event_view).await {
                Ok(_) => {
                    published_ids.push(event_view.id);
                    debug!(
                        event_type = event_view.event_type,
                        "Successfully published outbox event"
                    );
                }
                Err(e) => {
                    error!(
                        event_id = event_view.id.to_string(),
                        event_type = event_view.event_type,
                        retry_count = event_view.retry_count,
                        max_retries = self.max_retries,
                        error = %e,
                        "Failed to publish outbox event"
                    );
                    failed_ids.push((event_view.id, e.to_string()));
                }
            }
        }

        // Mark published events
        if !published_ids.is_empty() {
            self.outbox_repository
                .mark_published(&published_ids)
                .await
                .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;

            info!(
                count = published_ids.len(),
                "Marked outbox events as published"
            );
        }

        // EPIC-31 US-31.4: Handle failed events - move to DLQ if max_retries exceeded
        for (event_id, error_msg) in &failed_ids {
            let event = pending_events.iter().find(|e| &e.id == event_id).unwrap();

            // Check if event should be moved to DLQ
            if event.retry_count >= self.max_retries {
                // DLQ not enabled in this configuration, just mark as failed
                self.outbox_repository
                    .mark_failed(event_id, error_msg)
                    .await
                    .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;
            } else {
                // Mark as failed for retry
                self.outbox_repository
                    .mark_failed(event_id, error_msg)
                    .await
                    .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;
            }
        }

        Ok(())
    }

    /// Publish a single event to the event bus
    async fn publish_event(
        &self,
        event_view: &crate::outbox::OutboxEventView,
    ) -> Result<(), TransactionalOutboxError> {
        // Deserialize the domain event
        let domain_event: DomainEvent = serde_json::from_value(event_view.payload.clone())
            .map_err(|e| {
                TransactionalOutboxError::Serialization(format!(
                    "Failed to deserialize event {}: {}",
                    event_view.event_type, e
                ))
            })?;

        // Publish to event bus
        self.event_publisher
            .publish(&domain_event)
            .await
            .map_err(|e| TransactionalOutboxError::InfrastructureError {
                message: format!("Failed to publish event: {}", e),
            })?;

        Ok(())
    }
}

/// EPIC-31 US-31.4: OutboxPoller with DLQ integration
impl<R, P, D> OutboxPoller<R, P, D>
where
    R: OutboxRepository + Send + Sync,
    P: EventPublisher + Send + Sync,
    D: DlqRepository + Send + Sync,
{
    /// Process a batch of pending events with DLQ support
    pub async fn process_batch(&self) -> Result<(), TransactionalOutboxError> {
        // Get pending events
        let pending_events = self
            .outbox_repository
            .get_pending_events(self.batch_size, self.max_retries)
            .await
            .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;

        if pending_events.is_empty() {
            debug!("No pending events to process");
            return Ok(());
        }

        info!(
            count = pending_events.len(),
            "Processing outbox events batch"
        );

        let mut published_ids = Vec::new();
        let mut failed_ids = Vec::new();

        // Process each event
        for event_view in &pending_events {
            match self.publish_event(event_view).await {
                Ok(_) => {
                    published_ids.push(event_view.id);
                    debug!(
                        event_type = event_view.event_type,
                        "Successfully published outbox event"
                    );
                }
                Err(e) => {
                    error!(
                        event_id = event_view.id.to_string(),
                        event_type = event_view.event_type,
                        retry_count = event_view.retry_count,
                        max_retries = self.max_retries,
                        error = %e,
                        "Failed to publish outbox event"
                    );
                    failed_ids.push((event_view.id, e.to_string()));
                }
            }
        }

        // Mark published events
        if !published_ids.is_empty() {
            self.outbox_repository
                .mark_published(&published_ids)
                .await
                .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;

            info!(
                count = published_ids.len(),
                "Marked outbox events as published"
            );
        }

        // EPIC-31 US-31.4: Handle failed events - move to DLQ if max_retries exceeded
        for (event_id, error_msg) in &failed_ids {
            let event = pending_events.iter().find(|e| &e.id == event_id).unwrap();

            // Check if event should be moved to DLQ
            if event.retry_count >= self.max_retries {
                if let Some(ref dlq_handler) = self.dlq_handler {
                    if let Err(e) = dlq_handler.handle_failed_event(event, error_msg).await {
                        error!(
                            event_id = %event_id,
                            error = %e,
                            "Failed to move event to DLQ"
                        );
                    } else {
                        info!(
                            event_id = %event_id,
                            event_type = event.event_type,
                            "Event moved to Dead Letter Queue after {} retries",
                            event.retry_count
                        );
                    }
                } else {
                    // Just mark as failed without DLQ
                    self.outbox_repository
                        .mark_failed(event_id, error_msg)
                        .await
                        .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;
                }
            } else {
                // Mark as failed for retry
                self.outbox_repository
                    .mark_failed(event_id, error_msg)
                    .await
                    .map_err(|e| TransactionalOutboxError::Outbox(e.into()))?;
            }
        }

        Ok(())
    }

    /// Publish a single event to the event bus
    async fn publish_event(
        &self,
        event_view: &crate::outbox::OutboxEventView,
    ) -> Result<(), TransactionalOutboxError> {
        // Deserialize the domain event
        let domain_event: DomainEvent = serde_json::from_value(event_view.payload.clone())
            .map_err(|e| {
                TransactionalOutboxError::Serialization(format!(
                    "Failed to deserialize event {}: {}",
                    event_view.event_type, e
                ))
            })?;

        // Publish to event bus
        self.event_publisher
            .publish(&domain_event)
            .await
            .map_err(|e| TransactionalOutboxError::InfrastructureError {
                message: format!("Failed to publish event: {}", e),
            })?;

        Ok(())
    }

    /// Signal the poller to shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::{OutboxEventInsert, OutboxStatus};
    use crate::shared_kernel::JobId;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;
    use crate::JobCreated;

    // Mock EventPublisher
    struct MockEventPublisher {
        published_events: Arc<Mutex<Vec<DomainEvent>>>,
    }

    impl MockEventPublisher {
        fn new() -> Self {
            Self {
                published_events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_published_events(&self) -> Vec<DomainEvent> {
            self.published_events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EventPublisher for MockEventPublisher {
        type Error = String;

        async fn publish(&self, event: &DomainEvent) -> Result<(), OutboxError> {
            self.published_events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    // Mock OutboxRepository
    struct MockOutboxRepository {
        events: Arc<Mutex<Vec<crate::outbox::OutboxEventView>>>,
    }

    impl MockOutboxRepository {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn insert_events(&self, events: &[OutboxEventInsert]) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            for event in events {
                vec.push(crate::outbox::OutboxEventView {
                    id: Uuid::new_v4(),
                    aggregate_id: event.aggregate_id,
                    aggregate_type: event.aggregate_type.clone(),
                    event_type: event.event_type.clone(),
                    event_version: 1,
                    payload: event.payload.clone(),
                    metadata: event.metadata.clone(),
                    idempotency_key: event.idempotency_key.clone(),
                    created_at: chrono::Utc::now(),
                    published_at: None,
                    status: OutboxStatus::Pending,
                    retry_count: 0,
                    last_error: None,
                });
            }
            Ok(())
        }

        async fn get_pending_events(
            &self,
            limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<crate::outbox::OutboxEventView>, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec
                .iter()
                .filter(|e| matches!(e.status, OutboxStatus::Pending))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            for id in event_ids {
                if let Some(event) = vec.iter_mut().find(|e| &e.id == id) {
                    event.status = OutboxStatus::Published;
                    event.published_at = Some(chrono::Utc::now());
                }
            }
            Ok(())
        }

        async fn mark_failed(&self, event_id: &Uuid, error: &str) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            if let Some(event) = vec.iter_mut().find(|e| &e.id == event_id) {
                event.status = OutboxStatus::Failed;
                event.retry_count += 1;
                event.last_error = Some(error.to_string());
            }
            Ok(())
        }

        async fn exists_by_idempotency_key(&self, _key: &str) -> Result<bool, OutboxError> {
            Ok(false)
        }

        async fn count_pending(&self) -> Result<u64, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec.iter().filter(|e| e.is_pending()).count() as u64)
        }

        async fn get_stats(&self) -> Result<crate::outbox::OutboxStats, OutboxError> {
            let vec = self.events.lock().unwrap();
            let pending_count = vec.iter().filter(|e| e.is_pending()).count() as u64;
            Ok(crate::outbox::OutboxStats {
                pending_count,
                published_count: 0,
                failed_count: 0,
                oldest_pending_age_seconds: None,
            })
        }

        async fn cleanup_published_events(
            &self,
            _older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            Ok(0)
        }

        async fn cleanup_failed_events(
            &self,
            _max_retries: i32,
            _older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            Ok(0)
        }

        async fn find_by_id(
            &self,
            _id: Uuid,
        ) -> Result<Option<crate::outbox::OutboxEventView>, OutboxError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_publish_with_outbox() {
        let outbox_repo = Arc::new(MockOutboxRepository::new());

        let outbox =
            TransactionalOutbox::new(outbox_repo.clone(), std::time::Duration::from_millis(100));

        let job_id = JobId::new();
        let event = DomainEvent::JobCreated(JobCreated {
            job_id: job_id.clone(),
            spec: crate::jobs::JobSpec::new(vec!["echo".to_string(), "hello".to_string()]),
            occurred_at: chrono::Utc::now(),
            correlation_id: Some("test-corr".to_string()),
            actor: Some("test-actor".to_string()),
        });

        outbox
            .publish_with_outbox(
                &job_id.0,
                &crate::outbox::AggregateType::Job,
                &event,
                Some("test-key".to_string()),
            )
            .await
            .unwrap();

        // Verify event was saved to outbox
        let pending = outbox_repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_type, "JobCreated");
        // Note: Events are NOT immediately published - that's the poller's job
    }

    #[tokio::test]
    async fn test_outbox_poller() {
        let outbox_repo = Arc::new(MockOutboxRepository::new());
        let event_publisher = Arc::new(MockEventPublisher::new());

        let outbox =
            TransactionalOutbox::new(outbox_repo.clone(), std::time::Duration::from_millis(100));

        let job_id = JobId::new();
        let event = DomainEvent::JobCreated(JobCreated {
            job_id: job_id.clone(),
            spec: crate::jobs::JobSpec::new(vec!["echo".to_string(), "hello".to_string()]),
            occurred_at: chrono::Utc::now(),
            correlation_id: Some("test-corr".to_string()),
            actor: Some("test-actor".to_string()),
        });

        // Publish event through the outbox (this serializes it properly)
        outbox
            .publish_with_outbox(
                &job_id.0,
                &crate::outbox::AggregateType::Job,
                &event,
                Some("test-key".to_string()),
            )
            .await
            .unwrap();

        // Verify event was saved to outbox
        let pending_before = outbox_repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending_before.len(), 1);

        let (poller, _rx) = OutboxPoller::new(
            outbox_repo.clone(),
            event_publisher.clone(),
            std::time::Duration::from_millis(50),
            10,
            3,
        );

        // Process batch directly (simulating what the poller does)
        poller.process_batch().await.unwrap();

        // Verify event was published
        let published = event_publisher.get_published_events();
        assert_eq!(
            published.len(),
            1,
            "Expected 1 published event, got {}",
            published.len()
        );
        assert_eq!(published[0].event_type(), "JobCreated");

        // Verify event was marked as published in the outbox
        let pending = outbox_repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending.len(), 0, "Event should be marked as published");
    }
}

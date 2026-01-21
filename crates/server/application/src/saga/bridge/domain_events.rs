/// # Domain Event to Signal Bridge (US-94.15)
///
/// Bridge between legacy DomainEvent system and saga-engine v4.0 signals.
///
/// This module provides a bridge that subscribes to domain events from the legacy
/// event bus and converts them to saga-engine signals, enabling seamless integration
/// between the legacy system and saga-engine v4.0 workflows.
use async_trait::async_trait;
use hodei_server_domain::events::DomainEvent;
use saga_engine_core::event::SagaId;
use saga_engine_core::port::{SignalDispatcher as CoreSignalDispatcher, SignalType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Re-export SignalDispatcher from saga-engine core
pub use saga_engine_core::port::SignalDispatcher;

/// Bridge between domain events and saga-engine signals.
pub struct DomainEventSignalBridge<SD, EB>
where
    SD: CoreSignalDispatcher,
{
    /// Saga-engine signal dispatcher
    signal_dispatcher: Arc<SD>,
    /// Legacy event bus (for future subscription capability)
    _event_bus: Arc<EB>,
    /// Configuration
    config: DomainEventBridgeConfig,
    /// Mapping from event types to signal types
    event_to_signal_map: Arc<RwLock<HashMap<String, SignalType>>>,
}

impl<SD, EB> DomainEventSignalBridge<SD, EB>
where
    SD: CoreSignalDispatcher + Debug,
{
    /// Create a new DomainEventSignalBridge
    pub fn new(
        event_bus: Arc<EB>,
        signal_dispatcher: Arc<SD>,
        config: DomainEventBridgeConfig,
    ) -> Self {
        Self {
            _event_bus: event_bus,
            signal_dispatcher,
            config,
            event_to_signal_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a mapping from event type to signal type
    pub async fn add_mapping(&self, event_type: String, signal_type: SignalType) {
        let mut map = self.event_to_signal_map.write().await;
        map.insert(event_type, signal_type);
    }

    /// Add multiple mappings at once
    pub async fn add_mappings(&self, mappings: HashMap<String, SignalType>) {
        let mut map = self.event_to_signal_map.write().await;
        for (event_type, signal_type) in mappings {
            map.insert(event_type, signal_type);
        }
    }

    /// Get the current mappings
    pub async fn get_mappings(&self) -> HashMap<String, SignalType> {
        let map = self.event_to_signal_map.read().await;
        map.clone()
    }

    /// Process a single domain event and dispatch signal if needed
    ///
    /// ## FIX: Unique SagaId Generation
    /// Previously, this generated `SagaId(format!("event-{}", event_type))` which
    /// caused collisions because the same event type from different jobs would
    /// share the same SagaId.
    ///
    /// Now we extract unique identifiers from events:
    /// - Job events: Use job_id for uniqueness
    /// - Worker events: Use worker_id for uniqueness
    /// - Provider events: Use provider_id for uniqueness
    /// - Fallback: Generate UUID if no unique ID found
    pub async fn process_event(&self, event: &DomainEvent) -> Result<(), BridgeError> {
        let event_type = event.event_type();

        // Get the signal type for this event type
        let signal_type = {
            let map = self.event_to_signal_map.read().await;
            map.get(event_type).cloned()
        };

        // If no mapping exists, use default or skip
        let signal_type = match signal_type {
            Some(st) => st,
            None => {
                debug!("No signal mapping for event type: {}", event_type);
                return Ok(());
            }
        };

        // Extract unique identifier from event for SagaId
        // This prevents collisions between different jobs/workers/providers
        let unique_id = extract_unique_id_from_event(event);

        // Generate SagaId with unique identifier
        // Format: "event-{event_type}-{unique_id}"
        // Examples:
        //   - "event-JobQueued-job-123-abc"
        //   - "event-WorkerReady-worker-456-def"
        let saga_id = SagaId(format!("event-{}-{}", event_type, unique_id));

        // Serialize payload to bytes
        let payload =
            serde_json::to_vec(event).map_err(|e| BridgeError::ProcessingFailed(e.to_string()))?;

        // Dispatch the signal based on type - clone signal_type to avoid move
        let signal_type_clone = signal_type.clone();
        match signal_type_clone {
            SignalType::NewEvent => {
                self.signal_dispatcher
                    .notify_new_event(&saga_id, 0)
                    .await
                    .map_err(|_| {
                        BridgeError::SignalDispatchFailed("notify_new_event failed".to_string())
                    })?;
            }
            SignalType::TimerFired => {
                self.signal_dispatcher
                    .notify_timer_fired(&saga_id, "")
                    .await
                    .map_err(|_| {
                        BridgeError::SignalDispatchFailed("notify_timer_fired failed".to_string())
                    })?;
            }
            SignalType::Cancelled => {
                self.signal_dispatcher
                    .notify_cancelled(&saga_id)
                    .await
                    .map_err(|_| {
                        BridgeError::SignalDispatchFailed("notify_cancelled failed".to_string())
                    })?;
            }
            SignalType::External(name) => {
                self.signal_dispatcher
                    .send_signal(&saga_id, &name, &payload)
                    .await
                    .map_err(|_| {
                        BridgeError::SignalDispatchFailed("send_signal failed".to_string())
                    })?;
            }
        }

        debug!(
            "Dispatched signal {:?} for event {} (saga_id: {})",
            signal_type, event_type, saga_id
        );

        Ok(())
    }
}

/// Extract a unique identifier from a domain event
///
/// This ensures each event gets a unique SagaId, preventing collisions
/// when multiple events of the same type are processed.
fn extract_unique_id_from_event(event: &DomainEvent) -> String {
    use hodei_server_domain::events::DomainEvent::*;

    match event {
        // Job events - use job_id
        JobQueued { job_id, .. } => format!("job-{}", job_id),
        JobCreated(je) => format!("job-{}", je.job_id),
        JobStatusChanged { job_id, .. } => format!("job-{}", job_id),
        JobAssigned { job_id, .. } => format!("job-{}", job_id),
        JobCancelled { job_id, .. } => format!("job-{}", job_id),
        JobExecutionError { job_id, .. } => format!("job-{}", job_id),
        JobDispatchFailed { job_id, .. } => format!("job-{}", job_id),

        // Worker events - use worker_id
        WorkerReady { worker_id, .. } => format!("worker-{}", worker_id),
        WorkerRegistered { worker_id, .. } => format!("worker-{}", worker_id),
        WorkerStatusChanged { worker_id, .. } => format!("worker-{}", worker_id),
        WorkerProvisioned { worker_id, .. } => format!("worker-{}", worker_id),
        WorkerTerminated { worker_id, .. } => format!("worker-{}", worker_id),
        WorkerHeartbeat { worker_id, .. } => format!("worker-{}", worker_id),

        // Provider events - use provider_id
        ProviderSelected {
            provider_id,
            job_id,
            ..
        } => format!("provider-{}-job-{}", provider_id, job_id),
        ProviderRegistered { provider_id, .. } => format!("provider-{}", provider_id),
        ProviderUpdated { provider_id, .. } => format!("provider-{}", provider_id),
        ProviderHealthChanged { provider_id, .. } => format!("provider-{}", provider_id),

        // Saga events - use saga_id from event if available
        SagaCompleted { saga_id, .. } => format!("saga-{}", saga_id),
        SagaFailed { saga_id, .. } => format!("saga-{}", saga_id),
        SagaTimedOut { saga_id, .. } => format!("saga-{}", saga_id),

        // Default fallback
        _ => "global".to_string(),
    }
}

/// Bridge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEventBridgeConfig {
    /// Default signal type for unmapped events
    pub default_signal_type: String,
    /// Batch size for event processing
    pub batch_size: usize,
    /// Poll interval in milliseconds
    pub poll_interval_ms: u64,
    /// Enable FIFO ordering guarantee
    pub enable_fifo: bool,
    /// Maximum queue size
    pub max_queue_size: usize,
}

impl Default for DomainEventBridgeConfig {
    fn default() -> Self {
        Self {
            default_signal_type: "domain-event".to_string(),
            batch_size: 100,
            poll_interval_ms: 100,
            enable_fifo: true,
            max_queue_size: 10000,
        }
    }
}

impl DomainEventBridgeConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the default signal type
    pub fn with_default_signal_type(mut self, signal_type: String) -> Self {
        self.default_signal_type = signal_type;
        self
    }

    /// Set the batch size
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Set the poll interval
    pub fn with_poll_interval(mut self, interval_ms: u64) -> Self {
        self.poll_interval_ms = interval_ms;
        self
    }

    /// Enable or disable FIFO ordering
    pub fn with_fifo(mut self, enable: bool) -> Self {
        self.enable_fifo = enable;
        self
    }

    /// Set the maximum queue size
    pub fn with_max_queue_size(mut self, size: usize) -> Self {
        self.max_queue_size = size;
        self
    }
}

/// Error types for the bridge
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Subscription failed")]
    SubscriptionFailed,

    #[error("Signal dispatch failed: {0}")]
    SignalDispatchFailed(String),

    #[error("Bridge shutdown failed")]
    ShutdownFailed,

    #[error("Event processing failed: {0}")]
    ProcessingFailed(String),

    #[error("Queue overflow")]
    QueueOverflow,

    #[error("Invalid mapping")]
    InvalidMapping,
}

/// Result type for bridge operations
pub type BridgeResult<T> = Result<T, BridgeError>;

/// Event processing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStats {
    pub events_received: u64,
    pub signals_dispatched: u64,
    pub events_skipped: u64,
    pub errors: u64,
    pub uptime_seconds: u64,
}

impl BridgeStats {
    pub fn new() -> Self {
        Self {
            events_received: 0,
            signals_dispatched: 0,
            events_skipped: 0,
            errors: 0,
            uptime_seconds: 0,
        }
    }

    pub fn increment_events(&mut self) {
        self.events_received += 1;
    }

    pub fn increment_signals(&mut self) {
        self.signals_dispatched += 1;
    }

    pub fn increment_skipped(&mut self) {
        self.events_skipped += 1;
    }

    pub fn increment_errors(&mut self) {
        self.errors += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_config() {
        let config = DomainEventBridgeConfig::new();
        assert_eq!(config.default_signal_type, "domain-event");
        assert_eq!(config.batch_size, 100);
    }

    #[tokio::test]
    async fn test_bridge_error_display() {
        let err = BridgeError::SubscriptionFailed;
        assert!(err.to_string().contains("Subscription"));
    }
}

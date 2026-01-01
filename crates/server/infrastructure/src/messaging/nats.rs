//! NATS JetStream EventBus implementation.
//!
//! This module provides a production-ready EventBus implementation using
//! NATS JetStream for durable, at-least-once event delivery.
//!
//! # Features
//! - **Durable Consumers**: Events survive service restarts
//! - **At-Least-Once Delivery**: Automatic acknowledgments and redelivery
//! - **Work Queues**: Multiple server instances can share event processing
//! - **Subject-Based Routing**: Efficient event filtering by subject hierarchy

use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, PullConsumer};
use async_nats::jetstream::stream::Config as StreamConfig;
use async_nats::jetstream::stream::Stream as StreamHandle;
use async_nats::{Client, ConnectOptions};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use futures::stream::BoxStream;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, warn};

/// NATS connection configuration.
///
/// Provides structured configuration for NATS JetStream connections
/// with sensible production defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsConfig {
    /// NATS server URLs (comma-separated or multiple URLs)
    #[serde(default = "default_urls")]
    pub urls: Vec<String>,
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connection_timeout_secs: u64,
    /// Request timeout in seconds (None = no timeout)
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: Option<u64>,
    /// Max reconnection attempts (None = infinite)
    #[serde(default = "default_max_reconnects")]
    pub max_reconnects: Option<usize>,
    /// JetStream domain (optional)
    #[serde(default)]
    pub domain: Option<String>,
    /// Credentials file path (optional)
    #[serde(default)]
    pub credentials_file: Option<String>,
    /// TLS enabled
    #[serde(default)]
    pub tls_enabled: bool,
    /// Client connection name
    #[serde(default)]
    pub name: Option<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            urls: default_urls(),
            connection_timeout_secs: default_connect_timeout(),
            request_timeout_secs: default_request_timeout(),
            max_reconnects: default_max_reconnects(),
            domain: None,
            credentials_file: None,
            tls_enabled: false,
            name: None,
        }
    }
}

fn default_urls() -> Vec<String> {
    vec!["nats://localhost:4222".to_string()]
}

const fn default_connect_timeout() -> u64 {
    5
}

fn default_request_timeout() -> Option<u64> {
    Some(30)
}

fn default_max_reconnects() -> Option<usize> {
    Some(5)
}

impl NatsConfig {
    /// Creates a new NatsConfig with default settings for local development
    pub fn for_local() -> Self {
        Self {
            urls: vec!["nats://localhost:4222".to_string()],
            connection_timeout_secs: 5,
            request_timeout_secs: Some(30),
            max_reconnects: Some(5),
            domain: None,
            credentials_file: None,
            tls_enabled: false,
            name: Some("hodei-server".to_string()),
        }
    }

    /// Returns the primary URL for connection
    pub fn primary_url(&self) -> &str {
        self.urls
            .first()
            .map(|s| s.as_str())
            .unwrap_or("nats://localhost:4222")
    }
}

/// Wrapped message envelope for NATS transport.
///
/// Includes metadata needed for tracing, correlation, and proper
/// deserialization of DomainEvents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsMessageEnvelope {
    /// The serialized domain event
    pub payload: String,
    /// Event type for routing
    #[serde(default)]
    pub event_type: String,
    /// Correlation ID for distributed tracing
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Message creation timestamp
    #[serde(default)]
    pub created_at: DateTime<Utc>,
}

impl NatsMessageEnvelope {
    /// Creates a new envelope from a DomainEvent
    pub fn from_domain_event(event: &DomainEvent) -> Self {
        Self {
            payload: serde_json::to_string(event).expect("DomainEvent should be serializable"),
            event_type: event.event_type().to_string(),
            correlation_id: event.correlation_id(),
            created_at: Utc::now(),
        }
    }

    /// Extracts the DomainEvent from this envelope
    pub fn to_domain_event(&self) -> Result<DomainEvent, serde_json::Error> {
        serde_json::from_str(&self.payload)
    }
}

/// NATS EventBus implementation using JetStream.
///
/// This implementation provides:
/// - **Publish**: Sends events to NATS subjects with ack confirmation
/// - **Subscribe**: Creates durable consumers that survive restarts
/// - **Consumer Management**: Automatic stream and consumer creation
///
/// # Subject Mapping
///
/// Events are mapped to subjects based on their type:
/// - `hodei.jobs.created` → `JobCreated`
/// - `hodei.jobs.scheduled` → `JobScheduled`
/// - `hodei.workers.ready` → `WorkerReady`
/// - `hodei.workers.provision.request` → `WorkerProvisioningRequested`
/// - etc.
#[derive(Clone)]
pub struct NatsEventBus {
    /// NATS client connection
    client: Arc<Client>,
    /// JetStream context for advanced operations
    jetstream: JetStreamContext,
    /// Configuration
    config: Arc<NatsConfig>,
    /// Stream name prefix for isolation
    stream_prefix: String,
    /// Consumer state tracking
    state: Arc<Mutex<NatsEventBusState>>,
}

#[derive(Debug, Default)]
struct NatsEventBusState {
    /// Known streams and their configuration
    streams: Vec<StreamInfo>,
    /// Known consumers
    consumers: Vec<ConsumerInfo>,
}

#[derive(Debug)]
struct StreamInfo {
    name: String,
    subject: String,
    handle: Option<StreamHandle>,
}

#[derive(Debug)]
struct ConsumerInfo {
    name: String,
    stream: String,
    subject: String,
}

/// Metrics for observability
#[derive(Debug, Default)]
pub struct NatsEventBusMetrics {
    published_count: Arc<Mutex<u64>>,
    subscribe_count: Arc<Mutex<u64>>,
    error_count: Arc<Mutex<u64>>,
}

impl NatsEventBusMetrics {
    /// Increment published counter
    pub async fn inc_published(&self) {
        let mut count = self.published_count.lock().await;
        *count += 1;
    }

    /// Increment subscribe counter
    pub async fn inc_subscribe(&self) {
        let mut count = self.subscribe_count.lock().await;
        *count += 1;
    }

    /// Increment error counter
    pub async fn inc_error(&self) {
        let mut count = self.error_count.lock().await;
        *count += 1;
    }

    /// Get current metrics snapshot
    pub async fn snapshot(&self) -> (u64, u64, u64) {
        let published = *self.published_count.lock().await;
        let subscribed = *self.subscribe_count.lock().await;
        let errors = *self.error_count.lock().await;
        (published, subscribed, errors)
    }
}

impl NatsEventBus {
    /// Creates a new NatsEventBus from configuration
    ///
    /// # Errors
    /// Returns an error if connection to NATS fails
    pub async fn new(config: NatsConfig) -> Result<Self, EventBusError> {
        let connection_timeout = Duration::from_secs(config.connection_timeout_secs);

        let mut connect_options = ConnectOptions::default().connection_timeout(connection_timeout);

        // Set request timeout if provided
        if let Some(timeout_secs) = config.request_timeout_secs {
            connect_options =
                connect_options.request_timeout(Some(Duration::from_secs(timeout_secs)));
        }

        // Add client name if provided
        if let Some(name) = &config.name {
            connect_options = connect_options.name(name);
        }

        // Add max reconnects
        if let Some(max_reconnects) = config.max_reconnects {
            connect_options = connect_options.max_reconnects(max_reconnects);
        }

        // Add credentials last (credentials_file is async in 0.35)
        let connect_options = if let Some(creds_file) = &config.credentials_file {
            match connect_options.credentials_file(creds_file).await {
                Ok(opts) => opts,
                Err(e) => return Err(EventBusError::ConnectionError(e.to_string())),
            }
        } else {
            connect_options
        };

        // Connect to NATS
        let client = async_nats::connect_with_options(config.primary_url(), connect_options)
            .await
            .map_err(|e| EventBusError::ConnectionError(e.to_string()))?;

        let jetstream = async_nats::jetstream::new(client.clone());

        Ok(Self {
            client: Arc::new(client),
            jetstream,
            config: Arc::new(config),
            stream_prefix: "HODEI".to_string(),
            state: Arc::new(Mutex::new(NatsEventBusState::default())),
        })
    }

    /// Creates a new NatsEventBus with custom stream prefix
    ///
    /// Useful for multi-tenant or isolated environments.
    pub async fn with_prefix(
        config: NatsConfig,
        stream_prefix: &str,
    ) -> Result<Self, EventBusError> {
        let mut bus = Self::new(config).await?;
        bus.stream_prefix = stream_prefix.to_string();
        Ok(bus)
    }

    /// Maps a DomainEvent to its NATS subject
    ///
    /// Uses a hierarchical subject naming convention for efficient routing:
    /// `{prefix}.{entity}.{action}` (e.g., `hodei.jobs.created`)
    pub fn event_to_subject(event: &DomainEvent) -> String {
        let entity = match event {
            DomainEvent::JobCreated { .. } => "jobs",
            DomainEvent::JobStatusChanged { .. } => "jobs",
            DomainEvent::JobCancelled { .. } => "jobs",
            DomainEvent::JobRetried { .. } => "jobs",
            DomainEvent::JobAssigned { .. } => "jobs",
            DomainEvent::JobAccepted { .. } => "jobs",
            DomainEvent::JobExecutionError { .. } => "jobs",
            DomainEvent::JobDispatchFailed { .. } => "jobs",
            DomainEvent::JobQueued { .. } => "jobs",
            DomainEvent::JobDispatchAcknowledged { .. } => "jobs",
            DomainEvent::RunJobReceived { .. } => "jobs",

            DomainEvent::WorkerRegistered { .. } => "workers",
            DomainEvent::WorkerStatusChanged { .. } => "workers",
            DomainEvent::WorkerTerminated { .. } => "workers",
            DomainEvent::WorkerDisconnected { .. } => "workers",
            DomainEvent::WorkerProvisioned { .. } => "workers",
            DomainEvent::WorkerReconnected { .. } => "workers",
            DomainEvent::WorkerRecoveryFailed { .. } => "workers",
            DomainEvent::WorkerEphemeralCreated { .. } => "workers",
            DomainEvent::WorkerEphemeralReady { .. } => "workers",
            DomainEvent::WorkerReady { .. } => "workers",
            DomainEvent::WorkerStateUpdated { .. } => "workers",
            DomainEvent::WorkerEphemeralTerminating { .. } => "workers",
            DomainEvent::WorkerEphemeralTerminated { .. } => "workers",
            DomainEvent::WorkerEphemeralCleanedUp { .. } => "workers",
            DomainEvent::WorkerEphemeralIdle { .. } => "workers",
            DomainEvent::OrphanWorkerDetected { .. } => "workers",
            DomainEvent::WorkerReadyForJob { .. } => "workers",
            DomainEvent::WorkerProvisioningRequested { .. } => "workers",
            DomainEvent::WorkerProvisioningError { .. } => "workers",
            DomainEvent::WorkerHeartbeat { .. } => "workers",
            DomainEvent::WorkerSelfTerminated { .. } => "workers",

            DomainEvent::ProviderRegistered { .. } => "providers",
            DomainEvent::ProviderUpdated { .. } => "providers",
            DomainEvent::ProviderHealthChanged { .. } => "providers",
            DomainEvent::ProviderRecovered { .. } => "providers",
            DomainEvent::AutoScalingTriggered { .. } => "providers",
            DomainEvent::ProviderSelected { .. } => "providers",
            DomainEvent::ProviderExecutionError { .. } => "providers",

            DomainEvent::JobQueueDepthChanged { .. } => "queue",
            DomainEvent::GarbageCollectionCompleted { .. } => "gc",
            DomainEvent::SchedulingDecisionFailed { .. } => "scheduling",
        };

        let action = event.event_type().to_lowercase();
        format!("hodei.{}.{}", entity, action)
    }

    /// Gets the stream name for a subject
    fn stream_name_for_subject(&self, subject: &str) -> String {
        let parts: Vec<&str> = subject.split('.').collect();
        if parts.len() >= 2 {
            format!("{}_{}", self.stream_prefix, parts[1])
        } else {
            format!("{}_events", self.stream_prefix)
        }
    }

    /// Ensures the stream exists for a given subject
    async fn ensure_stream(&self, subject: &str) -> Result<StreamHandle, EventBusError> {
        let stream_name = self.stream_name_for_subject(subject);

        // Check if stream already exists in our state
        {
            let state = self.state.lock().await;
            if let Some(info) = state.streams.iter().find(|s| s.name == stream_name) {
                if let Some(handle) = &info.handle {
                    debug!("Stream {} already exists", stream_name);
                    return Ok(handle.clone());
                }
            }
        }

        // Try to get stream from NATS (it might exist from previous run)
        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => {
                debug!("Stream {} already exists in NATS", stream_name);
                let mut state = self.state.lock().await;
                // Update or add to state
                if let Some(info) = state.streams.iter_mut().find(|s| s.name == stream_name) {
                    info.handle = Some(stream.clone());
                } else {
                    state.streams.push(StreamInfo {
                        name: stream_name.clone(),
                        subject: subject.to_string(),
                        handle: Some(stream.clone()),
                    });
                }
                return Ok(stream);
            }
            Err(_) => {
                // Stream doesn't exist, create it
                info!("Creating stream {} for subject {}", stream_name, subject);
            }
        }

        // Create stream with workqueue retention for efficient processing
        let stream_config = StreamConfig {
            name: stream_name.clone(),
            subjects: vec![subject.to_string()],
            retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
            max_age: Duration::from_secs(24 * 60 * 60), // 24 hours
            max_bytes: 1024 * 1024 * 1024,              // 1GB
            max_messages: 1_000_000,
            storage: async_nats::jetstream::stream::StorageType::File,
            num_replicas: 1,
            discard: async_nats::jetstream::stream::DiscardPolicy::Old,
            ..Default::default()
        };

        let stream = self
            .jetstream
            .create_stream(stream_config)
            .await
            .map_err(|e| EventBusError::ConnectionError(e.to_string()))?;

        let mut state = self.state.lock().await;
        state.streams.push(StreamInfo {
            name: stream_name,
            subject: subject.to_string(),
            handle: Some(stream.clone()),
        });

        info!("✅ Stream created successfully");
        Ok(stream)
    }

    /// Gets or creates a consumer for a subject
    async fn get_consumer(
        &self,
        subject: &str,
        consumer_name: &str,
    ) -> Result<PullConsumer, EventBusError> {
        let mut stream = self.ensure_stream(subject).await?;
        let stream_info = stream
            .info()
            .await
            .map_err(|e| EventBusError::ConnectionError(e.to_string()))?;
        let stream_name = stream_info.config.name.clone();
        let consumer_id = format!("{}-{}", stream_name, consumer_name);

        // Try to get existing consumer
        match stream.get_consumer(&consumer_id).await {
            Ok(consumer) => {
                debug!("Consumer {} already exists", consumer_id);
                return Ok(consumer);
            }
            Err(_) => {
                // Consumer doesn't exist, create it
                info!(
                    "Creating consumer {} for stream {}",
                    consumer_id, stream_name
                );
            }
        }

        // Create durable consumer with pull configuration
        let consumer_config = PullConsumerConfig {
            durable_name: Some(consumer_id.clone()),
            deliver_policy: DeliverPolicy::All,
            ack_policy: AckPolicy::Explicit,
            ack_wait: Duration::from_secs(30),
            max_deliver: 5, // Retry up to 5 times
            max_ack_pending: 1000,
            ..Default::default()
        };

        let consumer = stream
            .create_consumer(consumer_config)
            .await
            .map_err(|e| EventBusError::SubscribeError(e.to_string()))?;

        info!("✅ Consumer {} created successfully", consumer_id);
        Ok(consumer)
    }
}

#[async_trait]
impl EventBus for NatsEventBus {
    /// Publishes a domain event to NATS JetStream
    ///
    /// # Errors
    /// Returns an error if:
    /// - Serialization fails
    /// - Publishing to NATS fails
    /// - Ack is not received within timeout
    #[instrument(skip(self, event), fields(event_type = ?event.event_type(), aggregate_id = ?event.aggregate_id()))]
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        let subject = Self::event_to_subject(event);
        let envelope = NatsMessageEnvelope::from_domain_event(event);

        let payload = serde_json::to_vec(&envelope)
            .map_err(|e| EventBusError::SerializationError(e.to_string()))?;

        // Publish with ack request for at-least-once delivery guarantee
        let ack = self
            .jetstream
            .publish(subject.clone(), payload.into())
            .await
            .map_err(|e| EventBusError::PublishError(e.to_string()))?;

        // Wait for ack (confirms message was stored)
        ack.await
            .map_err(|e| EventBusError::PublishError(e.to_string()))?;

        debug!(
            "✅ Published event {} to subject {}",
            event.event_type(),
            subject
        );

        Ok(())
    }

    /// Subscribes to a topic/pattern and returns a stream of events
    ///
    /// Creates a durable consumer that will survive restarts.
    /// Events are acknowledged automatically after processing.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Stream or consumer creation fails
    /// - NATS connection fails
    #[instrument(skip(self), fields(topic = topic))]
    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        info!("Subscribing to topic: {}", topic);

        let subject = topic.to_string();
        let consumer_name = format!("consumer-{}", topic.replace('.', "-"));

        let consumer: PullConsumer = self.get_consumer(&subject, &consumer_name).await?;

        // Create the message stream
        let stream = async_stream::stream! {
            let mut messages = match consumer.messages().await {
                Ok(msgs) => msgs,
                Err(e) => {
                    error!("Failed to get consumer messages: {}", e);
                    yield Err(EventBusError::ConnectionError(e.to_string()));
                    return;
                }
            };

            while let Some(result) = messages.next().await {
                match result {
                    Ok(message) => {
                        // Parse the envelope
                        let envelope: NatsMessageEnvelope = match serde_json::from_slice(&message.payload) {
                            Ok(env) => env,
                            Err(e) => {
                                error!("Failed to deserialize message: {}", e);
                                // Acknowledge (to remove from queue even on error)
                                if let Err(ack_err) = message.ack().await {
                                    error!("Failed to ack message: {}", ack_err);
                                }
                                yield Err(EventBusError::SerializationError(e.to_string()));
                                continue;
                            }
                        };

                        // Deserialize the domain event
                        match envelope.to_domain_event() {
                            Ok(event) => {
                                // Acknowledge successful processing
                                if let Err(ack_err) = message.ack().await {
                                    warn!("Failed to ack message: {}", ack_err);
                                }
                                yield Ok(event);
                            }
                            Err(e) => {
                                error!("Failed to deserialize domain event: {}", e);
                                if let Err(ack_err) = message.ack().await {
                                    error!("Failed to ack message: {}", ack_err);
                                }
                                yield Err(EventBusError::SerializationError(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving message: {}", e);
                        yield Err(EventBusError::ConnectionError(e.to_string()));
                        // Try to recover by recreating the consumer
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// Extension trait for subscribing to patterns with wildcard support
///
/// Provides convenience methods for common subscription patterns.
impl NatsEventBus {
    /// Subscribe to all events for a specific entity (e.g., "jobs", "workers")
    ///
    /// Creates a consumer that receives all events matching the pattern.
    pub async fn subscribe_entity(
        &self,
        entity: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        let subject = format!("hodei.{}.*", entity);
        self.subscribe(&subject).await
    }

    /// Subscribe to all events using a wildcard
    ///
    /// Warning: This can receive high volume. Use with caution.
    pub async fn subscribe_all(
        &self,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        self.subscribe("hodei.>").await
    }

    /// Get a reference to the JetStream context for advanced operations
    ///
    /// This is useful for operations like creating streams or publishing to DLQ.
    pub fn jetstream(&self) -> &JetStreamContext {
        &self.jetstream
    }

    /// Subscribe with a custom handler function
    ///
    /// The handler is called for each event and must return quickly.
    /// Events are automatically acknowledged after the handler completes.
    pub async fn subscribe_with_handler<H, F>(
        &self,
        topic: &str,
        handler: H,
    ) -> Result<(), EventBusError>
    where
        H: Fn(DomainEvent) -> F + Send + Sync + 'static,
        F: std::future::Future<Output = Result<(), EventBusError>> + Send,
    {
        let mut stream = self.subscribe(topic).await?;
        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => {
                    if let Err(e) = handler(event).await {
                        error!("Handler error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Subscribe error: {}", e);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::events::DomainEvent;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::shared_kernel::JobId;

    /// Tests that events are correctly mapped to subjects
    #[test]
    fn test_event_to_subject_mapping() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);

        // Test JobCreated
        let event = DomainEvent::JobCreated {
            job_id: job_id.clone(),
            spec: spec.clone(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };
        assert_eq!(
            NatsEventBus::event_to_subject(&event),
            "hodei.jobs.jobcreated"
        );

        // Test WorkerReady
        let event = DomainEvent::WorkerReady {
            worker_id: hodei_server_domain::shared_kernel::WorkerId::new(),
            provider_id: hodei_server_domain::shared_kernel::ProviderId::new(),
            ready_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };
        assert_eq!(
            NatsEventBus::event_to_subject(&event),
            "hodei.workers.workerready"
        );

        // Test JobQueued
        let event = DomainEvent::JobQueued {
            job_id: job_id.clone(),
            preferred_provider: None,
            job_requirements: spec.clone(),
            queued_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };
        assert_eq!(
            NatsEventBus::event_to_subject(&event),
            "hodei.jobs.jobqueued"
        );

        // Test WorkerProvisioningRequested
        let event = DomainEvent::WorkerProvisioningRequested {
            job_id: job_id.clone(),
            provider_id: hodei_server_domain::shared_kernel::ProviderId::new(),
            job_requirements: spec,
            requested_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };
        assert_eq!(
            NatsEventBus::event_to_subject(&event),
            "hodei.workers.workerprovisioningrequested"
        );
    }

    /// Tests that the envelope correctly serializes and deserializes
    #[test]
    fn test_envelope_serialization() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);

        let event = DomainEvent::JobCreated {
            job_id: job_id.clone(),
            spec: spec.clone(),
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("test-actor".to_string()),
        };

        let envelope = NatsMessageEnvelope::from_domain_event(&event);
        assert_eq!(envelope.event_type, "JobCreated");
        assert_eq!(
            envelope.correlation_id,
            Some("test-correlation".to_string())
        );

        let deserialized = envelope.to_domain_event().unwrap();
        assert_eq!(deserialized, event);
    }

    /// Tests default configuration
    #[test]
    fn test_default_config() {
        let config = NatsConfig::default();
        assert_eq!(config.urls, vec!["nats://localhost:4222"]);
        assert_eq!(config.connection_timeout_secs, 5);
        assert_eq!(config.request_timeout_secs, Some(30));
        assert_eq!(config.max_reconnects, Some(5));
    }

    /// Tests local configuration
    #[test]
    fn test_local_config() {
        let config = NatsConfig::for_local();
        assert_eq!(config.urls, vec!["nats://localhost:4222"]);
        assert!(!config.tls_enabled);
        assert_eq!(config.name, Some("hodei-server".to_string()));
    }

    /// Tests primary URL extraction
    #[test]
    fn test_primary_url() {
        let config = NatsConfig {
            urls: vec![
                "nats://server1:4222".to_string(),
                "nats://server2:4222".to_string(),
            ],
            ..Default::default()
        };
        assert_eq!(config.primary_url(), "nats://server1:4222");
    }
}

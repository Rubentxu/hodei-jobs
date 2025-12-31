//! Saga Event Consumer with NATS JetStream
//!
//! This module provides reactive saga execution by subscribing to
//! domain events via NATS JetStream and triggering appropriate sagas.

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::saga::SagaOrchestrator;
use hodei_server_domain::shared_kernel::DomainError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Configuration for saga event consumer
#[derive(Debug, Clone)]
pub struct SagaConsumerConfig {
    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Topic to subscribe to
    pub topic: String,

    /// Durable consumer name
    pub durable_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Maximum events to process concurrently
    pub concurrency: usize,

    /// Processing timeout per event
    pub processing_timeout: Duration,

    /// Enable dead letter queue
    pub enable_dlq: bool,

    /// Retry configuration
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub max_retry_delay: Duration,
}

impl Default for SagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_group: "saga-processors".to_string(),
            topic: "hodei_events".to_string(),
            durable_name: "saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            concurrency: 10,
            processing_timeout: Duration::from_secs(60),
            enable_dlq: true,
            max_retries: 3,
            retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_secs(60),
        }
    }
}

/// Result of saga trigger processing
#[derive(Debug)]
pub enum SagaTriggerResult {
    /// Saga was triggered successfully
    Triggered,
    /// Saga trigger was ignored (not applicable)
    Ignored,
    /// Saga trigger failed
    Failed(String),
}

/// Saga Event Consumer
///
/// Consumes events from NATS JetStream and triggers appropriate sagas.
#[derive(Clone)]
pub struct NatsSagaConsumer<SO>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    /// NATS client
    client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator for executing sagas
    orchestrator: Arc<SO>,

    /// Consumer configuration
    config: SagaConsumerConfig,

    /// Shutdown signal sender
    shutdown_tx: mpsc::Sender<()>,
}

impl<SO> NatsSagaConsumer<SO>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    /// Create a new NatsSagaConsumer
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<SO>,
        config: Option<SagaConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        Self {
            client,
            jetstream,
            orchestrator,
            config,
            shutdown_tx,
        }
    }

    /// Start consuming events
    pub async fn start(&self) -> Result<(), EventBusError> {
        info!(
            "📡 NatsSagaConsumer: Starting subscription to topic '{}' (group: '{}')",
            self.config.topic, self.config.consumer_group
        );

        // Create or get the stream
        let mut stream = self.ensure_stream().await?;

        let stream_info = stream
            .info()
            .await
            .map_err(|e| EventBusError::SubscribeError(e.to_string()))?;
        let stream_name = stream_info.config.name.clone();
        let consumer_id = format!("{}-{}", stream_name, self.config.durable_name);

        // Create durable consumer with pull configuration
        let consumer_config = PullConsumerConfig {
            durable_name: Some(consumer_id.clone()),
            deliver_policy: DeliverPolicy::All,
            ack_policy: AckPolicy::Explicit,
            ack_wait: Duration::from_secs(30),
            max_deliver: (self.config.max_retries + 1) as i64,
            max_ack_pending: self.config.concurrency as i64,
            ..Default::default()
        };

        let _consumer = stream
            .create_consumer(consumer_config)
            .await
            .map_err(|e| EventBusError::SubscribeError(e.to_string()))?;

        info!(
            "📡 NatsSagaConsumer: Created consumer '{}' for stream '{}'",
            consumer_id, stream_name
        );

        Ok(())
    }

    /// Ensure the stream exists
    async fn ensure_stream(&self) -> Result<async_nats::jetstream::stream::Stream, EventBusError> {
        let stream_name = format!("{}_SAGA_EVENTS", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => Ok(stream),
            Err(_) => {
                // Create stream with work queue retention
                let stream = self
                    .jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.clone(),
                        subjects: vec![self.config.topic.clone()],
                        retention: RetentionPolicy::WorkQueue,
                        max_messages: 100000,
                        max_bytes: 100 * 1024 * 1024, // 100MB
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| EventBusError::SubscribeError(e.to_string()))?;

                info!("📡 NatsSagaConsumer: Created stream '{}'", stream_name);
                Ok(stream)
            }
        }
    }

    /// Stop the consumer gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(()).await;
        info!("📡 NatsSagaConsumer: Stop signal sent");
    }
}

/// Builder for NatsSagaConsumer
#[derive(Debug)]
pub struct NatsSagaConsumerBuilder<SO>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<SO>>,
    config: Option<SagaConsumerConfig>,
}

impl<SO> Default for NatsSagaConsumerBuilder<SO>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            client: None,
            jetstream: None,
            orchestrator: None,
            config: None,
        }
    }
}

impl<SO> NatsSagaConsumerBuilder<SO>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_jetstream(mut self, jetstream: JetStreamContext) -> Self {
        self.jetstream = Some(jetstream);
        self
    }

    pub fn with_orchestrator(mut self, orchestrator: Arc<SO>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_config(mut self, config: SagaConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<NatsSagaConsumer<SO>> {
        let client = self
            .client
            .ok_or_else(|| anyhow::anyhow!("client is required"))?;
        let jetstream = self
            .jetstream
            .ok_or_else(|| anyhow::anyhow!("jetstream is required"))?;
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| anyhow::anyhow!("orchestrator is required"))?;

        Ok(NatsSagaConsumer::new(
            client,
            jetstream,
            orchestrator,
            self.config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saga_trigger_result_variants() {
        let triggered = SagaTriggerResult::Triggered;
        let ignored = SagaTriggerResult::Ignored;
        let failed = SagaTriggerResult::Failed("error".to_string());

        assert!(matches!(triggered, SagaTriggerResult::Triggered));
        assert!(matches!(ignored, SagaTriggerResult::Ignored));
        assert!(matches!(failed, SagaTriggerResult::Failed(_)));
    }

    #[test]
    fn test_config_default_values() {
        let config = SagaConsumerConfig::default();
        assert_eq!(config.topic, "hodei_events");
        assert_eq!(config.consumer_group, "saga-processors");
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.max_retries, 3);
    }
}

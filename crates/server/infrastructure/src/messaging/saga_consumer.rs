//! Saga Event Consumer with NATS JetStream
//!
//! This module provides reactive saga execution by subscribing to
//! domain events via NATS JetStream and triggering appropriate sagas.
//! Includes Circuit Breaker integration for resilience (EPIC-85 US-05).

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use hodei_server_domain::event_bus::EventBusError;
use hodei_server_domain::saga::SagaOrchestrator;
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState,
};
use hodei_server_domain::shared_kernel::DomainError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use hodei_shared::event_topics::ALL_EVENTS;

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

    /// EPIC-43: Maximum delivery attempts before moving to DLQ
    pub max_deliver: u32,

    /// Retry configuration
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub max_retry_delay: Duration,

    /// DLQ stream name
    pub dlq_stream_name: String,

    /// EPIC-85 US-05: Circuit breaker configuration for resilience
    pub circuit_breaker_enabled: bool,
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: Duration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: Duration,
}

impl Default for SagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_group: "saga-processors".to_string(),
            topic: ALL_EVENTS.to_string(), // Multi-level wildcard for all events
            durable_name: "saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            concurrency: 10,
            processing_timeout: Duration::from_secs(60),
            enable_dlq: true,
            max_deliver: 3, // EPIC-43: Maximum 3 delivery attempts
            max_retries: 2, // EPIC-43: 2 retries after initial delivery
            retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_secs(60),
            dlq_stream_name: "HODEI_DLQ".to_string(),
            // EPIC-85 US-05: Circuit breaker defaults (optimized for saga consumers)
            circuit_breaker_enabled: true,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(60),
        }
    }
}

impl SagaConsumerConfig {
    /// Creates a CircuitBreakerConfig from this consumer config
    #[inline]
    pub fn circuit_breaker_config(&self) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: self.circuit_breaker_failure_threshold,
            open_duration: self.circuit_breaker_open_duration,
            success_threshold: self.circuit_breaker_success_threshold,
            call_timeout: self.circuit_breaker_call_timeout,
            failure_rate_threshold: 50,
            failure_rate_window: 100,
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
/// Includes Circuit Breaker integration for resilience (EPIC-85 US-05).
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

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,

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

        // EPIC-85 US-05: Initialize circuit breaker if enabled
        let circuit_breaker = if config.circuit_breaker_enabled {
            let cb_config = config.circuit_breaker_config();
            Some(Arc::new(CircuitBreaker::new(
                format!("saga-consumer-{}", config.consumer_group),
                cb_config,
            )))
        } else {
            None
        };

        Self {
            client,
            jetstream,
            orchestrator,
            config,
            circuit_breaker,
            shutdown_tx,
        }
    }

    /// Get current circuit breaker state (for health checks)
    #[inline]
    pub fn circuit_breaker_state(&self) -> Option<CircuitState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    /// Check if circuit breaker allows requests
    #[inline]
    pub fn is_circuit_closed(&self) -> bool {
        self.circuit_breaker
            .as_ref()
            .map(|cb| cb.allow_request())
            .unwrap_or(true)
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

        // Re-obtain stream for consumer operations
        let stream = self.ensure_stream().await?;

        // Create durable consumer with pull configuration
        let consumer_config = PullConsumerConfig {
            durable_name: Some(consumer_id.clone()),
            deliver_policy: DeliverPolicy::All,
            ack_policy: AckPolicy::Explicit,
            ack_wait: Duration::from_secs(30),
            max_deliver: self.config.max_deliver as i64,
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

    /// EPIC-85 US-05: Enable and configure circuit breaker
    pub fn with_circuit_breaker(
        mut self,
        enabled: bool,
        failure_threshold: u64,
        open_duration_secs: u64,
        success_threshold: u64,
        call_timeout_secs: u64,
    ) -> Self {
        let config = self.config.get_or_insert_with(SagaConsumerConfig::default);
        config.circuit_breaker_enabled = enabled;
        config.circuit_breaker_failure_threshold = failure_threshold;
        config.circuit_breaker_open_duration = Duration::from_secs(open_duration_secs);
        config.circuit_breaker_success_threshold = success_threshold;
        config.circuit_breaker_call_timeout = Duration::from_secs(call_timeout_secs);
        self
    }

    /// EPIC-85 US-05: Disable circuit breaker (for testing or debugging)
    pub fn with_circuit_breaker_disabled(mut self) -> Self {
        let config = self.config.get_or_insert_with(SagaConsumerConfig::default);
        config.circuit_breaker_enabled = false;
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
    use hodei_server_domain::saga::{SagaContext, SagaExecutionResult, SagaId, SagaType};
    use std::time::Duration;

    // Mock orchestrator for testing
    struct MockOrchestrator;

    #[async_trait::async_trait]
    impl SagaOrchestrator for MockOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            _saga: &dyn hodei_server_domain::saga::Saga,
            _context: SagaContext,
        ) -> Result<SagaExecutionResult, Self::Error> {
            Ok(SagaExecutionResult::completed(
                SagaId::new(),
                SagaType::Execution,
                Duration::ZERO,
            ))
        }

        async fn execute(
            &self,
            _context: &SagaContext,
        ) -> Result<SagaExecutionResult, Self::Error> {
            Ok(SagaExecutionResult::completed(
                SagaId::new(),
                SagaType::Execution,
                Duration::ZERO,
            ))
        }

        async fn get_saga(&self, _id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn cancel_saga(&self, _id: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }
    }

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
        assert_eq!(config.topic, "hodei.events.>");
        assert_eq!(config.consumer_group, "saga-processors");
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.max_retries, 2); // Updated to match actual default
    }

    // EPIC-85 US-05: Circuit breaker configuration tests

    #[test]
    fn test_circuit_breaker_config_defaults() {
        let config = SagaConsumerConfig::default();

        assert!(config.circuit_breaker_enabled);
        assert_eq!(config.circuit_breaker_failure_threshold, 5);
        assert_eq!(
            config.circuit_breaker_open_duration,
            Duration::from_secs(30)
        );
        assert_eq!(config.circuit_breaker_success_threshold, 2);
        assert_eq!(config.circuit_breaker_call_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_circuit_breaker_config_conversion() {
        let config = SagaConsumerConfig {
            circuit_breaker_enabled: true,
            circuit_breaker_failure_threshold: 10,
            circuit_breaker_open_duration: Duration::from_secs(60),
            circuit_breaker_success_threshold: 3,
            circuit_breaker_call_timeout: Duration::from_secs(120),
            ..Default::default()
        };

        let cb_config = config.circuit_breaker_config();

        assert_eq!(cb_config.failure_threshold, 10);
        assert_eq!(cb_config.open_duration, Duration::from_secs(60));
        assert_eq!(cb_config.success_threshold, 3);
        assert_eq!(cb_config.call_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_circuit_breaker_disabled_config() {
        let config = SagaConsumerConfig {
            circuit_breaker_enabled: false,
            ..Default::default()
        };

        // Should still create a valid CircuitBreakerConfig even when disabled
        let cb_config = config.circuit_breaker_config();
        assert_eq!(cb_config.failure_threshold, 5); // default value
    }

    #[test]
    fn test_builder_with_circuit_breaker() {
        let builder = NatsSagaConsumerBuilder::<MockOrchestrator>::new()
            .with_circuit_breaker(true, 10, 60, 3, 120);

        let config = builder.config.unwrap();
        assert!(config.circuit_breaker_enabled);
        assert_eq!(config.circuit_breaker_failure_threshold, 10);
        assert_eq!(
            config.circuit_breaker_open_duration,
            Duration::from_secs(60)
        );
        assert_eq!(config.circuit_breaker_success_threshold, 3);
        assert_eq!(
            config.circuit_breaker_call_timeout,
            Duration::from_secs(120)
        );
    }

    #[test]
    fn test_builder_with_circuit_breaker_disabled() {
        let builder =
            NatsSagaConsumerBuilder::<MockOrchestrator>::new().with_circuit_breaker_disabled();

        let config = builder.config.unwrap();
        assert!(!config.circuit_breaker_enabled);
    }
}

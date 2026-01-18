//! CommandRelay - Hybrid LISTEN/NOTIFY + polling relay for commands
//!
//! This module provides a relay for processing commands from the hodei_commands table
//! using PostgreSQL LISTEN/NOTIFY for reactive notifications combined with polling
//! as a fallback mechanism.
//!
//! EPIC-63: Command Relay completo

use crate::persistence::command_outbox::PostgresCommandOutboxRepository;
use hodei_server_domain::command::{CommandOutboxRecord, CommandOutboxRepository};
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use hodei_server_domain::shared_kernel::DomainError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

/// Configuration for the command relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRelayConfig {
    /// Maximum number of commands to process in a single batch
    pub batch_size: usize,
    /// How often to poll for new commands (when queue is empty)
    pub poll_interval_ms: u64,
    /// Channel name for notifications
    pub channel: String,
    /// Maximum retries before dead-lettering
    pub max_retries: i32,

    /// EPIC-85 US-05: Circuit breaker configuration
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: Duration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: Duration,
}

impl Default for CommandRelayConfig {
    fn default() -> Self {
        Self {
            batch_size: 20,
            poll_interval_ms: 100,
            channel: "outbox_work".to_string(),
            max_retries: 3,
            // EPIC-85 US-05: Circuit breaker defaults
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(10),
        }
    }
}

/// Metrics collected by the command relay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRelayMetrics {
    pub commands_processed_total: u64,
    pub commands_completed_total: u64,
    pub commands_failed_total: u64,
    pub commands_retried_total: u64,
    pub commands_dead_lettered_total: u64,
    pub notifications_received: u64,
    pub polling_wakeups: u64,
    pub batch_count: u64,
}

impl CommandRelayMetrics {
    pub fn record_completed(&mut self) {
        self.commands_completed_total += 1;
        self.commands_processed_total += 1;
    }

    pub fn record_failed(&mut self) {
        self.commands_failed_total += 1;
        self.commands_processed_total += 1;
    }

    pub fn record_retry(&mut self) {
        self.commands_retried_total += 1;
    }

    pub fn record_dead_letter(&mut self) {
        self.commands_dead_lettered_total += 1;
    }

    pub fn record_notification(&mut self) {
        self.notifications_received += 1;
    }

    pub fn record_polling_wakeup(&mut self) {
        self.polling_wakeups += 1;
        self.batch_count += 1;
    }
}

/// Command processor trait.
#[async_trait::async_trait]
pub trait CommandProcessor: Send + Sync {
    async fn process_command(
        &self,
        command: &CommandOutboxRecord,
    ) -> Result<(), CommandProcessorError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CommandProcessorError {
    #[error("Handler error: {message}")]
    HandlerError { message: String },
    #[error("Retryable error: {message}")]
    RetryableError { message: String },
    #[error("Permanent error: {message}")]
    PermanentError { message: String },
}

/// Command Relay
///
/// A relay that processes commands from the hodei_commands table using
/// a hybrid LISTEN/NOTIFY + polling approach.
#[derive(Debug)]
pub struct CommandRelay<R: CommandOutboxRepository> {
    pool: PgPool,
    repository: Arc<R>,
    nats_client: Arc<async_nats::Client>,
    config: CommandRelayConfig,
    metrics: Arc<Mutex<CommandRelayMetrics>>,
    shutdown: broadcast::Sender<()>,
    listener: Option<crate::messaging::hybrid::PgNotifyListener>,

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl<R: CommandOutboxRepository> CommandRelay<R> {
    /// Create a new command relay.
    pub async fn new(
        pool: &PgPool,
        repository: Arc<R>,
        nats_client: Arc<async_nats::Client>,
        config: Option<CommandRelayConfig>,
    ) -> Result<(Self, broadcast::Sender<()>), sqlx::Error> {
        let config = config.unwrap_or_default();
        let (shutdown, _rx) = broadcast::channel(1);

        // Try to create listener, store in struct
        let listener = match crate::messaging::hybrid::PgNotifyListener::new(pool, &config.channel)
            .await
        {
            Ok(l) => Some(l),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create LISTEN/NOTIFY listener, polling-only mode");
                None
            }
        };

        // EPIC-85 US-05: Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("command-relay-{}", config.channel),
            CircuitBreakerConfig {
                failure_threshold: config.circuit_breaker_failure_threshold,
                open_duration: config.circuit_breaker_open_duration,
                success_threshold: config.circuit_breaker_success_threshold,
                call_timeout: config.circuit_breaker_call_timeout,
                failure_rate_threshold: 50,
                failure_rate_window: 100,
            },
        ));

        Ok((
            Self {
                pool: pool.clone(),
                repository,
                nats_client,
                config,
                metrics: Arc::new(Mutex::new(CommandRelayMetrics::default())),
                shutdown: shutdown.clone(),
                listener,
                circuit_breaker: Some(circuit_breaker),
            },
            shutdown,
        ))
    }

    /// EPIC-85 US-05: Get current circuit breaker state
    pub fn circuit_breaker_state(&self) -> CircuitState {
        self.circuit_breaker
            .as_ref()
            .map(|cb| cb.state())
            .unwrap_or(CircuitState::Closed)
    }

    /// EPIC-85 US-05: Check if circuit allows requests
    pub fn is_circuit_closed(&self) -> bool {
        self.circuit_breaker
            .as_ref()
            .map(|cb| cb.allow_request())
            .unwrap_or(true)
    }

    /// Run the command relay.
    ///
    /// Uses LISTEN/NOTIFY for reactive processing when available.
    /// Falls back to watchdog polling at 30s intervals when no listener is present.
    /// This prevents unnecessary DB load when the system is operating reactively.
    pub async fn run(mut self) {
        let mut watchdog_interval = interval(Duration::from_secs(30));
        let mut shutdown_rx = self.shutdown.subscribe();
        let has_listener = self.listener.is_some();

        // Log the operating mode
        if has_listener {
            info!(
                channel = self.config.channel,
                "Starting command relay in REACTIVE mode (LISTEN/NOTIFY)"
            );
        } else {
            warn!(
                channel = self.config.channel,
                "No notification listener available. Starting in DEGRADED mode with 30s watchdog polling"
            );
        }

        loop {
            tokio::select! {
                notification = self.recv_notification() => {
                    match notification {
                        Ok(Some(_)) => {
                            self.metrics.lock().await.record_notification();
                            self.process_pending_batch().await;
                        }
                        Ok(None) => {
                            // No listener available, this is expected in degraded mode
                        }
                        Err(e) => {
                            warn!(error = %e, "Notification error");
                            self.process_pending_batch().await;
                        }
                    }
                }
                _ = watchdog_interval.tick() => {
                    // Only poll in degraded mode (no listener)
                    if !has_listener {
                        self.metrics.lock().await.record_polling_wakeup();
                        info!("Watchdog tick: checking for pending commands in degraded mode");
                        self.process_pending_batch().await;
                    }
                    // In reactive mode, watchdog does nothing (notifications drive processing)
                }
                _ = shutdown_rx.recv() => {
                    info!("Command relay shutting down");
                    break;
                }
            }
        }
    }

    async fn recv_notification(
        &mut self,
    ) -> Result<Option<sqlx::postgres::PgNotification>, sqlx::Error> {
        if let Some(ref mut listener) = self.listener {
            listener.recv().await.map(Some)
        } else {
            Ok(None)
        }
    }

    /// EPIC-85 US-05: Calculate exponential backoff delay with jitter
    fn calculate_backoff(&self, retry_count: i32) -> Duration {
        let base = self.config.poll_interval_ms as f64 * (2.0_f64.powf(retry_count as f64));
        let base_secs = base / 1000.0;

        // Add jitter (±25% of the base delay)
        let jitter_range = base_secs * 0.25;
        let jitter = (rand::random::<f64>() * 2.0 - 1.0) * jitter_range;
        let final_delay = (base_secs + jitter).max(0.1); // Ensure at least 100ms

        Duration::from_secs_f64(final_delay)
    }

    /// Process a batch of pending commands.
    pub async fn process_pending_batch(&self) {
        let commands = match self
            .repository
            .get_pending_commands(self.config.batch_size, self.config.max_retries)
            .await
        {
            Ok(commands) => commands,
            Err(e) => {
                error!(error = %e, "Failed to fetch pending commands");
                return;
            }
        };

        if commands.is_empty() {
            return;
        }

        debug!(count = commands.len(), "Processing pending commands batch");

        // EPIC-85 US-05: Process commands in parallel using FuturesUnordered
        use futures::StreamExt;
        use futures::stream::FuturesUnordered;

        let tasks = FuturesUnordered::new();
        for command in commands {
            tasks.push(self.process_command_with_retry(command));
        }

        let mut results = tasks;
        while let Some(result) = results.next().await {
            let mut metrics = self.metrics.lock().await;
            match result {
                Ok(_) => {
                    metrics.record_completed();
                }
                Err(CommandProcessorError::RetryableError { .. }) => {
                    metrics.record_retry();
                }
                Err(CommandProcessorError::PermanentError { .. }) => {
                    metrics.record_dead_letter();
                }
                Err(CommandProcessorError::HandlerError { .. }) => {
                    metrics.record_failed();
                }
            }
        }
    }

    /// EPIC-85 US-05: Process a single command with retry logic and exponential backoff
    async fn process_command_with_retry(
        &self,
        command: CommandOutboxRecord,
    ) -> Result<(), CommandProcessorError> {
        let mut attempts = 0;
        let max_attempts = self.config.max_retries;

        loop {
            match self.process_single_command(&command).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        error!(
                            command_id = %command.id,
                            attempts = attempts,
                            "Command exceeded max retries: {}", e
                        );

                        // EPIC-85 US-05: Mark as failed in DB when max retries exceeded
                        let _ = self
                            .repository
                            .mark_failed(&command.id, &e.to_string())
                            .await;

                        // If it's a retryable error that exceeded max retries, treat as permanent
                        return Err(CommandProcessorError::PermanentError {
                            message: format!("Max retries exceeded: {}", e),
                        });
                    }

                    let delay = self.calculate_backoff(attempts);
                    warn!(
                        command_id = %command.id,
                        attempt = attempts,
                        delay_ms = delay.as_millis(),
                        "Command failed, retrying: {}", e
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Process a single command.
    async fn process_single_command(
        &self,
        command: &CommandOutboxRecord,
    ) -> Result<(), CommandProcessorError> {
        debug!(
            command_id = %command.id,
            command_type = command.command_type,
            "Processing command"
        );

        // 1. Determine NATS subject based on command type (and potentially target)
        // Convention: hodei.command.{command_type_lowercase}
        // e.g. AssignWorkerCommand -> hodei.command.assignworker
        // This allows consumers to subscribe specifically to commands they handle
        let subject = format!("hodei.command.{}", command.command_type.to_lowercase());

        // 2. Serialize payload (it's already JSON Value, so we just stringify it)
        // We wrap it in a standard envelope if needed, but for now raw payload is fine
        // as the consumers expect serialized command structs. Only commands are needed.
        let payload = serde_json::to_vec(&command.payload).map_err(|e| {
            CommandProcessorError::PermanentError {
                message: format!("Serialization error: {}", e),
            }
        })?;

        // 3. Publish to NATS with circuit breaker protection
        // EPIC-85 US-05: Wrap publication in circuit breaker
        let publication_result = if let Some(ref cb) = self.circuit_breaker {
            let nats_client = self.nats_client.clone();
            let subject_clone = subject.clone();
            let payload_clone = payload.clone();

            cb.execute(async move {
                nats_client
                    .publish(subject_clone, payload_clone.into())
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("NATS publish failed: {}", e),
                    })
            })
            .await
            .map_err(|e| match e {
                CircuitBreakerError::Open => CommandProcessorError::RetryableError {
                    message: "Circuit breaker open".to_string(),
                },
                CircuitBreakerError::Timeout => CommandProcessorError::RetryableError {
                    message: "Circuit breaker timeout".to_string(),
                },
                CircuitBreakerError::Failed(e) => CommandProcessorError::RetryableError {
                    message: e.to_string(),
                },
            })
        } else {
            self.nats_client
                .publish(subject.clone(), payload.into())
                .await
                .map_err(|e| CommandProcessorError::RetryableError {
                    message: format!("NATS publish failed: {}", e),
                })
        };

        // Record success or failure in circuit breaker
        if let Some(ref cb) = self.circuit_breaker {
            match publication_result {
                Ok(_) => cb.record_success(),
                Err(_) => cb.record_failure(),
            }
        }

        publication_result?;

        // 4. Mark as completed in DB
        self.repository
            .mark_completed(&command.id)
            .await
            .map_err(|e| CommandProcessorError::HandlerError {
                message: e.to_string(),
            })?;

        debug!(
            command_id = %command.id,
            subject = %subject,
            "Command processed successfully (Published to NATS)"
        );
        Ok(())
    }

    /// Get current metrics.
    pub async fn metrics(&self) -> CommandRelayMetrics {
        self.metrics.lock().await.clone()
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

/// Create a CommandRelay with the default PostgresCommandOutboxRepository.
pub async fn create_command_relay(
    pool: &PgPool,
    nats_client: Arc<async_nats::Client>,
    config: Option<CommandRelayConfig>,
) -> Result<
    (
        CommandRelay<PostgresCommandOutboxRepository>,
        broadcast::Sender<()>,
    ),
    sqlx::Error,
> {
    let repository = Arc::new(PostgresCommandOutboxRepository::new(pool.clone()));
    CommandRelay::new(pool, repository, nats_client, config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::command::{
        CommandOutboxInsert, CommandOutboxStatus, CommandTargetType,
    };
    use uuid::Uuid;

    // Mock repository for testing
    #[derive(Debug, Clone)]
    struct MockCommandRepository {
        commands: Arc<Mutex<Vec<CommandOutboxRecord>>>,
    }

    impl MockCommandRepository {
        fn new() -> Self {
            Self {
                commands: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl CommandOutboxRepository for MockCommandRepository {
        async fn insert_command(
            &self,
            _command: &CommandOutboxInsert,
        ) -> Result<Uuid, hodei_server_domain::command::CommandOutboxError> {
            Ok(Uuid::new_v4())
        }

        async fn get_pending_commands(
            &self,
            limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<CommandOutboxRecord>, hodei_server_domain::command::CommandOutboxError>
        {
            let commands = self.commands.lock().await;
            Ok(commands
                .iter()
                .filter(|c| matches!(c.status, CommandOutboxStatus::Pending))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_completed(
            &self,
            command_id: &Uuid,
        ) -> Result<(), hodei_server_domain::command::CommandOutboxError> {
            let mut commands = self.commands.lock().await;
            for cmd in commands.iter_mut() {
                if &cmd.id == command_id {
                    cmd.status = CommandOutboxStatus::Completed;
                }
            }
            Ok(())
        }

        async fn mark_failed(
            &self,
            _command_id: &Uuid,
            _error: &str,
        ) -> Result<(), hodei_server_domain::command::CommandOutboxError> {
            Ok(())
        }

        async fn exists_by_idempotency_key(
            &self,
            _key: &str,
        ) -> Result<bool, hodei_server_domain::command::CommandOutboxError> {
            Ok(false)
        }

        async fn get_stats(
            &self,
        ) -> Result<
            hodei_server_domain::command::CommandOutboxStats,
            hodei_server_domain::command::CommandOutboxError,
        > {
            let commands = self.commands.lock().await;
            Ok(hodei_server_domain::command::CommandOutboxStats {
                pending_count: commands
                    .iter()
                    .filter(|c| matches!(c.status, CommandOutboxStatus::Pending))
                    .count() as u64,
                completed_count: commands
                    .iter()
                    .filter(|c| matches!(c.status, CommandOutboxStatus::Completed))
                    .count() as u64,
                failed_count: commands
                    .iter()
                    .filter(|c| matches!(c.status, CommandOutboxStatus::Failed))
                    .count() as u64,
                oldest_pending_age_seconds: None,
            })
        }
    }

    #[test]
    fn test_relay_config_defaults() {
        let config = CommandRelayConfig::default();
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.poll_interval_ms, 100);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_metrics_defaults() {
        let metrics = CommandRelayMetrics::default();
        assert_eq!(metrics.commands_processed_total, 0);
        assert_eq!(metrics.commands_completed_total, 0);
    }

    #[test]
    fn test_metrics_record_completed() {
        let mut metrics = CommandRelayMetrics::default();
        metrics.record_completed();
        assert_eq!(metrics.commands_completed_total, 1);
        assert_eq!(metrics.commands_processed_total, 1);
    }

    #[test]
    fn test_metrics_record_failed() {
        let mut metrics = CommandRelayMetrics::default();
        metrics.record_failed();
        assert_eq!(metrics.commands_failed_total, 1);
        assert_eq!(metrics.commands_processed_total, 1);
    }

    #[test]
    fn test_metrics_record_dead_letter() {
        let mut metrics = CommandRelayMetrics::default();
        metrics.record_dead_letter();
        assert_eq!(metrics.commands_dead_lettered_total, 1);
    }

    #[test]
    fn test_metrics_record_retry() {
        let mut metrics = CommandRelayMetrics::default();
        metrics.record_retry();
        assert_eq!(metrics.commands_retried_total, 1);
    }

    #[tokio::test]
    #[ignore]
    async fn test_relay_with_mock_repository() {
        let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let repo = Arc::new(MockCommandRepository::new());
        let config = Some(CommandRelayConfig {
            batch_size: 10,
            poll_interval_ms: 100,
            channel: "test_command_channel".to_string(),
            max_retries: 3,
            // EPIC-85 US-05: Added missing fields
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(10),
        });

        // Use demo NATS for tests if needed, or ignore
        let client = async_nats::connect("demo.nats.io").await.unwrap();
        let (relay, _rx) = CommandRelay::new(&pool, repo, Arc::new(client), config)
            .await
            .unwrap();

        let metrics = relay.metrics().await;
        assert_eq!(metrics.commands_processed_total, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_shutdown_signal() {
        let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let repo = Arc::new(MockCommandRepository::new());
        let client = async_nats::connect("demo.nats.io").await.unwrap();
        let (relay, shutdown_tx) = CommandRelay::new(&pool, repo, Arc::new(client), None)
            .await
            .unwrap();

        let mut rx = shutdown_tx.subscribe();

        relay.shutdown();

        // The receiver should receive the shutdown signal
        let result = rx.recv().await;
        assert!(result.is_ok());
    }
}

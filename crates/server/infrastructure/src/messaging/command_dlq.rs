//! Command Dead Letter Queue (DLQ) Implementation for Saga Commands
//!
//! This module provides a DLQ mechanism for handling saga commands that cannot be
//! processed successfully after multiple retry attempts. Commands are moved
//! to a DLQ stream for later inspection and manual intervention.
//!
//! # Architecture
//!
//! The Command DLQ works by:
//! 1. Intercepting failed command processing in saga command consumers
//! 2. Tracking retry counts per command
//! 3. Moving commands exceeding max retries to the DLQ stream
//! 4. Logging DLQ entries for observability

use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::stream::Config as StreamConfig;
use chrono::{DateTime, Utc};
use hodei_server_domain::command::Command;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

/// Maximum number of retry attempts before moving to DLQ
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default DLQ subject for commands
const DEFAULT_DLQ_SUBJECT: &str = "hodei.commands.dlq";

/// Configuration for Command DLQ behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDlqConfig {
    /// Stream prefix for DLQ streams
    pub stream_prefix: String,
    /// Subject for DLQ commands
    pub dlq_subject: String,
    /// Maximum retry attempts before DLQ
    pub max_retries: u32,
    /// DLQ stream retention period in days
    pub retention_days: i64,
    /// Whether DLQ is enabled
    pub enabled: bool,
    /// Delay between retries (milliseconds)
    pub retry_delay_ms: u64,
}

impl Default for CommandDlqConfig {
    fn default() -> Self {
        Self {
            stream_prefix: "HODEI".to_string(),
            dlq_subject: DEFAULT_DLQ_SUBJECT.to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
            retention_days: 7,
            enabled: true,
            retry_delay_ms: 1000,
        }
    }
}

impl CommandDlqConfig {
    /// Create a new config with custom values
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable the DLQ
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set retention days
    pub fn with_retention_days(mut self, days: i64) -> Self {
        self.retention_days = days;
        self
    }

    /// Set retry delay
    pub fn with_retry_delay(mut self, ms: u64) -> Self {
        self.retry_delay_ms = ms;
        self
    }
}

/// A DLQ entry containing the failed command and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDlqEntry<C: Command> {
    /// Original command that failed processing
    pub command: C,
    /// Number of retry attempts made
    pub retry_count: u32,
    /// Error message from last failure
    pub last_error: String,
    /// All errors encountered during retries
    pub error_history: Vec<String>,
    /// When the command was first received
    pub received_at: DateTime<Utc>,
    /// When the command was moved to DLQ
    pub dlq_at: DateTime<Utc>,
    /// Original subject the command was published to
    pub original_subject: String,
    /// Consumer name that failed to process
    pub consumer_name: String,
    /// Saga ID associated with this command
    pub saga_id: Option<String>,
    /// Command type name for filtering
    pub command_type: String,
    /// Idempotency key
    pub idempotency_key: String,
}

impl<C: Command> CommandDlqEntry<C> {
    /// Create a new DLQ entry from a failed command
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command: C,
        retry_count: u32,
        last_error: String,
        error_history: Vec<String>,
        received_at: DateTime<Utc>,
        original_subject: String,
        consumer_name: String,
        saga_id: Option<String>,
    ) -> Self {
        // Save values before moving command
        let command_type = command.command_name().into_owned();
        let idempotency_key = command.idempotency_key().into_owned();

        Self {
            command,
            retry_count,
            last_error,
            error_history,
            received_at,
            dlq_at: Utc::now(),
            original_subject,
            consumer_name,
            saga_id,
            command_type,
            idempotency_key,
        }
    }
}

/// Result of a DLQ operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDlqOperationResult {
    /// Whether the operation was successful
    pub success: bool,
    /// Number of messages in DLQ after operation
    pub dlq_size: u64,
    /// Error message if operation failed
    pub error_message: Option<String>,
}

impl CommandDlqOperationResult {
    /// Create a success result
    pub fn success(dlq_size: u64) -> Self {
        Self {
            success: true,
            dlq_size,
            error_message: None,
        }
    }

    /// Create a failure result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            dlq_size: 0,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for Command DLQ operations
#[derive(Debug, thiserror::Error)]
pub enum CommandDlqError {
    #[error("DLQ is disabled")]
    Disabled,

    #[error("Failed to publish to DLQ: {source}")]
    PublishFailed { source: anyhow::Error },

    #[error("Failed to get DLQ stream info: {source}")]
    StreamInfoFailed { source: anyhow::Error },

    #[error("DLQ stream does not exist")]
    StreamNotFound,
}

/// Metrics for Command DLQ operations
#[derive(Debug, Default)]
pub struct CommandDlqMetrics {
    /// Total messages sent to DLQ
    pub messages_to_dlq: Arc<Mutex<u64>>,
    /// Total retry attempts
    pub retry_attempts: Arc<Mutex<u64>>,
    /// Total successful retries (processed after retries)
    pub successful_retries: Arc<Mutex<u64>>,
    /// Current DLQ size
    pub current_dlq_size: Arc<Mutex<u64>>,
}

impl CommandDlqMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            messages_to_dlq: Arc::new(Mutex::new(0)),
            retry_attempts: Arc::new(Mutex::new(0)),
            successful_retries: Arc::new(Mutex::new(0)),
            current_dlq_size: Arc::new(Mutex::new(0)),
        }
    }

    /// Increment messages to DLQ counter
    pub async fn increment_messages_to_dlq(&self) {
        let mut counter = self.messages_to_dlq.lock().await;
        *counter += 1;
    }

    /// Increment retry attempts counter
    pub async fn increment_retry_attempts(&self) {
        let mut counter = self.retry_attempts.lock().await;
        *counter += 1;
    }

    /// Increment successful retries counter
    pub async fn increment_successful_retries(&self) {
        let mut counter = self.successful_retries.lock().await;
        *counter += 1;
    }

    /// Get current metrics snapshot
    pub async fn snapshot(&self) -> CommandDlqMetricsSnapshot {
        CommandDlqMetricsSnapshot {
            messages_to_dlq: *self.messages_to_dlq.lock().await,
            retry_attempts: *self.retry_attempts.lock().await,
            successful_retries: *self.successful_retries.lock().await,
            current_dlq_size: *self.current_dlq_size.lock().await,
        }
    }
}

/// Snapshot of DLQ metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDlqMetricsSnapshot {
    /// Total messages sent to DLQ
    pub messages_to_dlq: u64,
    /// Total retry attempts
    pub retry_attempts: u64,
    /// Total successful retries
    pub successful_retries: u64,
    /// Current DLQ size
    pub current_dlq_size: u64,
}

/// Main Command DLQ struct
#[derive(Debug)]
pub struct CommandDlq<C: Command> {
    config: CommandDlqConfig,
    jetstream: JetStreamContext,
    metrics: Arc<CommandDlqMetrics>,
    _command: std::marker::PhantomData<C>,
}

impl<C: Command> CommandDlq<C> {
    /// Create a new Command DLQ
    pub fn new(
        config: CommandDlqConfig,
        jetstream: JetStreamContext,
        metrics: Arc<CommandDlqMetrics>,
    ) -> Self {
        Self {
            config,
            jetstream,
            metrics,
            _command: std::marker::PhantomData,
        }
    }

    /// Initialize the DLQ stream
    pub async fn initialize(&self) -> Result<(), CommandDlqError> {
        if !self.config.enabled {
            debug!("Command DLQ is disabled, skipping initialization");
            return Ok(());
        }

        let stream_name = format!("{}_{}", self.config.stream_prefix, "COMMAND_DLQ");

        // Check if stream already exists
        match self.jetstream.get_stream(&stream_name).await {
            Ok(_) => {
                debug!("DLQ stream {} already exists", stream_name);
            }
            Err(_) => {
                // Create the stream
                let stream_config = StreamConfig {
                    name: stream_name.clone(),
                    subjects: vec![self.config.dlq_subject.clone()],
                    max_messages: 100000,
                    max_bytes: 100 * 1024 * 1024, // 100MB
                    max_age: Duration::from_secs(self.config.retention_days as u64 * 24 * 60 * 60),
                    storage: async_nats::jetstream::stream::StorageType::File,
                    ..Default::default()
                };

                self.jetstream
                    .create_stream(stream_config)
                    .await
                    .map_err(|e| CommandDlqError::PublishFailed {
                        source: anyhow::anyhow!("Failed to create DLQ stream: {}", e),
                    })?;

                info!(
                    subject = %self.config.dlq_subject,
                    stream = %stream_name,
                    "Command DLQ stream created"
                );
            }
        }

        Ok(())
    }

    /// Move a failed command to the DLQ
    pub async fn move_to_dlq(
        &self,
        command: &C,
        retry_count: u32,
        last_error: String,
        error_history: Vec<String>,
        received_at: DateTime<Utc>,
        original_subject: &str,
        consumer_name: &str,
        saga_id: Option<&str>,
    ) -> Result<CommandDlqOperationResult, CommandDlqError>
    where
        C: Serialize + Clone,
    {
        if !self.config.enabled {
            return Err(CommandDlqError::Disabled);
        }

        let entry = CommandDlqEntry::new(
            command.clone(),
            retry_count,
            last_error,
            error_history,
            received_at,
            original_subject.to_string(),
            consumer_name.to_string(),
            saga_id.map(|s| s.to_string()),
        );

        let payload = serde_json::to_vec(&entry).map_err(|e| CommandDlqError::PublishFailed {
            source: anyhow::anyhow!("Failed to serialize DLQ entry: {}", e),
        })?;

        self.jetstream
            .publish(self.config.dlq_subject.clone(), payload.into())
            .await
            .map_err(|e| CommandDlqError::PublishFailed {
                source: anyhow::anyhow!("Failed to publish to DLQ: {}", e),
            })?;

        self.metrics.increment_messages_to_dlq().await;

        let dlq_size = self.get_dlq_size().await?;

        info!(
            command_type = %command.command_name(),
            consumer = %consumer_name,
            retry_count = %retry_count,
            dlq_size = %dlq_size,
            "Command moved to DLQ"
        );

        Ok(CommandDlqOperationResult::success(dlq_size))
    }

    /// Get current DLQ size
    pub async fn get_dlq_size(&self) -> Result<u64, CommandDlqError> {
        let stream_name = format!("{}_{}", self.config.stream_prefix, "COMMAND_DLQ");

        let mut stream = self.jetstream.get_stream(&stream_name).await.map_err(|e| {
            CommandDlqError::StreamInfoFailed {
                source: anyhow::anyhow!("Failed to get stream info: {}", e),
            }
        })?;

        let info = stream
            .info()
            .await
            .map_err(|e| CommandDlqError::StreamInfoFailed {
                source: anyhow::anyhow!("Failed to get stream info: {}", e),
            })?;

        let size = info.state.messages;
        *self.metrics.current_dlq_size.lock().await = size;

        Ok(size)
    }

    /// Get the DLQ subject
    pub fn dlq_subject(&self) -> &str {
        &self.config.dlq_subject
    }

    /// Check if DLQ is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get metrics reference
    pub fn metrics(&self) -> &Arc<CommandDlqMetrics> {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::command::CommandMetadataDefault;
    use std::borrow::Cow;

    // Test command for unit tests
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TestCommand {
        pub id: String,
        pub metadata: CommandMetadataDefault,
    }

    impl Command for TestCommand {
        type Output = ();

        fn command_name(&self) -> Cow<'static, str> {
            Cow::Owned("test_command".to_string())
        }

        fn idempotency_key(&self) -> Cow<'_, str> {
            Cow::Owned(format!("test-{}", self.id))
        }
    }

    #[test]
    fn test_dlq_config_defaults() {
        let config = CommandDlqConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(config.dlq_subject, DEFAULT_DLQ_SUBJECT);
    }

    #[test]
    fn test_dlq_config_builder() {
        let config = CommandDlqConfig::new()
            .with_max_retries(5)
            .with_retention_days(14)
            .with_retry_delay(2000);

        assert!(config.enabled);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retention_days, 14);
        assert_eq!(config.retry_delay_ms, 2000);
    }

    #[test]
    fn test_dlq_disabled() {
        let config = CommandDlqConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_command_dlq_entry_creation() {
        let command = TestCommand {
            id: "test-123".to_string(),
            metadata: CommandMetadataDefault::new(),
        };

        let entry = CommandDlqEntry::new(
            command.clone(),
            2,
            "Test error".to_string(),
            vec!["[attempt 1] Error 1".to_string()],
            Utc::now(),
            "hodei.commands.test".to_string(),
            "test-consumer".to_string(),
            Some("saga-123".to_string()),
        );

        assert_eq!(entry.retry_count, 2);
        assert_eq!(entry.last_error, "Test error");
        assert_eq!(entry.command_type, "test_command");
        assert_eq!(entry.saga_id, Some("saga-123".to_string()));
    }

    #[test]
    fn test_dlq_operation_result() {
        let success = CommandDlqOperationResult::success(100);
        assert!(success.success);
        assert_eq!(success.dlq_size, 100);

        let failure = CommandDlqOperationResult::failure("Test error");
        assert!(!failure.success);
        assert!(failure.error_message.is_some());
    }

    #[tokio::test]
    async fn test_metrics_increment() {
        let metrics = CommandDlqMetrics::new();

        metrics.increment_messages_to_dlq().await;
        metrics.increment_retry_attempts().await;
        metrics.increment_successful_retries().await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.messages_to_dlq, 1);
        assert_eq!(snapshot.retry_attempts, 1);
        assert_eq!(snapshot.successful_retries, 1);
    }
}

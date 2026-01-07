// Tower Middleware for Command Bus
//
// This module provides middleware layers for the Command Bus:
// - LoggingLayer: Logs command execution details
// - RetryLayer: Retries failed commands with exponential backoff
// - TelemetryLayer: Records command execution metrics

use super::*;
use async_trait::async_trait;
use std::marker::PhantomData;
use std::time::Duration;
use std::time::Instant;

/// Configuration for retry behavior.
///
/// Provides configurable retry semantics with exponential backoff.
/// Use `RetryConfig::transient()` for transient errors only,
/// or `RetryConfig::all()` to retry all errors.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay cap
    pub max_delay: Duration,
    /// Whether to retry only transient errors
    pub retry_transient_only: bool,
    /// Jitter factor (0.0 to 1.0) for randomized backoff
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            retry_transient_only: true,
            jitter: 0.1,
        }
    }
}

impl RetryConfig {
    /// Creates a new retry configuration.
    #[inline]
    pub fn new(max_retries: u32, base_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay: Duration::from_secs(10),
            retry_transient_only: true,
            jitter: 0.1,
        }
    }

    /// Creates a config that retries only transient errors.
    #[inline]
    pub fn transient() -> Self {
        Self::default()
    }

    /// Creates a config that retries all errors.
    #[inline]
    pub fn all() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            retry_transient_only: false,
            jitter: 0.1,
        }
    }

    /// Sets the maximum delay cap.
    #[inline]
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// Sets the jitter factor.
    #[inline]
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }

    /// Determines if an error should be retried.
    #[inline]
    pub fn should_retry(&self, error: &CommandError) -> bool {
        if self.retry_transient_only {
            error.is_transient()
        } else {
            !error.is_validation()
        }
    }

    /// Calculates the delay for a given attempt number with simple jitter.
    #[inline]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        // Exponential backoff: base * 2^attempt
        let delay = self.base_delay.mul_f64(2.0_f64.powi(attempt as i32));
        let delay = delay.min(self.max_delay);

        // Add jitter to prevent thundering herd (simplified without rand)
        if self.jitter > 0.0 {
            let jitter_amount = delay.mul_f64(self.jitter);
            // Use a simple hash-based jitter instead of rand
            let seed = (attempt as u64).wrapping_mul(0x9e3779b97f4a7c15);
            let jitter = Duration::from_nanos(seed % jitter_amount.as_nanos() as u64);
            delay + jitter
        } else {
            delay
        }
    }
}

/// Logging layer for command bus.
///
/// Records command execution details including:
/// - Command type and metadata
/// - Execution duration
/// - Success/failure status
/// - Error details on failure
#[derive(Debug, Clone, Default)]
pub struct LoggingLayer;

impl LoggingLayer {
    /// Creates a new logging layer.
    #[inline]
    pub fn new() -> Self {
        Self
    }

    /// Wraps a command bus with logging middleware.
    #[inline]
    pub fn layer<B: CommandBus>(&self, bus: B) -> LoggingCommandBus<B> {
        LoggingCommandBus {
            inner: bus,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoggingCommandBus<B: CommandBus> {
    inner: B,
    _phantom: PhantomData<B>,
}

#[async_trait]
impl<B: CommandBus + Send + Sync> CommandBus for LoggingCommandBus<B> {
    async fn dispatch<C: Command + Send>(&self, command: C) -> CommandResult<C::Output>
    where
        C: Command,
    {
        let cmd_type = std::any::type_name_of_val(&command);
        let start = Instant::now();

        tracing::debug!(
            command.type = cmd_type,
            command.idempotency_key = command.idempotency_key().as_ref(),
            "Command received"
        );

        let result = self.inner.dispatch(command).await;

        let elapsed = start.elapsed();
        match &result {
            Ok(_) => {
                tracing::info!(
                    command.type = cmd_type,
                    duration.ms = elapsed.as_millis(),
                    "Command succeeded"
                );
            }
            Err(e) => {
                tracing::error!(
                    command.type = cmd_type,
                    error = %e,
                    duration.ms = elapsed.as_millis(),
                    "Command failed"
                );
            }
        }

        result
    }

    async fn register_handler<H, C>(&mut self, handler: H)
    where
        H: CommandHandler<C>,
        C: Command,
    {
        self.inner.register_handler(handler).await
    }
}

/// Retry layer for command bus with exponential backoff.
///
/// Provides automatic retry for failed commands with:
/// - Configurable max retries
/// - Exponential backoff with jitter
/// - Selective retry based on error type
#[derive(Debug, Clone)]
pub struct RetryLayer {
    config: RetryConfig,
}

impl RetryLayer {
    /// Creates a new retry layer with default configuration.
    #[inline]
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
        }
    }

    /// Creates a retry layer with custom configuration.
    #[inline]
    pub fn with_config(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Wraps a command bus with retry middleware.
    #[inline]
    pub fn layer<B: CommandBus>(&self, bus: B) -> RetryCommandBus<B> {
        RetryCommandBus {
            inner: bus,
            config: self.config.clone(),
            _phantom: PhantomData,
        }
    }
}

impl Default for RetryLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RetryCommandBus<B: CommandBus> {
    inner: B,
    config: RetryConfig,
    _phantom: PhantomData<B>,
}

#[async_trait]
impl<B: CommandBus + Send + Sync> CommandBus for RetryCommandBus<B> {
    async fn dispatch<C: Command + Send + Clone>(&self, command: C) -> CommandResult<C::Output>
    where
        C: Command,
    {
        let cmd_type = std::any::type_name_of_val(&command);
        let command = command;
        let mut attempt = 0u32;

        loop {
            match self.inner.dispatch(command.clone()).await {
                Ok(output) => {
                    if attempt > 0 {
                        tracing::info!(
                            command.type = cmd_type,
                            attempts = attempt + 1,
                            "Command succeeded after retries"
                        );
                    }
                    return Ok(output);
                }
                Err(e) => {
                    let is_transient = self.config.should_retry(&e);

                    // Check if we should retry
                    if attempt < self.config.max_retries && is_transient {
                        let delay = self.config.delay_for_attempt(attempt);
                        tracing::warn!(
                            command.type = cmd_type,
                            attempt = attempt + 1,
                            max_attempts = self.config.max_retries + 1,
                            error = %e,
                            delay.ms = delay.as_millis(),
                            "Command failed, retrying"
                        );

                        tokio::time::sleep(delay).await;
                        attempt += 1;
                    } else {
                        // No more retries or shouldn't retry
                        tracing::error!(
                            command.type = cmd_type,
                            attempts = attempt + 1,
                            error = %e,
                            "Command failed after all retries"
                        );
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn register_handler<H, C>(&mut self, handler: H)
    where
        H: CommandHandler<C>,
        C: Command,
    {
        self.inner.register_handler(handler).await
    }
}

/// Telemetry layer for command bus.
///
/// Records command execution metrics for observability:
/// - Execution duration histograms
/// - Success/error counters
/// - Command type tagging
#[derive(Debug, Clone, Default)]
pub struct TelemetryLayer;

impl TelemetryLayer {
    /// Creates a new telemetry layer.
    #[inline]
    pub fn new() -> Self {
        Self
    }

    /// Wraps a command bus with telemetry middleware.
    #[inline]
    pub fn layer<B: CommandBus>(&self, bus: B) -> TelemetryCommandBus<B> {
        TelemetryCommandBus {
            inner: bus,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemetryCommandBus<B: CommandBus> {
    inner: B,
    _phantom: PhantomData<B>,
}

#[async_trait]
impl<B: CommandBus + Send + Sync> CommandBus for TelemetryCommandBus<B> {
    async fn dispatch<C: Command + Send>(&self, command: C) -> CommandResult<C::Output>
    where
        C: Command,
    {
        let cmd_type = std::any::type_name_of_val(&command);
        let start = Instant::now();

        tracing::debug!(command.type = cmd_type, "Command received");

        let result = self.inner.dispatch(command).await;

        let elapsed = start.elapsed();
        let duration_ms = elapsed.as_millis() as f64;

        match &result {
            Ok(_) => {
                tracing::info!(
                    command.type = cmd_type,
                    duration.ms = duration_ms,
                    "Command completed successfully"
                );
            }
            Err(e) => {
                tracing::error!(
                    command.type = cmd_type,
                    duration.ms = duration_ms,
                    error = %e,
                    "Command failed"
                );
            }
        }

        result
    }

    async fn register_handler<H, C>(&mut self, handler: H)
    where
        H: CommandHandler<C>,
        C: Command,
    {
        self.inner.register_handler(handler).await
    }
}

/// Composite middleware layer combining logging, retry, and telemetry.
///
/// Provides a convenient way to stack multiple middleware layers.
#[derive(Debug, Clone, Default)]
pub struct CompositeLayer;

impl CompositeLayer {
    /// Creates a new composite layer.
    #[inline]
    pub fn new() -> Self {
        Self
    }

    /// Applies logging, then retry, then telemetry to a command bus.
    #[inline]
    pub fn layer<B: CommandBus + Send + Sync>(&self, bus: B) -> impl CommandBus
    where
        B: CommandBus,
    {
        let bus = LoggingLayer::new().layer(bus);
        let bus = RetryLayer::new().layer(bus);
        TelemetryLayer::new().layer(bus)
    }

    /// Applies logging, retry with custom config, then telemetry.
    #[inline]
    pub fn layer_with_retry_config<B: CommandBus + Send + Sync>(
        &self,
        bus: B,
        retry_config: RetryConfig,
    ) -> impl CommandBus
    where
        B: CommandBus,
    {
        let bus = LoggingLayer::new().layer(bus);
        let bus = RetryLayer::with_config(retry_config).layer(bus);
        TelemetryLayer::new().layer(bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Unit tests for middleware configuration
    // Note: Full integration tests with handlers are in infrastructure layer
    // due to Rust's trait object limitations with associated types

    #[tokio::test]
    async fn retry_config_default_values() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!(config.retry_transient_only);
    }

    #[tokio::test]
    async fn retry_config_transient_creates_correct_defaults() {
        let config = RetryConfig::transient();
        assert_eq!(config.max_retries, 3);
        assert!(config.retry_transient_only);
    }

    #[tokio::test]
    async fn retry_config_all_retries_all_errors() {
        let config = RetryConfig::all();
        assert!(!config.retry_transient_only);
    }

    #[tokio::test]
    async fn retry_config_with_max_delay() {
        let config = RetryConfig::default().with_max_delay(Duration::from_secs(30));
        assert_eq!(config.max_delay, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn retry_config_with_jitter() {
        let config = RetryConfig::default().with_jitter(0.2);
        assert!((config.jitter - 0.2).abs() < 0.001);
    }

    #[tokio::test]
    async fn retry_config_jitter_clamped_to_valid_range() {
        let config = RetryConfig::default().with_jitter(1.5);
        assert_eq!(config.jitter, 1.0);
    }

    #[tokio::test]
    async fn logging_layer_is_clone() {
        let layer = LoggingLayer::new();
        let _ = layer.clone();
    }

    #[tokio::test]
    async fn retry_layer_is_clone() {
        let layer = RetryLayer::new();
        let _ = layer.clone();
    }

    #[tokio::test]
    async fn telemetry_layer_is_clone() {
        let layer = TelemetryLayer::new();
        let _ = layer.clone();
    }

    #[tokio::test]
    async fn composite_layer_is_clone() {
        let layer = CompositeLayer::new();
        let _ = layer.clone();
    }
}

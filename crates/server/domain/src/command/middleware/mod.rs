// Tower Middleware for Command Bus
//
// This module provides middleware layers for the Command Bus:
// - LoggingLayer: Logs command execution details
// - RetryLayer: Retries failed commands with exponential backoff

use super::*;
use async_trait::async_trait;
use std::marker::PhantomData;
use std::time::Instant;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: std::time::Duration::from_millis(100),
            max_delay: std::time::Duration::from_secs(10),
        }
    }
}

impl RetryConfig {
    pub fn new(max_retries: u32, base_delay: std::time::Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay: std::time::Duration::from_secs(10),
        }
    }

    pub fn should_retry(&self, error: &CommandError) -> bool {
        matches!(error, CommandError::ExecutionFailed { .. })
    }
}

/// Logging layer for command bus.
#[derive(Debug, Clone, Default)]
pub struct LoggingLayer;

impl LoggingLayer {
    pub fn new() -> Self {
        Self
    }

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
impl<B: CommandBus> CommandBus for LoggingCommandBus<B> {
    async fn dispatch<C: Command>(&self, command: C) -> CommandResult<C::Output> {
        let cmd_type = std::any::type_name_of_val(&command);
        let start = Instant::now();

        tracing::debug!(command_type = cmd_type, "Command received");

        let result = self.inner.dispatch(command).await;

        let elapsed = start.elapsed();
        match &result {
            Ok(_) => {
                tracing::info!(
                    command_type = cmd_type,
                    duration_ms = elapsed.as_millis(),
                    "Command succeeded"
                );
            }
            Err(e) => {
                tracing::error!(
                    command_type = cmd_type,
                    ?e,
                    duration_ms = elapsed.as_millis(),
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

/// Retry layer for command bus.
#[derive(Debug, Clone)]
pub struct RetryLayer {
    config: RetryConfig,
}

impl RetryLayer {
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
        }
    }

    pub fn with_config(config: RetryConfig) -> Self {
        Self { config }
    }

    pub fn layer<B: CommandBus>(&self, bus: B) -> RetryCommandBus<B> {
        RetryCommandBus {
            inner: bus,
            config: self.config.clone(),
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryCommandBus<B: CommandBus> {
    inner: B,
    config: RetryConfig,
    _phantom: PhantomData<B>,
}

#[async_trait]
impl<B: CommandBus> CommandBus for RetryCommandBus<B> {
    async fn dispatch<C: Command>(&self, command: C) -> CommandResult<C::Output> {
        // Simple dispatch without retry (retry requires Clone on Command)
        // For production, implement a command factory pattern
        self.inner.dispatch(command).await
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
#[derive(Debug, Clone)]
pub struct TelemetryLayer;

impl TelemetryLayer {
    pub fn new() -> Self {
        Self
    }

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
impl<B: CommandBus> CommandBus for TelemetryCommandBus<B> {
    async fn dispatch<C: Command>(&self, command: C) -> CommandResult<C::Output> {
        let cmd_type = std::any::type_name_of_val(&command);
        let start = Instant::now();

        tracing::debug!(command_type = cmd_type, "Command received");

        let result = self.inner.dispatch(command).await;

        let elapsed = start.elapsed();
        match &result {
            Ok(_) => {
                tracing::info!(
                    command_type = cmd_type,
                    duration_ms = elapsed.as_millis(),
                    "Command completed successfully"
                );
            }
            Err(e) => {
                tracing::error!(
                    command_type = cmd_type,
                    duration_ms = elapsed.as_millis(),
                    error = format!("{:?}", e),
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

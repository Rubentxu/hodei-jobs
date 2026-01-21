//! # CommandBus Port
//!
//! This module defines the [`CommandBus`] trait for command dispatching with routing.
//! Commands are synchronous operations that modify state, as opposed to events which
//! represent something that already happened.
//!
//! # Example
//!
//! ```ignore
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! #[derive(Debug)]
//! struct CreateUser {
//!     username: String,
//!     email: String,
//! }
//!
//! impl Command for CreateUser {
//!     type Result = Result<UserId, CreateUserError>;
//! }
//!
//! struct CreateUserHandler {
//!     repository: Arc<dyn UserRepository>,
//! }
//!
//! #[async_trait]
//! impl CommandHandler<CreateUser> for CreateUserHandler {
//!     async fn handle(&self, command: CreateUser) -> Result<UserId, CommandBusError> {
//!         // Business logic here
//!         Ok(UserId::new())
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

/// Trait for command dispatching with routing.
///
/// The CommandBus pattern provides:
/// - Command routing to appropriate handlers
/// - Synchronous command execution
/// - Error handling with context
#[async_trait]
pub trait CommandBus: Send + Sync {
    /// Send a command and get its result.
    ///
    /// The command is routed to the appropriate handler based on its type.
    async fn send<C>(&self, command: C) -> Result<C::Result, CommandBusError>
    where
        C: Command + Send;
}

/// Marker trait for commands.
///
/// Commands represent intent to perform an operation. They are synchronous
/// and expect a result. Unlike events, commands are imperative.
pub trait Command: Send {
    /// The result type of this command.
    type Result;

    /// Unique type identifier for this command.
    const TYPE_ID: &'static str;
}

/// Handler trait for a specific command type.
///
/// Implement this trait to handle commands of a specific type.
#[async_trait]
pub trait CommandHandler<C: Command>: Send {
    /// Handle the command and return a result.
    async fn handle(&self, command: C) -> Result<C::Result, CommandBusError>;
}

/// Errors that can occur when dispatching commands.
#[derive(Debug, Error)]
pub enum CommandBusError {
    /// Timeout waiting for command execution
    #[error("Timeout executing command: {0}")]
    Timeout(String),

    /// No handler registered for this command type
    #[error("Handler not found for command: {0}")]
    HandlerNotFound(&'static str),

    /// Error during command execution
    #[error("Command execution error: {0}")]
    ExecutionError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Command cancelled
    #[error("Command was cancelled")]
    Cancelled,
}

/// Configuration for command dispatching.
#[derive(Debug, Clone)]
pub struct CommandBusConfig {
    /// Default timeout for command execution
    pub default_timeout: std::time::Duration,
    /// Maximum retry attempts for transient failures
    pub max_retries: u32,
    /// Enable command logging
    pub enable_logging: bool,
}

impl Default for CommandBusConfig {
    fn default() -> Self {
        Self {
            default_timeout: std::time::Duration::from_secs(30),
            max_retries: 3,
            enable_logging: true,
        }
    }
}

/// Result metadata for a dispatched command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult<C: Command> {
    /// The command that was executed
    pub command: C,
    /// Execution duration
    pub duration: std::time::Duration,
    /// Whether the command succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl<C: Command> CommandResult<C> {
    /// Create a successful result.
    pub fn ok(command: C, duration: std::time::Duration) -> Self {
        Self {
            command,
            duration,
            success: true,
            error: None,
        }
    }

    /// Create a failed result.
    pub fn error(command: C, duration: std::time::Duration, error: String) -> Self {
        Self {
            command,
            duration,
            success: false,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestCommand;

    impl Command for TestCommand {
        type Result = String;
        const TYPE_ID: &'static str = "test.command";
    }

    #[derive(Debug)]
    struct TestHandler;

    #[async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        async fn handle(&self, _command: TestCommand) -> Result<String, CommandBusError> {
            Ok("handled".to_string())
        }
    }

    #[tokio::test]
    async fn test_command_bus_error_variants() {
        let error = CommandBusError::Timeout("test".to_string());
        assert!(error.to_string().contains("Timeout"));

        let error = CommandBusError::HandlerNotFound("TestCommand");
        assert!(error.to_string().contains("Handler not found"));

        let error = CommandBusError::ExecutionError("test error".to_string());
        assert!(error.to_string().contains("error"));

        let error = CommandBusError::Cancelled;
        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn test_command_config_defaults() {
        let config = CommandBusConfig::default();
        assert_eq!(config.default_timeout, std::time::Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_logging);
    }

    #[tokio::test]
    async fn test_command_result() {
        let cmd = TestCommand;
        let result = CommandResult::ok(cmd.clone(), std::time::Duration::from_millis(100));
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.duration.as_millis(), 100);

        let failed = CommandResult::error(
            cmd,
            std::time::Duration::from_millis(50),
            "error".to_string(),
        );
        assert!(!failed.success);
        assert!(failed.error.is_some());
    }
}

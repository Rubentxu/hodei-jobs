//! Command Bus Infrastructure
//!
//! Provides command traits and types for the application layer.
//! The actual bus implementation is provided by infrastructure.

use hodei_server_domain::shared_kernel::{DomainError, Result};
use std::fmt::Debug;

/// Command Bus errors
#[derive(Debug, thiserror::Error)]
pub enum CommandBusError {
    #[error("Handler not found for command: {0}")]
    HandlerNotFound(&'static str),
    #[error("Command validation failed: {0}")]
    ValidationError(String),
    #[error("Command bus overloaded")]
    Overloaded,
}

impl From<CommandBusError> for DomainError {
    fn from(err: CommandBusError) -> Self {
        match err {
            CommandBusError::HandlerNotFound(cmd) => DomainError::InfrastructureError {
                message: format!("No handler for command: {}", cmd),
            },
            CommandBusError::ValidationError(msg) => DomainError::InvalidJobSpec {
                field: "command".to_string(),
                reason: msg,
            },
            CommandBusError::Overloaded => DomainError::InfrastructureError {
                message: "Command bus is overloaded".to_string(),
            },
        }
    }
}

/// Command Bus configuration
#[derive(Debug, Clone, Default)]
pub struct CommandBusConfig {
    pub max_concurrent_commands: usize,
    pub command_timeout_secs: u64,
}

impl CommandBusConfig {
    pub fn new() -> Self {
        Self {
            max_concurrent_commands: 100,
            command_timeout_secs: 30,
        }
    }
}

/// Trait for all Commands in the system.
pub trait Command: Debug + Send + Sync + 'static {
    /// Unique command type name for routing
    const NAME: &'static str;
    /// Associated result type
    type Result;
}

/// Marker trait for commands that can be validated
pub trait ValidatableCommand: Command {
    fn validate(&self) -> Result<()>;
}

/// The Command Bus trait
#[async_trait::async_trait]
pub trait CommandBus: Send + Sync {
    async fn dispatch<C: Command>(&self, command: C) -> Result<C::Result>;
}

/// Command Handler trait
#[async_trait::async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    async fn handle(&self, command: C) -> Result<C::Result>;
}

/// In-memory Command Bus (basic implementation for single-node)
#[derive(Clone)]
pub struct InMemoryCommandBus {
    config: CommandBusConfig,
}

impl InMemoryCommandBus {
    pub fn new(config: Option<CommandBusConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
        }
    }

    pub fn config(&self) -> &CommandBusConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl CommandBus for InMemoryCommandBus {
    async fn dispatch<C: Command>(&self, _command: C) -> Result<C::Result> {
        // Basic implementation - actual dispatch would be provided by infrastructure
        panic!("InMemoryCommandBus::dispatch requires handler registration");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestCommand {
        pub value: i32,
    }

    impl Command for TestCommand {
        const NAME: &'static str = "TestCommand";
        type Result = i32;
    }

    #[tokio::test]
    async fn test_command_traits() {
        let cmd = TestCommand { value: 42 };
        assert_eq!(TestCommand::NAME, "TestCommand");
        // Type::Result is i32, verified by compilation
    }
}

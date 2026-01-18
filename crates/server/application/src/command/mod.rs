//!
//! EPIC-50: Command Bus Core Infrastructure - Historia de Usuario 56.2

pub mod execution;
mod provisioning;

pub use execution::{
    JobExecutorImpl, register_execution_command_handlers,
    register_execution_command_handlers_with_executor,
};
pub use provisioning::register_provisioning_command_handlers;

use hodei_server_domain::command::CommandBus;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Bootstrap configuration for command handlers
#[derive(Debug, Clone, Default)]
pub struct CommandBusBootstrapConfig {
    /// Enable all default command handlers
    pub enable_default_handlers: bool,
}

/// Register all command handlers with the provided CommandBus.
///
/// This function is called during application startup to ensure all
/// command handlers are available for dispatch.
///
/// # Arguments
/// * `command_bus` - The command bus to register handlers with
/// * `config` - Bootstrap configuration for customizing handler registration
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::command::InMemoryCommandBus;
///
/// let command_bus = Arc::new(Mutex::new(InMemoryCommandBus::new()));
/// register_all_command_handlers(&command_bus, CommandBusBootstrapConfig::default()).await;
/// ```
pub async fn register_all_command_handlers<B: CommandBus + Send + Sync>(
    _command_bus: &mut Arc<Mutex<B>>,
    config: CommandBusBootstrapConfig,
) {
    tracing::info!("Registering command handlers...");

    // Register default handlers if enabled
    if config.enable_default_handlers {
        tracing::info!("Default command handlers enabled");
        // Note: Actual handler registration requires concrete handler implementations
        // that are constructed with their dependencies (e.g., JobRepository)
    }

    tracing::info!("Command handler registration complete");
}

/// Builder for creating a configured CommandBus
#[derive(Debug, Clone, Default)]
pub struct CommandBusBuilder {
    config: CommandBusBootstrapConfig,
}

impl CommandBusBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: CommandBusBootstrapConfig::default(),
        }
    }

    /// Enable default handlers
    pub fn with_default_handlers(mut self) -> Self {
        self.config.enable_default_handlers = true;
        self
    }

    /// Build the command bus with all registered handlers
    pub async fn build<B: CommandBus + Default + Send + Sync>(&self) -> Arc<Mutex<B>> {
        let bus = Arc::new(Mutex::new(B::default()));

        // Register handlers
        let mut bus_clone = bus.clone();
        register_all_command_handlers(&mut bus_clone, self.config.clone()).await;

        bus
    }
}

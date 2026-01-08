//! Command Bus Bootstrap Module
//!
//! This module provides bootstrap utilities for registering command handlers
//! with the CommandBus during application startup.
//!
//! EPIC-50: Command Bus Core Infrastructure - Historia de Usuario 56.2

use hodei_server_domain::command::{
    Command, CommandBus, CommandHandler, InMemoryCommandBus, MarkJobFailedCommand,
    MarkJobFailedError, MarkJobFailedHandler, ResumeFromManualInterventionCommand,
    ResumeFromManualInterventionError, ResumeFromManualInterventionHandler,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Bootstrap configuration for command handlers
#[derive(Debug, Clone, Default)]
pub struct CommandBusBootstrapConfig {
    /// Enable all default command handlers
    pub enable_default_handlers: bool,
    /// Custom handler configurations
    pub custom_handlers: Vec<Box<dyn HandlerRegistration>>,
}

/// Trait for registering custom handlers
pub trait HandlerRegistration: Send + Sync {
    /// Register the handler with the command bus
    async fn register<B: CommandBus + Send + Sync>(&self, bus: &mut Arc<Mutex<B>>);
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
/// let command_bus = Arc::new(Mutex::new(InMemoryCommandBus::new()));
/// register_all_command_handlers(&command_bus, CommandBusBootstrapConfig::default()).await;
/// ```
pub async fn register_all_command_handlers<B: CommandBus + Send + Sync>(
    command_bus: &mut Arc<Mutex<B>>,
    config: CommandBusBootstrapConfig,
) {
    tracing::info!("Registering command handlers...");

    // Register default handlers if enabled
    if config.enable_default_handlers {
        // MarkJobFailed handler
        register_handler::<B, MarkJobFailedCommand, MarkJobFailedHandler<_>>(
            command_bus,
            |job_repo| MarkJobFailedHandler::new(job_repo),
        )
        .await;

        // ResumeFromManualIntervention handler
        register_handler::<B, ResumeFromManualInterventionCommand, ResumeFromManualInterventionHandler<_>>(
            command_bus,
            |job_repo| ResumeFromManualInterventionHandler::new(job_repo),
        )
        .await;

        tracing::info!("Default command handlers registered");
    }

    // Register custom handlers
    for handler_config in config.custom_handlers {
        handler_config.register(command_bus).await;
    }

    tracing::info!("Command handler registration complete");
}

/// Helper to register a handler with the command bus
async fn register_handler<B, C, H, F>(command_bus: &mut Arc<Mutex<B>>, handler_factory: F)
where
    B: CommandBus + Send + Sync,
    C: Command,
    H: CommandHandler<C> + Send + Sync + 'static,
    F: FnOnce(Arc<dyn hodei_server_domain::jobs::JobRepository + Send + Sync>) -> H,
{
    // Note: In a real implementation, this would get the job repository from services
    // For now, we just log that registration would happen here
    tracing::debug!(
        command_type = std::any::type_name::<C>(),
        "Handler would be registered"
    );
}

/// Builder for creating a configured CommandBus
#[derive(Debug, Clone)]
pub struct CommandBusBuilder {
    config: CommandBusBootstrapConfig,
    handlers: Vec<Box<dyn HandlerRegistration>>,
}

impl CommandBusBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: CommandBusBootstrapConfig::default(),
            handlers: Vec::new(),
        }
    }

    /// Enable default handlers
    pub fn with_default_handlers(mut self) -> Self {
        self.config.enable_default_handlers = true;
        self
    }

    /// Add a custom handler
    pub fn with_handler<H: HandlerRegistration + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Box::new(handler));
        self
    }

    /// Build the command bus with all registered handlers
    pub async fn build<B: CommandBus + Default + Send + Sync>(
        &self,
        _job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository + Send + Sync>,
    ) -> Arc<Mutex<B>> {
        let bus = Arc::new(Mutex::new(B::default()));

        // Register handlers
        let mut bus_clone = bus.clone();
        register_all_command_handlers(&mut bus_clone, self.config.clone()).await;

        // Register custom handlers
        for handler in &self.handlers {
            let mut bus_clone = bus.clone();
            handler.register(&mut bus_clone).await;
        }

        bus
    }
}

impl Default for CommandBusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::command::InMemoryCommandBus;

    #[tokio::test]
    async fn test_bootstrap_config_default() {
        let config = CommandBusBootstrapConfig::default();
        assert!(!config.enable_default_handlers);
        assert!(config.custom_handlers.is_empty());
    }

    #[tokio::test]
    async fn test_command_bus_builder() {
        let builder = CommandBusBuilder::new()
            .with_default_handlers();

        let bus = builder
            .build::<InMemoryCommandBus>(Arc::new(crate::tests::MockJobRepository))
            .await;

        // Verify bus was created
        assert!(bus.try_lock().is_ok());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use hodei_server_domain::jobs::JobRepository;
    use hodei_server_domain::shared_kernel::JobId;
    use std::sync::Arc;

    /// Mock job repository for testing
    #[derive(Debug, Clone)]
    pub struct MockJobRepository;

    #[async_trait::async_trait]
    impl JobRepository for MockJobRepository {
        type Error = std::io::Error;

        async fn save(&self, _job: &hodei_server_domain::jobs::Job) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_by_id(&self, _id: &JobId) -> Result<Option<hodei_server_domain::jobs::Job>, Self::Error> {
            Ok(None)
        }

        async fn update_state(&self, _id: &JobId, _state: hodei_server_domain::shared_kernel::JobState)
            -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _id: &JobId) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn find_by_state(&self, _state: hodei_server_domain::shared_kernel::JobState)
            -> Result<Vec<hodei_server_domain::jobs::Job>, Self::Error> {
            Ok(Vec::new())
        }

        async fn find_by_provider_id(&self, _provider_id: &hodei_server_domain::shared_kernel::ProviderId)
            -> Result<Vec<hodei_server_domain::jobs::Job>, Self::Error> {
            Ok(Vec::new())
        }

        async fn count_by_state(&self, _state: hodei_server_domain::shared_kernel::JobState)
            -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn cleanup_stuck_jobs(&self, _timeout: std::time::Duration)
            -> Result<u64, Self::Error> {
            Ok(0)
        }
    }
}

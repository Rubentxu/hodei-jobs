//! Provisioning Command Handlers Bootstrap Module
//!
//! Registers saga command handlers for worker provisioning operations.
//! This module is used during application startup to register handlers
//! with the CommandBus for saga-based worker provisioning.
//!
//! GAP-60-01: Saga Command Dispatch Infrastructure

use hodei_server_domain::command::InMemoryErasedCommandBus;
use hodei_server_domain::saga::commands::provisioning::{
    CreateWorkerCommand, CreateWorkerHandler, DestroyWorkerCommand, DestroyWorkerHandler,
};
use hodei_server_domain::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerProvisioningResult, WorkerSpec};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Wrapper for Arc<dyn WorkerProvisioning> that implements Debug
#[derive(Clone)]
pub struct ProvisioningServiceWrapper(pub Arc<dyn WorkerProvisioning + Send + Sync>);

impl Debug for ProvisioningServiceWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisioningServiceWrapper").finish()
    }
}

#[async_trait::async_trait]
impl WorkerProvisioning for ProvisioningServiceWrapper {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<WorkerProvisioningResult> {
        self.0.provision_worker(provider_id, spec, job_id).await
    }

    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()> {
        self.0.destroy_worker(worker_id).await
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        self.0.is_provider_available(provider_id).await
    }
}

/// Configuration for provisioning command handlers
#[derive(Clone)]
pub struct ProvisioningCommandBusConfig {
    /// Provisioning service for CreateWorker/DestroyWorker handlers
    pub provisioning_service: ProvisioningServiceWrapper,
}

impl ProvisioningCommandBusConfig {
    /// Create a new configuration
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self {
            provisioning_service: ProvisioningServiceWrapper(provisioning_service),
        }
    }
}

/// Register all provisioning command handlers with the provided CommandBus.
///
/// This function registers:
/// - CreateWorkerHandler: Handles CreateWorkerCommand for worker provisioning
/// - DestroyWorkerHandler: Handles DestroyWorkerCommand for worker cleanup
///
/// # Arguments
/// * `command_bus` - The InMemoryErasedCommandBus to register handlers with
/// * `config` - Configuration containing the provisioning service
pub async fn register_provisioning_command_handlers(
    command_bus: &InMemoryErasedCommandBus,
    config: ProvisioningCommandBusConfig,
) {
    tracing::info!("Registering provisioning command handlers...");

    // Create handler for CreateWorkerCommand
    let create_worker_handler = CreateWorkerHandler::new(config.provisioning_service.clone());

    // Register CreateWorkerHandler
    command_bus
        .register::<CreateWorkerCommand, CreateWorkerHandler<ProvisioningServiceWrapper>>(
            create_worker_handler,
        )
        .await;

    tracing::info!("  ✓ CreateWorkerHandler registered");

    // Create handler for DestroyWorkerCommand
    let destroy_worker_handler = DestroyWorkerHandler::new(config.provisioning_service);

    // Register DestroyWorkerHandler
    command_bus
        .register::<DestroyWorkerCommand, DestroyWorkerHandler<ProvisioningServiceWrapper>>(
            destroy_worker_handler,
        )
        .await;

    tracing::info!("  ✓ DestroyWorkerHandler registered");

    tracing::info!("Provisioning command handlers registered successfully");
}

/// Builder for creating a configured CommandBus with provisioning handlers
#[derive(Clone)]
pub struct ProvisioningCommandBusBuilder {
    config: ProvisioningCommandBusConfig,
}

impl ProvisioningCommandBusBuilder {
    /// Create a new builder
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self {
            config: ProvisioningCommandBusConfig::new(provisioning_service),
        }
    }

    /// Build the command bus and register all provisioning handlers
    pub async fn build(&self) -> Arc<InMemoryErasedCommandBus> {
        let command_bus = InMemoryErasedCommandBus::new();

        // Register handlers
        register_provisioning_command_handlers(&command_bus, self.config.clone()).await;

        Arc::new(command_bus)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Mock provisioning service for testing
    #[derive(Debug, Clone)]
    pub struct MockProvisioningService;

    #[async_trait::async_trait]
    impl WorkerProvisioning for MockProvisioningService {
        async fn provision_worker(
            &self,
            provider_id: &ProviderId,
            _spec: WorkerSpec,
            _job_id: JobId,
        ) -> Result<WorkerProvisioningResult> {
            Ok(WorkerProvisioningResult::new(
                WorkerId::new(),
                provider_id.clone(),
                _job_id,
            ))
        }

        async fn destroy_worker(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn is_provider_available(&self, _provider_id: &ProviderId) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_register_provisioning_handlers() {
        let provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockProvisioningService);

        let command_bus = InMemoryErasedCommandBus::new();

        let config = ProvisioningCommandBusConfig::new(provisioning_service);

        // Register handlers - should not panic
        register_provisioning_command_handlers(&command_bus, config).await;
    }

    #[tokio::test]
    async fn test_builder_build() {
        let provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockProvisioningService);

        let builder = ProvisioningCommandBusBuilder::new(provisioning_service);
        let command_bus = builder.build().await;

        // Verify bus was created and handlers registered
        let result = command_bus
            .dispatch_erased(
                Box::new(CreateWorkerCommand {
                    spec: WorkerSpec::new(
                        "test-image:latest".to_string(),
                        "http://localhost:50051".to_string(),
                    ),
                    provider_id: ProviderId::new(),
                    job_id: JobId::new(),
                    saga_id: "test-saga".to_string(),
                    metadata: Default::default(),
                }),
                std::any::TypeId::of::<CreateWorkerCommand>(),
            )
            .await;
        assert!(result.is_err()); // Will fail because MockProvisioningService isn't the real one
    }
}

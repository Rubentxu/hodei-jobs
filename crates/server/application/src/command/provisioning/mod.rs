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
use std::sync::Arc;

/// Register all provisioning command handlers with the provided CommandBus.
///
/// This function registers:
/// - CreateWorkerHandler: Handles CreateWorkerCommand for worker provisioning
/// - DestroyWorkerHandler: Handles DestroyWorkerCommand for worker cleanup
///
/// # Arguments
/// * `command_bus` - The InMemoryErasedCommandBus to register handlers with
/// * `provisioning_service` - Provisioning service trait object
pub async fn register_provisioning_command_handlers(
    command_bus: &InMemoryErasedCommandBus,
    provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>,
) {
    tracing::info!("Registering provisioning command handlers...");

    // Create handler for CreateWorkerCommand
    let create_worker_handler = CreateWorkerHandler::new(provisioning_service.clone());

    // Register CreateWorkerHandler
    command_bus
        .register::<CreateWorkerCommand, _>(create_worker_handler)
        .await;

    tracing::info!("  ✓ CreateWorkerHandler registered");

    // Create handler for DestroyWorkerCommand
    let destroy_worker_handler = DestroyWorkerHandler::new(provisioning_service);

    // Register DestroyWorkerHandler
    command_bus
        .register::<DestroyWorkerCommand, _>(destroy_worker_handler)
        .await;

    tracing::info!("  ✓ DestroyWorkerHandler registered");

    tracing::info!("Provisioning command handlers registered successfully");
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
    use hodei_server_domain::workers::WorkerProvisioningResult;

    /// Mock provisioning service for testing
    #[derive(Debug, Clone)]
    pub struct MockProvisioningService;

    #[async_trait::async_trait]
    impl WorkerProvisioning for MockProvisioningService {
        async fn provision_worker(
            &self,
            provider_id: &ProviderId,
            _spec: hodei_server_domain::workers::WorkerSpec,
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

        // Register handlers - should not panic
        register_provisioning_command_handlers(&command_bus, provisioning_service).await;
    }
}

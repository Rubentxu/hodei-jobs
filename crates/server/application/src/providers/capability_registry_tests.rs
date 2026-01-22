// Integration Tests for CapabilityRegistry
//
// These tests verify that the CapabilityRegistry is correctly integrated
// with the application startup sequence and real provider implementations.

#[cfg(test)]
mod tests {
    use crate::providers::capability_registry::CapabilityRegistry;
    use hodei_server_domain::shared_kernel::ProviderId;
    use hodei_server_domain::workers::provider_api::{WorkerHealth, WorkerLifecycle};
    use hodei_server_domain::workers::{WorkerHandle, WorkerSpec};
    use hodei_shared::WorkerState;
    use std::sync::Arc;

    // Simple mock provider for testing
    struct MockTestProvider;

    #[async_trait::async_trait]
    impl WorkerLifecycle for MockTestProvider {
        async fn create_worker(
            &self,
            _spec: &WorkerSpec,
        ) -> Result<WorkerHandle, hodei_server_domain::workers::ProviderError> {
            Err(
                hodei_server_domain::workers::ProviderError::ProvisioningFailed(
                    "test mock".to_string(),
                ),
            )
        }

        async fn get_worker_status(
            &self,
            _handle: &WorkerHandle,
        ) -> Result<WorkerState, hodei_server_domain::workers::ProviderError> {
            Ok(WorkerState::Ready)
        }

        async fn destroy_worker(
            &self,
            _handle: &WorkerHandle,
        ) -> Result<(), hodei_server_domain::workers::ProviderError> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl WorkerHealth for MockTestProvider {
        async fn health_check(
            &self,
        ) -> Result<
            hodei_server_domain::workers::provider_api::HealthStatus,
            hodei_server_domain::workers::ProviderError,
        > {
            Ok(hodei_server_domain::workers::provider_api::HealthStatus::Healthy)
        }
    }

    /// Test that CapabilityRegistry can be initialized and used
    #[test]
    fn test_capability_registry_initialization() {
        let registry = CapabilityRegistry::new();

        // Verify registry starts empty
        assert!(registry.is_empty());
        assert_eq!(registry.lifecycle_count(), 0);
        assert_eq!(registry.health_count(), 0);
    }

    /// Test that CapabilityRegistry works with mock provider types
    #[test]
    fn test_capability_registry_with_mock_provider() {
        let registry = CapabilityRegistry::new();
        let provider_id = ProviderId::new();
        let provider = Arc::new(MockTestProvider);

        // Register individual capabilities
        registry.register_lifecycle(
            provider_id.clone(),
            provider.clone() as Arc<dyn WorkerLifecycle>,
        );
        registry.register_health(
            provider_id.clone(),
            provider.clone() as Arc<dyn WorkerHealth>,
        );

        // Verify capabilities are registered
        assert!(registry.has_lifecycle(&provider_id));
        assert!(registry.has_health(&provider_id));
    }

    /// Test that we can retrieve and use capabilities from the registry
    #[test]
    fn test_capability_registry_retrieve_and_use() {
        let registry = CapabilityRegistry::new();
        let provider_id = ProviderId::new();
        let provider = Arc::new(MockTestProvider);

        registry.register_lifecycle(
            provider_id.clone(),
            provider.clone() as Arc<dyn WorkerLifecycle>,
        );

        // Retrieve the capability
        let lifecycle = registry.get_lifecycle(&provider_id);
        assert!(lifecycle.is_some());

        // Verify count
        assert_eq!(registry.lifecycle_count(), 1);
    }

    /// Test that CapabilityRegistry can handle multiple providers
    #[test]
    fn test_capability_registry_multiple_providers() {
        let registry = CapabilityRegistry::new();

        let provider1_id = ProviderId::new();
        let provider1 = Arc::new(MockTestProvider);

        let provider2_id = ProviderId::new();
        let provider2 = Arc::new(MockTestProvider);

        // Register providers with different capabilities
        registry.register_lifecycle(
            provider1_id.clone(),
            provider1.clone() as Arc<dyn WorkerLifecycle>,
        );
        registry.register_health(
            provider2_id.clone(),
            provider2.clone() as Arc<dyn WorkerHealth>,
        );

        // Verify counts
        assert_eq!(registry.lifecycle_count(), 1);
        assert_eq!(registry.health_count(), 1);

        // Verify all provider IDs
        let all_ids = registry.all_provider_ids();
        assert_eq!(all_ids.len(), 2);
        assert!(all_ids.contains(&provider1_id));
        assert!(all_ids.contains(&provider2_id));
    }

    /// Test that CapabilityRegistry can remove providers
    #[test]
    fn test_capability_registry_remove_provider() {
        let registry = CapabilityRegistry::new();
        let provider_id = ProviderId::new();
        let provider = Arc::new(MockTestProvider);

        registry.register_lifecycle(
            provider_id.clone(),
            provider.clone() as Arc<dyn WorkerLifecycle>,
        );
        assert!(registry.has_lifecycle(&provider_id));

        // Remove all capabilities
        registry.remove_all(&provider_id);
        assert!(!registry.has_lifecycle(&provider_id));
        assert!(registry.is_empty());
    }

    /// Test CapabilityRegistry bulk operations
    #[test]
    fn test_capability_registry_bulk_operations() {
        let registry = CapabilityRegistry::new();

        let providers: Vec<_> = (0..5)
            .map(|_| {
                let id = ProviderId::new();
                let provider = Arc::new(MockTestProvider);
                registry
                    .register_lifecycle(id.clone(), provider.clone() as Arc<dyn WorkerLifecycle>);
                id
            })
            .collect();

        assert_eq!(registry.lifecycle_count(), 5);

        // Get all lifecycle providers
        let all_lifecycle = registry.all_lifecycle();
        assert_eq!(all_lifecycle.len(), 5);

        // Verify all IDs are present
        let all_ids = registry.all_provider_ids();
        assert_eq!(all_ids.len(), 5);

        // Clear all
        registry.clear();
        assert!(registry.is_empty());
    }
}

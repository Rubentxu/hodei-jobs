//! Worker Provisioning Service Implementation
//!
//! Concrete implementation that uses WorkerLifecycleManager and OTP token store.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, warn};

use hodei_jobs_domain::otp_token_store::WorkerBootstrapTokenStore;
use hodei_jobs_domain::shared_kernel::{DomainError, ProviderId, Result};
use hodei_jobs_domain::worker::WorkerSpec;
use hodei_jobs_domain::worker_provider::WorkerProvider;
use hodei_jobs_domain::worker_registry::WorkerRegistry;

use crate::worker_provisioning::{ProvisioningResult, WorkerProvisioningService};

/// Default OTP TTL for provisioned workers (5 minutes)
const DEFAULT_OTP_TTL: Duration = Duration::from_secs(300);

/// Configuration for the provisioning service
#[derive(Debug, Clone)]
pub struct ProvisioningConfig {
    /// TTL for OTP tokens
    pub otp_ttl: Duration,
    /// Default worker image
    pub default_image: String,
    /// Server address for workers to connect to
    pub server_address: String,
    /// Maximum workers per provider
    pub max_workers_per_provider: usize,
}

impl Default for ProvisioningConfig {
    fn default() -> Self {
        Self {
            otp_ttl: DEFAULT_OTP_TTL,
            default_image: "hodei-worker:latest".to_string(),
            server_address: "http://localhost:50051".to_string(),
            max_workers_per_provider: 10,
        }
    }
}

impl ProvisioningConfig {
    pub fn new(server_address: String) -> Self {
        Self {
            server_address,
            ..Default::default()
        }
    }

    pub fn with_default_image(mut self, image: String) -> Self {
        self.default_image = image;
        self
    }

    pub fn with_otp_ttl(mut self, ttl: Duration) -> Self {
        self.otp_ttl = ttl;
        self
    }
}

/// Production implementation of WorkerProvisioningService
pub struct DefaultWorkerProvisioningService {
    /// Worker registry for registration
    registry: Arc<dyn WorkerRegistry>,
    /// OTP token store for authentication
    token_store: Arc<dyn WorkerBootstrapTokenStore>,
    /// Available providers
    providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
    /// Configuration
    config: ProvisioningConfig,
}

impl DefaultWorkerProvisioningService {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        token_store: Arc<dyn WorkerBootstrapTokenStore>,
        providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
        config: ProvisioningConfig,
    ) -> Self {
        Self {
            registry,
            token_store,
            providers,
            config,
        }
    }

    /// Get provider by ID
    async fn get_provider(&self, provider_id: &ProviderId) -> Result<Arc<dyn WorkerProvider>> {
        let providers = self.providers.read().await;
        providers.get(provider_id).cloned().ok_or_else(|| DomainError::ProviderNotFound {
            provider_id: provider_id.clone(),
        })
    }
}

#[async_trait]
impl WorkerProvisioningService for DefaultWorkerProvisioningService {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
    ) -> Result<ProvisioningResult> {
        info!(
            "Provisioning worker via provider {} with image {}",
            provider_id, spec.image
        );

        // Get the provider
        let provider = self.get_provider(provider_id).await?;

        // Check provider health
        let health = provider.health_check().await.map_err(|e| {
            DomainError::WorkerProvisioningFailed {
                message: format!("Provider health check failed: {}", e),
            }
        })?;

        if !matches!(
            health,
            hodei_jobs_domain::worker_provider::HealthStatus::Healthy
                | hodei_jobs_domain::worker_provider::HealthStatus::Degraded { .. }
        ) {
            return Err(DomainError::ProviderUnhealthy {
                provider_id: provider_id.clone(),
            });
        }

        // Create worker via provider
        let handle = provider.create_worker(&spec).await.map_err(|e| {
            DomainError::WorkerProvisioningFailed {
                message: e.to_string(),
            }
        })?;

        let worker_id = handle.worker_id.clone();

        // Register in registry
        let worker = self.registry.register(handle, spec).await?;

        info!("Worker {} registered, generating OTP", worker_id);

        // Generate OTP token
        let otp_token = self
            .token_store
            .issue(&worker_id, self.config.otp_ttl)
            .await?;

        info!(
            "Worker {} provisioned successfully with OTP",
            worker_id
        );

        Ok(ProvisioningResult::new(
            worker.id().clone(),
            otp_token.to_string(),
            provider_id.clone(),
        ))
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        let providers = self.providers.read().await;
        
        let Some(provider) = providers.get(provider_id) else {
            return Ok(false);
        };

        // Check health
        match provider.health_check().await {
            Ok(hodei_jobs_domain::worker_provider::HealthStatus::Healthy) => Ok(true),
            Ok(hodei_jobs_domain::worker_provider::HealthStatus::Degraded { reason }) => {
                warn!("Provider {} is degraded: {}", provider_id, reason);
                Ok(true) // Still available but degraded
            }
            Ok(_) => Ok(false),
            Err(e) => {
                warn!("Provider {} health check failed: {}", provider_id, e);
                Ok(false)
            }
        }
    }

    fn default_worker_spec(&self, _provider_id: &ProviderId) -> Option<WorkerSpec> {
        Some(WorkerSpec::new(
            self.config.default_image.clone(),
            self.config.server_address.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provisioning_config_default() {
        let config = ProvisioningConfig::default();
        assert_eq!(config.otp_ttl, DEFAULT_OTP_TTL);
        assert_eq!(config.default_image, "hodei-worker:latest");
    }

    #[test]
    fn test_provisioning_config_builder() {
        let config = ProvisioningConfig::new("http://server:50051".to_string())
            .with_default_image("custom-worker:v1".to_string())
            .with_otp_ttl(Duration::from_secs(600));

        assert_eq!(config.server_address, "http://server:50051");
        assert_eq!(config.default_image, "custom-worker:v1");
        assert_eq!(config.otp_ttl, Duration::from_secs(600));
    }

    #[test]
    fn test_provisioning_result() {
        let worker_id = hodei_jobs_domain::shared_kernel::WorkerId::new();
        let provider_id = ProviderId::new();
        let result = ProvisioningResult::new(
            worker_id.clone(),
            "test-otp".to_string(),
            provider_id.clone(),
        );

        assert_eq!(result.worker_id, worker_id);
        assert_eq!(result.otp_token, "test-otp");
        assert_eq!(result.provider_id, provider_id);
    }
}

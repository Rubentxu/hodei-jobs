//! Saga Command Handlers
//!
//! Implementations of command handlers for saga steps.
//! Following Hexagonal Architecture, handlers are in the application layer
//! while commands and interfaces remain in the domain layer.

pub mod provisioning_handlers;
pub mod execution_handlers;

pub use provisioning_handlers::{ArcEventBusWrapper, ValidateProviderHandler, PublishProvisionedHandler};
pub use execution_handlers::*;

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

use crate::saga::handlers::provisioning_handlers::ProviderRegistry;
use hodei_server_domain::providers::config::ProviderConfigRepository;
use hodei_server_domain::saga::commands::provisioning::ProviderConfig as SagaProviderConfig;
use hodei_server_domain::shared_kernel::{ProviderId, ProviderStatus, Result};

/// Adapter that implements ProviderRegistry using ProviderConfigRepository
/// This bridges the infrastructure layer (ProviderConfigRepository) with the
/// saga layer (ProviderRegistry trait).
#[derive(Debug)]
pub struct ProviderRegistryAdapter<R: Debug> {
    repo: Arc<R>,
}

impl<R: Debug> ProviderRegistryAdapter<R>
where
    R: ProviderConfigRepository + Debug + Send + Sync,
{
    #[inline]
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl<R> ProviderRegistry for ProviderRegistryAdapter<R>
where
    R: ProviderConfigRepository + Debug + Send + Sync + 'static,
{
    async fn get_provider_config(&self, provider_id: &ProviderId) -> Result<Option<SagaProviderConfig>> {
        let config = self.repo.find_by_id(provider_id).await?;
        Ok(config.map(|c| SagaProviderConfig {
            id: c.id,
            active_workers: c.active_workers,
            max_workers: c.max_workers,
            is_enabled: matches!(c.status, ProviderStatus::Active),
        }))
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        // Provider is available if it exists, is enabled, and has capacity
        if let Some(config) = self.repo.find_by_id(provider_id).await? {
            let is_enabled = matches!(config.status, ProviderStatus::Active);
            let has_capacity = config.active_workers < config.max_workers;
            Ok(is_enabled && has_capacity)
        } else {
            Ok(false)
        }
    }
}

//! Capability-based Provider Registry
//!
//! This module implements a capability-based provider registry following the
//! Interface Segregation Principle. Instead of storing providers as
//! `dyn WorkerProvider` (the deprecated combined trait), providers are stored
//! and accessed by their specific capabilities.
//!
//! # Architecture
//!
//! The registry maintains multiple indexes, one for each provider capability:
//! - `WorkerLifecycle`: Core lifecycle operations (create, destroy)
//! - `WorkerHealth`: Health check operations
//! - `WorkerLogs`: Log retrieval operations
//! - `WorkerCost`: Cost estimation operations
//! - `WorkerEligibility`: Provider eligibility checks
//! - `WorkerMetrics`: Metrics collection operations
//!
//! # Benefits
//!
//! 1. **Interface Segregation**: Clients depend only on the capabilities they use
//! 2. **Compile-time Safety**: Trait object types are checked at compile time
//! 3. **Testing**: Mock providers only need to implement required traits
//! 4. **Performance**: No runtime overhead for unused capabilities
//!
//! # Example
//!
//! ```ignore
//! // Register a provider (typically done at startup)
//! registry.register_lifecycle(provider_id, lifecycle_provider);
//! registry.register_health(provider_id, health_provider);
//!
//! // Use only the lifecycle capability
//! let lifecycle = registry.get_lifecycle(&provider_id).await?;
//! lifecycle.create_worker(&spec).await?;
//!
//! // Use only the health capability
//! let health = registry.get_health(&provider_id).await?;
//! health.health_check().await?;
//! ```

use dashmap::DashMap;
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::provider_api::{
    WorkerCost, WorkerEligibility, WorkerHealth, WorkerLifecycle, WorkerLogs, WorkerMetrics,
};
use std::sync::Arc;
use tracing::{debug, warn};

/// Capability-based Provider Registry
///
/// Stores provider instances by their specific capabilities rather than
/// the combined `WorkerProvider` trait. This allows clients to depend only
/// on the capabilities they actually use.
///
/// # Type Safety
///
/// Each capability is stored in a separate `DashMap` with the appropriate
/// trait object type. This provides compile-time guarantees that only
/// valid trait objects are stored.
///
/// # Concurrency
///
/// All operations are thread-safe and use `DashMap` for concurrent access
/// without locking.
///
/// # Design Principles
///
/// - **Interface Segregation Principle (ISP)**: Clients depend only on used capabilities
/// - **Single Responsibility Principle**: Each capability index has one responsibility
/// - **Open/Closed Principle**: New capabilities can be added without modifying existing code
#[derive(Clone, Default)]
pub struct CapabilityRegistry {
    /// Core lifecycle operations (create, destroy, get status)
    lifecycle: Arc<DashMap<ProviderId, Arc<dyn WorkerLifecycle>>>,
    /// Health check operations
    health: Arc<DashMap<ProviderId, Arc<dyn WorkerHealth>>>,
    /// Log retrieval operations
    logs: Arc<DashMap<ProviderId, Arc<dyn WorkerLogs>>>,
    /// Cost estimation operations
    cost: Arc<DashMap<ProviderId, Arc<dyn WorkerCost>>>,
    /// Eligibility check operations
    eligibility: Arc<DashMap<ProviderId, Arc<dyn WorkerEligibility>>>,
    /// Metrics collection operations
    metrics: Arc<DashMap<ProviderId, Arc<dyn WorkerMetrics>>>,
}

impl CapabilityRegistry {
    /// Create a new empty capability-based provider registry
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Registration Methods - Register providers by capability
    // ========================================================================

    /// Register a provider with lifecycle capability
    ///
    /// # Example
    ///
    /// ```ignore
    /// let provider = Arc::new(KubernetesProvider::new(...));
    /// registry.register_lifecycle(provider_id, provider);
    /// ```
    pub fn register_lifecycle(&self, provider_id: ProviderId, provider: Arc<dyn WorkerLifecycle>) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerLifecycle capability"
        );
        self.lifecycle.insert(provider_id, provider);
    }

    /// Register a provider with health check capability
    pub fn register_health(&self, provider_id: ProviderId, provider: Arc<dyn WorkerHealth>) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerHealth capability"
        );
        self.health.insert(provider_id, provider);
    }

    /// Register a provider with logs capability
    pub fn register_logs(&self, provider_id: ProviderId, provider: Arc<dyn WorkerLogs>) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerLogs capability"
        );
        self.logs.insert(provider_id, provider);
    }

    /// Register a provider with cost estimation capability
    pub fn register_cost(&self, provider_id: ProviderId, provider: Arc<dyn WorkerCost>) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerCost capability"
        );
        self.cost.insert(provider_id, provider);
    }

    /// Register a provider with eligibility check capability
    pub fn register_eligibility(
        &self,
        provider_id: ProviderId,
        provider: Arc<dyn WorkerEligibility>,
    ) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerEligibility capability"
        );
        self.eligibility.insert(provider_id, provider);
    }

    /// Register a provider with metrics collection capability
    pub fn register_metrics(&self, provider_id: ProviderId, provider: Arc<dyn WorkerMetrics>) {
        debug!(
            ?provider_id,
            "Registering provider with WorkerMetrics capability"
        );
        self.metrics.insert(provider_id, provider);
    }

    /// Register all capabilities from a provider that implements multiple ISP traits
    ///
    /// This is a convenience method for providers that implement all capabilities.
    /// The provider must implement all ISP traits.
    ///
    /// # Type Parameters
    ///
    /// * `T`: Provider type that implements all ISP traits
    ///
    /// # Example
    ///
    /// ```ignore
    /// let provider = Arc::new(KubernetesProvider::new(...));
    /// // provider implements: WorkerLifecycle + WorkerHealth + WorkerLogs + ...
    /// registry.register_all(provider_id, provider);
    /// ```
    pub fn register_all<T>(&self, provider_id: ProviderId, provider: Arc<T>)
    where
        T: WorkerLifecycle
            + WorkerHealth
            + WorkerLogs
            + WorkerCost
            + WorkerEligibility
            + WorkerMetrics
            + Send
            + Sync
            + 'static,
    {
        self.register_lifecycle(provider_id.clone(), provider.clone());
        self.register_health(provider_id.clone(), provider.clone());
        self.register_logs(provider_id.clone(), provider.clone());
        self.register_cost(provider_id.clone(), provider.clone());
        self.register_eligibility(provider_id.clone(), provider.clone());
        self.register_metrics(provider_id, provider);
    }

    // ========================================================================
    // Query Methods - Get providers by capability
    // ========================================================================

    /// Get a provider's lifecycle capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerLifecycle` trait.
    pub fn get_lifecycle(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerLifecycle>> {
        self.lifecycle.get(provider_id).map(|v| v.clone())
    }

    /// Get a provider's health check capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerHealth` trait.
    pub fn get_health(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerHealth>> {
        self.health.get(provider_id).map(|v| v.clone())
    }

    /// Get a provider's logs capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerLogs` trait.
    pub fn get_logs(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerLogs>> {
        self.logs.get(provider_id).map(|v| v.clone())
    }

    /// Get a provider's cost estimation capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerCost` trait.
    pub fn get_cost(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerCost>> {
        self.cost.get(provider_id).map(|v| v.clone())
    }

    /// Get a provider's eligibility check capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerEligibility` trait.
    pub fn get_eligibility(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerEligibility>> {
        self.eligibility.get(provider_id).map(|v| v.clone())
    }

    /// Get a provider's metrics collection capability
    ///
    /// Returns `None` if the provider is not registered or doesn't implement
    /// the `WorkerMetrics` trait.
    pub fn get_metrics(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerMetrics>> {
        self.metrics.get(provider_id).map(|v| v.clone())
    }

    // ========================================================================
    // Bulk Operations - Get all providers with a specific capability
    // ========================================================================

    /// Get all providers with lifecycle capability
    ///
    /// Returns a vector of (provider_id, provider) tuples.
    pub fn all_lifecycle(&self) -> Vec<(ProviderId, Arc<dyn WorkerLifecycle>)> {
        self.lifecycle
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get all providers with health check capability
    pub fn all_health(&self) -> Vec<(ProviderId, Arc<dyn WorkerHealth>)> {
        self.health
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get all providers with logs capability
    pub fn all_logs(&self) -> Vec<(ProviderId, Arc<dyn WorkerLogs>)> {
        self.logs
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get all provider IDs that have at least one capability registered
    pub fn all_provider_ids(&self) -> Vec<ProviderId> {
        let mut ids = std::collections::HashSet::new();
        ids.extend(self.lifecycle.iter().map(|e| e.key().clone()));
        ids.extend(self.health.iter().map(|e| e.key().clone()));
        ids.extend(self.logs.iter().map(|e| e.key().clone()));
        ids.extend(self.cost.iter().map(|e| e.key().clone()));
        ids.extend(self.eligibility.iter().map(|e| e.key().clone()));
        ids.extend(self.metrics.iter().map(|e| e.key().clone()));
        ids.into_iter().collect()
    }

    // ========================================================================
    // Removal Methods
    // ========================================================================

    /// Remove a provider's lifecycle capability
    ///
    /// Returns the removed provider if it existed.
    pub fn remove_lifecycle(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerLifecycle>> {
        debug!(?provider_id, "Removing WorkerLifecycle capability");
        self.lifecycle.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove a provider's health check capability
    pub fn remove_health(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerHealth>> {
        debug!(?provider_id, "Removing WorkerHealth capability");
        self.health.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove a provider's logs capability
    pub fn remove_logs(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerLogs>> {
        debug!(?provider_id, "Removing WorkerLogs capability");
        self.logs.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove a provider's cost estimation capability
    pub fn remove_cost(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerCost>> {
        debug!(?provider_id, "Removing WorkerCost capability");
        self.cost.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove a provider's eligibility check capability
    pub fn remove_eligibility(
        &self,
        provider_id: &ProviderId,
    ) -> Option<Arc<dyn WorkerEligibility>> {
        debug!(?provider_id, "Removing WorkerEligibility capability");
        self.eligibility.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove a provider's metrics collection capability
    pub fn remove_metrics(&self, provider_id: &ProviderId) -> Option<Arc<dyn WorkerMetrics>> {
        debug!(?provider_id, "Removing WorkerMetrics capability");
        self.metrics.remove(provider_id).map(|(_, v)| v)
    }

    /// Remove all capabilities for a provider
    ///
    /// This is useful when a provider is disabled or removed from the system.
    pub fn remove_all(&self, provider_id: &ProviderId) {
        debug!(?provider_id, "Removing all capabilities");
        self.lifecycle.remove(provider_id);
        self.health.remove(provider_id);
        self.logs.remove(provider_id);
        self.cost.remove(provider_id);
        self.eligibility.remove(provider_id);
        self.metrics.remove(provider_id);
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Check if a provider has the lifecycle capability registered
    pub fn has_lifecycle(&self, provider_id: &ProviderId) -> bool {
        self.lifecycle.contains_key(provider_id)
    }

    /// Check if a provider has the health check capability registered
    pub fn has_health(&self, provider_id: &ProviderId) -> bool {
        self.health.contains_key(provider_id)
    }

    /// Check if a provider has the logs capability registered
    pub fn has_logs(&self, provider_id: &ProviderId) -> bool {
        self.logs.contains_key(provider_id)
    }

    /// Check if a provider has the cost estimation capability registered
    pub fn has_cost(&self, provider_id: &ProviderId) -> bool {
        self.cost.contains_key(provider_id)
    }

    /// Check if a provider has the eligibility check capability registered
    pub fn has_eligibility(&self, provider_id: &ProviderId) -> bool {
        self.eligibility.contains_key(provider_id)
    }

    /// Check if a provider has the metrics collection capability registered
    pub fn has_metrics(&self, provider_id: &ProviderId) -> bool {
        self.metrics.contains_key(provider_id)
    }

    /// Get the number of providers with lifecycle capability
    pub fn lifecycle_count(&self) -> usize {
        self.lifecycle.len()
    }

    /// Get the number of providers with health check capability
    pub fn health_count(&self) -> usize {
        self.health.len()
    }

    /// Get the number of providers with logs capability
    pub fn logs_count(&self) -> usize {
        self.logs.len()
    }

    /// Clear all capabilities from the registry
    ///
    /// This is primarily useful for testing.
    pub fn clear(&self) {
        debug!("Clearing all provider capabilities");
        self.lifecycle.clear();
        self.health.clear();
        self.logs.clear();
        self.cost.clear();
        self.eligibility.clear();
        self.metrics.clear();
    }

    /// Check if the registry is empty (no providers registered)
    pub fn is_empty(&self) -> bool {
        self.lifecycle.is_empty()
            && self.health.is_empty()
            && self.logs.is_empty()
            && self.cost.is_empty()
            && self.eligibility.is_empty()
            && self.metrics.is_empty()
    }
}

#[cfg(test)]
mod tests {
    // Include the tests from the separate test file
    include!("capability_registry_tests.rs");
}

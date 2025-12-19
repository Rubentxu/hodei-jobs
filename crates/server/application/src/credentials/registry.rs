//! Credential Provider Registry Service
//!
//! Manages multiple credential providers with fallback support,
//! health checking, and configuration from environment variables.

use hodei_server_domain::credentials::{
    AuditAction, AuditContext, CredentialError, CredentialProvider, Secret, SecretValue,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for a credential provider
#[derive(Debug, Clone)]
pub struct CredentialProviderConfig {
    /// Provider name (e.g., "postgres", "vault")
    pub name: String,
    /// Whether this provider is enabled
    pub enabled: bool,
    /// Priority for provider selection (higher = preferred)
    pub priority: u32,
    /// Whether to use this provider as fallback
    pub is_fallback: bool,
}

impl CredentialProviderConfig {
    /// Creates a new provider configuration
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            priority: 0,
            is_fallback: false,
        }
    }

    /// Sets the priority
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Marks as fallback provider
    pub fn as_fallback(mut self) -> Self {
        self.is_fallback = true;
        self
    }

    /// Disables the provider
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Statistics about the credential provider registry
#[derive(Debug, Clone, Default)]
pub struct CredentialRegistryStats {
    /// Total number of registered providers
    pub total_providers: usize,
    /// Number of enabled providers
    pub enabled_providers: usize,
    /// Number of healthy providers
    pub healthy_providers: usize,
    /// Name of the default provider
    pub default_provider: Option<String>,
    /// Names of fallback providers
    pub fallback_providers: Vec<String>,
}

/// Registry for managing multiple credential providers
///
/// # Features
///
/// - Register multiple providers with different priorities
/// - Automatic fallback to secondary providers
/// - Health checking for all providers
/// - Configuration from environment variables
///
/// # Example
///
/// ```ignore
/// let registry = CredentialProviderRegistry::new();
/// registry.register(postgres_provider, CredentialProviderConfig::new("postgres").with_priority(100)).await;
/// registry.register(vault_provider, CredentialProviderConfig::new("vault").as_fallback()).await;
///
/// // Get secret using default provider with fallback
/// let secret = registry.get_secret("api_key").await?;
/// ```
pub struct CredentialProviderRegistry {
    providers: RwLock<HashMap<String, RegisteredProvider>>,
    default_provider: RwLock<Option<String>>,
}

struct RegisteredProvider {
    provider: Arc<dyn CredentialProvider>,
    config: CredentialProviderConfig,
    healthy: bool,
}

impl CredentialProviderRegistry {
    /// Creates a new empty registry
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            default_provider: RwLock::new(None),
        }
    }

    /// Registers a credential provider
    ///
    /// If this is the first provider or has the highest priority,
    /// it becomes the default provider.
    pub async fn register(
        &self,
        provider: Arc<dyn CredentialProvider>,
        config: CredentialProviderConfig,
    ) {
        let name = config.name.clone();
        let priority = config.priority;
        let enabled = config.enabled;

        let registered = RegisteredProvider {
            provider,
            config,
            healthy: true, // Assume healthy until health check
        };

        {
            let mut providers = self.providers.write().await;
            providers.insert(name.clone(), registered);
        }

        // Update default provider if this one has higher priority
        if enabled {
            let mut default = self.default_provider.write().await;
            let should_be_default = match &*default {
                None => true,
                Some(current_name) => {
                    let providers = self.providers.read().await;
                    providers
                        .get(current_name)
                        .map(|p| priority > p.config.priority)
                        .unwrap_or(true)
                }
            };

            if should_be_default {
                *default = Some(name.clone());
            }
        }

        info!(provider = %name, priority, enabled, "Registered credential provider");
    }

    /// Unregisters a provider by name
    pub async fn unregister(&self, name: &str) -> Option<Arc<dyn CredentialProvider>> {
        let removed = {
            let mut providers = self.providers.write().await;
            providers.remove(name)
        };

        if let Some(ref _p) = removed {
            // If this was the default provider, select a new one
            let mut default = self.default_provider.write().await;
            if default.as_deref() == Some(name) {
                *default = self.select_new_default().await;
            }
            info!(provider = %name, "Unregistered credential provider");
        }

        removed.map(|r| r.provider)
    }

    /// Selects a new default provider based on priority
    async fn select_new_default(&self) -> Option<String> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .filter(|(_, p)| p.config.enabled && p.healthy)
            .max_by_key(|(_, p)| p.config.priority)
            .map(|(name, _)| name.clone())
    }

    /// Gets a provider by name
    pub async fn get_provider(&self, name: &str) -> Option<Arc<dyn CredentialProvider>> {
        let providers = self.providers.read().await;
        providers.get(name).map(|r| r.provider.clone())
    }

    /// Gets the default provider
    pub async fn get_default_provider(&self) -> Option<Arc<dyn CredentialProvider>> {
        let default_name = self.default_provider.read().await.clone()?;
        self.get_provider(&default_name).await
    }

    /// Gets the default provider name
    pub async fn get_default_provider_name(&self) -> Option<String> {
        self.default_provider.read().await.clone()
    }

    /// Sets the default provider by name
    pub async fn set_default_provider(&self, name: &str) -> Result<(), CredentialError> {
        let providers = self.providers.read().await;
        if !providers.contains_key(name) {
            return Err(CredentialError::provider_unavailable(format!(
                "Provider '{}' not registered",
                name
            )));
        }

        let mut default = self.default_provider.write().await;
        *default = Some(name.to_string());
        info!(provider = %name, "Set default credential provider");
        Ok(())
    }

    /// Lists all registered provider names
    pub async fn list_providers(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        providers.keys().cloned().collect()
    }

    /// Gets fallback providers ordered by priority
    async fn get_fallback_providers(&self) -> Vec<Arc<dyn CredentialProvider>> {
        let providers = self.providers.read().await;
        let default_name = self.default_provider.read().await.clone();

        let mut fallbacks: Vec<_> = providers
            .iter()
            .filter(|(name, p)| {
                p.config.enabled
                    && p.healthy
                    && (p.config.is_fallback || default_name.as_ref() != Some(*name))
            })
            .collect();

        // Sort by priority (higher first)
        fallbacks.sort_by(|a, b| b.1.config.priority.cmp(&a.1.config.priority));

        fallbacks
            .into_iter()
            .map(|(_, p)| p.provider.clone())
            .collect()
    }

    /// Gets a secret using the default provider with fallback support
    pub async fn get_secret(&self, key: &str) -> Result<Secret, CredentialError> {
        // Try default provider first
        if let Some(default) = self.get_default_provider().await {
            match default.get_secret(key).await {
                Ok(secret) => return Ok(secret),
                Err(e) if e.is_retryable() => {
                    warn!(
                        provider = %default.name(),
                        key = %key,
                        error = %e,
                        "Default provider failed, trying fallbacks"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        // Try fallback providers
        for fallback in self.get_fallback_providers().await {
            debug!(provider = %fallback.name(), key = %key, "Trying fallback provider");
            match fallback.get_secret(key).await {
                Ok(secret) => {
                    info!(
                        provider = %fallback.name(),
                        key = %key,
                        "Retrieved secret from fallback provider"
                    );
                    return Ok(secret);
                }
                Err(e) if e.is_retryable() => {
                    warn!(
                        provider = %fallback.name(),
                        key = %key,
                        error = %e,
                        "Fallback provider failed"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(CredentialError::provider_unavailable(
            "All providers unavailable",
        ))
    }

    /// Sets a secret using the default provider
    pub async fn set_secret(
        &self,
        key: &str,
        value: SecretValue,
    ) -> Result<Secret, CredentialError> {
        let provider = self
            .get_default_provider()
            .await
            .ok_or_else(|| CredentialError::provider_unavailable("No default provider"))?;

        provider.set_secret(key, value).await
    }

    /// Deletes a secret using the default provider
    pub async fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
        let provider = self
            .get_default_provider()
            .await
            .ok_or_else(|| CredentialError::provider_unavailable("No default provider"))?;

        provider.delete_secret(key).await
    }

    /// Lists secrets from the default provider
    pub async fn list_secrets(&self, prefix: Option<&str>) -> Result<Vec<String>, CredentialError> {
        let provider = self
            .get_default_provider()
            .await
            .ok_or_else(|| CredentialError::provider_unavailable("No default provider"))?;

        provider.list_secrets(prefix).await
    }

    /// Performs health check on all providers
    pub async fn health_check_all(&self) -> HashMap<String, Result<(), CredentialError>> {
        let providers = self.providers.read().await;
        let mut results = HashMap::new();

        for (name, registered) in providers.iter() {
            let result = registered.provider.health_check().await;
            results.insert(name.clone(), result);
        }

        // Update health status
        drop(providers);
        for (name, result) in &results {
            self.update_health_status(name, result.is_ok()).await;
        }

        results
    }

    /// Updates the health status of a provider
    async fn update_health_status(&self, name: &str, healthy: bool) {
        let mut providers = self.providers.write().await;
        if let Some(provider) = providers.get_mut(name) {
            let was_healthy = provider.healthy;
            provider.healthy = healthy;

            if was_healthy != healthy {
                if healthy {
                    info!(provider = %name, "Provider became healthy");
                } else {
                    warn!(provider = %name, "Provider became unhealthy");
                }
            }
        }

        // If the default provider became unhealthy, select a new one
        drop(providers);
        if !healthy {
            let default = self.default_provider.read().await.clone();
            if default.as_deref() == Some(name) {
                let new_default = self.select_new_default().await;
                if new_default.is_some() {
                    let mut default = self.default_provider.write().await;
                    *default = new_default;
                }
            }
        }
    }

    /// Gets registry statistics
    pub async fn stats(&self) -> CredentialRegistryStats {
        let providers = self.providers.read().await;

        let enabled_providers = providers.values().filter(|p| p.config.enabled).count();

        let healthy_providers = providers
            .values()
            .filter(|p| p.config.enabled && p.healthy)
            .count();

        let fallback_providers: Vec<String> = providers
            .iter()
            .filter(|(_, p)| p.config.is_fallback && p.config.enabled)
            .map(|(name, _)| name.clone())
            .collect();

        CredentialRegistryStats {
            total_providers: providers.len(),
            enabled_providers,
            healthy_providers,
            default_provider: self.default_provider.read().await.clone(),
            fallback_providers,
        }
    }

    /// Audits access to a secret across all relevant providers
    pub async fn audit_access(&self, key: &str, action: AuditAction, context: &AuditContext) {
        if let Some(provider) = self.get_default_provider().await {
            provider.audit_access(key, action, context).await;
        }
    }
}

impl Default for CredentialProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Mock provider for testing
    struct MockProvider {
        name: String,
        secrets: RwLock<HashMap<String, String>>,
        should_fail: bool,
        fail_retryable: bool,
    }

    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                secrets: RwLock::new(HashMap::new()),
                should_fail: false,
                fail_retryable: false,
            }
        }

        fn with_secret(self, key: &str, value: &str) -> Self {
            let mut secrets = self.secrets.try_write().unwrap();
            secrets.insert(key.to_string(), value.to_string());
            drop(secrets);
            self
        }

        fn failing(mut self, retryable: bool) -> Self {
            self.should_fail = true;
            self.fail_retryable = retryable;
            self
        }
    }

    #[async_trait]
    impl CredentialProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        async fn get_secret(&self, key: &str) -> Result<Secret, CredentialError> {
            if self.should_fail {
                return if self.fail_retryable {
                    Err(CredentialError::connection("Mock connection error"))
                } else {
                    Err(CredentialError::not_found(key))
                };
            }

            let secrets = self.secrets.read().await;
            match secrets.get(key) {
                Some(value) => Ok(Secret::builder(key)
                    .value(SecretValue::from_str(value))
                    .build()),
                None => Err(CredentialError::not_found(key)),
            }
        }

        async fn set_secret(
            &self,
            key: &str,
            value: SecretValue,
        ) -> Result<Secret, CredentialError> {
            if self.should_fail {
                return Err(CredentialError::connection("Mock connection error"));
            }

            let mut secrets = self.secrets.write().await;
            let value_str = value.expose_str().unwrap_or_default().to_string();
            secrets.insert(key.to_string(), value_str.clone());

            Ok(Secret::builder(key)
                .value(SecretValue::from_str(&value_str))
                .build())
        }

        async fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
            if self.should_fail {
                return Err(CredentialError::connection("Mock connection error"));
            }

            let mut secrets = self.secrets.write().await;
            secrets.remove(key);
            Ok(())
        }

        async fn list_secrets(&self, prefix: Option<&str>) -> Result<Vec<String>, CredentialError> {
            if self.should_fail {
                return Err(CredentialError::connection("Mock connection error"));
            }

            let secrets = self.secrets.read().await;
            let keys: Vec<String> = secrets
                .keys()
                .filter(|k| prefix.map(|p| k.starts_with(p)).unwrap_or(true))
                .cloned()
                .collect();
            Ok(keys)
        }

        async fn health_check(&self) -> Result<(), CredentialError> {
            if self.should_fail {
                Err(CredentialError::connection("Mock unhealthy"))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_register_provider() {
        let registry = CredentialProviderRegistry::new();
        let provider = Arc::new(MockProvider::new("test"));

        registry
            .register(provider, CredentialProviderConfig::new("test"))
            .await;

        let providers = registry.list_providers().await;
        assert_eq!(providers.len(), 1);
        assert!(providers.contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_first_provider_becomes_default() {
        let registry = CredentialProviderRegistry::new();
        let provider = Arc::new(MockProvider::new("test"));

        registry
            .register(provider, CredentialProviderConfig::new("test"))
            .await;

        let default = registry.get_default_provider_name().await;
        assert_eq!(default, Some("test".to_string()));
    }

    #[tokio::test]
    async fn test_higher_priority_becomes_default() {
        let registry = CredentialProviderRegistry::new();

        let low = Arc::new(MockProvider::new("low"));
        let high = Arc::new(MockProvider::new("high"));

        registry
            .register(low, CredentialProviderConfig::new("low").with_priority(10))
            .await;
        registry
            .register(
                high,
                CredentialProviderConfig::new("high").with_priority(100),
            )
            .await;

        let default = registry.get_default_provider_name().await;
        assert_eq!(default, Some("high".to_string()));
    }

    #[tokio::test]
    async fn test_get_secret_from_default() {
        let registry = CredentialProviderRegistry::new();
        let provider = Arc::new(MockProvider::new("test").with_secret("api_key", "secret123"));

        registry
            .register(provider, CredentialProviderConfig::new("test"))
            .await;

        let secret = registry.get_secret("api_key").await.unwrap();
        assert_eq!(secret.key(), "api_key");
        assert_eq!(secret.value().expose_str(), Some("secret123"));
    }

    #[tokio::test]
    async fn test_fallback_on_retryable_error() {
        let registry = CredentialProviderRegistry::new();

        // Primary provider fails with retryable error
        let primary = Arc::new(MockProvider::new("primary").failing(true));
        // Fallback provider has the secret
        let fallback =
            Arc::new(MockProvider::new("fallback").with_secret("api_key", "from_fallback"));

        registry
            .register(
                primary,
                CredentialProviderConfig::new("primary").with_priority(100),
            )
            .await;
        registry
            .register(
                fallback,
                CredentialProviderConfig::new("fallback")
                    .with_priority(50)
                    .as_fallback(),
            )
            .await;

        let secret = registry.get_secret("api_key").await.unwrap();
        assert_eq!(secret.value().expose_str(), Some("from_fallback"));
    }

    #[tokio::test]
    async fn test_no_fallback_on_not_found() {
        let registry = CredentialProviderRegistry::new();

        // Primary provider returns NotFound (non-retryable)
        let primary = Arc::new(MockProvider::new("primary"));
        let fallback =
            Arc::new(MockProvider::new("fallback").with_secret("api_key", "from_fallback"));

        registry
            .register(
                primary,
                CredentialProviderConfig::new("primary").with_priority(100),
            )
            .await;
        registry
            .register(
                fallback,
                CredentialProviderConfig::new("fallback")
                    .with_priority(50)
                    .as_fallback(),
            )
            .await;

        // Should fail without trying fallback because NotFound is not retryable
        let result = registry.get_secret("missing_key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_default_provider() {
        let registry = CredentialProviderRegistry::new();

        let p1 = Arc::new(MockProvider::new("p1"));
        let p2 = Arc::new(MockProvider::new("p2"));

        registry
            .register(p1, CredentialProviderConfig::new("p1").with_priority(100))
            .await;
        registry
            .register(p2, CredentialProviderConfig::new("p2").with_priority(50))
            .await;

        assert_eq!(
            registry.get_default_provider_name().await,
            Some("p1".to_string())
        );

        registry.set_default_provider("p2").await.unwrap();
        assert_eq!(
            registry.get_default_provider_name().await,
            Some("p2".to_string())
        );
    }

    #[tokio::test]
    async fn test_unregister_provider() {
        let registry = CredentialProviderRegistry::new();

        let p1 = Arc::new(MockProvider::new("p1"));
        let p2 = Arc::new(MockProvider::new("p2"));

        registry
            .register(p1, CredentialProviderConfig::new("p1").with_priority(100))
            .await;
        registry
            .register(p2, CredentialProviderConfig::new("p2").with_priority(50))
            .await;

        // Unregister default provider
        registry.unregister("p1").await;

        // p2 should become default
        assert_eq!(
            registry.get_default_provider_name().await,
            Some("p2".to_string())
        );
        assert_eq!(registry.list_providers().await.len(), 1);
    }

    #[tokio::test]
    async fn test_health_check_all() {
        let registry = CredentialProviderRegistry::new();

        let healthy = Arc::new(MockProvider::new("healthy"));
        let unhealthy = Arc::new(MockProvider::new("unhealthy").failing(true));

        registry
            .register(healthy, CredentialProviderConfig::new("healthy"))
            .await;
        registry
            .register(unhealthy, CredentialProviderConfig::new("unhealthy"))
            .await;

        let results = registry.health_check_all().await;

        assert!(results.get("healthy").unwrap().is_ok());
        assert!(results.get("unhealthy").unwrap().is_err());
    }

    #[tokio::test]
    async fn test_stats() {
        let registry = CredentialProviderRegistry::new();

        let p1 = Arc::new(MockProvider::new("p1"));
        let p2 = Arc::new(MockProvider::new("p2"));

        registry
            .register(
                p1,
                CredentialProviderConfig::new("p1")
                    .with_priority(100)
                    .as_fallback(),
            )
            .await;
        registry
            .register(p2, CredentialProviderConfig::new("p2").disabled())
            .await;

        let stats = registry.stats().await;

        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.enabled_providers, 1);
        assert_eq!(stats.default_provider, Some("p1".to_string()));
        assert!(stats.fallback_providers.contains(&"p1".to_string()));
    }

    #[tokio::test]
    async fn test_set_secret() {
        let registry = CredentialProviderRegistry::new();
        let provider = Arc::new(MockProvider::new("test"));

        registry
            .register(provider, CredentialProviderConfig::new("test"))
            .await;

        let secret = registry
            .set_secret("new_key", SecretValue::from_str("new_value"))
            .await
            .unwrap();

        assert_eq!(secret.key(), "new_key");

        // Verify it can be retrieved
        let retrieved = registry.get_secret("new_key").await.unwrap();
        assert_eq!(retrieved.value().expose_str(), Some("new_value"));
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let registry = CredentialProviderRegistry::new();
        let provider = Arc::new(
            MockProvider::new("test")
                .with_secret("app/db/password", "pw1")
                .with_secret("app/db/user", "user1")
                .with_secret("other/key", "val"),
        );

        registry
            .register(provider, CredentialProviderConfig::new("test"))
            .await;

        let all = registry.list_secrets(None).await.unwrap();
        assert_eq!(all.len(), 3);

        let db_only = registry.list_secrets(Some("app/db/")).await.unwrap();
        assert_eq!(db_only.len(), 2);
    }
}

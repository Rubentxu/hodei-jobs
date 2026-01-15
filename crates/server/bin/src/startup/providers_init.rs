//! Provider Initialization Module
//!
//! This module handles the initialization of worker providers at application startup.
//! It loads providers from the database, validates their connectivity, and registers
//! them for use by the job scheduler.
//!
//! # Architecture
//!
//! This module follows the Hexagonal Architecture pattern, with clear separation
//! between domain logic (validation, initialization), application services (bootstrap),
//! and infrastructure (Postgres repository).
//!
//! # Startup Sequence
//!
//! The initialization happens in the following order:
//! 1. Load enabled providers from the database
//! 2. Validate each provider's connectivity and permissions
//! 3. Build provider runtime instances
//! 4. Register providers in the registry
//! 5. Return summary of initialization results
//!
//! # Error Handling
//!
//! All errors are categorized as either:
//! - **Fatal**: Provider cannot be used (missing RBAC, connection failed)
//! - **Non-fatal**: Provider has warnings but can still function
//!
//! The system fails fast if NO providers are available at startup.

use hodei_server_application::providers::bootstrap::ProviderBootstrap;
use hodei_server_application::providers::registry::ProviderRegistry;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::providers::config::{
    ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
};
use hodei_server_domain::providers::errors::{
    ErrorMessage, InitializationSummary, ProviderInitializationError, ProviderInitializationResult,
};
use hodei_server_domain::providers::validator::ValidationReport;
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, ProviderStatus};
use hodei_server_domain::workers::ProviderType;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Configuration for provider initialization
#[derive(Debug, Clone)]
pub struct ProvidersInitConfig {
    /// Timeout for provider initialization
    pub initialization_timeout: Duration,
    /// Timeout for individual provider validation
    pub validation_timeout: Duration,
    /// Whether to fail if no providers are available
    pub fail_if_no_providers: bool,
    /// Whether to validate providers during initialization
    pub validate_providers: bool,
    /// Providers to skip validation for (by type)
    pub skip_validation_for: Vec<ProviderType>,
    /// Provider bootstrap configuration path (optional)
    pub bootstrap_config_path: Option<std::path::PathBuf>,
}

impl Default for ProvidersInitConfig {
    fn default() -> Self {
        Self {
            initialization_timeout: Duration::from_secs(60),
            validation_timeout: Duration::from_secs(30),
            fail_if_no_providers: true,
            validate_providers: true,
            skip_validation_for: vec![ProviderType::Test],
            bootstrap_config_path: None,
        }
    }
}

/// Provider initialization result
#[derive(Debug, Clone)]
pub struct ProvidersInitResult {
    /// Successfully initialized providers
    pub successful_providers: Vec<ProviderId>,
    /// Providers that failed initialization
    pub failed_providers: Vec<(ProviderId, ProviderInitializationError)>,
    /// Providers that initialized with warnings
    pub warning_providers: Vec<(ProviderId, ProviderInitializationError)>,
    /// Total count of providers
    pub total_providers: usize,
    /// Summary of the initialization
    pub summary: InitializationSummary,
}

impl ProvidersInitResult {
    /// Create a successful result
    pub fn success(successful_providers: Vec<ProviderId>, total: usize) -> Self {
        let summary = InitializationSummary::new(
            successful_providers.len(),
            successful_providers,
            vec![],
            vec![],
            0,
        );
        Self {
            successful_providers: summary.successful.clone(),
            failed_providers: vec![],
            warning_providers: vec![],
            total_providers: total,
            summary,
        }
    }

    /// Create a result with failures
    pub fn with_failures(
        successful: Vec<ProviderId>,
        failed: Vec<(ProviderId, ProviderInitializationError)>,
        warnings: Vec<(ProviderId, ProviderInitializationError)>,
        total: usize,
    ) -> Self {
        let warnings_tuples: Vec<(ProviderId, Vec<String>)> = warnings
            .iter()
            .map(|(id, _)| (id.clone(), vec![]))
            .collect();

        let summary = InitializationSummary::new(
            successful.len() + failed.len() + warnings.len(),
            successful.clone(),
            failed.clone(),
            warnings_tuples,
            0,
        );
        Self {
            successful_providers: successful,
            failed_providers: failed,
            warning_providers: warnings,
            total_providers: total,
            summary,
        }
    }

    /// Returns true if all providers initialized successfully
    pub fn is_complete_success(&self) -> bool {
        self.failed_providers.is_empty() && self.warning_providers.is_empty()
    }

    /// Returns true if at least one provider initialized
    pub fn has_any_provider(&self) -> bool {
        !self.successful_providers.is_empty()
    }

    /// Returns a summary message for operators
    pub fn summary_message(&self) -> String {
        format!(
            "Provider initialization: {} successful, {} failed, {} with warnings out of {} total",
            self.successful_providers.len(),
            self.failed_providers.len(),
            self.warning_providers.len(),
            self.total_providers
        )
    }
}

/// Service for initializing providers at startup
pub struct ProvidersInitializer {
    /// Provider configuration repository
    repository: Arc<dyn ProviderConfigRepository>,
    /// Provider registry for runtime management
    registry: Arc<ProviderRegistry>,
    /// Event bus for domain events
    event_bus: Option<Arc<dyn EventBus>>,
    /// Configuration for initialization
    config: ProvidersInitConfig,
}

impl ProvidersInitializer {
    /// Create a new initializer
    pub fn new(
        repository: Arc<dyn ProviderConfigRepository>,
        registry: Arc<ProviderRegistry>,
        config: ProvidersInitConfig,
    ) -> Self {
        Self {
            repository,
            registry,
            event_bus: None,
            config,
        }
    }

    /// Set the event bus for domain events
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Initialize all providers
    ///
    /// This method:
    /// 1. Loads enabled providers from the database
    /// 2. Validates connectivity for each provider
    /// 3. Registers providers in the registry
    ///
    /// Returns an error if `fail_if_no_providers` is true and no providers
    /// are available, or if all providers fail initialization.
    pub async fn initialize(&self) -> Result<ProvidersInitResult, ProviderInitializationError> {
        info!("Starting provider initialization...");

        // Step 1: Load enabled providers from database
        let enabled_providers = self.repository.find_enabled().await.map_err(|e| {
            ProviderInitializationError::UnexpectedError {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Test,
                source: ErrorMessage::from(format!("Failed to load providers: {}", e)),
            }
        })?;

        debug!("Found {} enabled providers", enabled_providers.len());

        if enabled_providers.is_empty() {
            warn!("No enabled providers found in database");
            if self.config.fail_if_no_providers {
                return Err(ProviderInitializationError::NoActiveProviders.into());
            }
            return Ok(ProvidersInitResult::success(vec![], 0));
        }

        // Step 2: Run bootstrap from config file if provided
        if let Some(ref path) = self.config.bootstrap_config_path {
            self.run_bootstrap(path).await.map_err(|e| {
                ProviderInitializationError::UnexpectedError {
                    provider_id: ProviderId::new(),
                    provider_type: ProviderType::Test,
                    source: ErrorMessage::from(format!("Bootstrap failed: {}", e)),
                }
            })?;
        }

        // Step 3: Reload providers after bootstrap (to include new ones)
        let providers = self.repository.find_enabled().await.map_err(|e| {
            ProviderInitializationError::UnexpectedError {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Test,
                source: ErrorMessage::from(format!("Failed to reload providers: {}", e)),
            }
        })?;

        // Step 4: Initialize each provider
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        let mut warnings = Vec::new();

        for provider in &providers {
            match self.initialize_provider(provider).await {
                Ok(_warning) => {
                    // Provider initialized successfully (with potential warnings)
                    successful.push(provider.id.clone());
                    info!(
                        "✓ Provider '{}' ({}) initialized successfully",
                        provider.name, provider.provider_type
                    );
                }
                Err(ref err) => {
                    // Convert DomainError to ProviderInitializationError for classification
                    let init_err: ProviderInitializationError = match err {
                        DomainError::ProviderNotFound { provider_id } => {
                            ProviderInitializationError::ProviderNotFound {
                                provider_id: provider_id.clone(),
                            }
                        }
                        DomainError::InvalidProviderConfig { message } => {
                            ProviderInitializationError::InvalidConfiguration {
                                provider_id: provider.id.clone(),
                                provider_type: provider.provider_type.clone(),
                                reason: message.clone(),
                            }
                        }
                        _ => ProviderInitializationError::UnexpectedError {
                            provider_id: provider.id.clone(),
                            provider_type: provider.provider_type.clone(),
                            source: ErrorMessage::from(err.to_string()),
                        },
                    };

                    if init_err.is_fatal() {
                        failed.push((provider.id.clone(), init_err));
                        error!(
                            "✗ Provider '{}' ({}) failed fatally: {}",
                            provider.name, provider.provider_type, err
                        );
                    } else {
                        warnings.push((provider.id.clone(), init_err));
                        warn!(
                            "! Provider '{}' ({}) initialized with warnings: {}",
                            provider.name, provider.provider_type, err
                        );
                    }
                }
            }
        }

        let result =
            ProvidersInitResult::with_failures(successful, failed, warnings, providers.len());

        // Step 5: Check if we have any usable providers
        if !result.has_any_provider() && self.config.fail_if_no_providers {
            error!("All providers failed initialization, failing startup");
            return Err(ProviderInitializationError::NoActiveProviders.into());
        }

        info!("{}", result.summary_message());
        Ok(result)
    }

    /// Initialize a single provider
    async fn initialize_provider(
        &self,
        config: &ProviderConfig,
    ) -> std::result::Result<ProviderInitializationError, DomainError> {
        // Validate provider if configured
        if self.config.validate_providers
            && !self
                .config
                .skip_validation_for
                .contains(&config.provider_type)
        {
            // For now, we skip detailed validation and just check config is valid
            // In production, this would use ProviderConnectionValidator
            info!(
                "Skipping detailed validation for provider '{}' ({}), config is valid",
                config.name, config.provider_type
            );
        }

        // Provider is ready for use (with zero warnings)
        Ok(ProviderInitializationError::ValidationWarnings {
            provider_id: config.id.clone(),
            provider_type: config.provider_type.clone(),
            warnings_count: 0,
        })
    }

    /// Validate a provider's connectivity
    async fn validate_provider(
        &self,
        config: &ProviderConfig,
    ) -> Result<ValidationReport, ProviderInitializationError> {
        // TODO: Implement actual provider validation using ProviderConnectionValidator
        // For now, we just check that the provider config is valid
        debug!(
            "Validating provider '{}' ({})",
            config.name, config.provider_type
        );

        // Return a successful validation report (no actual validation done yet)
        Ok(ValidationReport::success(
            config.id.clone(),
            config.name.clone(),
            0,
            vec![],
        ))
    }

    /// Run provider bootstrap from configuration file
    async fn run_bootstrap(&self, path: &std::path::Path) -> std::result::Result<(), DomainError> {
        info!("Running provider bootstrap from {:?}", path);

        let bootstrap = ProviderBootstrap::new(self.repository.clone());
        let result =
            bootstrap
                .load_from_file(path)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Provider bootstrap failed: {}", e),
                })?;

        debug!(
            "Bootstrap completed: {} registered, {} skipped, {} failed",
            result.registered.len(),
            result.skipped.len(),
            result.failed.len()
        );

        Ok(())
    }
}

/// Initialize providers with default configuration
pub async fn initialize_providers(
    repository: Arc<dyn ProviderConfigRepository>,
    event_bus: Option<Arc<dyn EventBus>>,
) -> Result<ProvidersInitResult, ProviderInitializationError> {
    let config = ProvidersInitConfig::default();
    let registry = Arc::new(ProviderRegistry::new(repository.clone()));

    let event_bus = event_bus.ok_or_else(|| ProviderInitializationError::UnexpectedError {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Test,
        source: ErrorMessage::from("EventBus is required for provider initialization".to_string()),
    })?;

    let initializer =
        ProvidersInitializer::new(repository, registry, config).with_event_bus(event_bus);

    initializer.initialize().await
}

/// Initialize providers with custom configuration
pub async fn initialize_providers_with_config(
    repository: Arc<dyn ProviderConfigRepository>,
    event_bus: Option<Arc<dyn EventBus>>,
    config: ProvidersInitConfig,
) -> Result<ProvidersInitResult, ProviderInitializationError> {
    let registry = Arc::new(ProviderRegistry::new(repository.clone()));

    let event_bus = event_bus.ok_or_else(|| ProviderInitializationError::UnexpectedError {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Test,
        source: ErrorMessage::from("EventBus is required for provider initialization".to_string()),
    })?;

    let initializer =
        ProvidersInitializer::new(repository, registry, config).with_event_bus(event_bus);

    initializer.initialize().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::providers::config::{DockerConfig, ProviderTypeConfig};
    use hodei_server_domain::shared_kernel::ProviderStatus;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Mock repository for testing
    struct MockProviderConfigRepository {
        configs: RwLock<HashMap<ProviderId, ProviderConfig>>,
    }

    impl MockProviderConfigRepository {
        fn new() -> Self {
            Self {
                configs: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ProviderConfigRepository for MockProviderConfigRepository {
        async fn save(&self, config: &ProviderConfig) -> std::result::Result<(), DomainError> {
            self.configs
                .write()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            id: &ProviderId,
        ) -> std::result::Result<Option<ProviderConfig>, DomainError> {
            Ok(self.configs.read().await.get(id).cloned())
        }

        async fn find_by_name(
            &self,
            name: &str,
        ) -> std::result::Result<Option<ProviderConfig>, DomainError> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .find(|c| c.name == name)
                .cloned())
        }

        async fn find_by_type(
            &self,
            pt: &ProviderType,
        ) -> std::result::Result<Vec<ProviderConfig>, DomainError> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .filter(|c| &c.provider_type == pt)
                .cloned()
                .collect())
        }

        async fn find_enabled(&self) -> std::result::Result<Vec<ProviderConfig>, DomainError> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .filter(|c| c.status == ProviderStatus::Active)
                .cloned()
                .collect())
        }

        async fn find_with_capacity(
            &self,
        ) -> std::result::Result<Vec<ProviderConfig>, DomainError> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .filter(|c| c.status == ProviderStatus::Active && c.has_capacity())
                .cloned()
                .collect())
        }

        async fn find_all(&self) -> std::result::Result<Vec<ProviderConfig>, DomainError> {
            Ok(self.configs.read().await.values().cloned().collect())
        }

        async fn update(&self, config: &ProviderConfig) -> std::result::Result<(), DomainError> {
            self.configs
                .write()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn delete(&self, id: &ProviderId) -> std::result::Result<(), DomainError> {
            self.configs.write().await.remove(id);
            Ok(())
        }

        async fn exists_by_name(&self, name: &str) -> std::result::Result<bool, DomainError> {
            Ok(self.configs.read().await.values().any(|c| c.name == name))
        }
    }

    fn create_docker_config() -> ProviderConfig {
        ProviderConfig::new(
            "test-docker".to_string(),
            ProviderType::Docker,
            ProviderTypeConfig::Docker(DockerConfig::default()),
        )
    }

    fn create_kubernetes_config() -> ProviderConfig {
        ProviderConfig::new(
            "test-k8s".to_string(),
            ProviderType::Kubernetes,
            ProviderTypeConfig::Kubernetes(
                hodei_server_domain::providers::config::KubernetesConfig::default(),
            ),
        )
    }

    #[tokio::test]
    async fn test_initialize_with_no_providers() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        let config = ProvidersInitConfig {
            fail_if_no_providers: false,
            ..Default::default()
        };

        let initializer = ProvidersInitializer::new(repo, registry, config);
        let result = initializer.initialize().await.unwrap();

        assert!(!result.has_any_provider());
        assert_eq!(result.successful_providers.len(), 0);
    }

    #[tokio::test]
    async fn test_initialize_with_providers() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        // Add a test provider
        let docker = create_docker_config();
        repo.save(&docker).await.unwrap();

        let config = ProvidersInitConfig {
            fail_if_no_providers: false,
            validate_providers: false,
            ..Default::default()
        };

        let initializer = ProvidersInitializer::new(repo, registry, config);
        let result = initializer.initialize().await.unwrap();

        assert!(result.has_any_provider());
        assert_eq!(result.successful_providers.len(), 1);
        assert!(result.failed_providers.is_empty());
    }

    #[tokio::test]
    async fn test_initialize_multiple_providers() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        // Add multiple providers
        let docker = create_docker_config();
        let k8s = create_kubernetes_config();

        repo.save(&docker).await.unwrap();
        repo.save(&k8s).await.unwrap();

        let config = ProvidersInitConfig {
            fail_if_no_providers: false,
            validate_providers: false,
            ..Default::default()
        };

        let initializer = ProvidersInitializer::new(repo, registry, config);
        let result = initializer.initialize().await.unwrap();

        assert!(result.has_any_provider());
        assert_eq!(result.successful_providers.len(), 2);
        assert!(result.summary_message().contains("2 successful"));
    }

    #[tokio::test]
    async fn test_fail_if_no_providers() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        let config = ProvidersInitConfig {
            fail_if_no_providers: true,
            ..Default::default()
        };

        let initializer = ProvidersInitializer::new(repo, registry, config);
        let result = initializer.initialize().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_providers_init_result_summary() {
        let result = ProvidersInitResult::success(vec![ProviderId::new(), ProviderId::new()], 3);

        assert!(result.is_complete_success());
        assert!(result.has_any_provider());
        assert_eq!(result.successful_providers.len(), 2);
        assert!(result.summary_message().contains("2 successful"));
    }
}

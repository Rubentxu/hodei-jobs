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
//! 2. Validate each provider's connectivity and permissions using KubernetesConnectionValidator
//! 3. Build provider runtime instances (KubernetesProvider, DockerProvider, etc.)
//! 4. Register providers in ProviderRegistry for lifecycle management
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
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::providers::config::{
    DockerConfig, ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
};
use hodei_server_domain::providers::errors::{
    ErrorMessage, InitializationSummary, ProviderInitializationError,
};
use hodei_server_domain::providers::validator::ProviderConnectionValidator;
use hodei_server_domain::shared_kernel::{DomainError, ProviderId};
use hodei_server_domain::workers::{ProviderType, WorkerProvider};
use hodei_server_infrastructure::providers::docker::DockerProvider;
use hodei_server_infrastructure::providers::kubernetes::KubernetesProvider;
use hodei_server_infrastructure::providers::kubernetes_validator::KubernetesConnectionValidator;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Metrics for provider initialization
#[derive(Debug, Clone, Default)]
pub struct ProviderInitMetrics {
    pub duration_ms: u64,
    pub providers_total: usize,
    pub providers_successful: usize,
    pub providers_failed: usize,
    pub providers_with_warnings: usize,
}

impl ProviderInitMetrics {
    pub fn record(
        &mut self,
        duration_ms: u64,
        total: usize,
        successful: usize,
        failed: usize,
        warnings: usize,
    ) {
        self.duration_ms = duration_ms;
        self.providers_total = total;
        self.providers_successful = successful;
        self.providers_failed = failed;
        self.providers_with_warnings = warnings;
    }
}

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
    /// Provider registry for runtime management (configuration)
    registry: Arc<ProviderRegistry>,
    /// Event bus for domain events
    event_bus: Option<Arc<dyn EventBus>>,
    /// Configuration for initialization
    config: ProvidersInitConfig,
    /// Optional lifecycle manager for registering runtime providers
    lifecycle_manager: Option<Arc<hodei_server_application::workers::WorkerLifecycleManager>>,
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
            lifecycle_manager: None,
        }
    }

    /// Set the event bus for domain events
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set the lifecycle manager for registering runtime providers
    pub fn with_lifecycle_manager(
        mut self,
        lifecycle_manager: Arc<hodei_server_application::workers::WorkerLifecycleManager>,
    ) -> Self {
        self.lifecycle_manager = Some(lifecycle_manager);
        self
    }

    /// Initialize all providers
    ///
    /// This method:
    /// 1. Loads enabled providers from the database
    /// 2. Validates connectivity for each provider using ProviderConnectionValidator
    /// 3. Creates runtime provider instances (KubernetesProvider, DockerProvider, etc.)
    /// 4. Registers providers in the lifecycle manager for worker provisioning
    /// 5. Publishes ProviderRegistered events
    ///
    /// Returns an error if `fail_if_no_providers` is true and no providers
    /// are available, or if all providers fail initialization.
    pub async fn initialize(&self) -> Result<ProvidersInitResult, ProviderInitializationError> {
        let start_time = Instant::now();
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
                return Err(ProviderInitializationError::NoActiveProviders {
                    details: "No enabled providers found in database".to_string(),
                });
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
        let mut initialized_providers: Vec<(ProviderId, Arc<dyn WorkerProvider>)> = Vec::new();

        for provider_config in &providers {
            match self.create_and_register_provider(provider_config).await {
                Ok(provider) => {
                    successful.push(provider_config.id.clone());
                    initialized_providers.push((provider_config.id.clone(), provider));
                    info!(
                        "✓ Provider '{}' ({}) initialized and registered successfully",
                        provider_config.name, provider_config.provider_type
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
                                provider_id: provider_config.id.clone(),
                                provider_type: provider_config.provider_type.clone(),
                                reason: message.clone(),
                            }
                        }
                        _ => ProviderInitializationError::UnexpectedError {
                            provider_id: provider_config.id.clone(),
                            provider_type: provider_config.provider_type.clone(),
                            source: ErrorMessage::from(err.to_string()),
                        },
                    };

                    if init_err.is_fatal() {
                        failed.push((provider_config.id.clone(), init_err));
                        error!(
                            "✗ Provider '{}' ({}) failed fatally: {}",
                            provider_config.name, provider_config.provider_type, err
                        );
                    } else {
                        warnings.push((provider_config.id.clone(), init_err));
                        warn!(
                            "! Provider '{}' ({}) initialized with warnings: {}",
                            provider_config.name, provider_config.provider_type, err
                        );
                    }
                }
            }
        }

        // Step 5: Register all successfully initialized providers in lifecycle manager
        if let Some(ref lifecycle) = self.lifecycle_manager {
            for (provider_id, provider) in &initialized_providers {
                lifecycle.register_provider(provider.clone()).await;
                debug!("Registered provider {} in lifecycle manager", provider_id);
            }
        }

        // Calculate duration
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let result = ProvidersInitResult::with_failures(
            successful.clone(),
            failed.clone(),
            warnings.clone(),
            providers.len(),
        );

        // Step 6: Check if we have any usable providers
        if !result.has_any_provider() && self.config.fail_if_no_providers {
            // Generate detailed error message with all failure reasons
            let failure_details: Vec<String> = failed
                .iter()
                .map(|(id, err)| {
                    format!(
                        "  - Provider '{}' ({}): {}",
                        id.as_uuid(),
                        err.provider_type()
                            .map(|t| format!("{}", t))
                            .unwrap_or_else(|| "unknown".to_string()),
                        err.to_user_message()
                    )
                })
                .chain(warnings.iter().map(|(id, err)| {
                    format!(
                        "  - Provider '{}' ({}): {} [WARNING]",
                        id.as_uuid(),
                        err.provider_type()
                            .map(|t| format!("{}", t))
                            .unwrap_or_else(|| "unknown".to_string()),
                        err.to_user_message()
                    )
                }))
                .collect();

            let detailed_error = if failure_details.is_empty() {
                "All providers failed initialization. No failure details available.".to_string()
            } else {
                format!(
                    "All providers failed initialization. {} provider(s) failed:\n{}",
                    failure_details.len(),
                    failure_details.join("\n")
                )
            };

            error!("{}", detailed_error);
            error!(
                "To resolve provider issues, see: https://hodei-jobs.io/docs/providers/kubernetes-setup"
            );
            return Err(ProviderInitializationError::NoActiveProviders {
                details: detailed_error,
            });
        }

        // Step 7: Publish ProviderRegistered events for successful providers
        self.publish_provider_registered_events(&successful).await;

        // Record metrics
        info!(
            "Provider initialization completed in {}ms: {} successful, {} failed, {} warnings",
            duration_ms,
            successful.len(),
            failed.len(),
            warnings.len()
        );

        Ok(result)
    }

    /// Create and register a provider from configuration
    async fn create_and_register_provider(
        &self,
        config: &ProviderConfig,
    ) -> std::result::Result<Arc<dyn WorkerProvider>, DomainError> {
        // Step 1: Validate provider configuration and connectivity
        self.validate_provider(config).await?;

        // Step 2: Create provider instance based on type
        let provider: Arc<dyn WorkerProvider> = match &config.type_config {
            ProviderTypeConfig::Kubernetes(k8s_config) => {
                let k8s_provider = self.create_kubernetes_provider(config, k8s_config).await?;
                Arc::new(k8s_provider) as Arc<dyn WorkerProvider>
            }
            ProviderTypeConfig::Docker(docker_config) => {
                let docker_provider = self.create_docker_provider(config, docker_config).await?;
                Arc::new(docker_provider) as Arc<dyn WorkerProvider>
            }
            _ => {
                // Unsupported provider type - return error
                return Err(DomainError::InvalidProviderConfig {
                    message: format!(
                        "Provider type '{}' is not supported in this build",
                        config.provider_type
                    ),
                });
            }
        };

        Ok(provider)
    }

    /// Create a KubernetesProvider from configuration
    async fn create_kubernetes_provider(
        &self,
        config: &ProviderConfig,
        k8s_config: &hodei_server_domain::providers::KubernetesConfig,
    ) -> std::result::Result<KubernetesProvider, DomainError> {
        // Use the ProviderId from the config, not a new one
        let provider_id = config.id.clone();

        // Convert domain config to infrastructure config
        let tolerations: Vec<hodei_server_infrastructure::providers::KubernetesToleration> =
            k8s_config
                .tolerations
                .iter()
                .map(
                    |t| hodei_server_infrastructure::providers::KubernetesToleration {
                        key: Some(t.key.clone()),
                        operator: Some(t.operator.clone()),
                        value: t.value.clone(),
                        effect: Some(t.effect.clone()),
                        toleration_seconds: None,
                    },
                )
                .collect();

        let infra_k8s_config = hodei_server_infrastructure::providers::KubernetesConfig {
            namespace: k8s_config.namespace.clone(),
            enable_dynamic_namespaces: false,
            namespace_prefix: "hodei".to_string(),
            kubeconfig_path: k8s_config.kubeconfig_path.clone(),
            context: None,
            service_account: Some(k8s_config.service_account.clone()),
            base_labels: k8s_config.node_selector.clone(),
            base_annotations: HashMap::new(),
            node_selector: k8s_config.node_selector.clone(),
            tolerations,
            pod_affinity: None,
            pod_anti_affinity: None,
            image_pull_secrets: k8s_config.image_pull_secrets.clone(),
            default_cpu_request: "100m".to_string(),
            default_memory_request: "128Mi".to_string(),
            default_cpu_limit: "1000m".to_string(),
            default_memory_limit: "512Mi".to_string(),
            ttl_seconds_after_finished: None,
            creation_timeout_secs: 300,
            pod_security_standard:
                hodei_server_infrastructure::providers::PodSecurityStandard::Baseline,
            security_context:
                hodei_server_infrastructure::providers::SecurityContextConfig::default(),
            enable_hpa: false,
            hpa_config: hodei_server_infrastructure::providers::HPAConfig::default(),
        };

        // Create KubernetesProvider with the infrastructure config
        let provider = KubernetesProvider::with_provider_id(provider_id, infra_k8s_config)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!(
                    "Failed to create KubernetesProvider for '{}': {}",
                    config.name, e
                ),
            })?;

        info!(
            "Created KubernetesProvider '{}' with namespace '{}'",
            config.name, k8s_config.namespace
        );

        Ok(provider)
    }

    /// Create a DockerProvider from configuration
    async fn create_docker_provider(
        &self,
        config: &ProviderConfig,
        docker_config: &DockerConfig,
    ) -> std::result::Result<DockerProvider, DomainError> {
        let provider = DockerProvider::with_config(docker_config.clone())
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!(
                    "Failed to create DockerProvider for '{}': {}",
                    config.name, e
                ),
            })?;

        info!(
            "Created DockerProvider '{}' with socket '{}'",
            config.name, docker_config.socket_path
        );

        Ok(provider)
    }

    /// Validate a provider's connectivity and permissions
    async fn validate_provider(&self, config: &ProviderConfig) -> Result<(), DomainError> {
        // Skip validation for test providers
        if config.provider_type == ProviderType::Test {
            return Ok(());
        }

        // Create validator based on provider type
        let validator: Box<dyn ProviderConnectionValidator> = match &config.type_config {
            ProviderTypeConfig::Kubernetes(_) => Box::new(KubernetesConnectionValidator::new()),
            _ => {
                // For other types, skip validation for now
                debug!(
                    "Skipping validation for provider type '{}'",
                    config.provider_type
                );
                return Ok(());
            }
        };

        // Run validation with timeout
        let validation_result =
            tokio::time::timeout(self.config.validation_timeout, validator.validate(config))
                .await
                .map_err(|_| DomainError::InfrastructureError {
                    message: format!(
                        "Validation timed out for provider '{}' after {}s",
                        config.name,
                        self.config.validation_timeout.as_secs()
                    ),
                })?
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Validation failed for provider '{}': {}", config.name, e),
                });

        match validation_result {
            Ok(()) => {
                debug!("Provider '{}' validation passed", config.name);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Publish ProviderRegistered events for all successfully initialized providers
    async fn publish_provider_registered_events(&self, provider_ids: &[ProviderId]) {
        if let Some(ref event_bus) = self.event_bus {
            for provider_id in provider_ids {
                let event = DomainEvent::ProviderRegistered {
                    provider_id: provider_id.clone(),
                    provider_type: "unknown".to_string(), // Could fetch from registry
                    config_summary: format!("Provider {} registered at startup", provider_id),
                    occurred_at: chrono::Utc::now(),
                    correlation_id: Some("startup".to_string()),
                    actor: Some("provider-initializer".to_string()),
                };

                if let Err(e) = event_bus.publish(&event).await {
                    error!(
                        "Failed to publish ProviderRegistered event for {}: {}",
                        provider_id, e
                    );
                }
            }
        }
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

// Provider Bootstrap Service
// Carga providers desde configuración YAML al arranque

use hodei_server_domain::providers::{
    DockerConfig, FargateConfig, KubernetesConfig, LambdaConfig, ProviderConfig,
    ProviderConfigRepository, ProviderTypeConfig, ValidationReport,
};
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, Result};
use hodei_server_domain::workers::ProviderType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::warn;

/// Configuration for bootstrap of providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBootstrapConfig {
    /// Whether to validate providers during bootstrap
    #[serde(default = "default_validate_on_bootstrap")]
    pub validate_on_bootstrap: bool,
    /// Validation configuration
    #[serde(default)]
    pub validation_config: Option<BootstrapValidationConfig>,
    /// List of providers to register
    pub providers: Vec<ProviderDefinition>,
}

fn default_validate_on_bootstrap() -> bool {
    true
}

/// Validation configuration for bootstrap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapValidationConfig {
    /// Timeout for connection validation
    #[serde(default = "default_validation_timeout")]
    pub timeout_secs: u64,
    /// Whether to validate permissions
    #[serde(default = "default_validate_permissions")]
    pub validate_permissions: bool,
    /// Whether to check resource quotas
    #[serde(default = "default_validate_resources")]
    pub validate_resources: bool,
    /// Whether to skip validation for specific provider types
    #[serde(default)]
    pub skip_validation_for: Vec<String>,
}

fn default_validation_timeout() -> u64 {
    30
}

fn default_validate_permissions() -> bool {
    true
}

fn default_validate_resources() -> bool {
    true
}

/// Definition of a provider in YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefinition {
    /// Unique name of the provider
    pub name: String,
    /// Provider type
    #[serde(rename = "type")]
    pub provider_type: String,
    /// Priority (higher = preferred)
    #[serde(default)]
    pub priority: i32,
    /// Maximum concurrent workers
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Type-specific configuration
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
}

fn default_max_workers() -> u32 {
    10
}

/// Event channel for bootstrap events
pub type BootstrapEventSender = mpsc::Sender<BootstrapEvent>;

/// Bootstrap events for observability
#[derive(Debug, Clone)]
pub enum BootstrapEvent {
    /// Started loading providers
    Started { config_path: String },
    /// Provider registration started
    ProviderRegistrationStarted { name: String },
    /// Provider validation started
    ProviderValidationStarted {
        name: String,
        provider_id: ProviderId,
    },
    /// Provider validation completed
    ProviderValidationCompleted {
        name: String,
        provider_id: ProviderId,
        report: ValidationReport,
    },
    /// Provider registered successfully
    ProviderRegistered {
        name: String,
        provider_id: ProviderId,
    },
    /// Provider skipped (already exists)
    ProviderSkipped { name: String },
    /// Provider failed
    ProviderFailed { name: String, error: String },
    /// Bootstrap completed
    Completed {
        registered: usize,
        skipped: usize,
        failed: usize,
    },
}

/// Service for provider bootstrap
pub struct ProviderBootstrap {
    /// Provider configuration repository
    repository: Arc<dyn ProviderConfigRepository>,
    /// Event sender for observability
    event_sender: Arc<Mutex<Option<BootstrapEventSender>>>,
}

impl ProviderBootstrap {
    /// Create a new bootstrap service
    pub fn new(repository: Arc<dyn ProviderConfigRepository>) -> Self {
        Self {
            repository,
            event_sender: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an event sender for observability
    pub async fn set_event_sender(&self, sender: BootstrapEventSender) {
        let mut sender_ref = self.event_sender.lock().await;
        *sender_ref = Some(sender);
    }

    /// Emit a bootstrap event
    async fn emit_event(&self, event: BootstrapEvent) {
        let sender_opt = self.event_sender.lock().await;
        if let Some(sender) = sender_opt.as_ref() {
            if let Err(e) = sender.send(event).await {
                warn!("Failed to emit bootstrap event: {}", e);
            }
        }
    }

    /// Load providers from file
    pub async fn load_from_file(&self, path: &Path) -> Result<BootstrapResult> {
        self.emit_event(BootstrapEvent::Started {
            config_path: path.to_string_lossy().to_string(),
        })
        .await;

        let content =
            std::fs::read_to_string(path).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to read bootstrap file: {}", e),
            })?;

        self.load_from_yaml(&content).await
    }

    /// Load providers from YAML string
    pub async fn load_from_yaml(&self, yaml_content: &str) -> Result<BootstrapResult> {
        let config: ProviderBootstrapConfig =
            serde_yaml::from_str(yaml_content).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to parse bootstrap YAML: {}", e),
            })?;

        self.bootstrap_providers(&config).await
    }

    /// Execute provider bootstrap
    pub async fn bootstrap_providers(
        &self,
        config: &ProviderBootstrapConfig,
    ) -> Result<BootstrapResult> {
        let mut result = BootstrapResult::default();

        for definition in &config.providers {
            match self
                .register_provider_from_definition(definition, config)
                .await
            {
                Ok(provider) => {
                    result.registered.push(provider.name.clone());
                }
                Err(e) => {
                    if e.to_string().contains("already exists") {
                        result.skipped.push(definition.name.clone());
                    } else {
                        result.failed.push((definition.name.clone(), e.to_string()));
                    }
                }
            }
        }

        self.emit_event(BootstrapEvent::Completed {
            registered: result.registered.len(),
            skipped: result.skipped.len(),
            failed: result.failed.len(),
        })
        .await;

        Ok(result)
    }

    /// Register a provider from YAML definition
    async fn register_provider_from_definition(
        &self,
        def: &ProviderDefinition,
        _bootstrap_config: &ProviderBootstrapConfig,
    ) -> Result<ProviderConfig> {
        // Check if provider already exists
        if self.repository.exists_by_name(&def.name).await? {
            self.emit_event(BootstrapEvent::ProviderSkipped {
                name: def.name.clone(),
            })
            .await;
            return Err(DomainError::InvalidProviderConfig {
                message: format!("Provider '{}' already exists", def.name),
            });
        }

        // Parse provider type
        let provider_type = self.parse_provider_type(&def.provider_type)?;

        // Create type-specific configuration
        let type_config = self.create_type_config(&provider_type, &def.config)?;

        // Create ProviderConfig
        let mut provider = ProviderConfig::new(def.name.clone(), provider_type, type_config)
            .with_priority(def.priority)
            .with_max_workers(def.max_workers);

        // Add tags
        for tag in &def.tags {
            provider = provider.with_tag(tag);
        }

        // Add metadata
        provider.metadata = def.metadata.clone();

        // Save to repository
        self.repository.save(&provider).await?;

        self.emit_event(BootstrapEvent::ProviderRegistered {
            name: def.name.clone(),
            provider_id: provider.id.clone(),
        })
        .await;

        Ok(provider)
    }

    /// Parse provider type from string
    fn parse_provider_type(&self, type_str: &str) -> Result<ProviderType> {
        match type_str.to_lowercase().as_str() {
            "docker" => Ok(ProviderType::Docker),
            "kubernetes" | "k8s" => Ok(ProviderType::Kubernetes),
            "lambda" | "aws_lambda" => Ok(ProviderType::Lambda),
            "fargate" | "aws_fargate" => Ok(ProviderType::Fargate),
            "cloud_run" | "cloudrun" => Ok(ProviderType::CloudRun),
            "container_apps" | "azure_container_apps" => Ok(ProviderType::ContainerApps),
            "cloud_functions" | "gcp_functions" => Ok(ProviderType::CloudFunctions),
            "azure_functions" => Ok(ProviderType::AzureFunctions),
            "ec2" | "aws_ec2" => Ok(ProviderType::EC2),
            "compute_engine" | "gce" => Ok(ProviderType::ComputeEngine),
            "azure_vms" | "azure_vm" => Ok(ProviderType::AzureVMs),
            "bare_metal" | "baremetal" => Ok(ProviderType::BareMetal),
            other => Ok(ProviderType::Custom(other.to_string())),
        }
    }

    /// Create type-specific configuration
    fn create_type_config(
        &self,
        provider_type: &ProviderType,
        config_map: &HashMap<String, serde_yaml::Value>,
    ) -> Result<ProviderTypeConfig> {
        match provider_type {
            ProviderType::Docker => {
                let mut dc = DockerConfig::default();
                if let Some(v) = config_map.get("socket_path") {
                    dc.socket_path = v.as_str().unwrap_or(&dc.socket_path).to_string();
                }
                if let Some(v) = config_map.get("default_image") {
                    dc.default_image = v.as_str().unwrap_or(&dc.default_image).to_string();
                }
                if let Some(v) = config_map.get("network") {
                    dc.network = Some(v.as_str().unwrap_or("bridge").to_string());
                }
                Ok(ProviderTypeConfig::Docker(dc))
            }
            ProviderType::Kubernetes => {
                let mut kc = KubernetesConfig::default();
                if let Some(v) = config_map.get("kubeconfig_path") {
                    kc.kubeconfig_path = Some(v.as_str().unwrap_or("").to_string());
                }
                if let Some(v) = config_map.get("namespace") {
                    kc.namespace = v.as_str().unwrap_or(&kc.namespace).to_string();
                }
                if let Some(v) = config_map.get("service_account") {
                    kc.service_account = v.as_str().unwrap_or(&kc.service_account).to_string();
                }
                if let Some(v) = config_map.get("default_image") {
                    kc.default_image = v.as_str().unwrap_or(&kc.default_image).to_string();
                }
                Ok(ProviderTypeConfig::Kubernetes(kc))
            }
            ProviderType::Lambda => {
                let mut lc = LambdaConfig::default();
                if let Some(v) = config_map.get("region") {
                    lc.region = v.as_str().unwrap_or(&lc.region).to_string();
                }
                if let Some(v) = config_map.get("role_arn") {
                    lc.role_arn = v.as_str().unwrap_or(&lc.role_arn).to_string();
                }
                if let Some(v) = config_map.get("runtime") {
                    lc.runtime = v.as_str().unwrap_or(&lc.runtime).to_string();
                }
                if let Some(v) = config_map.get("handler") {
                    lc.handler = v.as_str().unwrap_or(&lc.handler).to_string();
                }
                Ok(ProviderTypeConfig::Lambda(lc))
            }
            ProviderType::Fargate => {
                let fc = FargateConfig {
                    region: config_map
                        .get("region")
                        .and_then(|v| v.as_str())
                        .unwrap_or("us-east-1")
                        .to_string(),
                    cluster_arn: config_map
                        .get("cluster_arn")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    subnets: vec![],
                    security_groups: vec![],
                    execution_role_arn: config_map
                        .get("execution_role_arn")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    task_role_arn: None,
                    default_image: config_map
                        .get("default_image")
                        .and_then(|v| v.as_str())
                        .unwrap_or("hodei-jobs-worker:latest")
                        .to_string(),
                };
                Ok(ProviderTypeConfig::Fargate(fc))
            }
            // For other types, use Docker as fallback
            _ => Ok(ProviderTypeConfig::Docker(DockerConfig::default())),
        }
    }
}

/// Bootstrap result
#[derive(Debug, Clone, Default)]
pub struct BootstrapResult {
    /// Providers registered successfully
    pub registered: Vec<String>,
    /// Providers that already existed (skipped)
    pub skipped: Vec<String>,
    /// Providers that failed (name, error)
    pub failed: Vec<(String, String)>,
}

impl BootstrapResult {
    /// Returns a summary string
    pub fn summary(&self) -> String {
        format!(
            "Bootstrap complete: {} registered, {} skipped, {} failed",
            self.registered.len(),
            self.skipped.len(),
            self.failed.len()
        )
    }

    /// Returns true if all providers were registered successfully
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderStatus;
    use tokio::sync::RwLock;

    struct MockProviderConfigRepository {
        configs: RwLock<HashMap<String, ProviderConfig>>,
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
        async fn save(&self, config: &ProviderConfig) -> Result<()> {
            self.configs
                .write()
                .await
                .insert(config.name.clone(), config.clone());
            Ok(())
        }
        async fn find_by_id(
            &self,
            _id: &hodei_server_domain::shared_kernel::ProviderId,
        ) -> Result<Option<ProviderConfig>> {
            Ok(None)
        }
        async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
            Ok(self.configs.read().await.get(name).cloned())
        }
        async fn find_by_type(&self, _pt: &ProviderType) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .filter(|c| c.status == ProviderStatus::Active)
                .cloned()
                .collect())
        }
        async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values().cloned().collect())
        }
        async fn update(&self, config: &ProviderConfig) -> Result<()> {
            self.configs
                .write()
                .await
                .insert(config.name.clone(), config.clone());
            Ok(())
        }
        async fn delete(&self, _id: &hodei_server_domain::shared_kernel::ProviderId) -> Result<()> {
            Ok(())
        }
        async fn exists_by_name(&self, name: &str) -> Result<bool> {
            Ok(self.configs.read().await.contains_key(name))
        }
    }

    #[tokio::test]
    async fn test_bootstrap_from_yaml() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let bootstrap = ProviderBootstrap::new(repo.clone());

        let yaml = r#"
providers:
  - name: docker-local
    type: docker
    priority: 10
    max_workers: 5
    tags:
      - local
      - development
    config:
      socket_path: /var/run/docker.sock
      default_image: hodei-worker:latest
      network: bridge

  - name: k8s-production
    type: kubernetes
    priority: 100
    max_workers: 50
    tags:
      - production
      - cloud
    config:
      namespace: hodei-prod
      service_account: hodei-worker
"#;

        let result = bootstrap.load_from_yaml(yaml).await.unwrap();

        assert_eq!(result.registered.len(), 2);
        assert!(result.registered.contains(&"docker-local".to_string()));
        assert!(result.registered.contains(&"k8s-production".to_string()));
        assert!(result.skipped.is_empty());
        assert!(result.failed.is_empty());

        // Verificar que se guardaron
        let all = repo.find_all().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_bootstrap_skips_existing() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let bootstrap = ProviderBootstrap::new(repo.clone());

        let yaml = r#"
providers:
  - name: existing-provider
    type: docker
"#;

        // Primera carga
        let result1 = bootstrap.load_from_yaml(yaml).await.unwrap();
        assert_eq!(result1.registered.len(), 1);

        // Segunda carga - debe omitir
        let result2 = bootstrap.load_from_yaml(yaml).await.unwrap();
        assert_eq!(result2.registered.len(), 0);
        assert_eq!(result2.skipped.len(), 1);
    }

    #[tokio::test]
    async fn test_parse_provider_types() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let bootstrap = ProviderBootstrap::new(repo);

        assert_eq!(
            bootstrap.parse_provider_type("docker").unwrap(),
            ProviderType::Docker
        );
        assert_eq!(
            bootstrap.parse_provider_type("kubernetes").unwrap(),
            ProviderType::Kubernetes
        );
        assert_eq!(
            bootstrap.parse_provider_type("k8s").unwrap(),
            ProviderType::Kubernetes
        );
        assert_eq!(
            bootstrap.parse_provider_type("lambda").unwrap(),
            ProviderType::Lambda
        );
        assert_eq!(
            bootstrap.parse_provider_type("aws_lambda").unwrap(),
            ProviderType::Lambda
        );
        assert_eq!(
            bootstrap.parse_provider_type("custom_type").unwrap(),
            ProviderType::Custom("custom_type".to_string())
        );
    }
}

// Provider Bootstrap Service
// Carga providers desde configuración YAML al arranque

use hodei_server_domain::providers::{
    ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
    DockerConfig, KubernetesConfig, LambdaConfig, FargateConfig,
};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use hodei_server_domain::workers::ProviderType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Configuración de bootstrap de providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBootstrapConfig {
    /// Lista de providers a registrar
    pub providers: Vec<ProviderDefinition>,
}

/// Definición de un provider en YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefinition {
    /// Nombre único del provider
    pub name: String,
    /// Tipo de provider
    #[serde(rename = "type")]
    pub provider_type: String,
    /// Prioridad (mayor = preferido)
    #[serde(default)]
    pub priority: i32,
    /// Máximo de workers concurrentes
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Configuración específica del tipo
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
}

fn default_max_workers() -> u32 {
    10
}

/// Servicio para bootstrap de providers
pub struct ProviderBootstrap {
    repository: Arc<dyn ProviderConfigRepository>,
}

impl ProviderBootstrap {
    pub fn new(repository: Arc<dyn ProviderConfigRepository>) -> Self {
        Self { repository }
    }

    /// Cargar providers desde archivo YAML
    pub async fn load_from_file(&self, path: &Path) -> Result<BootstrapResult> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to read bootstrap file: {}", e),
            })?;

        self.load_from_yaml(&content).await
    }

    /// Cargar providers desde string YAML
    pub async fn load_from_yaml(&self, yaml_content: &str) -> Result<BootstrapResult> {
        let config: ProviderBootstrapConfig = serde_yaml::from_str(yaml_content)
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to parse bootstrap YAML: {}", e),
            })?;

        self.bootstrap_providers(&config).await
    }

    /// Ejecutar bootstrap de providers
    pub async fn bootstrap_providers(&self, config: &ProviderBootstrapConfig) -> Result<BootstrapResult> {
        let mut result = BootstrapResult::default();

        for definition in &config.providers {
            match self.register_provider_from_definition(definition).await {
                Ok(_) => {
                    result.registered.push(definition.name.clone());
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

        Ok(result)
    }

    /// Registrar provider desde definición YAML
    async fn register_provider_from_definition(&self, def: &ProviderDefinition) -> Result<ProviderConfig> {
        // Verificar si ya existe
        if self.repository.exists_by_name(&def.name).await? {
            return Err(DomainError::InvalidProviderConfig {
                message: format!("Provider '{}' already exists", def.name),
            });
        }

        // Parsear tipo de provider
        let provider_type = self.parse_provider_type(&def.provider_type)?;

        // Crear configuración específica del tipo
        let type_config = self.create_type_config(&provider_type, &def.config)?;

        // Crear ProviderConfig
        let mut config = ProviderConfig::new(def.name.clone(), provider_type, type_config)
            .with_priority(def.priority)
            .with_max_workers(def.max_workers);

        // Añadir tags
        for tag in &def.tags {
            config = config.with_tag(tag);
        }

        // Añadir metadata
        config.metadata = def.metadata.clone();

        // Guardar en repositorio
        self.repository.save(&config).await?;

        Ok(config)
    }

    /// Parsear tipo de provider desde string
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

    /// Crear configuración específica del tipo
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
                    region: config_map.get("region")
                        .and_then(|v| v.as_str())
                        .unwrap_or("us-east-1")
                        .to_string(),
                    cluster_arn: config_map.get("cluster_arn")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    subnets: vec![],
                    security_groups: vec![],
                    execution_role_arn: config_map.get("execution_role_arn")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    task_role_arn: None,
                    default_image: config_map.get("default_image")
                        .and_then(|v| v.as_str())
                        .unwrap_or("hodei-jobs-worker:latest")
                        .to_string(),
                };
                Ok(ProviderTypeConfig::Fargate(fc))
            }
            // Para otros tipos, usar Docker como fallback
            _ => Ok(ProviderTypeConfig::Docker(DockerConfig::default())),
        }
    }
}

/// Resultado del bootstrap
#[derive(Debug, Clone, Default)]
pub struct BootstrapResult {
    /// Providers registrados exitosamente
    pub registered: Vec<String>,
    /// Providers que ya existían (omitidos)
    pub skipped: Vec<String>,
    /// Providers que fallaron (nombre, error)
    pub failed: Vec<(String, String)>,
}

impl BootstrapResult {
    pub fn summary(&self) -> String {
        format!(
            "Bootstrap complete: {} registered, {} skipped, {} failed",
            self.registered.len(),
            self.skipped.len(),
            self.failed.len()
        )
    }

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
            self.configs.write().await.insert(config.name.clone(), config.clone());
            Ok(())
        }
        async fn find_by_id(&self, _id: &hodei_server_domain::shared_kernel::ProviderId) -> Result<Option<ProviderConfig>> {
            Ok(None)
        }
        async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
            Ok(self.configs.read().await.get(name).cloned())
        }
        async fn find_by_type(&self, _pt: &ProviderType) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values()
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
            self.configs.write().await.insert(config.name.clone(), config.clone());
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

        assert_eq!(bootstrap.parse_provider_type("docker").unwrap(), ProviderType::Docker);
        assert_eq!(bootstrap.parse_provider_type("kubernetes").unwrap(), ProviderType::Kubernetes);
        assert_eq!(bootstrap.parse_provider_type("k8s").unwrap(), ProviderType::Kubernetes);
        assert_eq!(bootstrap.parse_provider_type("lambda").unwrap(), ProviderType::Lambda);
        assert_eq!(bootstrap.parse_provider_type("aws_lambda").unwrap(), ProviderType::Lambda);
        assert_eq!(bootstrap.parse_provider_type("custom_type").unwrap(), ProviderType::Custom("custom_type".to_string()));
    }
}

// Provider Configuration - Configuración persistida de providers

use crate::shared_kernel::{ProviderId, ProviderStatus};
use crate::workers::{ProviderCapabilities, ProviderType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuración de un WorkerProvider registrado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// ID único del provider
    pub id: ProviderId,
    /// Nombre descriptivo
    pub name: String,
    /// Tipo de provider
    pub provider_type: ProviderType,
    /// Estado del provider (enabled/disabled)
    pub status: ProviderStatus,
    /// Capacidades anunciadas
    pub capabilities: ProviderCapabilities,
    /// Configuración específica del tipo
    pub type_config: ProviderTypeConfig,
    /// Prioridad para selección (mayor = preferido)
    pub priority: i32,
    /// Límite máximo de workers concurrentes
    pub max_workers: u32,
    /// Workers actualmente activos
    pub active_workers: u32,
    /// Fecha de registro
    pub created_at: DateTime<Utc>,
    /// Última actualización
    pub updated_at: DateTime<Utc>,
    /// Tags para filtrado
    pub tags: Vec<String>,
    /// Metadata adicional
    pub metadata: HashMap<String, String>,
}

impl ProviderConfig {
    pub fn new(name: String, provider_type: ProviderType, type_config: ProviderTypeConfig) -> Self {
        let now = Utc::now();
        Self {
            id: ProviderId::new(),
            name,
            provider_type,
            status: ProviderStatus::Active,
            capabilities: ProviderCapabilities::default(),
            type_config,
            priority: 0,
            max_workers: 10,
            active_workers: 0,
            created_at: now,
            updated_at: now,
            tags: vec![],
            metadata: HashMap::new(),
        }
    }

    /// Creates a new ProviderConfig with a specific ID
    pub fn with_id(
        id: ProviderId,
        name: String,
        provider_type: ProviderType,
        type_config: ProviderTypeConfig,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            provider_type,
            status: ProviderStatus::Active,
            capabilities: ProviderCapabilities::default(),
            type_config,
            priority: 0,
            max_workers: 10,
            active_workers: 0,
            created_at: now,
            updated_at: now,
            tags: vec![],
            metadata: HashMap::new(),
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_max_workers(mut self, max_workers: u32) -> Self {
        self.max_workers = max_workers;
        self
    }

    pub fn with_capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self.status, ProviderStatus::Active)
    }

    pub fn has_capacity(&self) -> bool {
        self.active_workers < self.max_workers
    }

    pub fn increment_workers(&mut self) {
        self.active_workers += 1;
        self.updated_at = Utc::now();
    }

    pub fn decrement_workers(&mut self) {
        if self.active_workers > 0 {
            self.active_workers -= 1;
        }
        self.updated_at = Utc::now();
    }
}

/// Configuración específica por tipo de provider
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderTypeConfig {
    // === CONTAINERS ===
    Docker(DockerConfig),
    Kubernetes(KubernetesConfig),
    Fargate(FargateConfig),
    CloudRun(CloudRunConfig),
    ContainerApps(ContainerAppsConfig),

    // === SERVERLESS ===
    Lambda(LambdaConfig),
    CloudFunctions(CloudFunctionsConfig),
    AzureFunctions(AzureFunctionsConfig),

    // === VIRTUAL MACHINES ===
    EC2(EC2Config),
    ComputeEngine(ComputeEngineConfig),
    AzureVMs(AzureVMsConfig),

    // === OTHER ===
    BareMetal(BareMetalConfig),
    Custom(CustomConfig),
}

// === CONTAINER CONFIGS ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    /// Path al socket de Docker
    pub socket_path: String,
    /// Imagen por defecto del worker
    pub default_image: String,
    /// Red de Docker a usar
    pub network: Option<String>,
    /// Autenticación de registry
    pub registry_auth: Option<RegistryAuth>,
    /// Configuración TLS
    pub tls: Option<TlsConfig>,
    /// Límites de recursos por defecto
    pub default_resources: Option<ContainerResources>,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            socket_path: "/var/run/docker.sock".to_string(),
            default_image: "hodei-jobs-worker:v3".to_string(),
            network: None,
            registry_auth: None,
            tls: None,
            default_resources: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Path a kubeconfig (None = in-cluster)
    pub kubeconfig_path: Option<String>,
    /// Namespace donde crear pods
    pub namespace: String,
    /// Service account para los pods
    pub service_account: String,
    /// Imagen por defecto
    pub default_image: String,
    /// Image pull secrets
    pub image_pull_secrets: Vec<String>,
    /// Node selector
    pub node_selector: HashMap<String, String>,
    /// Tolerations
    pub tolerations: Vec<Toleration>,
    /// Límites de recursos por defecto
    pub default_resources: Option<ContainerResources>,
}

impl Default for KubernetesConfig {
    fn default() -> Self {
        Self {
            kubeconfig_path: None,
            namespace: "hodei-jobs".to_string(),
            service_account: "hodei-jobs-worker".to_string(),
            default_image: "hodei-jobs-worker:latest".to_string(),
            image_pull_secrets: vec![],
            node_selector: HashMap::new(),
            tolerations: vec![],
            default_resources: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FargateConfig {
    pub region: String,
    pub cluster_arn: String,
    pub subnets: Vec<String>,
    pub security_groups: Vec<String>,
    pub execution_role_arn: String,
    pub task_role_arn: Option<String>,
    pub default_image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudRunConfig {
    pub project_id: String,
    pub region: String,
    pub service_account: Option<String>,
    pub vpc_connector: Option<String>,
    pub default_image: String,
    pub max_instances: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerAppsConfig {
    pub subscription_id: String,
    pub resource_group: String,
    pub environment_name: String,
    pub default_image: String,
}

// === SERVERLESS CONFIGS ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaConfig {
    pub region: String,
    pub role_arn: String,
    pub runtime: String,
    pub handler: String,
    pub vpc_config: Option<VpcConfig>,
    pub layers: Vec<String>,
    pub timeout_seconds: u32,
    pub memory_mb: u32,
}

impl Default for LambdaConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            role_arn: String::new(),
            runtime: "provided.al2".to_string(),
            handler: "bootstrap".to_string(),
            vpc_config: None,
            layers: vec![],
            timeout_seconds: 900,
            memory_mb: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFunctionsConfig {
    pub project_id: String,
    pub region: String,
    pub runtime: String,
    pub service_account: Option<String>,
    pub vpc_connector: Option<String>,
    pub timeout_seconds: u32,
    pub memory_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureFunctionsConfig {
    pub subscription_id: String,
    pub resource_group: String,
    pub function_app_name: String,
    pub runtime: String,
}

// === VM CONFIGS ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EC2Config {
    pub region: String,
    pub ami_id: String,
    pub instance_type: String,
    pub key_name: Option<String>,
    pub security_group_ids: Vec<String>,
    pub subnet_id: Option<String>,
    pub iam_instance_profile: Option<String>,
    pub user_data_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeEngineConfig {
    pub project_id: String,
    pub zone: String,
    pub machine_type: String,
    pub image_family: String,
    pub image_project: String,
    pub network: String,
    pub service_account: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureVMsConfig {
    pub subscription_id: String,
    pub resource_group: String,
    pub location: String,
    pub vm_size: String,
    pub image_reference: ImageReference,
    pub admin_username: String,
    pub ssh_public_key: String,
}

// === OTHER CONFIGS ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BareMetalConfig {
    pub hosts: Vec<BareMetalHost>,
    pub ssh_user: String,
    pub ssh_key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomConfig {
    pub plugin_path: String,
    pub config: HashMap<String, serde_json::Value>,
}

// === SHARED TYPES ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuth {
    pub username: String,
    pub password: String,
    pub server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub ca_cert_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResources {
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toleration {
    pub key: String,
    pub operator: String,
    pub value: Option<String>,
    pub effect: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpcConfig {
    pub subnet_ids: Vec<String>,
    pub security_group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageReference {
    pub publisher: String,
    pub offer: String,
    pub sku: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BareMetalHost {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub labels: HashMap<String, String>,
}

// ============================================================================
// Repository Trait
// ============================================================================

use crate::shared_kernel::Result;

/// Repositorio para persistir y recuperar configuraciones de providers
#[async_trait::async_trait]
pub trait ProviderConfigRepository: Send + Sync {
    /// Guardar una configuración de provider
    async fn save(&self, config: &ProviderConfig) -> Result<()>;

    /// Buscar por ID
    async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>>;

    /// Buscar por nombre
    async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>>;

    /// Buscar por tipo
    async fn find_by_type(&self, provider_type: &ProviderType) -> Result<Vec<ProviderConfig>>;

    /// Obtener todos los providers habilitados
    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>>;

    /// Obtener todos los providers con capacidad disponible
    async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>>;

    /// Obtener todos los providers
    async fn find_all(&self) -> Result<Vec<ProviderConfig>>;

    /// Actualizar configuración
    async fn update(&self, config: &ProviderConfig) -> Result<()>;

    /// Eliminar configuración
    async fn delete(&self, id: &ProviderId) -> Result<()>;

    /// Verificar si existe por nombre
    async fn exists_by_name(&self, name: &str) -> Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_creation() {
        let config = ProviderConfig::new(
            "docker-local".to_string(),
            ProviderType::Docker,
            ProviderTypeConfig::Docker(DockerConfig::default()),
        );

        assert_eq!(config.name, "docker-local");
        assert!(config.is_enabled());
        assert!(config.has_capacity());
    }

    #[test]
    fn test_provider_config_builder() {
        let config = ProviderConfig::new(
            "test".to_string(),
            ProviderType::Docker,
            ProviderTypeConfig::Docker(DockerConfig::default()),
        )
        .with_priority(10)
        .with_max_workers(5)
        .with_tag("production");

        assert_eq!(config.priority, 10);
        assert_eq!(config.max_workers, 5);
        assert!(config.tags.contains(&"production".to_string()));
    }

    #[test]
    fn test_provider_capacity_tracking() {
        let mut config = ProviderConfig::new(
            "test".to_string(),
            ProviderType::Docker,
            ProviderTypeConfig::Docker(DockerConfig::default()),
        )
        .with_max_workers(2);

        assert!(config.has_capacity());
        assert_eq!(config.active_workers, 0);

        config.increment_workers();
        assert_eq!(config.active_workers, 1);
        assert!(config.has_capacity());

        config.increment_workers();
        assert_eq!(config.active_workers, 2);
        assert!(!config.has_capacity());

        config.decrement_workers();
        assert_eq!(config.active_workers, 1);
        assert!(config.has_capacity());
    }

    #[test]
    fn test_docker_config_default() {
        let config = DockerConfig::default();
        assert_eq!(config.socket_path, "/var/run/docker.sock");
        assert_eq!(config.default_image, "hodei-jobs-worker:v3");
    }

    #[test]
    fn test_kubernetes_config_default() {
        let config = KubernetesConfig::default();
        assert!(config.kubeconfig_path.is_none());
        assert_eq!(config.namespace, "hodei-jobs");
    }

    #[test]
    fn test_provider_type_config_serialization() {
        let config = ProviderTypeConfig::Docker(DockerConfig::default());
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("docker"));

        let deserialized: ProviderTypeConfig = serde_json::from_str(&json).unwrap();
        if let ProviderTypeConfig::Docker(dc) = deserialized {
            assert_eq!(dc.socket_path, "/var/run/docker.sock");
        } else {
            panic!("Wrong variant");
        }
    }
}

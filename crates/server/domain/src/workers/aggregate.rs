// Worker Domain - Entidades para workers on-demand

use crate::events::TerminationReason;
use crate::shared_kernel::{
    Aggregate, DomainError, JobId, ProviderId, Result, WorkerId, WorkerState,
};
use crate::workers::ProviderConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

/// Tipos de provider soportados
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderType {
    // Containers
    Docker,
    Kubernetes,
    Fargate,
    CloudRun,
    ContainerApps,
    // Serverless
    Lambda,
    CloudFunctions,
    AzureFunctions,
    // Virtual Machines
    EC2,
    ComputeEngine,
    AzureVMs,
    // Testing
    Test,
    // Other
    BareMetal,
    Custom(String),
}

impl ProviderType {
    pub fn category(&self) -> ProviderCategory {
        match self {
            Self::Docker
            | Self::Kubernetes
            | Self::Fargate
            | Self::CloudRun
            | Self::ContainerApps => ProviderCategory::Container,

            Self::Lambda | Self::CloudFunctions | Self::AzureFunctions => {
                ProviderCategory::Serverless
            }

            Self::EC2 | Self::ComputeEngine | Self::AzureVMs => ProviderCategory::VirtualMachine,

            Self::Test | Self::BareMetal | Self::Custom(_) => ProviderCategory::BareMetal,
        }
    }

    /// Get the normalized name as a static string reference
    ///
    /// This avoids allocation by returning a compile-time string reference.
    /// For Custom types, returns "custom" as the base type name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Kubernetes => "kubernetes",
            Self::Fargate => "fargate",
            Self::CloudRun => "cloudrun",
            Self::ContainerApps => "containerapps",
            Self::Lambda => "lambda",
            Self::CloudFunctions => "cloudfunctions",
            Self::AzureFunctions => "azurefunctions",
            Self::EC2 => "ec2",
            Self::ComputeEngine => "computeengine",
            Self::AzureVMs => "azurevms",
            Self::Test => "test",
            Self::BareMetal => "baremetal",
            Self::Custom(_) => "custom",
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Docker => write!(f, "docker"),
            Self::Kubernetes => write!(f, "kubernetes"),
            Self::Fargate => write!(f, "fargate"),
            Self::CloudRun => write!(f, "cloudrun"),
            Self::ContainerApps => write!(f, "containerapps"),
            Self::Lambda => write!(f, "lambda"),
            Self::CloudFunctions => write!(f, "cloudfunctions"),
            Self::AzureFunctions => write!(f, "azurefunctions"),
            Self::EC2 => write!(f, "ec2"),
            Self::ComputeEngine => write!(f, "computeengine"),
            Self::AzureVMs => write!(f, "azurevms"),
            Self::Test => write!(f, "test"),
            Self::BareMetal => write!(f, "baremetal"),
            Self::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

impl std::str::FromStr for ProviderType {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "docker" => Ok(Self::Docker),
            "kubernetes" | "k8s" => Ok(Self::Kubernetes),
            "fargate" => Ok(Self::Fargate),
            "cloudrun" => Ok(Self::CloudRun),
            "containerapps" => Ok(Self::ContainerApps),
            "lambda" => Ok(Self::Lambda),
            "cloudfunctions" => Ok(Self::CloudFunctions),
            "azurefunctions" => Ok(Self::AzureFunctions),
            "ec2" => Ok(Self::EC2),
            "computeengine" => Ok(Self::ComputeEngine),
            "azurevms" => Ok(Self::AzureVMs),
            "test" => Ok(Self::Test),
            "baremetal" | "bare_metal" => Ok(Self::BareMetal),
            custom if custom.starts_with("custom:") => Ok(Self::Custom(
                custom.trim_start_matches("custom:").to_string(),
            )),
            "custom" => Ok(Self::Custom("unknown".to_string())),
            _ => Err(()),
        }
    }
}

/// Custom serializer for ProviderType that uses the Display implementation
/// This ensures ProviderType serializes as a string (e.g., "kubernetes")
/// instead of an object (e.g., {"Kubernetes": null})
impl Serialize for ProviderType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Custom deserializer for ProviderType that parses strings like "kubernetes"
impl<'de> Deserialize<'de> for ProviderType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ProviderTypeVisitor;

        impl<'de> serde::de::Visitor<'de> for ProviderTypeVisitor {
            type Value = ProviderType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a provider type string (e.g., 'docker', 'kubernetes')")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                ProviderType::from_str(value).map_err(|_| {
                    serde::de::Error::unknown_variant(
                        value,
                        &[
                            "docker",
                            "kubernetes",
                            "fargate",
                            "cloudrun",
                            "containerapps",
                            "lambda",
                            "cloudfunctions",
                            "azurefunctions",
                            "ec2",
                            "computeengine",
                            "azurevms",
                            "test",
                            "baremetal",
                            "custom",
                        ],
                    )
                })
            }
        }

        deserializer.deserialize_str(ProviderTypeVisitor)
    }
}

/// Categorías de providers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCategory {
    Container,
    Serverless,
    VirtualMachine,
    BareMetal,
}

/// Arquitectura del worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Architecture {
    #[default]
    Amd64,
    Arm64,
    Arm,
}

/// Recursos requeridos para un worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// CPU cores (puede ser fraccionario: 0.5, 1.0, 2.0)
    pub cpu_cores: f64,
    /// Memoria en bytes
    pub memory_bytes: i64,
    /// Almacenamiento en bytes
    pub disk_bytes: i64,
    /// GPUs requeridas
    pub gpu_count: u32,
    /// Tipo de GPU requerido (opcional)
    pub gpu_type: Option<String>,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu_cores: 1.0,
            memory_bytes: 512 * 1024 * 1024, // 512MB
            disk_bytes: 1024 * 1024 * 1024,  // 1GB
            gpu_count: 0,
            gpu_type: None,
        }
    }
}

/// Tipos de volumen soportados
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeSpec {
    /// Persistent Volume Claim
    Persistent {
        name: String,
        claim_name: String,
        read_only: bool,
    },
    /// Ephemeral volume (emptyDir)
    Ephemeral {
        name: String,
        size_limit: Option<i64>,
    },
    /// Host path volume
    HostPath {
        name: String,
        path: String,
        read_only: bool,
    },
}

/// Especificación para crear un worker on-demand
///
/// Esta estructura es agnóstica de la infraestructura. Para configuración
/// específica del provider, use el campo `provider_config` con el patrón
/// Extension Objects (EPIC-23).
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::workers::{WorkerSpec, ProviderConfig, KubernetesConfigExt};
///
/// let k8s_config = KubernetesConfigExt::default();
/// let provider_config = ProviderConfig::Kubernetes(k8s_config);
///
/// let spec = WorkerSpec::new("image:latest", "server:50051")
///     .with_provider_config(provider_config);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpec {
    /// ID único para el worker
    pub worker_id: WorkerId,
    /// Imagen base del worker (contiene el Agent)
    pub image: String,
    /// Recursos requeridos
    pub resources: ResourceRequirements,
    /// Labels para el worker
    pub labels: HashMap<String, String>,
    /// Annotations para el worker (EPIC-21 US-07)
    pub annotations: HashMap<String, String>,
    /// Variables de entorno
    pub environment: HashMap<String, String>,
    /// Volúmenes a montar
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,
    /// Dirección del servidor Hodei para que el agent se conecte
    pub server_address: String,
    /// Timeout máximo de vida del worker
    pub max_lifetime: Duration,
    /// Timeout de idle (sin jobs)
    pub idle_timeout: Duration,
    /// Tiempo de gracia tras completar job antes de terminate (EPIC-26 US-26.7)
    /// Si es None, el worker se destruye inmediatamente tras completar job
    /// Si es Some(duration), el worker espera ese tiempo antes de destruirse
    #[serde(default)]
    pub ttl_after_completion: Option<Duration>,
    /// Arquitectura requerida
    pub architecture: Architecture,
    /// Capabilities requeridas
    pub required_capabilities: Vec<String>,
    /// Provider-specific configuration using Extension Objects pattern
    ///
    /// Este campo reemplaza el uso de campos específicos de providers
    /// (como `kubernetes`) y permite que `WorkerSpec` permanezca agnóstico
    /// de la infraestructura.
    #[serde(default)]
    pub provider_config: Option<ProviderConfig>,
    /// @deprecated Use `provider_config` instead. This field will be removed in a future version.
    #[serde(default)]
    pub kubernetes: KubernetesWorkerConfig,
}

/// Kubernetes-specific worker configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubernetesWorkerConfig {
    /// Custom annotations for the Pod
    pub annotations: HashMap<String, String>,
    /// Custom labels for the Pod (extends base labels)
    pub custom_labels: HashMap<String, String>,
    /// Node selector for Pod scheduling
    pub node_selector: HashMap<String, String>,
    /// Service account to use for the Pod
    pub service_account: Option<String>,
    /// Init containers to run before the main container
    pub init_containers: Vec<KubernetesContainer>,
    /// Sidecar containers to run alongside the main container
    pub sidecar_containers: Vec<KubernetesContainer>,
    /// Affinity rules for pod scheduling
    pub affinity: Option<KubernetesAffinity>,
    /// Tolerations for pod scheduling
    pub tolerations: Vec<KubernetesToleration>,
    /// DNS policy for the Pod
    pub dns_policy: Option<String>,
    /// DNS config for the Pod
    pub dns_config: Option<KubernetesDNSConfig>,
    /// Host aliases to add to the Pod's hosts file
    pub host_aliases: Vec<KubernetesHostAlias>,
    /// Security context for the Pod
    pub security_context: Option<KubernetesSecurityContext>,
}

/// Container specification for Kubernetes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesContainer {
    /// Container name
    pub name: String,
    /// Container image
    pub image: String,
    /// Container image pull policy
    pub image_pull_policy: Option<String>,
    /// Commands to run
    pub command: Vec<String>,
    /// Arguments to pass
    pub args: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Environment variables from sources
    pub env_from: Vec<KubernetesEnvFrom>,
    /// Ports to expose
    pub ports: Vec<KubernetesContainerPort>,
    /// Volume mounts
    pub volume_mounts: Vec<KubernetesVolumeMount>,
    /// Resource requirements
    pub resources: Option<ResourceRequirements>,
    /// Security context for the container
    pub security_context: Option<KubernetesContainerSecurityContext>,
}

/// Environment variable source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesEnvFrom {
    pub config_map_ref: Option<KubernetesConfigMapRef>,
    pub secret_ref: Option<KubernetesSecretRef>,
    pub prefix: Option<String>,
}

/// ConfigMap reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfigMapRef {
    pub name: String,
    pub optional: Option<bool>,
}

/// Secret reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesSecretRef {
    pub name: String,
    pub optional: Option<bool>,
}

/// Container port
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesContainerPort {
    pub name: Option<String>,
    pub container_port: i32,
    pub protocol: Option<String>,
    pub host_ip: Option<String>,
    pub host_port: Option<i32>,
}

/// Volume mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesVolumeMount {
    pub name: String,
    pub mount_path: String,
    pub sub_path: Option<String>,
    pub read_only: Option<bool>,
}

/// Affinity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesAffinity {
    pub node_affinity: Option<KubernetesNodeAffinity>,
    pub pod_affinity: Option<KubernetesPodAffinity>,
    pub pod_anti_affinity: Option<KubernetesPodAntiAffinity>,
}

/// Node affinity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesNodeAffinity {
    pub preferred_during_scheduling_ignored_during_execution:
        Vec<KubernetesPreferredSchedulingTerm>,
    pub required_during_scheduling_ignored_during_execution: Option<KubernetesNodeSelector>,
}

/// Preferred scheduling term
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesPreferredSchedulingTerm {
    pub weight: i32,
    pub preference: KubernetesNodeSelectorTerm,
}

/// Node selector term
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesNodeSelectorTerm {
    pub match_expressions: Vec<KubernetesNodeSelectorRequirement>,
}

/// Node selector requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesNodeSelectorRequirement {
    pub key: String,
    pub operator: String,
    pub values: Vec<String>,
}

/// Node selector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesNodeSelector {
    pub node_selector_terms: Vec<KubernetesNodeSelectorTerm>,
}

/// Pod affinity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesPodAffinity {
    pub required_during_scheduling_ignored_during_execution: Vec<KubernetesPodAffinityTerm>,
}

/// Pod anti-affinity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesPodAntiAffinity {
    pub required_during_scheduling_ignored_during_execution: Vec<KubernetesPodAffinityTerm>,
}

/// Pod affinity term
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesPodAffinityTerm {
    pub label_selector: Option<KubernetesLabelSelector>,
    pub namespace_selector: Option<KubernetesLabelSelector>,
    pub namespaces: Vec<String>,
    pub topology_key: String,
}

/// Label selector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesLabelSelector {
    pub match_labels: Option<HashMap<String, String>>,
    pub match_expressions: Vec<KubernetesLabelSelectorRequirement>,
}

/// Label selector requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesLabelSelectorRequirement {
    pub key: String,
    pub operator: String,
    pub values: Vec<String>,
}

/// Toleration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
    pub toleration_seconds: Option<i64>,
}

/// DNS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesDNSConfig {
    pub nameservers: Vec<String>,
    pub searches: Vec<String>,
    pub options: Vec<KubernetesDNSConfigOption>,
}

/// DNS config option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesDNSConfigOption {
    pub name: String,
    pub value: Option<String>,
}

/// Host alias
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesHostAlias {
    pub ip: String,
    pub hostnames: Vec<String>,
}

/// Security context for Pod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesSecurityContext {
    pub run_as_non_root: Option<bool>,
    pub run_as_user: Option<i64>,
    pub run_as_group: Option<i64>,
    pub fs_group: Option<i64>,
    pub seccomp_profile: Option<KubernetesSeccompProfile>,
}

/// Security context for Container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesContainerSecurityContext {
    pub run_as_non_root: Option<bool>,
    pub run_as_user: Option<i64>,
    pub run_as_group: Option<i64>,
    pub allow_privilege_escalation: Option<bool>,
    pub capabilities: Option<KubernetesCapabilities>,
    pub read_only_root_filesystem: Option<bool>,
    pub seccomp_profile: Option<KubernetesSeccompProfile>,
}

/// Seccomp profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesSeccompProfile {
    pub type_: String,
    pub localhost_profile: Option<String>,
}

/// Capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesCapabilities {
    pub add: Vec<String>,
    pub drop: Vec<String>,
}

impl WorkerSpec {
    pub fn new(image: String, server_address: String) -> Self {
        Self {
            worker_id: WorkerId::new(),
            image,
            resources: ResourceRequirements::default(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            environment: HashMap::new(),
            volumes: Vec::new(),
            server_address,
            max_lifetime: Duration::from_secs(3600), // 1 hora default
            idle_timeout: Duration::from_secs(300),  // 5 minutos default
            ttl_after_completion: None,              // EPIC-26 US-26.7: inmediato por defecto
            architecture: Architecture::default(),
            required_capabilities: vec![],
            provider_config: None,
            kubernetes: KubernetesWorkerConfig::default(),
        }
    }

    /// Set TTL after job completion (EPIC-26 US-26.7)
    ///
    /// El worker esperará este tiempo tras completar un job antes de destruirse.
    /// Si es None, el worker se destruye inmediatamente.
    pub fn with_ttl_after_completion(mut self, ttl: Duration) -> Self {
        self.ttl_after_completion = Some(ttl);
        self
    }

    /// Set provider-specific configuration using Extension Objects pattern
    ///
    /// Este método permite configurar aspectos específicos del provider
    /// sin contaminar `WorkerSpec` con campos de infraestructura.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use hodei_server_domain::workers::{ProviderConfig, KubernetesConfigExt, WorkerSpec};
    ///
    /// let k8s_config = KubernetesConfigExt::default();
    /// let provider_config = ProviderConfig::Kubernetes(k8s_config);
    ///
    /// let spec = WorkerSpec::new("image:latest", "server:50051")
    ///     .with_provider_config(provider_config);
    /// ```
    pub fn with_provider_config(mut self, config: ProviderConfig) -> Self {
        self.provider_config = Some(config);
        self
    }

    pub fn with_resources(mut self, resources: ResourceRequirements) -> Self {
        self.resources = resources;
        self
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    pub fn with_volume(mut self, volume: VolumeSpec) -> Self {
        self.volumes.push(volume);
        self
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.required_capabilities.push(capability.into());
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_annotation(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.kubernetes.annotations.insert(key.into(), value.into());
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_label(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.kubernetes
            .custom_labels
            .insert(key.into(), value.into());
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_node_selector(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.kubernetes
            .node_selector
            .insert(key.into(), value.into());
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_service_account(mut self, service_account: impl Into<String>) -> Self {
        self.kubernetes.service_account = Some(service_account.into());
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_init_container(mut self, container: KubernetesContainer) -> Self {
        self.kubernetes.init_containers.push(container);
        self
    }

    /// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
    pub fn with_kubernetes_sidecar_container(mut self, container: KubernetesContainer) -> Self {
        self.kubernetes.sidecar_containers.push(container);
        self
    }
}

/// Handle opaco al worker creado por el provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHandle {
    /// ID del worker
    pub worker_id: WorkerId,
    /// ID específico del provider (container_id, pod_name, instance_id, function_arn)
    pub provider_resource_id: String,
    /// Tipo de provider que creó el worker
    pub provider_type: ProviderType,
    /// ID del provider que creó el worker
    pub provider_id: ProviderId,
    /// Timestamp de creación
    pub created_at: DateTime<Utc>,
    /// Metadata adicional específica del provider
    pub metadata: HashMap<String, serde_json::Value>,
}

impl WorkerHandle {
    pub fn new(
        worker_id: WorkerId,
        provider_resource_id: String,
        provider_type: ProviderType,
        provider_id: ProviderId,
    ) -> Self {
        Self {
            worker_id,
            provider_resource_id,
            provider_type,
            provider_id,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Worker aggregate - representa un worker efímero en el sistema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    /// ID único del worker
    id: WorkerId,
    /// Handle al recurso del provider
    handle: WorkerHandle,
    /// Estado actual del worker
    state: WorkerState,
    /// Especificación con la que se creó
    spec: WorkerSpec,
    /// Job actualmente asignado (si hay)
    current_job_id: Option<JobId>,
    /// Jobs ejecutados por este worker
    jobs_executed: u32,
    /// Timestamp del último job completado (EPIC-26 US-26.7)
    job_completed_at: Option<DateTime<Utc>>,
    /// Última vez que se recibió heartbeat
    last_heartbeat: DateTime<Utc>,
    /// Fecha de creación
    created_at: DateTime<Utc>,
    /// Fecha de última actualización
    updated_at: DateTime<Utc>,
}

impl Worker {
    /// Crea un nuevo worker en estado Provisioning
    pub fn new(handle: WorkerHandle, spec: WorkerSpec) -> Self {
        let now = Utc::now();
        Self {
            id: handle.worker_id.clone(),
            handle,
            state: WorkerState::Creating,
            spec,
            current_job_id: None,
            jobs_executed: 0,
            job_completed_at: None, // EPIC-26 US-26.7
            last_heartbeat: now,
            created_at: now,
            updated_at: now,
        }
    }

    /// Restaura un worker desde los campos de la base de datos
    pub fn from_database(
        handle: WorkerHandle,
        spec: WorkerSpec,
        state: WorkerState,
        current_job_id: Option<JobId>,
        last_heartbeat: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        // Use current time if last_heartbeat is None (worker hasn't sent heartbeat yet)
        let heartbeat = last_heartbeat.unwrap_or_else(chrono::Utc::now);

        Self {
            id: handle.worker_id.clone(),
            handle,
            state,
            spec,
            current_job_id,
            jobs_executed: 0,       // Could be stored in DB if needed
            job_completed_at: None, // EPIC-26 US-26.7: nuevo campo, por defecto None
            last_heartbeat: heartbeat,
            created_at,
            updated_at,
        }
    }

    // === Getters ===

    pub fn id(&self) -> &WorkerId {
        &self.id
    }

    pub fn handle(&self) -> &WorkerHandle {
        &self.handle
    }

    pub fn state(&self) -> &WorkerState {
        &self.state
    }

    pub fn spec(&self) -> &WorkerSpec {
        &self.spec
    }

    pub fn current_job_id(&self) -> Option<&JobId> {
        self.current_job_id.as_ref()
    }

    pub fn jobs_executed(&self) -> u32 {
        self.jobs_executed
    }

    pub fn last_heartbeat(&self) -> DateTime<Utc> {
        self.last_heartbeat
    }

    /// Returns the last heartbeat as an Option (for compatibility)
    pub fn last_heartbeat_at(&self) -> Option<DateTime<Utc>> {
        Some(self.last_heartbeat)
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn provider_type(&self) -> &ProviderType {
        &self.handle.provider_type
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.handle.provider_id
    }

    // === State transitions ===

    /// Transición de estado centralizada y validada.
    ///
    /// Este método valida que la transición de estado sea válida según la máquina de estados
    /// del Worker y actualiza el timestamp de modificación.
    ///
    /// # Arguments
    /// * `new_state` - El nuevo estado al que se transiciona
    /// * `reason` - Razón de la transición (para logs/trazabilidad)
    ///
    /// # Returns
    /// Ok(()) si la transición es válida, Err(DomainError::InvalidWorkerStateTransition) si no lo es
    ///
    /// # State Machine
    /// - Creating → Connecting, Terminated
    /// - Connecting → Ready, Terminated
    /// - Ready → Busy, Draining, Terminating
    /// - Busy → Ready, Draining, Terminating, Terminated
    /// - Draining → Ready, Terminating, Terminated
    /// - Terminating → Terminated
    /// - Terminated → (sin transiciones salientes)
    fn transition_to(&mut self, new_state: WorkerState, _reason: &str) -> Result<()> {
        let current_state = self.state.clone();

        // Validar que la transición es permitida
        if !current_state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidWorkerStateTransition {
                current: current_state.to_string(),
                requested: new_state.to_string(),
            });
        }

        // Realizar la transición
        self.state = new_state;
        self.updated_at = Utc::now();

        Ok(())
    }

    /// Worker listo (agent registrado exitosamente)
    /// En Crash-Only Design, esto viene directamente de Creating
    /// Esta transición es idempotente: si ya está en Ready, no hace nada
    pub fn mark_ready(&mut self) -> Result<()> {
        // Idempotent: si ya está en Ready, no hacemos nada
        if self.state.is_ready() {
            return Ok(());
        }
        self.transition_to(WorkerState::Ready, "Agent registered successfully")?;
        self.last_heartbeat = self.updated_at;
        Ok(())
    }

    /// Asignar job al worker
    pub fn assign_job(&mut self, job_id: JobId) -> Result<()> {
        // Validar transición primero
        self.transition_to(WorkerState::Busy, "Job assigned to worker")?;

        // Actualizar campos específicos del job
        self.current_job_id = Some(job_id);

        Ok(())
    }

    /// Job completado - worker se termina (efímero)
    pub fn complete_job(&mut self) -> Result<()> {
        // Validación de dominio: solo se puede completar un job si hay uno asignado
        let job_id = self.current_job_id.clone().ok_or_else(|| {
            DomainError::InvalidWorkerStateTransition {
                current: self.state.to_string(),
                requested: WorkerState::Terminated.to_string(),
            }
        })?;

        // Validar transición y actualizar estado
        self.transition_to(
            WorkerState::Terminated,
            &format!("Job {} completed - ephemeral worker terminating", job_id),
        )?;

        // Actualizar campos específicos del completion
        self.current_job_id = None;
        self.jobs_executed += 1;
        // EPIC-26 US-26.7: Track completion time para TTL after completion
        self.job_completed_at = Some(self.updated_at);

        Ok(())
    }

    /// Marcar worker como terminado directamente
    /// En Crash-Only Design, no hay estados intermedios de draining/terminating
    pub fn mark_terminating(&mut self) -> Result<()> {
        // Si ya está terminado, es un no-op
        if self.state.is_terminated() {
            return Ok(());
        }

        self.transition_to(WorkerState::Terminated, "Worker terminated")
    }

    /// Worker falló - se marca como Terminated (PRD v6.0 no tiene Failed state)
    pub fn mark_failed(&mut self, _reason: String) -> Result<()> {
        self.transition_to(WorkerState::Terminated, "Worker failed")
    }

    /// Actualizar heartbeat
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
        self.updated_at = Utc::now();
    }

    /// Verificar si el worker ha excedido idle timeout
    pub fn is_idle_timeout(&self) -> bool {
        if !matches!(self.state, WorkerState::Ready) {
            return false;
        }

        let idle_duration = Utc::now()
            .signed_duration_since(self.last_heartbeat)
            .to_std()
            .unwrap_or(Duration::ZERO);

        idle_duration > self.spec.idle_timeout
    }

    /// Verificar si el worker ha excedido max lifetime (EPIC-26 US-26.7)
    pub fn is_lifetime_exceeded(&self) -> bool {
        let lifetime = Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::ZERO);

        lifetime > self.spec.max_lifetime
    }

    /// Verificar si el TTL after completion ha sido excedido (EPIC-26 US-26.7)
    ///
    /// Returns true si:
    /// - El worker tiene un `ttl_after_completion` configurado Y ha pasado ese tiempo
    /// - O si NO hay TTL configurado (None) Y hay un job completado (destrucción inmediata)
    pub fn is_ttl_after_completion_exceeded(&self) -> bool {
        let Some(completed_at) = self.job_completed_at else {
            return false; // No hay job completado, no aplica
        };

        let Some(ttl) = self.spec.ttl_after_completion else {
            return true; // No hay TTL configurado, destrucción inmediata
        };

        let elapsed = Utc::now()
            .signed_duration_since(completed_at)
            .to_std()
            .unwrap_or(Duration::ZERO);

        elapsed > ttl
    }

    /// Determinar la razón de terminación basada en el estado del worker (EPIC-26 US-26.7)
    pub fn termination_reason(&self) -> TerminationReason {
        if self.is_lifetime_exceeded() {
            TerminationReason::LifetimeExceeded
        } else if self.is_idle_timeout() {
            TerminationReason::IdleTimeout
        } else if self.is_ttl_after_completion_exceeded() {
            TerminationReason::JobCompleted
        } else {
            TerminationReason::Unregistered
        }
    }

    /// Obtener el timestamp de job completado (EPIC-26 US-26.7)
    pub fn job_completed_at(&self) -> Option<DateTime<Utc>> {
        self.job_completed_at
    }

    /// Obtener idle timeout en segundos (EPIC-26 US-26.7)
    pub fn idle_timeout_secs(&self) -> u64 {
        self.spec.idle_timeout.as_secs()
    }

    /// Verificar si el worker está sano (heartbeat reciente)
    pub fn is_healthy(&self, heartbeat_timeout: Duration) -> bool {
        if self.state.is_terminated() {
            return false;
        }

        let since_heartbeat = Utc::now()
            .signed_duration_since(self.last_heartbeat)
            .to_std()
            .unwrap_or(Duration::ZERO);

        since_heartbeat <= heartbeat_timeout
    }
}

impl Aggregate for Worker {
    type Id = WorkerId;

    fn aggregate_id(&self) -> &Self::Id {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_worker() -> Worker {
        let spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-123".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        Worker::new(handle, spec)
    }

    #[test]
    fn test_worker_lifecycle() {
        let mut worker = create_test_worker();

        // Estado inicial: Creating
        assert_eq!(*worker.state(), WorkerState::Creating);

        // Transición a Ready (Crash-Only: Creating -> Ready directamente)
        worker.mark_ready().unwrap();
        assert_eq!(*worker.state(), WorkerState::Ready);
        assert!(worker.state().can_accept_jobs());

        // Asignar job -> Busy
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();
        assert_eq!(*worker.state(), WorkerState::Busy);
        assert!(!worker.state().can_accept_jobs());

        // Completar job -> Terminated (EPIC-21: Workers efímeros)
        worker.complete_job().unwrap();
        assert_eq!(*worker.state(), WorkerState::Terminated);
        assert_eq!(worker.jobs_executed(), 1);
        assert!(worker.state().is_terminated());
    }

    #[test]
    fn test_worker_cannot_accept_job_when_busy() {
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();

        worker.assign_job(JobId::new()).unwrap();

        let result = worker.assign_job(JobId::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_worker_terminates_after_successful_job_completion() {
        // GIVEN: Worker en estado Busy con job asignado
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();

        // WHEN: complete_job() es llamado
        worker.complete_job().unwrap();

        // THEN: Estado debe ser Terminated
        assert_eq!(*worker.state(), WorkerState::Terminated);
        assert!(worker.state().is_terminated());
        assert!(!worker.state().can_accept_jobs());
        // AND: current_job_id debe ser None
        assert!(worker.current_job_id().is_none());
        // AND: jobs_executed debe ser incremented
        assert_eq!(worker.jobs_executed(), 1);
    }

    #[test]
    fn test_worker_cannot_complete_job_when_not_busy() {
        // GIVEN: Worker en estado Ready
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();

        // WHEN: complete_job() es llamado
        let result = worker.complete_job();

        // THEN: Debe retornar error InvalidWorkerStateTransition
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::InvalidWorkerStateTransition { .. })
        ));
        // AND: Estado debe permanecer Ready
        assert_eq!(*worker.state(), WorkerState::Ready);
    }

    #[test]
    fn test_worker_termination_emits_correct_event() {
        // GIVEN: Worker en estado Busy
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();

        // WHEN: complete_job() es llamado
        worker.complete_job().unwrap();

        // THEN: Estado debe ser Terminated con reason implícito JobCompleted
        assert_eq!(*worker.state(), WorkerState::Terminated);
        // NOTA: El evento WorkerTerminated se emite en la aplicación, no en el dominio
        // El dominio solo cambia el estado
    }

    // === Crash-Only Worker Tests ===
    // Tests simplificados para el modelo Crash-Only (4 estados)

    #[test]
    fn test_mark_ready_from_creating_succeeds() {
        // GIVEN: Worker en estado Creating
        let mut worker = create_test_worker();
        assert_eq!(*worker.state(), WorkerState::Creating);

        // WHEN: mark_ready() es llamado
        let result = worker.mark_ready();

        // THEN: Transición exitosa
        assert!(result.is_ok());
        assert_eq!(*worker.state(), WorkerState::Ready);
    }

    #[test]
    fn test_mark_ready_from_ready_is_idempotent() {
        // GIVEN: Worker en estado Ready
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        assert_eq!(*worker.state(), WorkerState::Ready);

        // WHEN: mark_ready() es llamado de nuevo
        // THEN: Debe ser idempotente (éxito, no transición)
        let result = worker.mark_ready();
        assert!(result.is_ok());
        // AND: Estado debe permanecer Ready
        assert_eq!(*worker.state(), WorkerState::Ready);
    }

    #[test]
    fn test_mark_ready_from_terminated_fails() {
        // GIVEN: Worker en estado Terminated
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();
        worker.complete_job().unwrap();
        assert_eq!(*worker.state(), WorkerState::Terminated);

        // WHEN: mark_ready() es llamado
        let result = worker.mark_ready();

        // THEN: Debe fallar
        assert!(result.is_err());
    }

    #[test]
    fn test_assign_job_from_ready_succeeds() {
        // GIVEN: Worker en estado Ready
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        assert_eq!(*worker.state(), WorkerState::Ready);

        // WHEN: assign_job() es llamado
        let job_id = JobId::new();
        let result = worker.assign_job(job_id.clone());

        // THEN: Transición exitosa
        assert!(result.is_ok());
        assert_eq!(*worker.state(), WorkerState::Busy);
        assert_eq!(worker.current_job_id, Some(job_id));
    }

    #[test]
    fn test_assign_job_from_terminated_fails() {
        // GIVEN: Worker en estado Terminated
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();
        worker.complete_job().unwrap();
        assert_eq!(*worker.state(), WorkerState::Terminated);

        // WHEN: assign_job() es llamado
        let result = worker.assign_job(JobId::new());

        // THEN: Debe fallar
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::InvalidWorkerStateTransition { .. })
        ));
    }

    #[test]
    fn test_assign_job_from_creating_fails() {
        // GIVEN: Worker en estado Creating
        let mut worker = create_test_worker();
        assert_eq!(*worker.state(), WorkerState::Creating);

        // WHEN: assign_job() es llamado
        let result = worker.assign_job(JobId::new());

        // THEN: Debe fallar
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::InvalidWorkerStateTransition { .. })
        ));
    }

    #[test]
    fn test_complete_job_from_busy_succeeds() {
        // GIVEN: Worker en estado Busy
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id.clone()).unwrap();
        assert_eq!(*worker.state(), WorkerState::Busy);

        // WHEN: complete_job() es llamado
        let result = worker.complete_job();

        // THEN: Transición exitosa a Terminated
        assert!(result.is_ok());
        assert_eq!(*worker.state(), WorkerState::Terminated);
        assert!(worker.current_job_id.is_none());
        assert_eq!(worker.jobs_executed, 1);
    }

    #[test]
    fn test_complete_job_from_creating_fails() {
        // GIVEN: Worker en estado Creating
        let mut worker = create_test_worker();
        assert_eq!(*worker.state(), WorkerState::Creating);

        // WHEN: complete_job() es llamado
        let result = worker.complete_job();

        // THEN: Debe fallar con InvalidWorkerStateTransition (no puede completar job si nunca empezó)
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::InvalidWorkerStateTransition { .. })
        ));
        // AND: Estado debe permanecer Creating
        assert_eq!(*worker.state(), WorkerState::Creating);
    }

    #[test]
    fn test_mark_terminating_from_ready_succeeds() {
        // GIVEN: Worker en estado Ready
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        assert_eq!(*worker.state(), WorkerState::Ready);

        // WHEN: mark_terminating() es llamado (directamente a Terminated en Crash-Only)
        let result = worker.mark_terminating();

        // THEN: Transición exitosa a Terminated
        assert!(result.is_ok());
        assert_eq!(*worker.state(), WorkerState::Terminated);
    }

    #[test]
    fn test_mark_terminating_from_terminated_no_op() {
        // GIVEN: Worker en estado Terminated
        let mut worker = create_test_worker();
        worker.mark_ready().unwrap();
        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap();
        worker.complete_job().unwrap();
        assert_eq!(*worker.state(), WorkerState::Terminated);

        // WHEN: mark_terminating() es llamado
        let result = worker.mark_terminating();

        // THEN: Es un no-op (ya terminado), retorna Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_to_updates_timestamps() {
        // GIVEN: Worker en estado Creating
        let mut worker = create_test_worker();
        let before = worker.updated_at;

        //WHEN: mark_ready() es llamado (Creating -> Ready)
        worker.mark_ready().unwrap();

        // THEN: updated_at debe ser actualizado
        assert!(worker.updated_at > before);
    }

    #[test]
    fn test_worker_state_machine_happy_path() {
        // GIVEN: Worker recién creado
        let mut worker = create_test_worker();

        // THEN: Estado inicial es Creating
        assert_eq!(*worker.state(), WorkerState::Creating);

        // WHEN: Ciclo de vida completo (Crash-Only: 4 estados)
        worker.mark_ready().unwrap(); // Creating -> Ready
        assert_eq!(*worker.state(), WorkerState::Ready);

        let job_id = JobId::new();
        worker.assign_job(job_id).unwrap(); // Ready -> Busy
        assert_eq!(*worker.state(), WorkerState::Busy);

        worker.complete_job().unwrap(); // Busy -> Terminated
        assert_eq!(*worker.state(), WorkerState::Terminated);

        // AND: jobs_executed fue incrementado
        assert_eq!(worker.jobs_executed, 1);
    }

    #[test]
    fn test_invalid_transitions_return_descriptive_error() {
        // GIVEN: Worker en estado Creating
        let mut worker = create_test_worker();

        // WHEN: Transiciones inválidas desde Creating
        let result1 = worker.assign_job(JobId::new());
        let result2 = worker.complete_job();

        // THEN: Todos deben fallar con InvalidWorkerStateTransition
        for result in [result1, result2] {
            assert!(result.is_err());
            if let Err(DomainError::InvalidWorkerStateTransition { current, .. }) = result {
                // El estado debe ser CREATING (Display impl es uppercase)
                assert_eq!(current, "CREATING");
            } else {
                panic!("Expected InvalidWorkerStateTransition error");
            }
        }
    }

    // === ProviderType Serialization Tests ===

    #[test]
    fn test_provider_type_serialize_to_string() {
        // GIVEN: ProviderType::Kubernetes
        let provider_type = ProviderType::Kubernetes;

        // WHEN: Serialize to JSON
        let json = serde_json::to_string(&provider_type).unwrap();

        // THEN: Should be a plain string "kubernetes", not an object
        assert_eq!(json, "\"kubernetes\"");
    }

    #[test]
    fn test_provider_type_deserialize_from_string() {
        // GIVEN: JSON string for kubernetes
        let json = "\"kubernetes\"";

        // WHEN: Deserialize
        let provider_type: ProviderType = serde_json::from_str(json).unwrap();

        // THEN: Should be ProviderType::Kubernetes
        assert_eq!(provider_type, ProviderType::Kubernetes);
    }

    #[test]
    fn test_provider_type_serialize_all_variants() {
        let variants = [
            (ProviderType::Docker, "\"docker\""),
            (ProviderType::Kubernetes, "\"kubernetes\""),
            (ProviderType::Fargate, "\"fargate\""),
            (ProviderType::CloudRun, "\"cloudrun\""),
            (ProviderType::ContainerApps, "\"containerapps\""),
            (ProviderType::Lambda, "\"lambda\""),
            (ProviderType::CloudFunctions, "\"cloudfunctions\""),
            (ProviderType::AzureFunctions, "\"azurefunctions\""),
            (ProviderType::EC2, "\"ec2\""),
            (ProviderType::ComputeEngine, "\"computeengine\""),
            (ProviderType::AzureVMs, "\"azurevms\""),
            (ProviderType::Test, "\"test\""),
            (ProviderType::BareMetal, "\"baremetal\""),
        ];

        for (provider_type, expected_json) in variants {
            let json = serde_json::to_string(&provider_type).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", provider_type);
        }
    }

    #[test]
    fn test_provider_type_deserialize_all_variants() {
        let variants = [
            ("docker", ProviderType::Docker),
            ("kubernetes", ProviderType::Kubernetes),
            ("k8s", ProviderType::Kubernetes),
            ("fargate", ProviderType::Fargate),
            ("cloudrun", ProviderType::CloudRun),
            ("containerapps", ProviderType::ContainerApps),
            ("lambda", ProviderType::Lambda),
            ("cloudfunctions", ProviderType::CloudFunctions),
            ("azurefunctions", ProviderType::AzureFunctions),
            ("ec2", ProviderType::EC2),
            ("computeengine", ProviderType::ComputeEngine),
            ("azurevms", ProviderType::AzureVMs),
            ("test", ProviderType::Test),
            ("baremetal", ProviderType::BareMetal),
            ("bare_metal", ProviderType::BareMetal),
        ];

        for (json_str, expected) in variants {
            let provider_type: ProviderType =
                serde_json::from_str(&format!("\"{}\"", json_str)).unwrap();
            assert_eq!(provider_type, expected, "Failed for {}", json_str);
        }
    }

    #[test]
    fn test_provider_type_roundtrip_serialization() {
        // Test roundtrip for all variants
        let variants = [
            ProviderType::Docker,
            ProviderType::Kubernetes,
            ProviderType::Fargate,
            ProviderType::Test,
            ProviderType::BareMetal,
            ProviderType::Custom("test-provider".to_string()),
        ];

        for original in variants {
            let json = serde_json::to_string(&original).unwrap();
            let deserialized: ProviderType = serde_json::from_str(&json).unwrap();
            assert_eq!(original, deserialized);
        }
    }

    #[test]
    fn test_provider_type_custom_serialization() {
        // GIVEN: A custom provider type
        let custom = ProviderType::Custom("my-custom-provider".to_string());

        // WHEN: Serialize to JSON
        let json = serde_json::to_string(&custom).unwrap();

        // THEN: Should serialize as "custom:my-custom-provider"
        assert_eq!(json, "\"custom:my-custom-provider\"");
    }

    #[test]
    fn test_provider_type_deserialize_case_insensitive() {
        // GIVEN: Uppercase provider type string
        let json = "\"KUBERNETES\"";

        // WHEN: Deserialize
        let provider_type: ProviderType = serde_json::from_str(json).unwrap();

        // THEN: Should be ProviderType::Kubernetes (case insensitive)
        assert_eq!(provider_type, ProviderType::Kubernetes);
    }
}

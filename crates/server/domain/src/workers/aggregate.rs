// Worker Domain - Entidades para workers on-demand

use crate::shared_kernel::{
    Aggregate, DomainError, JobId, ProviderId, Result, WorkerId, WorkerState,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Tipos de provider soportados
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Categorías de providers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCategory {
    Container,
    Serverless,
    VirtualMachine,
    BareMetal,
}

/// Arquitectura del worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    Amd64,
    Arm64,
    Arm,
}

impl Default for Architecture {
    fn default() -> Self {
        Self::Amd64
    }
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
    /// Arquitectura requerida
    pub architecture: Architecture,
    /// Capabilities requeridas
    pub required_capabilities: Vec<String>,
    /// Kubernetes-specific configuration
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
            architecture: Architecture::default(),
            required_capabilities: vec![],
            kubernetes: KubernetesWorkerConfig::default(),
        }
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

    /// Add Kubernetes-specific annotation
    pub fn with_kubernetes_annotation(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.kubernetes.annotations.insert(key.into(), value.into());
        self
    }

    /// Add Kubernetes-specific custom label
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

    /// Add Kubernetes node selector
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

    /// Set Kubernetes service account
    pub fn with_kubernetes_service_account(mut self, service_account: impl Into<String>) -> Self {
        self.kubernetes.service_account = Some(service_account.into());
        self
    }

    /// Add Kubernetes init container
    pub fn with_kubernetes_init_container(mut self, container: KubernetesContainer) -> Self {
        self.kubernetes.init_containers.push(container);
        self
    }

    /// Add Kubernetes sidecar container
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
            jobs_executed: 0, // Could be stored in DB if needed
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

    /// Worker conectando (agent arrancando gRPC)
    pub fn mark_connecting(&mut self) -> Result<()> {
        match &self.state {
            WorkerState::Creating => {
                self.state = WorkerState::Connecting;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err(DomainError::WorkerNotAvailable {
                worker_id: self.id.clone(),
            }),
        }
    }

    /// Alias para retrocompatibilidad
    pub fn mark_starting(&mut self) -> Result<()> {
        self.mark_connecting()
    }

    /// Worker está listo (agent registrado)
    pub fn mark_ready(&mut self) -> Result<()> {
        match &self.state {
            WorkerState::Connecting | WorkerState::Busy | WorkerState::Draining => {
                self.state = WorkerState::Ready;
                self.updated_at = Utc::now();
                self.last_heartbeat = Utc::now();
                Ok(())
            }
            _ => Err(DomainError::WorkerNotAvailable {
                worker_id: self.id.clone(),
            }),
        }
    }

    /// Asignar job al worker
    pub fn assign_job(&mut self, job_id: JobId) -> Result<()> {
        if !self.state.can_accept_jobs() {
            return Err(DomainError::WorkerNotAvailable {
                worker_id: self.id.clone(),
            });
        }

        self.current_job_id = Some(job_id);
        self.state = WorkerState::Busy;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Job completado - worker se termina (efímero)
    pub fn complete_job(&mut self) -> Result<()> {
        match self.state {
            WorkerState::Busy | WorkerState::Draining => {
                self.current_job_id = None;
                self.jobs_executed += 1;
                self.state = WorkerState::Terminated;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err(DomainError::WorkerNotAvailable {
                worker_id: self.id.clone(),
            }),
        }
    }

    /// Marcar worker como draining (no acepta nuevos jobs)
    pub fn mark_draining(&mut self) -> Result<()> {
        match &self.state {
            WorkerState::Ready | WorkerState::Busy => {
                self.state = WorkerState::Draining;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Iniciar terminación del worker
    pub fn mark_terminating(&mut self) -> Result<()> {
        if self.state.is_terminated() {
            return Ok(()); // Ya terminado
        }

        self.state = WorkerState::Terminating;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Worker terminado
    pub fn mark_terminated(&mut self) -> Result<()> {
        self.state = WorkerState::Terminated;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Worker falló - se marca como Terminated (PRD v6.0 no tiene Failed state)
    pub fn mark_failed(&mut self, _reason: String) -> Result<()> {
        self.state = WorkerState::Terminated;
        self.updated_at = Utc::now();
        Ok(())
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

    /// Verificar si el worker ha excedido max lifetime
    pub fn is_lifetime_exceeded(&self) -> bool {
        let lifetime = Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::ZERO);

        lifetime > self.spec.max_lifetime
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

        // Transición a Connecting
        worker.mark_connecting().unwrap();
        assert_eq!(*worker.state(), WorkerState::Connecting);

        // Transición a Ready
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

        // Test alternativo: Worker puede ser marcado como draining ANTES de completar job
        let mut worker2 = create_test_worker();
        worker2.mark_connecting().unwrap();
        worker2.mark_ready().unwrap();
        worker2.assign_job(JobId::new()).unwrap();

        // Draining antes de completar (para jobs largos)
        worker2.mark_draining().unwrap();
        assert_eq!(*worker2.state(), WorkerState::Draining);

        // Completar job -> Terminated (desde Draining también termina)
        worker2.complete_job().unwrap();
        assert_eq!(*worker2.state(), WorkerState::Terminated);
    }

    #[test]
    fn test_worker_cannot_accept_job_when_busy() {
        let mut worker = create_test_worker();
        worker.mark_starting().unwrap();
        worker.mark_ready().unwrap();

        worker.assign_job(JobId::new()).unwrap();

        let result = worker.assign_job(JobId::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_worker_terminates_after_successful_job_completion() {
        // GIVEN: Worker en estado Busy con job asignado
        let mut worker = create_test_worker();
        worker.mark_starting().unwrap();
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
        worker.mark_starting().unwrap();
        worker.mark_ready().unwrap();

        // WHEN: complete_job() es llamado
        let result = worker.complete_job();

        // THEN: Debe retornar error WorkerNotAvailable
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::WorkerNotAvailable { .. })
        ));
        // AND: Estado debe permanecer Ready
        assert_eq!(*worker.state(), WorkerState::Ready);
    }

    #[test]
    fn test_worker_termination_emits_correct_event() {
        // GIVEN: Worker en estado Busy
        let mut worker = create_test_worker();
        worker.mark_starting().unwrap();
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
}

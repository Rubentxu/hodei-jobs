// WorkerProvider Trait - Abstracción para crear workers on-demand
//
// Este módulo implementa el patrón ISP (Interface Segregation Principle) dividiendo
// el trait WorkerProvider en traits más pequeños y cohesivos con default implementations.
//
// También implementa el patrón Extension Objects para configuración específica de providers.

use crate::shared_kernel::{DomainError, ProviderId, WorkerState};
use crate::workers::{
    Architecture, KubernetesToleration, ProviderCategory, ProviderType, ResourceRequirements,
    WorkerHandle, WorkerSpec,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// Extension Objects Pattern - WorkerProviderConfig
// ============================================================================

/// Trait for provider-specific configuration extension objects.
///
/// This trait implements the Extension Objects pattern, allowing `WorkerSpec`
/// to remain provider-agnostic while still supporting provider-specific config.
///
/// # Example
///
/// ```ignore
/// async fn create_worker(
///     &self,
///     spec: &WorkerSpec,
///     config: Option<&dyn WorkerProviderConfig>,
/// ) -> Result<WorkerHandle, ProviderError> {
///     if let Some(k8s_config) = config.and_then(|c| c.as_any().downcast_ref::<KubernetesConfigExt>()) {
///         // Use K8s-specific configuration
///     }
///     // ...
/// }
/// ```
pub trait WorkerProviderConfig: Send + Sync {
    /// Returns the provider type this configuration is for
    fn provider_type(&self) -> ProviderType;

    /// Returns a reference to the configuration as a `dyn Any`
    ///
    /// This enables downcasting to concrete types in infrastructure layer:
    /// ```ignore
    /// if let Some(k8s_config) = config.as_any().downcast_ref::<KubernetesConfigExt>() {
    ///     // use k8s_config
    /// }
    /// ```
    fn as_any(&self) -> &dyn Any;
}

/// Extension object for Kubernetes-specific configuration
///
/// This struct contains simple configuration fields that can be set independently
/// of the main KubernetesWorkerConfig. Complex types (containers, affinity, etc.)
/// are handled by the deprecated `kubernetes` field in WorkerSpec.
///
/// This allows `WorkerSpec` to support provider-agnostic configuration while
/// maintaining backwards compatibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KubernetesConfigExt {
    /// Custom annotations for the Pod
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    /// Custom labels for the Pod (extends base labels)
    #[serde(default)]
    pub custom_labels: HashMap<String, String>,
    /// Node selector for Pod scheduling
    #[serde(default)]
    pub node_selector: HashMap<String, String>,
    /// Service account to use for the Pod
    pub service_account: Option<String>,
    /// Init container images (names only, configuration uses kubernetes.init_containers)
    #[serde(default)]
    pub init_container_images: HashMap<String, String>,
    /// Sidecar container images (names only, configuration uses kubernetes.sidecar_containers)
    #[serde(default)]
    pub sidecar_container_images: HashMap<String, String>,
    /// Tolerations for pod scheduling
    #[serde(default)]
    pub tolerations: Vec<KubernetesToleration>,
    /// DNS policy for the Pod
    pub dns_policy: Option<String>,
}

impl WorkerProviderConfig for KubernetesConfigExt {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Kubernetes
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Extension object for Docker-specific configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfigExt {
    /// Network mode to use
    pub network_mode: Option<String>,
    /// Extra hosts to add to container
    #[serde(default)]
    pub extra_hosts: Vec<String>,
    /// Restart policy
    pub restart_policy: Option<String>,
    /// Privileged mode
    pub privileged: bool,
    /// Runtime to use (e.g., "nvidia")
    pub runtime: Option<String>,
    /// Device mappings
    #[serde(default)]
    pub devices: Vec<String>,
    /// Ulimit settings
    #[serde(default)]
    pub ulimits: Vec<String>,
    /// Sysctls
    #[serde(default)]
    pub sysctls: HashMap<String, String>,
}

impl WorkerProviderConfig for DockerConfigExt {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Docker
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Extension object for Firecracker-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerConfigExt {
    /// Path to kernel image
    pub kernel_path: String,
    /// Path to root filesystem
    pub rootfs_path: String,
    /// Kernel command line args
    #[serde(default)]
    pub kernel_args: Vec<String>,
    /// Network configuration
    pub network: Option<FirecrackerNetworkConfig>,
    /// Enable vsock for gRPC communication
    pub vsock: bool,
    /// CPU template (C3 or T2)
    #[serde(default)]
    pub cpu_template: Option<String>,
    /// Snapshot path for resuming
    #[serde(default)]
    pub snapshot_path: Option<String>,
}

impl Default for FirecrackerConfigExt {
    fn default() -> Self {
        Self {
            kernel_path: String::new(),
            rootfs_path: String::new(),
            kernel_args: Vec::new(),
            network: None,
            vsock: false,
            cpu_template: None,
            snapshot_path: None,
        }
    }
}

impl WorkerProviderConfig for FirecrackerConfigExt {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Custom("firecracker".to_string())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Enum that wraps all provider-specific configuration types.
///
/// This enum implements Serialize, Deserialize, Debug, and Clone, allowing
/// it to be stored directly in WorkerSpec. Each variant corresponds to a
/// specific provider's configuration.
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::workers::{ProviderConfig, KubernetesConfigExt};
///
/// let k8s_config = KubernetesConfigExt::default();
/// let provider_config = ProviderConfig::Kubernetes(k8s_config);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    /// Kubernetes-specific configuration
    #[serde(rename = "kubernetes")]
    Kubernetes(KubernetesConfigExt),
    /// Docker-specific configuration
    #[serde(rename = "docker")]
    Docker(DockerConfigExt),
    /// Firecracker-specific configuration
    #[serde(rename = "firecracker")]
    Firecracker(FirecrackerConfigExt),
}

impl WorkerProviderConfig for ProviderConfig {
    fn provider_type(&self) -> ProviderType {
        match self {
            ProviderConfig::Kubernetes(config) => config.provider_type(),
            ProviderConfig::Docker(config) => config.provider_type(),
            ProviderConfig::Firecracker(config) => config.provider_type(),
        }
    }

    fn as_any(&self) -> &dyn Any {
        match self {
            ProviderConfig::Kubernetes(config) => config.as_any(),
            ProviderConfig::Docker(config) => config.as_any(),
            ProviderConfig::Firecracker(config) => config.as_any(),
        }
    }
}

// Forward traits from inner types
impl ProviderConfig {
    /// Returns a reference as the concrete type if it matches
    pub fn as_kubernetes(&self) -> Option<&KubernetesConfigExt> {
        match self {
            ProviderConfig::Kubernetes(config) => Some(config),
            _ => None,
        }
    }

    /// Returns a reference as the concrete type if it matches
    pub fn as_docker(&self) -> Option<&DockerConfigExt> {
        match self {
            ProviderConfig::Docker(config) => Some(config),
            _ => None,
        }
    }

    /// Returns a reference as the concrete type if it matches
    pub fn as_firecracker(&self) -> Option<&FirecrackerConfigExt> {
        match self {
            ProviderConfig::Firecracker(config) => Some(config),
            _ => None,
        }
    }

    /// Converts from a concrete config to ProviderConfig
    pub fn kubernetes(config: KubernetesConfigExt) -> Self {
        ProviderConfig::Kubernetes(config)
    }

    /// Converts from a concrete config to ProviderConfig
    pub fn docker(config: DockerConfigExt) -> Self {
        ProviderConfig::Docker(config)
    }

    /// Converts from a concrete config to ProviderConfig
    pub fn firecracker(config: FirecrackerConfigExt) -> Self {
        ProviderConfig::Firecracker(config)
    }
}

/// Firecracker network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerNetworkConfig {
    /// TAP device name (auto-generated if empty)
    pub tap_name: Option<String>,
    /// IP address for the microVM
    pub ip_address: Option<String>,
    /// Gateway IP
    pub gateway: Option<String>,
    /// MTU for the network interface
    pub mtu: Option<u32>,
}

/// Helper function to get provider config by type
pub fn get_provider_config<T: WorkerProviderConfig + 'static>(
    config: Option<&dyn WorkerProviderConfig>,
) -> Option<&T> {
    config.and_then(|c| c.as_any().downcast_ref::<T>())
}

// ============================================================================
// Trait base para identidad del provider
// ============================================================================

/// Trait base con los métodos de identidad del provider.
/// Este trait debe ser implementado por todos los providers.
pub trait WorkerProviderIdentity: Send + Sync {
    /// Identificador único del provider configurado
    fn provider_id(&self) -> &ProviderId;

    /// Tipo de provider
    fn provider_type(&self) -> ProviderType;

    /// Categoría del provider (Container, Serverless, VM)
    fn category(&self) -> ProviderCategory {
        self.provider_type().category()
    }

    /// Capacidades que pueden ofrecer los workers
    fn capabilities(&self) -> &ProviderCapabilities;
}

/// Trait helper para acceder a las capacidades
pub trait HasCapabilities: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;
}

// Implementación automática para tipos que ya tienen capabilities()
impl<T: WorkerProviderIdentity + Send + Sync> HasCapabilities for T {
    fn capabilities(&self) -> &ProviderCapabilities {
        WorkerProviderIdentity::capabilities(self)
    }
}

// ============================================================================
// ISP Traits - Segregated interfaces
// ============================================================================

/// Trait esencial para el ciclo de vida del worker (ISP - Core)
#[async_trait]
pub trait WorkerLifecycle: Send + Sync {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
}

/// Trait para gestión de logs del worker (ISP - Logs)
#[async_trait]
pub trait WorkerLogs: Send + Sync {
    async fn get_worker_logs(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> Result<Vec<LogEntry>, ProviderError>;
}

/// Trait para estimación de costos (ISP - Cost)
pub trait WorkerCost: Send + Sync {
    fn estimate_cost(&self, spec: &WorkerSpec, duration: Duration) -> Option<CostEstimate>;
    fn estimated_startup_time(&self) -> Duration;
}

/// Trait para health checks del provider (ISP - Health)
#[async_trait]
pub trait WorkerHealth: Send + Sync {
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;
}

/// Trait para validación de elegibilidad (ISP - Eligibility)
pub trait WorkerEligibility: Send + Sync {
    fn can_fulfill(&self, requirements: &JobRequirements) -> bool;
}

/// Trait para métricas del provider (ISP - Metrics)
pub trait WorkerMetrics: Send + Sync {
    fn get_performance_metrics(&self) -> ProviderPerformanceMetrics;
    fn record_worker_creation(&self, startup_time: Duration, success: bool);
    fn get_startup_time_history(&self) -> Vec<Duration>;
    fn calculate_average_cost_per_hour(&self) -> f64;
    fn calculate_health_score(&self) -> f64;
}

// ============================================================================
// State Adapter Pattern
// ============================================================================

/// Trait for mapping external provider states to WorkerState.
///
/// This implements the State Adapter Pattern, isolating the translation logic
/// for worker states from different providers. Each provider implements this
/// trait for their specific state type.
///
/// # Example
///
/// ```ignore
/// impl StateMapper<ContainerState> for DockerStateMapper {
///     fn to_worker_state(&self, state: &ContainerState) -> WorkerState {
///         match state.status {
///             Some("running") => WorkerState::Ready,
///             Some("exited") => WorkerState::Terminated,
///             _ => WorkerState::Creating,
///         }
///     }
/// }
/// ```
pub trait StateMapper<T> {
    /// Converts an external provider state to WorkerState
    fn to_worker_state(&self, state: &T) -> WorkerState;

    /// Converts a WorkerState to the external provider state
    fn from_worker_state(&self, state: WorkerState) -> T;
}

/// Docker-specific state mapper.
///
/// Maps Docker container states to WorkerState according to PRD v6.0.
#[derive(Debug, Clone, Default)]
pub struct DockerStateMapper;

impl DockerStateMapper {
    pub fn new() -> Self {
        Self
    }
}

impl DockerStateMapper {
    /// Map Docker container status to WorkerState
    pub fn map_container_status(status: Option<&str>) -> WorkerState {
        match status {
            Some("created") => WorkerState::Connecting,
            Some("running") => WorkerState::Ready,
            Some("paused") => WorkerState::Draining,
            Some("restarting") => WorkerState::Connecting,
            Some("removing") => WorkerState::Terminating,
            Some("exited") | Some("dead") => WorkerState::Terminated,
            None | Some("") => WorkerState::Creating,
            _ => WorkerState::Creating,
        }
    }
}

impl<T: AsRef<str>> StateMapper<T> for DockerStateMapper {
    fn to_worker_state(&self, state: &T) -> WorkerState {
        Self::map_container_status(Some(state.as_ref()))
    }

    fn from_worker_state(&self, state: WorkerState) -> T {
        let s: &str = match state {
            WorkerState::Creating | WorkerState::Connecting | WorkerState::Busy => "created",
            WorkerState::Ready => "running",
            WorkerState::Draining => "paused",
            WorkerState::Terminating | WorkerState::Terminated => "exited",
        };
        // Return a static string reference wrapped in a newtype
        // This is simplified - in practice, you'd need to return owned type
        unimplemented!("from_worker_state returns owned type, use explicit conversion")
    }
}

/// Kubernetes-specific state mapper.
///
/// Maps Kubernetes Pod phases to WorkerState.
#[derive(Debug, Clone, Default)]
pub struct KubernetesStateMapper;

impl KubernetesStateMapper {
    pub fn new() -> Self {
        Self
    }
}

impl KubernetesStateMapper {
    /// Map K8s Pod phase to WorkerState
    pub fn map_pod_phase(phase: Option<&str>, container_ready: bool) -> WorkerState {
        match phase {
            Some("Pending") => WorkerState::Creating,
            Some("Running") => {
                if container_ready {
                    WorkerState::Ready
                } else {
                    WorkerState::Connecting
                }
            }
            Some("Succeeded") | Some("Failed") => WorkerState::Terminated,
            Some("Unknown") | None => WorkerState::Creating,
            _ => WorkerState::Creating,
        }
    }

    /// Check if container is ready from Pod status
    pub fn is_container_ready(status: Option<&str>, ready_condition: Option<bool>) -> bool {
        match (status, ready_condition) {
            (_, Some(true)) => true,
            (Some("Running"), None) => true,
            _ => false,
        }
    }
}

impl StateMapper<String> for KubernetesStateMapper {
    fn to_worker_state(&self, state: &String) -> WorkerState {
        Self::map_pod_phase(Some(state.as_str()), false)
    }

    fn from_worker_state(&self, state: WorkerState) -> String {
        match state {
            WorkerState::Creating | WorkerState::Connecting | WorkerState::Busy => {
                "Pending".to_string()
            }
            WorkerState::Ready => "Running".to_string(),
            WorkerState::Draining => "Running".to_string(), // Pod still running, just draining
            WorkerState::Terminating | WorkerState::Terminated => "Succeeded".to_string(),
        }
    }
}

/// Firecracker-specific state mapper.
///
/// Maps Firecracker MicroVM states to WorkerState.
#[derive(Debug, Clone, Default)]
pub struct FirecrackerStateMapper;

impl FirecrackerStateMapper {
    pub fn new() -> Self {
        Self
    }
}

impl FirecrackerStateMapper {
    /// Map Firecracker MicroVM state to WorkerState
    pub fn map_vm_state(state: &str) -> WorkerState {
        match state {
            s if s.eq_ignore_ascii_case("Creating") => WorkerState::Creating,
            s if s.eq_ignore_ascii_case("Running") => WorkerState::Ready,
            s if s.eq_ignore_ascii_case("Stopping") => WorkerState::Draining,
            s if s.eq_ignore_ascii_case("Stopped") => WorkerState::Terminated,
            s if s.eq_ignore_ascii_case("Failed") => WorkerState::Terminated,
            _ => WorkerState::Creating,
        }
    }
}

impl StateMapper<String> for FirecrackerStateMapper {
    fn to_worker_state(&self, state: &String) -> WorkerState {
        Self::map_vm_state(state.as_str())
    }

    fn from_worker_state(&self, state: WorkerState) -> String {
        match state {
            WorkerState::Creating | WorkerState::Connecting | WorkerState::Busy => {
                "Creating".to_string()
            }
            WorkerState::Ready => "Running".to_string(),
            WorkerState::Draining => "Stopping".to_string(),
            WorkerState::Terminating | WorkerState::Terminated => "Stopped".to_string(),
        }
    }
}

// ============================================================================
// WorkerProvider - Trait principal
// ============================================================================

/// Trait principal para todos los providers de workers.
/// Combina identidad + lifecycle + features opcionales.
///
/// Para usar métodos de los sub-traits con `dyn WorkerProvider`, usa llamadas
/// cualificadas como `<dyn WorkerProvider as WorkerLifecycle>::create_worker(provider, spec)`
#[async_trait]
pub trait WorkerProvider:
    WorkerProviderIdentity
    + WorkerLifecycle
    + WorkerLogs
    + WorkerCost
    + WorkerHealth
    + WorkerEligibility
    + WorkerMetrics
    + Send
    + Sync
{
}

// ============================================================================
// WorkerProviderExt - Extension trait para trait objects
// ============================================================================

/// Extension trait que proporciona métodos unificados para `dyn WorkerProvider`.
/// Esto解决 el problema de que los trait objects no pueden llamar métodos de supertraits.
///
/// # Example
///
/// ```
/// use hodei_server_domain::workers::WorkerProviderExt;
///
/// // Using with a concrete provider that implements WorkerProvider
/// // let health = provider.health_check_ext().await;
/// ```
#[async_trait]
pub trait WorkerProviderExt: WorkerProvider {
    /// Wrapper para health_check
    async fn health_check_ext(&self) -> Result<HealthStatus, ProviderError> {
        WorkerHealth::health_check(self).await
    }

    /// Wrapper para create_worker
    async fn create_worker_ext(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        WorkerLifecycle::create_worker(self, spec).await
    }

    /// Wrapper para get_worker_status
    async fn get_worker_status_ext(
        &self,
        handle: &WorkerHandle,
    ) -> Result<WorkerState, ProviderError> {
        WorkerLifecycle::get_worker_status(self, handle).await
    }

    /// Wrapper para destroy_worker
    async fn destroy_worker_ext(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        WorkerLifecycle::destroy_worker(self, handle).await
    }

    /// Wrapper para get_worker_logs
    async fn get_worker_logs_ext(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> Result<Vec<LogEntry>, ProviderError> {
        WorkerLogs::get_worker_logs(self, handle, tail).await
    }

    /// Wrapper para estimate_cost
    fn estimate_cost_ext(&self, spec: &WorkerSpec, duration: Duration) -> Option<CostEstimate> {
        WorkerCost::estimate_cost(self, spec, duration)
    }

    /// Wrapper para estimated_startup_time
    fn estimated_startup_time_ext(&self) -> Duration {
        WorkerCost::estimated_startup_time(self)
    }

    /// Wrapper para can_fulfill
    fn can_fulfill_ext(&self, requirements: &JobRequirements) -> bool {
        WorkerEligibility::can_fulfill(self, requirements)
    }

    /// Wrapper para get_performance_metrics
    fn get_performance_metrics_ext(&self) -> ProviderPerformanceMetrics {
        WorkerMetrics::get_performance_metrics(self)
    }

    /// Wrapper para record_worker_creation
    fn record_worker_creation_ext(&self, startup_time: Duration, success: bool) {
        WorkerMetrics::record_worker_creation(self, startup_time, success)
    }

    /// Wrapper para get_startup_time_history
    fn get_startup_time_history_ext(&self) -> Vec<Duration> {
        WorkerMetrics::get_startup_time_history(self)
    }

    /// Wrapper para calculate_average_cost_per_hour
    fn calculate_average_cost_per_hour_ext(&self) -> f64 {
        WorkerMetrics::calculate_average_cost_per_hour(self)
    }

    /// Wrapper para calculate_health_score
    fn calculate_health_score_ext(&self) -> f64 {
        WorkerMetrics::calculate_health_score(self)
    }
}

// Implementación blanket para cualquier T que implemente WorkerProvider
impl<T: WorkerProvider + Send + Sync> WorkerProviderExt for T {}

// Implementación explícita para dyn WorkerProvider (trait object)
#[async_trait]
impl WorkerProviderExt for dyn WorkerProvider {
    async fn health_check_ext(&self) -> Result<HealthStatus, ProviderError> {
        WorkerHealth::health_check(self).await
    }

    async fn create_worker_ext(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        WorkerLifecycle::create_worker(self, spec).await
    }

    async fn get_worker_status_ext(
        &self,
        handle: &WorkerHandle,
    ) -> Result<WorkerState, ProviderError> {
        WorkerLifecycle::get_worker_status(self, handle).await
    }

    async fn destroy_worker_ext(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        WorkerLifecycle::destroy_worker(self, handle).await
    }

    async fn get_worker_logs_ext(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> Result<Vec<LogEntry>, ProviderError> {
        WorkerLogs::get_worker_logs(self, handle, tail).await
    }

    fn estimate_cost_ext(&self, spec: &WorkerSpec, duration: Duration) -> Option<CostEstimate> {
        WorkerCost::estimate_cost(self, spec, duration)
    }

    fn estimated_startup_time_ext(&self) -> Duration {
        WorkerCost::estimated_startup_time(self)
    }

    fn can_fulfill_ext(&self, requirements: &JobRequirements) -> bool {
        WorkerEligibility::can_fulfill(self, requirements)
    }

    fn get_performance_metrics_ext(&self) -> ProviderPerformanceMetrics {
        WorkerMetrics::get_performance_metrics(self)
    }

    fn record_worker_creation_ext(&self, startup_time: Duration, success: bool) {
        WorkerMetrics::record_worker_creation(self, startup_time, success)
    }

    fn get_startup_time_history_ext(&self) -> Vec<Duration> {
        WorkerMetrics::get_startup_time_history(self)
    }

    fn calculate_average_cost_per_hour_ext(&self) -> f64 {
        WorkerMetrics::calculate_average_cost_per_hour(self)
    }

    fn calculate_health_score_ext(&self) -> f64 {
        WorkerMetrics::calculate_health_score(self)
    }
}

// ============================================================================
// Shared Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// GPU vendor enumeration for typed provider capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Custom(String),
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "Nvidia"),
            GpuVendor::Amd => write!(f, "Amd"),
            GpuVendor::Intel => write!(f, "Intel"),
            GpuVendor::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// GPU model enumeration for typed provider capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuModel {
    TeslaV100,
    TeslaA100,
    TeslaT4,
    RTX3090,
    RTX4090,
    MI100,
    MI200,
    Custom { name: String, vram_gb: u32 },
}

impl std::fmt::Display for GpuModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuModel::TeslaV100 => write!(f, "Tesla V100"),
            GpuModel::TeslaA100 => write!(f, "Tesla A100"),
            GpuModel::TeslaT4 => write!(f, "Tesla T4"),
            GpuModel::RTX3090 => write!(f, "RTX 3090"),
            GpuModel::RTX4090 => write!(f, "RTX 4090"),
            GpuModel::MI100 => write!(f, "MI100"),
            GpuModel::MI200 => write!(f, "MI200"),
            GpuModel::Custom { name, .. } => write!(f, "{}", name),
        }
    }
}

/// Provider feature enumeration for typed capabilities
///
/// Replaces the generic `HashMap<String, Value>` with type-safe enum variants.
/// This provides compile-time validation and better documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderFeature {
    /// GPU capabilities with vendor and model information
    Gpu {
        vendor: GpuVendor,
        models: Vec<GpuModel>,
        max_count: u32,
        cuda_support: bool,
        rocm_support: bool,
    },
    /// CPU capabilities with architecture support
    Cpu {
        min_cores: u32,
        max_cores: u32,
        architectures: Vec<Architecture>,
        min_frequency_mhz: Option<u32>,
        max_frequency_mhz: Option<u32>,
    },
    /// Memory capabilities
    Memory {
        min_bytes: u64,
        max_bytes: u64,
        ecc_support: bool,
        hbm_support: bool,
    },
    /// Storage capabilities
    Storage {
        persistent: bool,
        max_bytes: u64,
        disk_types: Vec<DiskType>,
        raid_support: bool,
    },
    /// Network capabilities
    Network {
        custom_networking: bool,
        max_bandwidth_mbps: u32,
        max_connections: u32,
        firewall_support: bool,
        vpc_support: bool,
    },
    /// Runtime support
    Runtime {
        name: String,
        versions: Vec<String>,
        container_runtimes: Vec<String>,
    },
    /// Security features
    Security {
        encryption_at_rest: bool,
        encryption_in_transit: bool,
        secure_boot: bool,
        tpm_support: bool,
        isolated_tenant: bool,
    },
    /// Specialized workload support
    Specialized {
        supports_mpi: bool,
        supports_gpu_direct: bool,
        supports_rdma: bool,
        supports_fpga: bool,
        supports_sgx: bool,
    },
    /// Operating system support
    OsSupport {
        linux_distros: Vec<String>,
        windows_versions: Vec<String>,
        minimal_kernel_version: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiskType {
    Hdd,
    Ssd,
    Nvme,
    Ephemeral,
    NetworkHdd,
    NetworkSsd,
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Hdd => write!(f, "HDD"),
            DiskType::Ssd => write!(f, "SSD"),
            DiskType::Nvme => write!(f, "NVMe"),
            DiskType::Ephemeral => write!(f, "Ephemeral"),
            DiskType::NetworkHdd => write!(f, "Network HDD"),
            DiskType::NetworkSsd => write!(f, "Network SSD"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub max_resources: ResourceLimits,
    pub gpu_support: bool,
    pub gpu_types: Vec<String>,
    pub architectures: Vec<Architecture>,
    pub runtimes: Vec<String>,
    pub regions: Vec<String>,
    pub max_execution_time: Option<Duration>,
    pub persistent_storage: bool,
    pub custom_networking: bool,
    /// Typed features replacing HashMap<String, Value>
    #[serde(default)]
    pub features: Vec<ProviderFeature>,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            max_resources: ResourceLimits::default(),
            gpu_support: false,
            gpu_types: vec![],
            architectures: vec![Architecture::Amd64],
            runtimes: vec!["shell".to_string()],
            regions: vec!["local".to_string()],
            max_execution_time: Some(Duration::from_secs(3600)),
            persistent_storage: false,
            custom_networking: false,
            features: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_cpu_cores: f64,
    pub max_memory_bytes: i64,
    pub max_disk_bytes: i64,
    pub max_gpu_count: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_cores: 4.0,
            max_memory_bytes: 8 * 1024 * 1024 * 1024,
            max_disk_bytes: 100 * 1024 * 1024 * 1024,
            max_gpu_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequirements {
    pub resources: ResourceRequirements,
    pub architecture: Option<Architecture>,
    pub required_capabilities: Vec<String>,
    pub required_labels: HashMap<String, String>,
    pub allowed_regions: Vec<String>,
    pub timeout: Option<Duration>,
    pub preferred_category: Option<ProviderCategory>,
}

impl Default for JobRequirements {
    fn default() -> Self {
        Self {
            resources: ResourceRequirements::default(),
            architecture: None,
            required_capabilities: vec![],
            required_labels: HashMap::new(),
            allowed_regions: vec![],
            timeout: None,
            preferred_category: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub currency: String,
    pub amount: f64,
    pub unit: CostUnit,
    pub breakdown: HashMap<String, f64>,
}

impl CostEstimate {
    pub fn zero() -> Self {
        Self {
            currency: "USD".to_string(),
            amount: 0.0,
            unit: CostUnit::PerHour,
            breakdown: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CostUnit {
    PerSecond,
    PerMinute,
    PerHour,
    PerInvocation,
    PerGBSecond,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Worker not found: {0}")]
    WorkerNotFound(String),

    #[error("Provisioning failed: {0}")]
    ProvisioningFailed(String),

    #[error("Provisioning timeout")]
    ProvisioningTimeout,

    #[error("Operation timeout: {0}")]
    Timeout(String),

    #[error("Provider not ready: {0}")]
    NotReady(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Provider specific error: {0}")]
    ProviderSpecific(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ProviderError> for DomainError {
    fn from(err: ProviderError) -> Self {
        match err {
            ProviderError::WorkerNotFound(id) => DomainError::WorkerProvisioningFailed {
                message: format!("Worker not found: {}", id),
            },
            ProviderError::ProvisioningFailed(msg) => {
                DomainError::WorkerProvisioningFailed { message: msg }
            }
            ProviderError::ProvisioningTimeout => DomainError::WorkerProvisioningTimeout,
            _ => DomainError::InfrastructureError {
                message: err.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderPerformanceMetrics {
    pub startup_times: Vec<Duration>,
    pub success_rate: f64,
    pub avg_cost_per_hour: f64,
    pub workers_per_minute: f64,
    pub errors_per_minute: f64,
    pub avg_resource_usage: ResourceUsageStats,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceUsageStats {
    pub avg_cpu_millicores: f64,
    pub avg_memory_bytes: u64,
    pub avg_disk_bytes: u64,
    pub peak_cpu_millicores: f64,
    pub peak_memory_bytes: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::WorkerId;
    use std::sync::LazyLock;

    struct MockProvider;

    impl WorkerProviderIdentity for MockProvider {
        fn provider_id(&self) -> &ProviderId {
            static ID: LazyLock<ProviderId> = LazyLock::new(ProviderId::new);
            &ID
        }
        fn provider_type(&self) -> ProviderType {
            ProviderType::Docker
        }
        fn capabilities(&self) -> &ProviderCapabilities {
            static CAPS: LazyLock<ProviderCapabilities> =
                LazyLock::new(ProviderCapabilities::default);
            &CAPS
        }
    }

    #[async_trait]
    impl WorkerLifecycle for MockProvider {
        async fn create_worker(&self, _spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
            Err(ProviderError::UnsupportedOperation(
                "Not implemented".to_string(),
            ))
        }
        async fn get_worker_status(
            &self,
            _handle: &WorkerHandle,
        ) -> Result<WorkerState, ProviderError> {
            Err(ProviderError::UnsupportedOperation(
                "Not implemented".to_string(),
            ))
        }
        async fn destroy_worker(&self, _handle: &WorkerHandle) -> Result<(), ProviderError> {
            Err(ProviderError::UnsupportedOperation(
                "Not implemented".to_string(),
            ))
        }
    }

    #[async_trait]
    impl WorkerHealth for MockProvider {
        async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[async_trait]
    impl WorkerLogs for MockProvider {
        async fn get_worker_logs(
            &self,
            _handle: &WorkerHandle,
            _tail: Option<u32>,
        ) -> Result<Vec<LogEntry>, ProviderError> {
            Err(ProviderError::UnsupportedOperation(
                "Logs retrieval not supported".to_string(),
            ))
        }
    }

    impl WorkerCost for MockProvider {
        fn estimate_cost(&self, _spec: &WorkerSpec, _duration: Duration) -> Option<CostEstimate> {
            None
        }
        fn estimated_startup_time(&self) -> Duration {
            Duration::from_secs(30)
        }
    }

    impl WorkerEligibility for MockProvider {
        fn can_fulfill(&self, requirements: &JobRequirements) -> bool {
            let caps = WorkerProviderIdentity::capabilities(self);
            let resources = &requirements.resources;

            if resources.cpu_cores > caps.max_resources.max_cpu_cores {
                return false;
            }
            if resources.memory_bytes > caps.max_resources.max_memory_bytes {
                return false;
            }
            if resources.gpu_count > 0 && !caps.gpu_support {
                return false;
            }
            if let Some(ref arch) = requirements.architecture {
                if !caps.architectures.contains(arch) {
                    return false;
                }
            }
            true
        }
    }

    impl WorkerMetrics for MockProvider {
        fn get_performance_metrics(&self) -> ProviderPerformanceMetrics {
            ProviderPerformanceMetrics::default()
        }
        fn record_worker_creation(&self, _startup_time: Duration, _success: bool) {}
        fn get_startup_time_history(&self) -> Vec<Duration> {
            Vec::new()
        }
        fn calculate_average_cost_per_hour(&self) -> f64 {
            0.0
        }
        fn calculate_health_score(&self) -> f64 {
            0.95
        }
    }

    #[test]
    fn test_health_status_default() {
        assert_eq!(HealthStatus::default(), HealthStatus::Unknown);
    }

    #[test]
    fn test_provider_capabilities_default() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.gpu_support);
        assert!(caps.architectures.contains(&Architecture::Amd64));
    }

    #[test]
    fn test_cost_estimate_zero() {
        let cost = CostEstimate::zero();
        assert_eq!(cost.amount, 0.0);
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn test_provider_error_conversion() {
        let err = ProviderError::ProvisioningFailed("test".to_string());
        let domain_err: DomainError = err.into();
        assert!(matches!(
            domain_err,
            DomainError::WorkerProvisioningFailed { .. }
        ));
    }

    #[tokio::test]
    async fn test_worker_logs_default_implementation() {
        let mock = MockProvider;
        let handle = WorkerHandle::new(
            WorkerId::new(),
            "test".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let result = mock.get_worker_logs(&handle, None).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("not supported"));
        }
    }

    #[test]
    fn test_worker_cost_default_implementation() {
        let mock = MockProvider;
        assert_eq!(mock.estimated_startup_time(), Duration::from_secs(30));
        assert!(
            mock.estimate_cost(
                &WorkerSpec::new("test".to_string(), "http://localhost".to_string()),
                Duration::from_secs(60)
            )
            .is_none()
        );
    }

    #[test]
    fn test_worker_metrics_default_implementation() {
        let mock = MockProvider;
        let metrics = mock.get_performance_metrics();
        assert!(metrics.startup_times.is_empty());
        assert_eq!(mock.calculate_health_score(), 0.95);
    }

    #[test]
    fn test_worker_eligibility_default_implementation() {
        let mock = MockProvider;
        let req = JobRequirements {
            resources: ResourceRequirements {
                cpu_cores: 2.0,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(mock.can_fulfill(&req));
    }

    #[test]
    fn test_worker_eligibility_exceeds_resources() {
        let mock = MockProvider;
        let req = JobRequirements {
            resources: ResourceRequirements {
                cpu_cores: 100.0,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!mock.can_fulfill(&req));
    }

    // =========================================================================
    // Extension Objects Pattern Tests
    // =========================================================================
    // Extension Objects Pattern Tests
    // =========================================================================

    #[test]
    fn test_kubernetes_config_ext_default() {
        let config = KubernetesConfigExt::default();
        assert!(config.annotations.is_empty());
        assert!(config.custom_labels.is_empty());
        assert!(config.node_selector.is_empty());
        assert!(config.init_container_images.is_empty());
        assert!(config.sidecar_container_images.is_empty());
        assert!(config.tolerations.is_empty());
    }

    #[test]
    fn test_kubernetes_config_ext_builder_pattern() {
        let mut config = KubernetesConfigExt::default();
        config
            .annotations
            .insert("key".to_string(), "value".to_string());
        config
            .node_selector
            .insert("disktype".to_string(), "ssd".to_string());
        config.service_account = Some("my-sa".to_string());

        assert_eq!(config.annotations.get("key"), Some(&"value".to_string()));
        assert_eq!(
            config.node_selector.get("disktype"),
            Some(&"ssd".to_string())
        );
        assert_eq!(config.service_account, Some("my-sa".to_string()));
    }

    #[test]
    fn test_kubernetes_config_ext_implements_worker_provider_config() {
        let config = KubernetesConfigExt::default();
        assert_eq!(config.provider_type(), ProviderType::Kubernetes);
        assert!(config.as_any().is::<KubernetesConfigExt>());
    }

    #[test]
    fn test_docker_config_ext_default() {
        let config = DockerConfigExt::default();
        assert!(config.extra_hosts.is_empty());
        assert!(config.devices.is_empty());
        assert!(config.ulimits.is_empty());
        assert!(config.sysctls.is_empty());
        assert!(!config.privileged);
    }

    #[test]
    fn test_docker_config_ext_implements_worker_provider_config() {
        let config = DockerConfigExt::default();
        assert_eq!(config.provider_type(), ProviderType::Docker);
        assert!(config.as_any().is::<DockerConfigExt>());
    }

    #[test]
    fn test_firecracker_config_ext_default() {
        let config = FirecrackerConfigExt::default();
        assert!(config.kernel_args.is_empty());
        assert!(config.network.is_none());
        assert!(!config.vsock);
        assert!(config.snapshot_path.is_none());
    }

    #[test]
    fn test_firecracker_config_ext_implements_worker_provider_config() {
        let config = FirecrackerConfigExt::default();
        assert_eq!(
            config.provider_type(),
            ProviderType::Custom("firecracker".to_string())
        );
        assert!(config.as_any().is::<FirecrackerConfigExt>());
    }

    #[test]
    fn test_get_provider_config_helper_kubernetes() {
        let k8s_config: Box<dyn WorkerProviderConfig> = Box::new(KubernetesConfigExt::default());
        let config_ref: Option<&dyn WorkerProviderConfig> = Some(k8s_config.as_ref());

        let extracted = get_provider_config::<KubernetesConfigExt>(config_ref);
        assert!(extracted.is_some());
    }

    #[test]
    fn test_get_provider_config_helper_wrong_type() {
        let k8s_config: Box<dyn WorkerProviderConfig> = Box::new(KubernetesConfigExt::default());
        let config_ref: Option<&dyn WorkerProviderConfig> = Some(k8s_config.as_ref());

        // Try to extract as Docker config - should fail
        let extracted = get_provider_config::<DockerConfigExt>(config_ref);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_get_provider_config_helper_none() {
        let extracted = get_provider_config::<KubernetesConfigExt>(None);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_worker_provider_config_downcast() {
        let k8s_config: Box<dyn WorkerProviderConfig> = Box::new(KubernetesConfigExt {
            annotations: HashMap::from([("test".to_string(), "value".to_string())]),
            ..Default::default()
        });

        // Downcast to Any and then to concrete type
        let any_ref = k8s_config.as_any();
        let concrete = any_ref.downcast_ref::<KubernetesConfigExt>();
        assert!(concrete.is_some());
        assert_eq!(
            concrete.unwrap().annotations.get("test"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_firecracker_network_config_serialization() {
        let network = FirecrackerNetworkConfig {
            tap_name: Some("tap0".to_string()),
            ip_address: Some("192.168.1.100".to_string()),
            gateway: Some("192.168.1.1".to_string()),
            mtu: Some(1500),
        };

        let serialized = serde_json::to_string(&network).unwrap();
        let deserialized: FirecrackerNetworkConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.tap_name, network.tap_name);
        assert_eq!(deserialized.ip_address, network.ip_address);
    }

    #[test]
    fn test_kubernetes_config_ext_serialization_roundtrip() {
        let original = KubernetesConfigExt {
            annotations: HashMap::from([("env".to_string(), "prod".to_string())]),
            custom_labels: HashMap::from([("team".to_string(), "platform".to_string())]),
            node_selector: HashMap::from([("node-type".to_string(), "worker".to_string())]),
            service_account: Some("hodei-worker".to_string()),
            init_container_images: HashMap::from([(
                "init".to_string(),
                "busybox:latest".to_string(),
            )]),
            sidecar_container_images: HashMap::from([(
                "sidecar".to_string(),
                "sidecar:latest".to_string(),
            )]),
            tolerations: vec![],
            dns_policy: None,
        };

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: KubernetesConfigExt = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.annotations, original.annotations);
        assert_eq!(deserialized.custom_labels, original.custom_labels);
        assert_eq!(deserialized.node_selector, original.node_selector);
        assert_eq!(deserialized.service_account, original.service_account);
        assert_eq!(deserialized.init_container_images.len(), 1);
        assert_eq!(deserialized.sidecar_container_images.len(), 1);
    }

    // =========================================================================
    // State Adapter Pattern Tests
    // =========================================================================

    #[test]
    fn test_docker_state_mapper_creating() {
        let mapper = DockerStateMapper::new();
        assert_eq!(mapper.to_worker_state(&"created"), WorkerState::Connecting);
        assert_eq!(mapper.to_worker_state(&"created"), WorkerState::Connecting);
    }

    #[test]
    fn test_docker_state_mapper_ready() {
        let mapper = DockerStateMapper::new();
        assert_eq!(mapper.to_worker_state(&"running"), WorkerState::Ready);
    }

    #[test]
    fn test_docker_state_mapper_draining() {
        let mapper = DockerStateMapper::new();
        assert_eq!(mapper.to_worker_state(&"paused"), WorkerState::Draining);
    }

    #[test]
    fn test_docker_state_mapper_terminated() {
        let mapper = DockerStateMapper::new();
        assert_eq!(mapper.to_worker_state(&"exited"), WorkerState::Terminated);
        assert_eq!(mapper.to_worker_state(&"dead"), WorkerState::Terminated);
    }

    #[test]
    fn test_docker_state_mapper_creating_state() {
        let mapper = DockerStateMapper::new();
        assert_eq!(mapper.to_worker_state(&"created"), WorkerState::Connecting);
        assert_eq!(mapper.to_worker_state(&""), WorkerState::Creating);
        assert_eq!(mapper.to_worker_state(&"unknown"), WorkerState::Creating);
    }

    #[test]
    fn test_docker_state_mapper_public_helper() {
        assert_eq!(
            DockerStateMapper::map_container_status(Some("running")),
            WorkerState::Ready
        );
        assert_eq!(
            DockerStateMapper::map_container_status(Some("exited")),
            WorkerState::Terminated
        );
        assert_eq!(
            DockerStateMapper::map_container_status(None),
            WorkerState::Creating
        );
    }

    #[test]
    fn test_kubernetes_state_mapper_pending() {
        let mapper = KubernetesStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Pending".to_string()),
            WorkerState::Creating
        );
    }

    #[test]
    fn test_kubernetes_state_mapper_running_ready() {
        let mapper = KubernetesStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Running".to_string()),
            WorkerState::Connecting
        );
    }

    #[test]
    fn test_kubernetes_state_mapper_terminated() {
        let mapper = KubernetesStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Succeeded".to_string()),
            WorkerState::Terminated
        );
        assert_eq!(
            mapper.to_worker_state(&"Failed".to_string()),
            WorkerState::Terminated
        );
    }

    #[test]
    fn test_kubernetes_state_mapper_from_worker_state() {
        let mapper = KubernetesStateMapper::new();
        assert_eq!(mapper.from_worker_state(WorkerState::Creating), "Pending");
        assert_eq!(mapper.from_worker_state(WorkerState::Ready), "Running");
        assert_eq!(
            mapper.from_worker_state(WorkerState::Terminated),
            "Succeeded"
        );
    }

    #[test]
    fn test_kubernetes_state_mapper_public_helper() {
        assert_eq!(
            KubernetesStateMapper::map_pod_phase(Some("Pending"), false),
            WorkerState::Creating
        );
        assert_eq!(
            KubernetesStateMapper::map_pod_phase(Some("Running"), true),
            WorkerState::Ready
        );
        assert_eq!(
            KubernetesStateMapper::map_pod_phase(Some("Running"), false),
            WorkerState::Connecting
        );
        assert_eq!(
            KubernetesStateMapper::map_pod_phase(Some("Succeeded"), false),
            WorkerState::Terminated
        );
    }

    #[test]
    fn test_firecracker_state_mapper_creating() {
        let mapper = FirecrackerStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Creating".to_string()),
            WorkerState::Creating
        );
        assert_eq!(
            mapper.to_worker_state(&"creating".to_string()),
            WorkerState::Creating
        );
    }

    #[test]
    fn test_firecracker_state_mapper_ready() {
        let mapper = FirecrackerStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Running".to_string()),
            WorkerState::Ready
        );
    }

    #[test]
    fn test_firecracker_state_mapper_draining() {
        let mapper = FirecrackerStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Stopping".to_string()),
            WorkerState::Draining
        );
    }

    #[test]
    fn test_firecracker_state_mapper_terminated() {
        let mapper = FirecrackerStateMapper::new();
        assert_eq!(
            mapper.to_worker_state(&"Stopped".to_string()),
            WorkerState::Terminated
        );
        assert_eq!(
            mapper.to_worker_state(&"Failed".to_string()),
            WorkerState::Terminated
        );
    }

    #[test]
    fn test_firecracker_state_mapper_from_worker_state() {
        let mapper = FirecrackerStateMapper::new();
        assert_eq!(mapper.from_worker_state(WorkerState::Creating), "Creating");
        assert_eq!(mapper.from_worker_state(WorkerState::Ready), "Running");
        assert_eq!(mapper.from_worker_state(WorkerState::Draining), "Stopping");
        assert_eq!(mapper.from_worker_state(WorkerState::Terminated), "Stopped");
    }

    #[test]
    fn test_firecracker_state_mapper_public_helper() {
        assert_eq!(
            FirecrackerStateMapper::map_vm_state("Creating"),
            WorkerState::Creating
        );
        assert_eq!(
            FirecrackerStateMapper::map_vm_state("RUNNING"),
            WorkerState::Ready
        );
        assert_eq!(
            FirecrackerStateMapper::map_vm_state("Stopped"),
            WorkerState::Terminated
        );
        assert_eq!(
            FirecrackerStateMapper::map_vm_state("Unknown"),
            WorkerState::Creating
        );
    }

    #[test]
    fn test_state_mapper_trait_object() {
        // Test that state mappers can be used as trait objects
        let docker_mapper: Box<dyn StateMapper<&str>> = Box::new(DockerStateMapper::new());
        let k8s_mapper: Box<dyn StateMapper<String>> = Box::new(KubernetesStateMapper::new());
        let fc_mapper: Box<dyn StateMapper<String>> = Box::new(FirecrackerStateMapper::new());

        let running_str = "running";
        let exited_str = "exited";

        // Docker handles its specific container states
        let docker_state = docker_mapper.to_worker_state(&running_str);
        assert_eq!(docker_state, WorkerState::Ready);

        let docker_state = docker_mapper.to_worker_state(&exited_str);
        assert_eq!(docker_state, WorkerState::Terminated);

        // K8s handles pod phases (Pending -> Creating, Succeeded -> Terminated)
        let k8s_state = k8s_mapper.to_worker_state(&"Pending".to_string());
        assert_eq!(k8s_state, WorkerState::Creating);

        let k8s_state = k8s_mapper.to_worker_state(&"Succeeded".to_string());
        assert_eq!(k8s_state, WorkerState::Terminated);

        // Firecracker handles its specific VM states
        let fc_state = fc_mapper.to_worker_state(&"Running".to_string());
        assert_eq!(fc_state, WorkerState::Ready);

        let fc_state = fc_mapper.to_worker_state(&"Creating".to_string());
        assert_eq!(fc_state, WorkerState::Creating);
    }

    // ========================================================================
    // US-23.4: ProviderCapabilities Typed Features Tests
    // ========================================================================

    #[test]
    fn test_gpu_vendor_display() {
        assert_eq!(GpuVendor::Nvidia.to_string(), "Nvidia");
        assert_eq!(GpuVendor::Amd.to_string(), "Amd");
        assert_eq!(GpuVendor::Intel.to_string(), "Intel");
        assert_eq!(
            GpuVendor::Custom("CustomVendor".to_string()).to_string(),
            "CustomVendor"
        );
    }

    #[test]
    fn test_gpu_model_display() {
        assert_eq!(GpuModel::TeslaV100.to_string(), "Tesla V100");
        assert_eq!(GpuModel::RTX3090.to_string(), "RTX 3090");
        assert_eq!(
            GpuModel::Custom {
                name: "CustomGPU".to_string(),
                vram_gb: 16
            }
            .to_string(),
            "CustomGPU"
        );
    }

    #[test]
    fn test_disk_type_display() {
        assert_eq!(DiskType::Hdd.to_string(), "HDD");
        assert_eq!(DiskType::Ssd.to_string(), "SSD");
        assert_eq!(DiskType::Nvme.to_string(), "NVMe");
        assert_eq!(DiskType::Ephemeral.to_string(), "Ephemeral");
        assert_eq!(DiskType::NetworkHdd.to_string(), "Network HDD");
        assert_eq!(DiskType::NetworkSsd.to_string(), "Network SSD");
    }

    #[test]
    fn test_provider_feature_gpu_serialization() {
        let gpu_feature = ProviderFeature::Gpu {
            vendor: GpuVendor::Nvidia,
            models: vec![GpuModel::TeslaV100, GpuModel::RTX3090],
            max_count: 4,
            cuda_support: true,
            rocm_support: false,
        };
        let json = serde_json::to_string(&gpu_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, gpu_feature);
    }

    #[test]
    fn test_provider_feature_cpu_serialization() {
        let cpu_feature = ProviderFeature::Cpu {
            min_cores: 2,
            max_cores: 64,
            architectures: vec![Architecture::Amd64, Architecture::Arm64],
            min_frequency_mhz: Some(2000),
            max_frequency_mhz: Some(3500),
        };
        let json = serde_json::to_string(&cpu_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cpu_feature);
    }

    #[test]
    fn test_provider_feature_network_serialization() {
        let network_feature = ProviderFeature::Network {
            custom_networking: true,
            max_bandwidth_mbps: 10000,
            max_connections: 100000,
            firewall_support: true,
            vpc_support: true,
        };
        let json = serde_json::to_string(&network_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, network_feature);
    }

    #[test]
    fn test_provider_feature_storage_serialization() {
        let storage_feature = ProviderFeature::Storage {
            persistent: true,
            max_bytes: 1024 * 1024 * 1024 * 1000,
            disk_types: vec![DiskType::Ssd, DiskType::Nvme],
            raid_support: true,
        };
        let json = serde_json::to_string(&storage_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, storage_feature);
    }

    #[test]
    fn test_provider_feature_runtime_serialization() {
        let runtime_feature = ProviderFeature::Runtime {
            name: "containerd".to_string(),
            versions: vec!["1.6.0".to_string(), "1.7.0".to_string()],
            container_runtimes: vec!["docker".to_string(), "containerd".to_string()],
        };
        let json = serde_json::to_string(&runtime_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, runtime_feature);
    }

    #[test]
    fn test_provider_feature_security_serialization() {
        let security_feature = ProviderFeature::Security {
            encryption_at_rest: true,
            encryption_in_transit: true,
            secure_boot: true,
            tpm_support: true,
            isolated_tenant: true,
        };
        let json = serde_json::to_string(&security_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, security_feature);
    }

    #[test]
    fn test_provider_feature_specialized_serialization() {
        let specialized_feature = ProviderFeature::Specialized {
            supports_mpi: true,
            supports_gpu_direct: true,
            supports_rdma: true,
            supports_fpga: false,
            supports_sgx: false,
        };
        let json = serde_json::to_string(&specialized_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, specialized_feature);
    }

    #[test]
    fn test_provider_feature_os_support_serialization() {
        let os_feature = ProviderFeature::OsSupport {
            linux_distros: vec![
                "Ubuntu".to_string(),
                "CentOS".to_string(),
                "Amazon Linux".to_string(),
            ],
            windows_versions: vec!["2022".to_string()],
            minimal_kernel_version: Some("5.10.0".to_string()),
        };
        let json = serde_json::to_string(&os_feature).unwrap();
        let deserialized: ProviderFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, os_feature);
    }

    #[test]
    fn test_provider_capabilities_with_typed_features() {
        let caps = ProviderCapabilities {
            max_resources: ResourceLimits::default(),
            gpu_support: true,
            gpu_types: vec!["nvidia".to_string()],
            architectures: vec![Architecture::Amd64, Architecture::Arm64],
            runtimes: vec!["docker".to_string(), "containerd".to_string()],
            regions: vec!["us-east-1".to_string(), "eu-west-1".to_string()],
            max_execution_time: Some(Duration::from_secs(7200)),
            persistent_storage: true,
            custom_networking: true,
            features: vec![
                ProviderFeature::Gpu {
                    vendor: GpuVendor::Nvidia,
                    models: vec![GpuModel::TeslaV100],
                    max_count: 2,
                    cuda_support: true,
                    rocm_support: false,
                },
                ProviderFeature::Network {
                    custom_networking: true,
                    max_bandwidth_mbps: 10000,
                    max_connections: 100000,
                    firewall_support: true,
                    vpc_support: true,
                },
            ],
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&caps).unwrap();
        let deserialized: ProviderCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, caps);
    }

    #[test]
    fn test_provider_capabilities_default_no_features() {
        let caps = ProviderCapabilities::default();
        assert!(caps.features.is_empty());
    }

    #[test]
    fn test_provider_capabilities_find_feature() {
        let caps = ProviderCapabilities {
            max_resources: ResourceLimits::default(),
            gpu_support: true,
            gpu_types: vec![],
            architectures: vec![Architecture::Amd64],
            runtimes: vec![],
            regions: vec![],
            max_execution_time: None,
            persistent_storage: false,
            custom_networking: false,
            features: vec![
                ProviderFeature::Gpu {
                    vendor: GpuVendor::Nvidia,
                    models: vec![GpuModel::TeslaV100],
                    max_count: 4,
                    cuda_support: true,
                    rocm_support: false,
                },
                ProviderFeature::Memory {
                    min_bytes: 8 * 1024 * 1024 * 1024,
                    max_bytes: 64 * 1024 * 1024 * 1024,
                    ecc_support: true,
                    hbm_support: false,
                },
            ],
        };

        // Find GPU feature
        let gpu_feature = caps
            .features
            .iter()
            .find(|f| matches!(f, ProviderFeature::Gpu { .. }));
        assert!(gpu_feature.is_some());
        if let ProviderFeature::Gpu {
            vendor, max_count, ..
        } = gpu_feature.unwrap()
        {
            assert_eq!(*vendor, GpuVendor::Nvidia);
            assert_eq!(*max_count, 4);
        }

        // Find Memory feature
        let mem_feature = caps
            .features
            .iter()
            .find(|f| matches!(f, ProviderFeature::Memory { .. }));
        assert!(mem_feature.is_some());
        if let ProviderFeature::Memory { max_bytes, .. } = mem_feature.unwrap() {
            assert_eq!(*max_bytes, 64 * 1024 * 1024 * 1024);
        }
    }
}

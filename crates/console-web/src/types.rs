//! Shared types for the Hodei Operator Dashboard

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Marker trait for provider configurations
pub trait ProviderConfig: Send + Sync {}

/// Provider types supported by Hodei
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderType {
    /// Docker provider
    #[serde(rename = "docker")]
    Docker,
    /// Kubernetes provider
    #[serde(rename = "kubernetes")]
    Kubernetes,
    /// Firecracker provider
    #[serde(rename = "firecracker")]
    Firecracker,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    /// Provider name
    pub name: String,
    /// Target namespace
    pub namespace: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Whether the provider is enabled
    pub enabled: bool,
    /// Configuration details
    pub config: ProviderConfigDetail,
    /// Creation timestamp
    pub created_at: String,
}

/// Provider configuration details (union type based on provider)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProviderConfigDetail {
    /// Docker-specific configuration
    #[serde(default)]
    pub docker: Option<DockerConfig>,
    /// Kubernetes-specific configuration
    #[serde(default)]
    pub kubernetes: Option<KubernetesConfig>,
    /// Firecracker-specific configuration
    #[serde(default)]
    pub firecracker: Option<FirecrackerConfig>,
}

/// Docker provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerConfig {
    /// Docker host address
    pub host: String,
    /// Docker network name
    pub network: Option<String>,
    /// Registry configuration
    pub registry: Option<RegistryConfig>,
}

/// Kubernetes provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KubernetesConfig {
    /// Kubernetes namespace
    pub namespace: String,
    /// Optional service account name
    pub service_account: Option<String>,
    /// Pod tolerations
    pub tolerations: Option<Vec<Toleration>>,
    /// Node selector labels
    pub node_selector: Option<HashMap<String, String>>,
}

/// Firecracker provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirecrackerConfig {
    /// Path to Firecracker socket
    pub socket_path: String,
    /// Path to kernel image
    pub kernel: Option<String>,
    /// Path to initrd image
    pub initrd: Option<String>,
}

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryConfig {
    /// Registry server URL
    pub server: String,
    /// Registry username
    pub username: Option<String>,
    /// Name of secret containing registry password
    pub password_secret_ref: Option<String>,
}

/// Kubernetes toleration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Toleration {
    /// Toleration key
    pub key: String,
    /// Toleration operator (e.g., "Exists", "Equal")
    pub operator: String,
    /// Toleration value
    pub value: Option<String>,
    /// Toleration effect (e.g., "NoSchedule")
    pub effect: String,
}

/// Worker Pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerPool {
    /// Pool name
    pub name: String,
    /// Target namespace
    pub namespace: String,
    /// Associated provider name
    pub provider: String,
    /// Minimum number of replicas
    pub min_replicas: i32,
    /// Maximum number of replicas
    pub max_replicas: i32,
    /// Current number of replicas
    pub current_replicas: i32,
    /// Worker image name
    pub image: String,
    /// Resource requirements
    pub resources: ResourceRequirements,
    /// Optional pool-specific configuration
    pub config: Option<PoolConfig>,
    /// Creation timestamp
    pub created_at: String,
}

/// Resource requirements for a worker pool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceRequirements {
    /// Resource requests
    pub requests: Option<ResourceSpec>,
    /// Resource limits
    pub limits: Option<ResourceSpec>,
}

/// Resource specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceSpec {
    /// Memory amount (e.g., "512Mi")
    pub memory: Option<String>,
    /// CPU amount (e.g., "500m")
    pub cpu: Option<String>,
}

/// Pool-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PoolConfig {
    /// Kubernetes-specific pool configuration
    pub kubernetes: Option<KubernetesPoolConfig>,
    /// Docker-specific pool configuration
    pub docker: Option<DockerPoolConfig>,
}

/// Kubernetes pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KubernetesPoolConfig {
    /// Optional namespace override
    pub namespace: Option<String>,
    /// Optional service account name
    pub service_account: Option<String>,
    /// Pod tolerations
    pub tolerations: Option<Vec<Toleration>>,
    /// Node selector labels
    pub node_selector: Option<HashMap<String, String>>,
    /// Pod affinity configuration
    pub affinity: Option<serde_json::Value>,
}

/// Docker pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DockerPoolConfig {
    /// Optional network name
    pub network: Option<String>,
    /// List of volume mounts
    pub volumes: Option<Vec<Volume>>,
}

/// Docker volume mount
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Volume {
    /// Host source path
    pub source: String,
    /// Container target path
    pub target: String,
    /// Whether the volume is read-only
    pub read_only: bool,
}

/// Operator settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorSettings {
    /// Kubernetes namespace
    pub namespace: String,
    /// Namespace to watch for resources
    pub watch_namespace: String,
    /// Server API address
    pub server_address: String,
    /// Active log level
    pub log_level: LogLevel,
    /// Interval for health checks
    pub health_check_interval: i32,
}

/// Log levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    /// Finest-grained informational events
    #[serde(rename = "trace")]
    Trace,
    /// Potentially useful for debugging
    #[serde(rename = "debug")]
    Debug,
    /// General informational messages
    #[serde(rename = "info")]
    Info,
    /// Potentially harmful situations
    #[serde(rename = "warn")]
    Warn,
    /// Error events that might still allow the application to continue running
    #[serde(rename = "error")]
    Error,
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    /// Job is waiting for a worker
    #[serde(rename = "pending")]
    Pending,
    /// Job is currently being executed
    #[serde(rename = "running")]
    Running,
    /// Job completed successfully
    #[serde(rename = "completed")]
    Completed,
    /// Job failed during execution
    #[serde(rename = "failed")]
    Failed,
    /// Job was manually cancelled
    #[serde(rename = "cancelled")]
    Cancelled,
    /// Status is unknown
    #[serde(rename = "unknown")]
    Unknown,
}

/// Worker state (matches proto definition)
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerState {
    /// State is unknown
    Unknown = 0,
    /// Worker is registered but not yet ready
    Registered = 1,
    /// Worker is ready and waiting for jobs
    Idle = 2,
    /// Worker is currently executing a job
    Running = 3,
    /// Worker has been terminated
    Terminated = 4,
}

impl WorkerState {
    /// Convert from i32 (proto value)
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(WorkerState::Unknown),
            1 => Some(WorkerState::Registered),
            2 => Some(WorkerState::Idle),
            3 => Some(WorkerState::Running),
            4 => Some(WorkerState::Terminated),
            _ => None,
        }
    }
}

/// Job information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    /// Unique identifier
    pub id: String,
    /// Job name
    pub name: String,
    /// Target namespace
    pub namespace: String,
    /// Command to execute
    pub command: String,
    /// Optional command arguments
    pub arguments: Option<Vec<String>>,
    /// Current job status
    pub status: JobStatus,
    /// Associated provider name
    pub provider: Option<String>,
    /// Creation timestamp
    pub created_at: String,
    /// Start timestamp
    pub started_at: Option<String>,
    /// Completion timestamp
    pub completed_at: Option<String>,
}

/// Dashboard statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardStats {
    /// Total number of jobs
    pub total_jobs: u32,
    /// Number of running jobs
    pub running_jobs: u32,
    /// Number of completed jobs
    pub completed_jobs: u32,
    /// Number of failed jobs
    pub failed_jobs: u32,
    /// Total number of worker pools
    pub total_pools: u32,
    /// Number of active worker pools
    pub active_pools: u32,
    /// Total number of providers
    pub total_providers: u32,
    /// Number of enabled providers
    pub enabled_providers: u32,
}

/// Notification for user feedback
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    /// Notification kind (success, error, etc.)
    pub kind: NotificationKind,
    /// Notification title
    pub title: String,
    /// Detailed message
    pub message: String,
}

/// Notification kind
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationKind {
    /// Success notification
    Success,
    /// Error notification
    Error,
    /// Warning notification
    Warning,
    /// Informational notification
    Info,
}

/// Form input for creating/updating providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderInput {
    /// Provider name
    pub name: String,
    /// Type of provider (e.g., "docker", "kubernetes")
    pub provider_type: String,
    /// Whether the provider is enabled
    pub enabled: bool,
    /// Optional Docker-specific configuration
    pub docker_config: Option<DockerConfigInput>,
    /// Optional Kubernetes-specific configuration
    pub kubernetes_config: Option<KubernetesConfigInput>,
    /// Optional Firecracker-specific configuration
    pub firecracker_config: Option<FirecrackerConfigInput>,
}

/// Docker config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockerConfigInput {
    /// Docker host address
    pub host: String,
    /// Optional Docker network
    pub network: Option<String>,
    /// Optional registry server URL
    pub registry_server: Option<String>,
    /// Optional registry username
    pub registry_username: Option<String>,
}

/// Kubernetes config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubernetesConfigInput {
    /// Target namespace
    pub namespace: String,
    /// Optional service account name
    pub service_account: Option<String>,
}

/// Firecracker config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirecrackerConfigInput {
    /// Path to Firecracker socket
    pub socket_path: String,
    /// Path to kernel image
    pub kernel: Option<String>,
}

/// Form input for creating/updating worker pools
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerPoolInput {
    /// Pool name
    pub name: String,
    /// Provider name
    pub provider: String,
    /// Minimum number of replicas
    pub min_replicas: i32,
    /// Maximum number of replicas
    pub max_replicas: i32,
    /// Worker image
    pub image: String,
    /// CPU request
    pub cpu_request: Option<String>,
    /// CPU limit
    pub cpu_limit: Option<String>,
    /// Memory request
    pub memory_request: Option<String>,
    /// Memory limit
    pub memory_limit: Option<String>,
}

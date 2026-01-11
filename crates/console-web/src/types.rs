//! Shared types for the Hodei Operator Dashboard

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider types supported by Hodei
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderType {
    #[serde(rename = "docker")]
    Docker,
    #[serde(rename = "kubernetes")]
    Kubernetes,
    #[serde(rename = "firecracker")]
    Firecracker,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub name: String,
    pub namespace: String,
    pub provider_type: ProviderType,
    pub enabled: bool,
    pub config: ProviderConfigDetail,
    pub created_at: String,
}

/// Provider configuration details (union type based on provider)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProviderConfigDetail {
    #[serde(default)]
    pub docker: Option<DockerConfig>,
    #[serde(default)]
    pub kubernetes: Option<KubernetesConfig>,
    #[serde(default)]
    pub firecracker: Option<FirecrackerConfig>,
}

/// Docker provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerConfig {
    pub host: String,
    pub network: Option<String>,
    pub registry: Option<RegistryConfig>,
}

/// Kubernetes provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KubernetesConfig {
    pub namespace: String,
    pub service_account: Option<String>,
    pub tolerations: Option<Vec<Toleration>>,
    pub node_selector: Option<HashMap<String, String>>,
}

/// Firecracker provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirecrackerConfig {
    pub socket_path: String,
    pub kernel: Option<String>,
    pub initrd: Option<String>,
}

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryConfig {
    pub server: String,
    pub username: Option<String>,
    pub password_secret_ref: Option<String>,
}

/// Kubernetes toleration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Toleration {
    pub key: String,
    pub operator: String,
    pub value: Option<String>,
    pub effect: String,
}

/// Worker Pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerPool {
    pub name: String,
    pub namespace: String,
    pub provider: String,
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub current_replicas: i32,
    pub image: String,
    pub resources: ResourceRequirements,
    pub config: Option<PoolConfig>,
    pub created_at: String,
}

/// Resource requirements for a worker pool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceRequirements {
    pub requests: Option<ResourceSpec>,
    pub limits: Option<ResourceSpec>,
}

/// Resource specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceSpec {
    pub memory: Option<String>,
    pub cpu: Option<String>,
}

/// Pool-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PoolConfig {
    pub kubernetes: Option<KubernetesPoolConfig>,
    pub docker: Option<DockerPoolConfig>,
}

/// Kubernetes pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KubernetesPoolConfig {
    pub namespace: Option<String>,
    pub service_account: Option<String>,
    pub tolerations: Option<Vec<Toleration>>,
    pub node_selector: Option<HashMap<String, String>>,
    pub affinity: Option<serde_json::Value>,
}

/// Docker pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DockerPoolConfig {
    pub network: Option<String>,
    pub volumes: Option<Vec<Volume>>,
}

/// Docker volume mount
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Volume {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

/// Operator settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorSettings {
    pub namespace: String,
    pub watch_namespace: String,
    pub server_address: String,
    pub log_level: LogLevel,
    pub health_check_interval: i32,
}

/// Log levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    #[serde(rename = "trace")]
    Trace,
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "error")]
    Error,
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

/// Job information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub command: String,
    pub arguments: Option<Vec<String>>,
    pub status: JobStatus,
    pub provider: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Dashboard statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardStats {
    pub total_jobs: u32,
    pub running_jobs: u32,
    pub completed_jobs: u32,
    pub failed_jobs: u32,
    pub total_pools: u32,
    pub active_pools: u32,
    pub total_providers: u32,
    pub enabled_providers: u32,
}

/// Notification for user feedback
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub kind: NotificationKind,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotificationKind {
    Success,
    Error,
    Warning,
    Info,
}

/// Form input for creating/updating providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderInput {
    pub name: String,
    pub provider_type: String,
    pub enabled: bool,
    pub docker_config: Option<DockerConfigInput>,
    pub kubernetes_config: Option<KubernetesConfigInput>,
    pub firecracker_config: Option<FirecrackerConfigInput>,
}

/// Docker config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockerConfigInput {
    pub host: String,
    pub network: Option<String>,
    pub registry_server: Option<String>,
    pub registry_username: Option<String>,
}

/// Kubernetes config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubernetesConfigInput {
    pub namespace: String,
    pub service_account: Option<String>,
}

/// Firecracker config input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirecrackerConfigInput {
    pub socket_path: String,
    pub kernel: Option<String>,
}

/// Form input for creating/updating worker pools
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerPoolInput {
    pub name: String,
    pub provider: String,
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub image: String,
    pub cpu_request: Option<String>,
    pub cpu_limit: Option<String>,
    pub memory_request: Option<String>,
    pub memory_limit: Option<String>,
}

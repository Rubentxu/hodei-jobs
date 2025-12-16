//! Kubernetes Worker Provider Implementation
//!
//! Production-ready implementation of WorkerProvider using Kubernetes Pods.
//! Uses kube-rs for native Kubernetes API interaction.

use async_trait::async_trait;
use k8s_openapi::api::core::v1::{
    Container, EnvVar, Pod, PodSpec, ResourceRequirements as K8sResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::{
    Client, Config,
    api::{Api, DeleteParams, ListParams, LogParams, PostParams},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tracing::{debug, info};

use hodei_jobs_domain::{
    shared_kernel::{DomainError, ProviderId, Result, WorkerId, WorkerState},
    worker::{Architecture, ProviderType, WorkerHandle, WorkerSpec},
    worker_provider::{
        HealthStatus, LogEntry, LogLevel, ProviderCapabilities, ProviderError, ResourceLimits,
        WorkerProvider,
    },
};

// ============================================================================
// Configuration Types (HU-7.1)
// ============================================================================

/// Kubernetes toleration for pod scheduling
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubernetesToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
    pub toleration_seconds: Option<i64>,
}

impl KubernetesToleration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn with_operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_effect(mut self, effect: impl Into<String>) -> Self {
        self.effect = Some(effect.into());
        self
    }
}

/// Configuration for the Kubernetes Provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Namespace where worker Pods will be created
    pub namespace: String,
    /// Path to kubeconfig file (None = in-cluster config)
    pub kubeconfig_path: Option<String>,
    /// Kubeconfig context to use (None = current-context)
    pub context: Option<String>,
    /// Service account for worker Pods
    pub service_account: Option<String>,
    /// Base labels applied to all worker Pods
    pub base_labels: HashMap<String, String>,
    /// Base annotations applied to all worker Pods
    pub base_annotations: HashMap<String, String>,
    /// Node selector for Pod scheduling
    pub node_selector: HashMap<String, String>,
    /// Tolerations for Pod scheduling
    pub tolerations: Vec<KubernetesToleration>,
    /// Image pull secrets for private registries
    pub image_pull_secrets: Vec<String>,
    /// Default CPU request (e.g., "100m")
    pub default_cpu_request: String,
    /// Default memory request (e.g., "128Mi")
    pub default_memory_request: String,
    /// Default CPU limit (e.g., "1000m")
    pub default_cpu_limit: String,
    /// Default memory limit (e.g., "512Mi")
    pub default_memory_limit: String,
    /// TTL seconds after Pod finishes (for cleanup)
    pub ttl_seconds_after_finished: Option<i32>,
    /// Timeout for Pod creation (seconds)
    pub creation_timeout_secs: u64,
}

impl Default for KubernetesConfig {
    fn default() -> Self {
        let mut base_labels = HashMap::new();
        base_labels.insert("app".to_string(), "hodei-jobs-worker".to_string());
        base_labels.insert("hodei.io/managed".to_string(), "true".to_string());

        Self {
            namespace: "hodei-jobs-workers".to_string(),
            kubeconfig_path: None,
            context: None,
            service_account: Some("hodei-jobs-worker".to_string()),
            base_labels,
            base_annotations: HashMap::new(),
            node_selector: HashMap::new(),
            tolerations: Vec::new(),
            image_pull_secrets: Vec::new(),
            default_cpu_request: "100m".to_string(),
            default_memory_request: "128Mi".to_string(),
            default_cpu_limit: "1000m".to_string(),
            default_memory_limit: "512Mi".to_string(),
            ttl_seconds_after_finished: Some(300),
            creation_timeout_secs: 60,
        }
    }
}

/// Builder for KubernetesConfig
pub struct KubernetesConfigBuilder {
    config: KubernetesConfig,
}

impl KubernetesConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: KubernetesConfig::default(),
        }
    }

    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.config.namespace = namespace.into();
        self
    }

    pub fn kubeconfig_path(mut self, path: impl Into<String>) -> Self {
        self.config.kubeconfig_path = Some(path.into());
        self
    }

    pub fn context(mut self, context: impl Into<String>) -> Self {
        self.config.context = Some(context.into());
        self
    }

    pub fn service_account(mut self, sa: impl Into<String>) -> Self {
        self.config.service_account = Some(sa.into());
        self
    }

    pub fn add_base_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.base_labels.insert(key.into(), value.into());
        self
    }

    pub fn add_base_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config
            .base_annotations
            .insert(key.into(), value.into());
        self
    }

    pub fn add_node_selector(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.node_selector.insert(key.into(), value.into());
        self
    }

    pub fn add_toleration(mut self, toleration: KubernetesToleration) -> Self {
        self.config.tolerations.push(toleration);
        self
    }

    pub fn add_image_pull_secret(mut self, secret: impl Into<String>) -> Self {
        self.config.image_pull_secrets.push(secret.into());
        self
    }

    pub fn default_cpu_request(mut self, cpu: impl Into<String>) -> Self {
        self.config.default_cpu_request = cpu.into();
        self
    }

    pub fn default_memory_request(mut self, memory: impl Into<String>) -> Self {
        self.config.default_memory_request = memory.into();
        self
    }

    pub fn default_cpu_limit(mut self, cpu: impl Into<String>) -> Self {
        self.config.default_cpu_limit = cpu.into();
        self
    }

    pub fn default_memory_limit(mut self, memory: impl Into<String>) -> Self {
        self.config.default_memory_limit = memory.into();
        self
    }

    pub fn ttl_seconds_after_finished(mut self, ttl: i32) -> Self {
        self.config.ttl_seconds_after_finished = Some(ttl);
        self
    }

    pub fn creation_timeout_secs(mut self, timeout: u64) -> Self {
        self.config.creation_timeout_secs = timeout;
        self
    }

    /// Build the configuration, validating required fields
    pub fn build(self) -> Result<KubernetesConfig> {
        self.validate()?;
        Ok(self.config)
    }

    fn validate(&self) -> Result<()> {
        if self.config.namespace.is_empty() {
            return Err(DomainError::InvalidProviderConfig {
                message: "Kubernetes namespace cannot be empty".to_string(),
            });
        }
        Ok(())
    }
}

impl Default for KubernetesConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl KubernetesConfig {
    /// Create a new builder
    pub fn builder() -> KubernetesConfigBuilder {
        KubernetesConfigBuilder::new()
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut builder = KubernetesConfigBuilder::new();

        if let Ok(namespace) = std::env::var("HODEI_K8S_NAMESPACE") {
            builder = builder.namespace(namespace);
        }

        if let Ok(kubeconfig) = std::env::var("HODEI_K8S_KUBECONFIG") {
            builder = builder.kubeconfig_path(kubeconfig);
        }

        if let Ok(context) = std::env::var("HODEI_K8S_CONTEXT") {
            builder = builder.context(context);
        }

        if let Ok(sa) = std::env::var("HODEI_K8S_SERVICE_ACCOUNT") {
            builder = builder.service_account(sa);
        }

        if let Ok(secret) = std::env::var("HODEI_K8S_IMAGE_PULL_SECRET") {
            builder = builder.add_image_pull_secret(secret);
        }

        if let Ok(cpu) = std::env::var("HODEI_K8S_DEFAULT_CPU_REQUEST") {
            builder = builder.default_cpu_request(cpu);
        }

        if let Ok(memory) = std::env::var("HODEI_K8S_DEFAULT_MEMORY_REQUEST") {
            builder = builder.default_memory_request(memory);
        }

        builder.build()
    }
}

// ============================================================================
// Kubernetes Provider (HU-7.2+)
// ============================================================================

/// Kubernetes Provider for creating ephemeral worker Pods
#[derive(Clone)]
pub struct KubernetesProvider {
    provider_id: ProviderId,
    client: Client,
    config: KubernetesConfig,
    capabilities: ProviderCapabilities,
}

impl KubernetesProvider {
    /// Create a new KubernetesProvider with in-cluster configuration
    pub async fn new() -> Result<Self> {
        let config = KubernetesConfig::default();
        Self::with_config(config).await
    }

    /// Create a new KubernetesProvider with custom configuration
    pub async fn with_config(config: KubernetesConfig) -> Result<Self> {
        let client = Self::create_client(&config).await?;
        let capabilities = Self::default_capabilities();

        Ok(Self {
            provider_id: ProviderId::new(),
            client,
            config,
            capabilities,
        })
    }

    /// Create a new KubernetesProvider with a specific provider ID
    pub async fn with_provider_id(
        provider_id: ProviderId,
        config: KubernetesConfig,
    ) -> Result<Self> {
        let client = Self::create_client(&config).await?;
        let capabilities = Self::default_capabilities();

        Ok(Self {
            provider_id,
            client,
            config,
            capabilities,
        })
    }

    async fn create_client(config: &KubernetesConfig) -> Result<Client> {
        let kube_config = match (&config.kubeconfig_path, &config.context) {
            (Some(path), Some(ctx)) => {
                let options = kube::config::KubeConfigOptions {
                    context: Some(ctx.clone()),
                    cluster: None,
                    user: None,
                };
                let kubeconfig = kube::config::Kubeconfig::read_from(path).map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to read kubeconfig from {}: {}", path, e),
                    }
                })?;
                Config::from_custom_kubeconfig(kubeconfig, &options)
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create Kubernetes config: {}", e),
                    })?
            }
            (Some(path), None) => {
                let kubeconfig = kube::config::Kubeconfig::read_from(path).map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to read kubeconfig from {}: {}", path, e),
                    }
                })?;
                Config::from_custom_kubeconfig(
                    kubeconfig,
                    &kube::config::KubeConfigOptions::default(),
                )
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to create Kubernetes config: {}", e),
                })?
            }
            (None, _) => Config::infer()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to infer Kubernetes config: {}", e),
                })?,
        };

        Client::try_from(kube_config).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create Kubernetes client: {}", e),
        })
    }

    fn default_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            max_resources: ResourceLimits {
                max_cpu_cores: 64.0,
                max_memory_bytes: 256 * 1024 * 1024 * 1024, // 256GB
                max_disk_bytes: 1024 * 1024 * 1024 * 1024,  // 1TB
                max_gpu_count: 8,
            },
            gpu_support: true,
            gpu_types: vec![
                "nvidia-tesla-v100".to_string(),
                "nvidia-tesla-t4".to_string(),
            ],
            architectures: vec![Architecture::Amd64, Architecture::Arm64],
            runtimes: vec![
                "shell".to_string(),
                "python".to_string(),
                "node".to_string(),
            ],
            regions: vec!["default".to_string()],
            max_execution_time: Some(Duration::from_secs(86400)), // 24 hours
            persistent_storage: true,
            custom_networking: true,
            features: HashMap::new(),
        }
    }

    /// Generate Pod name from worker ID
    fn pod_name(worker_id: &WorkerId) -> String {
        format!("hodei-worker-{}", worker_id)
    }

    /// Create Pod spec from WorkerSpec
    fn create_pod_spec(&self, spec: &WorkerSpec, otp_token: Option<&str>) -> Pod {
        let pod_name = Self::pod_name(&spec.worker_id);

        // Build labels
        let mut labels: BTreeMap<String, String> = self
            .config
            .base_labels
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        labels.insert("hodei.io/worker-id".to_string(), spec.worker_id.to_string());
        labels.insert(
            "hodei.io/provider-id".to_string(),
            self.provider_id.to_string(),
        );

        // Add spec labels
        for (k, v) in &spec.labels {
            labels.insert(k.clone(), v.clone());
        }

        // Build annotations
        let annotations: BTreeMap<String, String> = self
            .config
            .base_annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Build environment variables
        let mut env_vars = vec![
            EnvVar {
                name: "HODEI_WORKER_ID".to_string(),
                value: Some(spec.worker_id.to_string()),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_SERVER_ADDRESS".to_string(),
                value: Some(spec.server_address.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_WORKER_MODE".to_string(),
                value: Some("ephemeral".to_string()),
                ..Default::default()
            },
        ];

        if let Some(token) = otp_token {
            env_vars.push(EnvVar {
                name: "HODEI_OTP_TOKEN".to_string(),
                value: Some(token.to_string()),
                ..Default::default()
            });
        }

        // Add spec environment variables
        for (k, v) in &spec.environment {
            env_vars.push(EnvVar {
                name: k.clone(),
                value: Some(v.clone()),
                ..Default::default()
            });
        }

        // Build resource requirements
        let mut requests = BTreeMap::new();
        let mut limits = BTreeMap::new();

        // CPU
        let cpu_cores = spec.resources.cpu_cores;
        let cpu_request = if cpu_cores > 0.0 {
            format!("{}m", (cpu_cores * 1000.0) as i64)
        } else {
            self.config.default_cpu_request.clone()
        };
        let cpu_limit = if cpu_cores > 0.0 {
            format!("{}m", (cpu_cores * 1000.0) as i64)
        } else {
            self.config.default_cpu_limit.clone()
        };
        requests.insert("cpu".to_string(), Quantity(cpu_request));
        limits.insert("cpu".to_string(), Quantity(cpu_limit));

        // Memory
        let memory_bytes = spec.resources.memory_bytes;
        let memory_request = if memory_bytes > 0 {
            format!("{}Mi", memory_bytes / (1024 * 1024))
        } else {
            self.config.default_memory_request.clone()
        };
        let memory_limit = if memory_bytes > 0 {
            format!("{}Mi", memory_bytes / (1024 * 1024))
        } else {
            self.config.default_memory_limit.clone()
        };
        requests.insert("memory".to_string(), Quantity(memory_request));
        limits.insert("memory".to_string(), Quantity(memory_limit));

        let resources = K8sResourceRequirements {
            claims: None,
            requests: Some(requests),
            limits: Some(limits),
        };

        // Build node selector
        let node_selector: Option<BTreeMap<String, String>> =
            if self.config.node_selector.is_empty() {
                None
            } else {
                Some(
                    self.config
                        .node_selector
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                )
            };

        // Build tolerations
        let tolerations: Option<Vec<k8s_openapi::api::core::v1::Toleration>> =
            if self.config.tolerations.is_empty() {
                None
            } else {
                Some(
                    self.config
                        .tolerations
                        .iter()
                        .map(|t| k8s_openapi::api::core::v1::Toleration {
                            key: t.key.clone(),
                            operator: t.operator.clone(),
                            value: t.value.clone(),
                            effect: t.effect.clone(),
                            toleration_seconds: t.toleration_seconds,
                        })
                        .collect(),
                )
            };

        // Build image pull secrets
        let image_pull_secrets: Option<Vec<k8s_openapi::api::core::v1::LocalObjectReference>> =
            if self.config.image_pull_secrets.is_empty() {
                None
            } else {
                Some(
                    self.config
                        .image_pull_secrets
                        .iter()
                        .map(|s| k8s_openapi::api::core::v1::LocalObjectReference {
                            name: s.clone(),
                        })
                        .collect(),
                )
            };

        // Build container
        let container = Container {
            name: "worker".to_string(),
            image: Some(spec.image.clone()),
            env: Some(env_vars),
            resources: Some(resources),
            ..Default::default()
        };

        // Build Pod
        Pod {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(pod_name),
                namespace: Some(self.config.namespace.clone()),
                labels: Some(labels),
                annotations: Some(annotations),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![container],
                restart_policy: Some("Never".to_string()),
                service_account_name: self.config.service_account.clone(),
                node_selector,
                tolerations,
                image_pull_secrets,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Map Kubernetes Pod phase to WorkerState
    fn map_pod_phase(phase: Option<&str>, container_ready: bool) -> WorkerState {
        match phase {
            Some("Pending") => WorkerState::Creating,
            Some("Running") => {
                if container_ready {
                    WorkerState::Ready
                } else {
                    WorkerState::Connecting
                }
            }
            Some("Succeeded") => WorkerState::Terminated,
            Some("Failed") => WorkerState::Terminated,
            Some("Unknown") | None => WorkerState::Creating,
            _ => WorkerState::Creating,
        }
    }

    /// Check if container is ready
    fn is_container_ready(pod: &Pod) -> bool {
        pod.status
            .as_ref()
            .and_then(|s| s.container_statuses.as_ref())
            .map(|statuses| statuses.iter().any(|cs| cs.ready))
            .unwrap_or(false)
    }
}

#[async_trait]
impl WorkerProvider for KubernetesProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Kubernetes
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        info!("Creating Kubernetes worker Pod: {}", worker_id);

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let pod_name = Self::pod_name(&worker_id);

        // Check if Pod already exists
        if pods
            .get_opt(&pod_name)
            .await
            .map_err(|e| {
                ProviderError::ProvisioningFailed(format!("Failed to check existing Pod: {}", e))
            })?
            .is_some()
        {
            return Err(ProviderError::ProvisioningFailed(format!(
                "Pod {} already exists",
                pod_name
            )));
        }

        // Get OTP token from spec environment if present
        let otp_token = spec.environment.get("HODEI_OTP_TOKEN").map(|s| s.as_str());

        // Create Pod spec
        let pod = self.create_pod_spec(spec, otp_token);

        // Create Pod
        let created_pod = pods
            .create(&PostParams::default(), &pod)
            .await
            .map_err(|e| {
                ProviderError::ProvisioningFailed(format!("Failed to create Pod: {}", e))
            })?;

        let pod_uid = created_pod
            .metadata
            .uid
            .clone()
            .unwrap_or_else(|| pod_name.clone());

        info!(
            "Worker Pod {} created successfully (uid: {})",
            pod_name, pod_uid
        );

        let handle = WorkerHandle::new(
            worker_id,
            pod_name.clone(),
            ProviderType::Kubernetes,
            self.provider_id.clone(),
        )
        .with_metadata(
            "namespace",
            serde_json::json!(self.config.namespace.clone()),
        )
        .with_metadata("pod_uid", serde_json::json!(pod_uid));

        Ok(handle)
    }

    async fn get_worker_status(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<WorkerState, ProviderError> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let pod_name = &handle.provider_resource_id;

        let pod = pods.get_opt(pod_name).await.map_err(|e| {
            ProviderError::WorkerNotFound(format!("Failed to get Pod {}: {}", pod_name, e))
        })?;

        match pod {
            Some(p) => {
                let phase = p.status.as_ref().and_then(|s| s.phase.as_deref());
                let container_ready = Self::is_container_ready(&p);
                Ok(Self::map_pod_phase(phase, container_ready))
            }
            None => Ok(WorkerState::Terminated),
        }
    }

    async fn destroy_worker(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<(), ProviderError> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let pod_name = &handle.provider_resource_id;

        info!("Destroying Kubernetes worker Pod: {}", pod_name);

        // Delete with grace period
        let dp = DeleteParams {
            grace_period_seconds: Some(30),
            ..Default::default()
        };

        match pods.delete(pod_name, &dp).await {
            Ok(_) => {
                info!("Worker Pod {} deleted successfully", pod_name);
                Ok(())
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                debug!("Pod {} already deleted", pod_name);
                Ok(())
            }
            Err(e) => Err(ProviderError::Internal(format!(
                "Failed to delete Pod {}: {}",
                pod_name, e
            ))),
        }
    }

    async fn get_worker_logs(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> std::result::Result<Vec<LogEntry>, ProviderError> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let pod_name = &handle.provider_resource_id;

        let mut lp = LogParams::default();
        if let Some(lines) = tail {
            lp.tail_lines = Some(lines as i64);
        }
        lp.timestamps = true;

        let logs = pods.logs(pod_name, &lp).await.map_err(|e| {
            ProviderError::Internal(format!("Failed to get logs for Pod {}: {}", pod_name, e))
        })?;

        let entries: Vec<LogEntry> = logs
            .lines()
            .map(|line| {
                let (timestamp, message) = parse_k8s_log_line(line);
                LogEntry {
                    timestamp,
                    level: LogLevel::Info,
                    message,
                    source: "container".to_string(),
                }
            })
            .collect();

        Ok(entries)
    }

    async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
        // Try to list pods in the namespace to verify connectivity
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);

        match pods.list(&ListParams::default().limit(1)).await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(kube::Error::Api(ae)) if ae.code == 403 => Ok(HealthStatus::Degraded {
                reason: format!(
                    "Insufficient permissions in namespace {}",
                    self.config.namespace
                ),
            }),
            Err(e) => Ok(HealthStatus::Unhealthy {
                reason: format!("Failed to connect to Kubernetes API: {}", e),
            }),
        }
    }

    fn estimated_startup_time(&self) -> Duration {
        Duration::from_secs(15)
    }
}

/// Parse Kubernetes log line with timestamp
fn parse_k8s_log_line(line: &str) -> (chrono::DateTime<chrono::Utc>, String) {
    // K8s log format: "2024-01-01T12:00:00.000000000Z message"
    if line.len() > 30 && line.chars().nth(4) == Some('-') {
        let (timestamp_str, message) = line.split_at(30);
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(timestamp_str.trim()) {
            return (ts.with_timezone(&chrono::Utc), message.trim().to_string());
        }
    }
    (chrono::Utc::now(), line.to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kubernetes_config_default() {
        let config = KubernetesConfig::default();
        assert_eq!(config.namespace, "hodei-jobs-workers");
        assert!(config.kubeconfig_path.is_none());
        assert_eq!(
            config.service_account,
            Some("hodei-jobs-worker".to_string())
        );
        assert!(config.base_labels.contains_key("app"));
    }

    #[test]
    fn test_kubernetes_config_builder() {
        let config = KubernetesConfig::builder()
            .namespace("custom-namespace")
            .kubeconfig_path("/path/to/kubeconfig")
            .context("my-context")
            .service_account("custom-sa")
            .add_base_label("team", "platform")
            .add_node_selector("node-type", "worker")
            .add_image_pull_secret("my-secret")
            .default_cpu_request("200m")
            .default_memory_request("256Mi")
            .build()
            .expect("should build config");

        assert_eq!(config.namespace, "custom-namespace");
        assert_eq!(
            config.kubeconfig_path,
            Some("/path/to/kubeconfig".to_string())
        );
        assert_eq!(config.context, Some("my-context".to_string()));
        assert_eq!(config.service_account, Some("custom-sa".to_string()));
        assert_eq!(
            config.base_labels.get("team"),
            Some(&"platform".to_string())
        );
        assert_eq!(
            config.node_selector.get("node-type"),
            Some(&"worker".to_string())
        );
        assert!(config.image_pull_secrets.contains(&"my-secret".to_string()));
        assert_eq!(config.default_cpu_request, "200m");
        assert_eq!(config.default_memory_request, "256Mi");
    }

    #[test]
    fn test_kubernetes_config_builder_validation() {
        let result = KubernetesConfig::builder().namespace("").build();

        assert!(result.is_err());
    }

    #[test]
    fn test_toleration_builder() {
        let toleration = KubernetesToleration::new()
            .with_key("dedicated")
            .with_operator("Equal")
            .with_value("hodei")
            .with_effect("NoSchedule");

        assert_eq!(toleration.key, Some("dedicated".to_string()));
        assert_eq!(toleration.operator, Some("Equal".to_string()));
        assert_eq!(toleration.value, Some("hodei".to_string()));
        assert_eq!(toleration.effect, Some("NoSchedule".to_string()));
    }

    #[test]
    fn test_pod_name_generation() {
        let worker_id = WorkerId::new();
        let pod_name = KubernetesProvider::pod_name(&worker_id);
        assert!(pod_name.starts_with("hodei-worker-"));
    }

    #[test]
    fn test_map_pod_phase() {
        assert!(matches!(
            KubernetesProvider::map_pod_phase(Some("Pending"), false),
            WorkerState::Creating
        ));
        assert!(matches!(
            KubernetesProvider::map_pod_phase(Some("Running"), true),
            WorkerState::Ready
        ));
        assert!(matches!(
            KubernetesProvider::map_pod_phase(Some("Running"), false),
            WorkerState::Connecting
        ));
        assert!(matches!(
            KubernetesProvider::map_pod_phase(Some("Succeeded"), false),
            WorkerState::Terminated
        ));
        assert!(matches!(
            KubernetesProvider::map_pod_phase(Some("Failed"), false),
            WorkerState::Terminated
        ));
        assert!(matches!(
            KubernetesProvider::map_pod_phase(None, false),
            WorkerState::Creating
        ));
    }

    #[test]
    fn test_parse_k8s_log_line() {
        use chrono::Datelike;
        let line = "2024-01-15T10:30:00.123456789Z This is a log message";
        let (timestamp, message) = parse_k8s_log_line(line);
        assert_eq!(message, "This is a log message");
        assert_eq!(timestamp.year(), 2024);
    }

    #[test]
    fn test_parse_k8s_log_line_no_timestamp() {
        let line = "Just a plain message";
        let (_timestamp, message) = parse_k8s_log_line(line);
        assert_eq!(message, "Just a plain message");
    }

    #[test]
    fn test_default_capabilities() {
        let caps = KubernetesProvider::default_capabilities();
        assert!(caps.gpu_support);
        assert!(caps.architectures.contains(&Architecture::Amd64));
        assert!(caps.architectures.contains(&Architecture::Arm64));
        assert!(caps.max_resources.max_cpu_cores > 0.0);
    }
}

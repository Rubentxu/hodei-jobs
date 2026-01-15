//! Kubernetes Worker Provider Implementation
//!
//! Production-ready implementation of WorkerProvider using Kubernetes Pods.
//! Uses kube-rs for native Kubernetes API interaction.
//!
//! Architecture: Pod construction delegated to PodSpecFactory (GAP-GO-04).

use async_trait::async_trait;
use futures::Stream;
use k8s_openapi::api::core::v1::{
    Container, EnvVar, Pod, ResourceRequirements as K8sResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::{
    Client, Config,
    api::{Api, DeleteParams, ListParams, LogParams, PostParams},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::providers::metrics_collector::ProviderMetricsCollector;
use crate::providers::pod_spec_factory::PodSpecFactory;
use hodei_server_domain::{
    shared_kernel::{DomainError, ProviderId, Result, WorkerId, WorkerState},
    workers::{
        Architecture, CostEstimate, GpuModel, GpuVendor, HealthStatus, JobRequirements, LogEntry,
        LogLevel, ProviderCapabilities, ProviderError, ProviderFeature, ProviderPerformanceMetrics,
        ProviderType, ResourceLimits, WorkerCost, WorkerEligibility, WorkerEventSource,
        WorkerHandle, WorkerHealth, WorkerInfrastructureEvent, WorkerLifecycle, WorkerLogs,
        WorkerMetrics, WorkerProvider, WorkerProviderIdentity, WorkerSpec,
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

/// Pod affinity rule for scheduling
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PodAffinityRule {
    pub label_selector: BTreeMap<String, String>,
    pub topology_key: String,
    pub weight: i32,
}

impl PodAffinityRule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_selector.insert(key.into(), value.into());
        self
    }

    pub fn with_topology_key(mut self, key: impl Into<String>) -> Self {
        self.topology_key = key.into();
        self
    }

    pub fn with_weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }
}

/// Pod anti-affinity rule for scheduling
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PodAntiAffinityRule {
    pub label_selector: BTreeMap<String, String>,
    pub topology_key: String,
    pub weight: i32,
}

impl PodAntiAffinityRule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_selector.insert(key.into(), value.into());
        self
    }

    pub fn with_topology_key(mut self, key: impl Into<String>) -> Self {
        self.topology_key = key.into();
        self
    }

    pub fn with_weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }
}

/// Pod Security Standard level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PodSecurityStandard {
    Restricted,
    Baseline,
    Privileged,
}

impl Default for PodSecurityStandard {
    fn default() -> Self {
        Self::Restricted
    }
}

impl std::fmt::Display for PodSecurityStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Restricted => write!(f, "restricted"),
            Self::Baseline => write!(f, "baseline"),
            Self::Privileged => write!(f, "privileged"),
        }
    }
}

/// Security Context configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityContextConfig {
    pub run_as_non_root: bool,
    pub run_as_user: Option<i64>,
    pub run_as_group: Option<i64>,
    pub read_only_root_filesystem: bool,
    pub allow_privilege_escalation: bool,
    pub drop_capabilities: Vec<String>,
    pub add_capabilities: Vec<String>,
    pub seccomp_profile_type: Option<String>,
}

impl SecurityContextConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run_as_non_root(mut self) -> Self {
        self.run_as_non_root = true;
        self
    }

    pub fn run_as_user(mut self, uid: i64) -> Self {
        self.run_as_user = Some(uid);
        self
    }

    pub fn read_only_root_fs(mut self) -> Self {
        self.read_only_root_filesystem = true;
        self
    }

    pub fn drop_capability(mut self, capability: impl Into<String>) -> Self {
        self.drop_capabilities.push(capability.into());
        self
    }

    pub fn add_capability(mut self, capability: impl Into<String>) -> Self {
        self.add_capabilities.push(capability.into());
        self
    }

    pub fn seccomp_profile(mut self, profile: impl Into<String>) -> Self {
        self.seccomp_profile_type = Some(profile.into());
        self
    }
}

/// Configuration for the Kubernetes Provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Namespace where worker Pods will be created (default)
    pub namespace: String,
    /// Allow dynamic namespace creation per tenant
    pub enable_dynamic_namespaces: bool,
    /// Default namespace prefix for tenant-specific namespaces
    pub namespace_prefix: String,
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
    /// Pod affinity rules for co-location
    pub pod_affinity: Option<Vec<PodAffinityRule>>,
    /// Pod anti-affinity rules for spread
    pub pod_anti_affinity: Option<Vec<PodAntiAffinityRule>>,
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
    /// Pod Security Standard level (restricted/baseline/privileged)
    pub pod_security_standard: PodSecurityStandard,
    /// Security Context configuration
    pub security_context: SecurityContextConfig,
    /// Enable Horizontal Pod Autoscaler for workers
    pub enable_hpa: bool,
    /// HPA configuration for auto-scaling
    pub hpa_config: super::kubernetes_hpa::HPAConfig,
}

impl Default for KubernetesConfig {
    fn default() -> Self {
        let mut base_labels = HashMap::new();
        base_labels.insert("app".to_string(), "hodei-jobs-worker".to_string());
        base_labels.insert("hodei.io/managed".to_string(), "true".to_string());

        let security_context = SecurityContextConfig {
            run_as_non_root: true,
            run_as_user: Some(1000),
            run_as_group: Some(1000),
            read_only_root_filesystem: true,
            allow_privilege_escalation: false,
            drop_capabilities: vec!["ALL".to_string()],
            add_capabilities: Vec::new(),
            seccomp_profile_type: Some("RuntimeDefault".to_string()),
        };

        Self {
            namespace: "hodei-jobs-workers".to_string(),
            enable_dynamic_namespaces: false,
            namespace_prefix: "hodei-tenant".to_string(),
            kubeconfig_path: None,
            context: None,
            service_account: Some("hodei-jobs-worker".to_string()),
            base_labels,
            base_annotations: HashMap::new(),
            node_selector: HashMap::new(),
            tolerations: Vec::new(),
            pod_affinity: None,
            pod_anti_affinity: None,
            image_pull_secrets: Vec::new(),
            default_cpu_request: "100m".to_string(),
            default_memory_request: "128Mi".to_string(),
            default_cpu_limit: "1000m".to_string(),
            default_memory_limit: "512Mi".to_string(),
            ttl_seconds_after_finished: Some(300),
            creation_timeout_secs: 60,
            pod_security_standard: PodSecurityStandard::Restricted,
            security_context,
            enable_hpa: false,
            hpa_config: super::kubernetes_hpa::HPAConfig::default(),
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

    pub fn add_pod_affinity(mut self, affinity: PodAffinityRule) -> Self {
        if let Some(ref mut affinities) = self.config.pod_affinity {
            affinities.push(affinity);
        } else {
            self.config.pod_affinity = Some(vec![affinity]);
        }
        self
    }

    pub fn add_pod_anti_affinity(mut self, anti_affinity: PodAntiAffinityRule) -> Self {
        if let Some(ref mut anti_affinities) = self.config.pod_anti_affinity {
            anti_affinities.push(anti_affinity);
        } else {
            self.config.pod_anti_affinity = Some(vec![anti_affinity]);
        }
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

    pub fn pod_security_standard(mut self, standard: PodSecurityStandard) -> Self {
        self.config.pod_security_standard = standard;
        self
    }

    pub fn security_context(mut self, context: SecurityContextConfig) -> Self {
        self.config.security_context = context;
        self
    }

    pub fn enable_dynamic_namespaces(mut self, enabled: bool) -> Self {
        self.config.enable_dynamic_namespaces = enabled;
        self
    }

    pub fn namespace_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.namespace_prefix = prefix.into();
        self
    }

    pub fn enable_hpa(mut self, enabled: bool) -> Self {
        self.config.enable_hpa = enabled;
        self
    }

    pub fn hpa_config(mut self, config: super::kubernetes_hpa::HPAConfig) -> Self {
        self.config.hpa_config = config;
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
// Kubernetes Provider Builder
// ============================================================================

/// Builder for KubernetesProvider
pub struct KubernetesProviderBuilder {
    provider_id: Option<ProviderId>,
    config: KubernetesConfig,
}

impl KubernetesProviderBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            provider_id: None,
            config: KubernetesConfig::default(),
        }
    }

    /// Set the provider ID
    pub fn with_provider_id(mut self, provider_id: ProviderId) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    /// Set the configuration
    pub fn with_config(mut self, config: KubernetesConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the KubernetesProvider
    pub async fn build(self) -> Result<KubernetesProvider> {
        let provider_id = self.provider_id.unwrap_or_else(ProviderId::new);
        KubernetesProvider::with_provider_id(provider_id, self.config).await
    }
}

impl Default for KubernetesProviderBuilder {
    fn default() -> Self {
        Self::new()
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
    /// Metrics collector for performance tracking
    metrics_collector: ProviderMetricsCollector,
    /// PodSpec factory for creating Pod specs (GAP-GO-04)
    pod_spec_factory: PodSpecFactory,
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
        let provider_id = ProviderId::new();
        let metrics_collector = ProviderMetricsCollector::new(provider_id.clone());
        let pod_spec_factory = PodSpecFactory::from_kubernetes_config(&config);

        Ok(Self {
            provider_id,
            client,
            config,
            capabilities,
            metrics_collector,
            pod_spec_factory,
        })
    }

    /// Create a new KubernetesProvider with a specific provider ID
    pub async fn with_provider_id(
        provider_id: ProviderId,
        config: KubernetesConfig,
    ) -> Result<Self> {
        let client = Self::create_client(&config).await?;
        let capabilities = Self::default_capabilities();
        let metrics_collector = ProviderMetricsCollector::new(provider_id.clone());
        let pod_spec_factory = PodSpecFactory::from_kubernetes_config(&config);

        Ok(Self {
            provider_id,
            client,
            config,
            capabilities,
            metrics_collector,
            pod_spec_factory,
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
        // Create typed GPU feature if supported
        let gpu_feature = if true {
            Some(ProviderFeature::Gpu {
                vendor: GpuVendor::Nvidia,
                models: vec![GpuModel::TeslaV100, GpuModel::TeslaT4],
                max_count: 8,
                cuda_support: true,
                rocm_support: false,
            })
        } else {
            None
        };

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
            features: gpu_feature.into_iter().collect(),
        }
    }

    /// Generate Pod name from worker ID
    fn pod_name(worker_id: &WorkerId) -> String {
        format!("hodei-worker-{}", worker_id)
    }

    /// Create a Pod spec using PodSpecFactory (GAP-GO-04)
    ///
    /// This method is kept for backward compatibility with tests.
    /// Production code should use `create_worker()` which internally uses the factory.
    pub fn create_pod_spec_with_namespace(
        &self,
        spec: &WorkerSpec,
        otp_token: Option<&str>,
        namespace: &str,
    ) -> Pod {
        let build_result = self.pod_spec_factory.build_pod(
            spec,
            otp_token,
            namespace,
            &self.provider_id.to_string(),
        );
        build_result.pod
    }

    /// Map Kubernetes Pod phase to WorkerState (Crash-Only Design)
    fn map_pod_phase(phase: Option<&str>, container_ready: bool) -> WorkerState {
        match phase {
            Some("Pending") => WorkerState::Creating,
            Some("Running") => {
                if container_ready {
                    WorkerState::Ready
                } else {
                    WorkerState::Creating // No Connecting state in Crash-Only Design
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

    /// Get GPU resource name based on GPU type (delegated to PodSpecFactory)
    fn get_gpu_resource_name(&self, gpu_type: &Option<String>) -> String {
        match gpu_type.as_deref() {
            Some("nvidia-tesla-v100") | Some("nvidia-tesla-t4") | Some("nvidia-tesla-a100") => {
                "nvidia.com/gpu".to_string()
            }
            Some("amd-mi100") | Some("amd-mi200") => "amd.com/gpu".to_string(),
            Some("intel-xe") => "intel.com/xe".to_string(),
            _ => "nvidia.com/gpu".to_string(),
        }
    }

    // =========================================================================
    // Legacy Pod construction methods - Now delegated to PodSpecFactory
    // See: crates/server/infrastructure/src/providers/pod_spec_factory.rs
    // The following methods were removed:
    // - build_node_selector_with_k8s_config (now in PodSpecFactory::build_node_selector)
    // - build_tolerations (now in PodSpecFactory::build_tolerations)
    // - build_volumes (now in PodSpecFactory::build_volumes)
    // - build_volume_mounts (now in PodSpecFactory::build_volume_mounts)
    // - build_affinity (now in PodSpecFactory::build_affinity)
    // - build_security_context (now in PodSpecFactory::build_security_context)
    // - build_init_containers (now in PodSpecFactory::build_init_containers)
    // - build_sidecar_containers (now in PodSpecFactory::build_sidecar_containers)
    // =========================================================================

    /// Determine namespace for a worker based on configuration and labels
    fn get_namespace_for_worker(&self, spec: &WorkerSpec) -> String {
        // Check if dynamic namespaces are enabled and there's a tenant label
        if self.config.enable_dynamic_namespaces {
            if let Some(tenant_id) = spec.labels.get("hodei.io/tenant-id") {
                return format!("{}-{}", self.config.namespace_prefix, tenant_id);
            }
        }

        // Default to configured namespace
        self.config.namespace.clone()
    }

    /// Ensure namespace exists, create if necessary
    async fn ensure_namespace_exists(&self, namespace: &str) -> Result<()> {
        if !self.config.enable_dynamic_namespaces || namespace == self.config.namespace {
            return Ok(());
        }

        let namespaces: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(self.client.clone());

        match namespaces.get_opt(namespace).await {
            Ok(Some(_)) => {
                // Namespace exists
                Ok(())
            }
            Ok(None) => {
                // Create namespace
                info!("Creating namespace: {}", namespace);
                let namespace_obj = k8s_openapi::api::core::v1::Namespace {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(namespace.to_string()),
                        labels: Some(BTreeMap::from([
                            ("hodei.io/managed".to_string(), "true".to_string()),
                            ("hodei.io/tenant-namespace".to_string(), "true".to_string()),
                        ])),
                        ..Default::default()
                    },
                    spec: Some(k8s_openapi::api::core::v1::NamespaceSpec { finalizers: None }),
                    status: None,
                };

                match namespaces
                    .create(&PostParams::default(), &namespace_obj)
                    .await
                {
                    Ok(_) => {
                        info!("Namespace {} created successfully", namespace);
                        Ok(())
                    }
                    Err(kube::Error::Api(ae)) if ae.code == 409 => {
                        // Namespace already exists (race condition)
                        info!("Namespace {} already exists", namespace);
                        Ok(())
                    }
                    Err(e) => Err(DomainError::InfrastructureError {
                        message: format!("Failed to create namespace {}: {}", namespace, e),
                    }),
                }
            }
            Err(e) => Err(DomainError::InfrastructureError {
                message: format!("Failed to check namespace {}: {}", namespace, e),
            }),
        }
    }

    /// Build init containers from Kubernetes config
    fn build_init_containers(&self, spec: &WorkerSpec) -> Vec<Container> {
        let mut containers = Vec::new();

        for k8s_container in &spec.kubernetes.init_containers {
            let env_vars: Vec<EnvVar> = k8s_container
                .env
                .iter()
                .map(|(k, v)| EnvVar {
                    name: k.clone(),
                    value: Some(v.clone()),
                    ..Default::default()
                })
                .collect();

            let volume_mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> = k8s_container
                .volume_mounts
                .iter()
                .map(|vm| k8s_openapi::api::core::v1::VolumeMount {
                    name: vm.name.clone(),
                    mount_path: vm.mount_path.clone(),
                    sub_path: vm.sub_path.clone(),
                    read_only: vm.read_only,
                    sub_path_expr: None,
                    mount_propagation: None,
                    recursive_read_only: None,
                })
                .collect();

            let resources = if let Some(ref req) = k8s_container.resources {
                let mut requests = BTreeMap::new();
                let mut limits = BTreeMap::new();

                if req.cpu_cores > 0.0 {
                    let cpu = format!("{}m", (req.cpu_cores * 1000.0) as i64);
                    requests.insert("cpu".to_string(), Quantity(cpu.clone()));
                    limits.insert("cpu".to_string(), Quantity(cpu));
                }

                if req.memory_bytes > 0 {
                    let memory = format!("{}Mi", req.memory_bytes / (1024 * 1024));
                    requests.insert("memory".to_string(), Quantity(memory.clone()));
                    limits.insert("memory".to_string(), Quantity(memory));
                }

                Some(K8sResourceRequirements {
                    claims: None,
                    requests: Some(requests),
                    limits: Some(limits),
                })
            } else {
                None
            };

            containers.push(Container {
                name: k8s_container.name.clone(),
                image: Some(k8s_container.image.clone()),
                command: if k8s_container.command.is_empty() {
                    None
                } else {
                    Some(k8s_container.command.clone())
                },
                args: if k8s_container.args.is_empty() {
                    None
                } else {
                    Some(k8s_container.args.clone())
                },
                env: if env_vars.is_empty() {
                    None
                } else {
                    Some(env_vars)
                },
                env_from: None, // TODO: Implement env_from support
                ports: None,    // TODO: Implement ports support
                resources,
                volume_mounts: if volume_mounts.is_empty() {
                    None
                } else {
                    Some(volume_mounts)
                },
                security_context: None, // TODO: Implement security context
                image_pull_policy: k8s_container.image_pull_policy.clone(),
                ..Default::default()
            });
        }

        containers
    }

    /// Build sidecar containers from Kubernetes config
    fn build_sidecar_containers(&self, spec: &WorkerSpec) -> Vec<Container> {
        let mut containers = Vec::new();

        for k8s_container in &spec.kubernetes.sidecar_containers {
            let env_vars: Vec<EnvVar> = k8s_container
                .env
                .iter()
                .map(|(k, v)| EnvVar {
                    name: k.clone(),
                    value: Some(v.clone()),
                    ..Default::default()
                })
                .collect();

            let volume_mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> = k8s_container
                .volume_mounts
                .iter()
                .map(|vm| k8s_openapi::api::core::v1::VolumeMount {
                    name: vm.name.clone(),
                    mount_path: vm.mount_path.clone(),
                    sub_path: vm.sub_path.clone(),
                    read_only: vm.read_only,
                    sub_path_expr: None,
                    mount_propagation: None,
                    recursive_read_only: None,
                })
                .collect();

            let resources = if let Some(ref req) = k8s_container.resources {
                let mut requests = BTreeMap::new();
                let mut limits = BTreeMap::new();

                if req.cpu_cores > 0.0 {
                    let cpu = format!("{}m", (req.cpu_cores * 1000.0) as i64);
                    requests.insert("cpu".to_string(), Quantity(cpu.clone()));
                    limits.insert("cpu".to_string(), Quantity(cpu));
                }

                if req.memory_bytes > 0 {
                    let memory = format!("{}Mi", req.memory_bytes / (1024 * 1024));
                    requests.insert("memory".to_string(), Quantity(memory.clone()));
                    limits.insert("memory".to_string(), Quantity(memory));
                }

                Some(K8sResourceRequirements {
                    claims: None,
                    requests: Some(requests),
                    limits: Some(limits),
                })
            } else {
                None
            };

            containers.push(Container {
                name: k8s_container.name.clone(),
                image: Some(k8s_container.image.clone()),
                command: if k8s_container.command.is_empty() {
                    None
                } else {
                    Some(k8s_container.command.clone())
                },
                args: if k8s_container.args.is_empty() {
                    None
                } else {
                    Some(k8s_container.args.clone())
                },
                env: if env_vars.is_empty() {
                    None
                } else {
                    Some(env_vars)
                },
                env_from: None, // TODO: Implement env_from support
                ports: None,    // TODO: Implement ports support
                resources,
                volume_mounts: if volume_mounts.is_empty() {
                    None
                } else {
                    Some(volume_mounts)
                },
                security_context: None, // TODO: Implement security context
                image_pull_policy: k8s_container.image_pull_policy.clone(),
                ..Default::default()
            });
        }

        containers
    }
}

#[async_trait]
impl WorkerProviderIdentity for KubernetesProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Kubernetes
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }
}

#[async_trait]
impl WorkerLifecycle for KubernetesProvider {
    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        info!("Creating Kubernetes worker Pod: {}", worker_id);

        let startup_timer = Instant::now();

        // Determine namespace for this worker
        let namespace = self.get_namespace_for_worker(spec);

        // Ensure namespace exists if dynamic namespaces are enabled
        if let Err(e) = self.ensure_namespace_exists(&namespace).await {
            warn!("Failed to ensure namespace {} exists: {}", namespace, e);
            // Continue anyway, namespace might already exist
        }

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &namespace);
        let pod_name = PodSpecFactory::generate_pod_name(&worker_id);

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

        // Create Pod spec using PodSpecFactory (GAP-GO-04)
        let build_result = self.pod_spec_factory.build_pod(
            spec,
            otp_token,
            &namespace,
            &self.provider_id.to_string(),
        );
        let pod = build_result.pod;

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

        let startup_time = startup_timer.elapsed();
        info!(
            "Worker Pod {} created successfully (uid: {}) (startup time: {:?})",
            pod_name, pod_uid, startup_time
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

        // Record metrics for successful worker creation
        self.metrics_collector
            .record_worker_creation(startup_time, true)
            .await;

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
}

#[async_trait]
impl WorkerLogs for KubernetesProvider {
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
}

#[async_trait]
impl WorkerHealth for KubernetesProvider {
    async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
        // Try to list pods in the namespace to verify connectivity
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);

        // Timeout de 5 segundos para el health check
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(timeout_duration, pods.list(&ListParams::default().limit(1)))
            .await
        {
            Ok(Ok(_)) => Ok(HealthStatus::Healthy),
            Ok(Err(kube::Error::Api(ae))) if ae.code == 403 => Ok(HealthStatus::Degraded {
                reason: format!(
                    "Insufficient permissions in namespace {}",
                    self.config.namespace
                ),
            }),
            Ok(Err(e)) => Ok(HealthStatus::Unhealthy {
                reason: format!("Failed to connect to Kubernetes API: {}", e),
            }),
            Err(_) => Ok(HealthStatus::Unhealthy {
                reason: "Health check timed out after 5 seconds".to_string(),
            }),
        }
    }
}

impl WorkerCost for KubernetesProvider {
    fn estimate_cost(&self, _spec: &WorkerSpec, _duration: Duration) -> Option<CostEstimate> {
        None
    }

    fn estimated_startup_time(&self) -> Duration {
        Duration::from_secs(15)
    }
}

impl WorkerEligibility for KubernetesProvider {
    fn can_fulfill(&self, requirements: &JobRequirements) -> bool {
        // Check architecture compatibility
        if let Some(required_arch) = &requirements.architecture {
            if !self.capabilities.architectures.contains(required_arch) {
                return false;
            }
        }

        // Check resource requirements
        if self.capabilities.max_resources.max_cpu_cores < requirements.resources.cpu_cores
            || self.capabilities.max_resources.max_memory_bytes
                < requirements.resources.memory_bytes
        {
            return false;
        }

        // Check GPU requirements
        if requirements.resources.gpu_count > 0 && !self.capabilities.gpu_support {
            return false;
        }

        // Check required capabilities
        // For typed features, we check if any feature matches the required capability
        let has_required_capability = if requirements.required_capabilities.is_empty() {
            true
        } else {
            let capability_check = |feat: &ProviderFeature| -> bool {
                match feat {
                    ProviderFeature::Gpu { .. } => requirements
                        .required_capabilities
                        .contains(&"gpu".to_string()),
                    ProviderFeature::Network {
                        custom_networking, ..
                    } => {
                        requirements
                            .required_capabilities
                            .contains(&"custom_networking".to_string())
                            && *custom_networking
                    }
                    ProviderFeature::Storage { persistent, .. } => {
                        (requirements
                            .required_capabilities
                            .contains(&"persistent_storage".to_string())
                            || requirements
                                .required_capabilities
                                .contains(&"storage".to_string()))
                            && *persistent
                    }
                    ProviderFeature::Runtime { name, versions, .. } => requirements
                        .required_capabilities
                        .iter()
                        .any(|cap| cap == name || versions.iter().any(|v| cap == v)),
                    ProviderFeature::Security {
                        isolated_tenant, ..
                    } => {
                        requirements
                            .required_capabilities
                            .contains(&"isolated_tenant".to_string())
                            && *isolated_tenant
                    }
                    ProviderFeature::Specialized {
                        supports_mpi,
                        supports_gpu_direct,
                        supports_rdma,
                        ..
                    } => {
                        (requirements
                            .required_capabilities
                            .contains(&"mpi".to_string())
                            && *supports_mpi)
                            || (requirements
                                .required_capabilities
                                .contains(&"gpu_direct".to_string())
                                && *supports_gpu_direct)
                            || (requirements
                                .required_capabilities
                                .contains(&"rdma".to_string())
                                && *supports_rdma)
                    }
                    _ => false,
                }
            };
            self.capabilities.features.iter().any(capability_check)
        };

        if !has_required_capability && !requirements.required_capabilities.is_empty() {
            return false;
        }

        true
    }
}

impl WorkerMetrics for KubernetesProvider {
    fn get_performance_metrics(&self) -> ProviderPerformanceMetrics {
        self.metrics_collector.get_metrics()
    }

    fn record_worker_creation(&self, startup_time: Duration, success: bool) {
        // Spawn a task to record metrics without blocking
        let collector = self.metrics_collector.clone();
        tokio::spawn(async move {
            collector
                .record_worker_creation(startup_time, success)
                .await;
        });
    }

    fn get_startup_time_history(&self) -> Vec<Duration> {
        self.metrics_collector.get_startup_times()
    }

    fn calculate_average_cost_per_hour(&self) -> f64 {
        // Kubernetes: ~$0.10/vCPU/h + $0.10/GB RAM/h (overhead de orquestación)
        // Calculate based on average resource usage
        let avg_resources = self.metrics_collector.get_average_resource_usage();
        let cpu_cost = avg_resources.avg_cpu_millicores / 1000.0 * 0.10; // vCPU per hour
        let memory_cost =
            (avg_resources.avg_memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) * 0.10; // GB per hour
        cpu_cost + memory_cost
    }

    fn calculate_health_score(&self) -> f64 {
        self.metrics_collector.calculate_health_score()
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
    use crate::providers::pod_spec_factory::PodSpecFactory;
    use hodei_server_domain::workers::KubernetesContainer;

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
            WorkerState::Creating // No Connecting state in Crash-Only Design
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

    #[tokio::test]
    async fn test_gpu_resource_name_v100() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("nvidia-tesla-v100".to_string()));
        assert_eq!(resource_name, "nvidia.com/gpu");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_t4() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("nvidia-tesla-t4".to_string()));
        assert_eq!(resource_name, "nvidia.com/gpu");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_a100() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("nvidia-tesla-a100".to_string()));
        assert_eq!(resource_name, "nvidia.com/gpu");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_amd() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("amd-mi100".to_string()));
        assert_eq!(resource_name, "amd.com/gpu");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_intel() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("intel-xe".to_string()));
        assert_eq!(resource_name, "intel.com/xe");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_unknown() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&Some("unknown-gpu".to_string()));
        assert_eq!(resource_name, "nvidia.com/gpu");
    }

    #[tokio::test]
    async fn test_gpu_resource_name_none() {
        let provider = KubernetesProvider::new().await.unwrap();
        let resource_name = provider.get_gpu_resource_name(&None);
        assert_eq!(resource_name, "nvidia.com/gpu");
    }

    // =========================================================================
    // Tests using PodSpecFactory (GAP-GO-04)
    // Legacy build_* method tests moved to pod_spec_factory::tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_pod_spec_with_factory() {
        let provider = KubernetesProvider::new().await.unwrap();
        let spec = WorkerSpec::new(
            "alpine:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let pod = provider.create_pod_spec_with_namespace(&spec, Some("test-otp"), "default");

        assert!(pod.metadata.name.unwrap().starts_with("hodei-worker-"));
        assert_eq!(pod.metadata.namespace, Some("default".to_string()));
        assert!(pod.spec.is_some());
    }

    #[tokio::test]
    async fn test_kubernetes_job_specific_config() {
        let config = KubernetesConfig::default();
        let provider = KubernetesProvider::with_config(config).await.unwrap();

        let mut spec = WorkerSpec::new(
            "alpine:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        // Add custom Kubernetes annotations
        spec = spec.with_kubernetes_annotation("custom-annotation", "custom-value");

        // Add custom Kubernetes labels
        spec = spec.with_kubernetes_label("custom-label", "custom-label-value");

        // Add Kubernetes node selector
        spec = spec.with_kubernetes_node_selector("node-type", "high-memory");

        // Add init container
        let init_container = KubernetesContainer {
            name: "init-db".to_string(),
            image: "alpine:latest".to_string(),
            image_pull_policy: Some("Always".to_string()),
            command: vec!["sh".to_string(), "-c".to_string()],
            args: vec!["echo init".to_string()],
            env: HashMap::new(),
            env_from: vec![],
            ports: vec![],
            volume_mounts: vec![],
            resources: None,
            security_context: None,
        };
        spec = spec.with_kubernetes_init_container(init_container);

        // Add sidecar container
        let sidecar_container = KubernetesContainer {
            name: "sidecar-logger".to_string(),
            image: "alpine:latest".to_string(),
            image_pull_policy: Some("IfNotPresent".to_string()),
            command: vec!["sh".to_string(), "-c".to_string()],
            args: vec!["tail -f /dev/null".to_string()],
            env: HashMap::new(),
            env_from: vec![],
            ports: vec![],
            volume_mounts: vec![],
            resources: None,
            security_context: None,
        };
        spec = spec.with_kubernetes_sidecar_container(sidecar_container);

        // Set service account
        spec = spec.with_kubernetes_service_account("custom-service-account");

        // Create Pod spec using factory
        let pod = provider.create_pod_spec_with_namespace(&spec, None, "default");

        // Verify custom annotations
        assert!(pod.metadata.annotations.is_some());
        let annotations = pod.metadata.annotations.unwrap();
        assert_eq!(
            annotations.get("custom-annotation"),
            Some(&"custom-value".to_string())
        );

        // Verify custom labels
        assert!(pod.metadata.labels.is_some());
        let labels = pod.metadata.labels.unwrap();
        assert_eq!(
            labels.get("custom-label"),
            Some(&"custom-label-value".to_string())
        );

        // Verify node selector
        assert!(pod.spec.is_some());
        let pod_spec = pod.spec.unwrap();
        assert!(pod_spec.node_selector.is_some());
        let node_selector = pod_spec.node_selector.unwrap();
        assert_eq!(
            node_selector.get("node-type"),
            Some(&"high-memory".to_string())
        );

        // Verify service account
        assert_eq!(
            pod_spec.service_account_name,
            Some("custom-service-account".to_string())
        );

        // Verify init containers
        assert!(pod_spec.init_containers.is_some());
        let init_containers = pod_spec.init_containers.unwrap();
        assert_eq!(init_containers.len(), 1);
        assert_eq!(init_containers[0].name, "init-db");

        // Verify sidecar containers
        assert_eq!(pod_spec.containers.len(), 2); // sidecar + main container
        assert_eq!(pod_spec.containers[0].name, "sidecar-logger");
        assert_eq!(pod_spec.containers[1].name, "worker");
    }
}

// Blanket implementation of WorkerProvider trait combining all ISP traits
// This allows KubernetesProvider to be used as dyn WorkerProvider
#[async_trait]
impl WorkerProvider for KubernetesProvider {}

// Stub implementation for WorkerEventSource - Kubernetes events not yet connected
#[async_trait]
impl WorkerEventSource for KubernetesProvider {
    async fn subscribe(
        &self,
    ) -> std::result::Result<
        Pin<
            Box<
                dyn Stream<Item = std::result::Result<WorkerInfrastructureEvent, ProviderError>>
                    + Send,
            >,
        >,
        ProviderError,
    > {
        Ok(Box::pin(futures::stream::empty()))
    }
}

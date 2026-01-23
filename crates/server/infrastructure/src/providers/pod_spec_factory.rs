//! PodSpec Factory
//!
//! Componente dedicado para la construcción de Kubernetes PodSpecs.
//! Extraído de KubernetesProvider para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Construir PodSpec completo desde WorkerSpec
//! - Construir containers (main, init, sidecar)
//! - Construir volumes y volume mounts
//! - Construir affinity, tolerations, node selector
//! - Construir security context

use k8s_openapi::api::core::v1::{
    Container, EnvVar, Pod, PodSpec, ResourceRequirements as K8sResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use std::collections::{BTreeMap, HashMap};

use super::PodSecurityStandard;
use hodei_server_domain::shared_kernel::WorkerId;
use hodei_server_domain::workers::{VolumeSpec, WorkerSpec};

/// Security Context configuration
#[derive(Debug, Clone, Default)]
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

/// Pod affinity rule for scheduling
#[derive(Debug, Clone, Default)]
pub struct PodAffinityRule {
    pub label_selector: BTreeMap<String, String>,
    pub topology_key: String,
    pub weight: i32,
}

/// Pod anti-affinity rule for scheduling
#[derive(Debug, Clone, Default)]
pub struct PodAntiAffinityRule {
    pub label_selector: BTreeMap<String, String>,
    pub topology_key: String,
    pub weight: i32,
}

/// Kubernetes toleration for pod scheduling
#[derive(Debug, Clone, Default)]
pub struct KubernetesToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
    pub toleration_seconds: Option<i64>,
}

/// Configuration for the Kubernetes Provider (subset needed by factory)
#[derive(Debug, Clone)]
pub struct PodSpecFactoryConfig {
    /// Labels base para todos los pods
    pub base_labels: HashMap<String, String>,
    /// Annotations base para todos los pods
    pub base_annotations: HashMap<String, String>,
    /// Default CPU request
    pub default_cpu_request: String,
    /// Default CPU limit
    pub default_cpu_limit: String,
    /// Default memory request
    pub default_memory_request: String,
    /// Default memory limit
    pub default_memory_limit: String,
    /// TTL seconds after job finishes
    pub ttl_seconds_after_finished: Option<i64>,
    /// Service account name
    pub service_account: Option<String>,
    /// Image pull secrets
    pub image_pull_secrets: Vec<String>,
    /// Node selector for Pod scheduling
    pub node_selector: HashMap<String, String>,
    /// Tolerations for Pod scheduling
    pub tolerations: Vec<KubernetesToleration>,
    /// Pod affinity rules for co-location
    pub pod_affinity: Option<Vec<PodAffinityRule>>,
    /// Pod anti-affinity rules for spread
    pub pod_anti_affinity: Option<Vec<PodAntiAffinityRule>>,
    /// Pod Security Standard level
    pub pod_security_standard: PodSecurityStandard,
    /// Security Context configuration
    pub security_context: SecurityContextConfig,
}

impl Default for PodSpecFactoryConfig {
    fn default() -> Self {
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
            base_labels: HashMap::new(),
            base_annotations: HashMap::new(),
            default_cpu_request: "100m".to_string(),
            default_cpu_limit: "1000m".to_string(),
            default_memory_request: "128Mi".to_string(),
            default_memory_limit: "512Mi".to_string(),
            ttl_seconds_after_finished: None,
            service_account: None,
            image_pull_secrets: vec![],
            node_selector: HashMap::new(),
            tolerations: Vec::new(),
            pod_affinity: None,
            pod_anti_affinity: None,
            pod_security_standard: PodSecurityStandard::Restricted,
            security_context,
        }
    }
}

/// Resultado de la construcción de un PodSpec
#[derive(Debug)]
pub struct PodSpecBuildResult {
    /// El Pod construido
    pub pod: Pod,
    /// Recursos de GPU detectados
    pub gpu_resource_name: Option<String>,
}

/// Parse server address URL into components
///
/// Extracts protocol, hostname, and port from a URL like "http://host:9090"
fn parse_server_address(url: &str) -> (String, String, String) {
    let mut protocol = "http".to_string();
    let mut hostname = url.to_string();
    let mut port = "9090".to_string();

    // Extract protocol
    if let Some(colon_pos) = url.find("://") {
        protocol = url[..colon_pos].to_string();
        hostname = url[colon_pos + 3..].to_string();
    }

    // Extract port from hostname:port
    if let Some(colon_pos) = hostname.find(':') {
        port = hostname[colon_pos + 1..].to_string();
        hostname = hostname[..colon_pos].to_string();
    }

    (protocol, hostname, port)
}

/// PodSpecFactory - Factory para construcción de Kubernetes PodSpecs
///
/// Encapsula toda la lógica de construcción de PodSpecs desde WorkerSpecs,
/// incluyendo containers, volumes, affinity, y configuración de seguridad.
#[derive(Clone)]
pub struct PodSpecFactory {
    /// Configuración del factory
    config: PodSpecFactoryConfig,
}

impl PodSpecFactory {
    /// Crear nuevo PodSpecFactory
    pub fn new(config: Option<PodSpecFactoryConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
        }
    }

    /// Crear PodSpecFactory desde KubernetesConfig
    ///
    /// Este método permite reutilizar la configuración del provider
    /// para construir PodSpecs de forma consistente.
    pub fn from_kubernetes_config(config: &super::KubernetesConfig) -> Self {
        Self {
            config: PodSpecFactoryConfig {
                base_labels: config.base_labels.clone(),
                base_annotations: config.base_annotations.clone(),
                default_cpu_request: config.default_cpu_request.clone(),
                default_cpu_limit: config.default_cpu_limit.clone(),
                default_memory_request: config.default_memory_request.clone(),
                default_memory_limit: config.default_memory_limit.clone(),
                ttl_seconds_after_finished: config.ttl_seconds_after_finished.map(|t| t as i64),
                service_account: config.service_account.clone(),
                image_pull_secrets: config.image_pull_secrets.clone(),
                node_selector: config.node_selector.clone(),
                tolerations: config
                    .tolerations
                    .iter()
                    .map(|t| KubernetesToleration {
                        key: t.key.clone(),
                        operator: t.operator.clone(),
                        value: t.value.clone(),
                        effect: t.effect.clone(),
                        toleration_seconds: t.toleration_seconds,
                    })
                    .collect(),
                pod_affinity: config.pod_affinity.as_ref().map(|affinities| {
                    affinities
                        .iter()
                        .map(|a| PodAffinityRule {
                            label_selector: a.label_selector.clone(),
                            topology_key: a.topology_key.clone(),
                            weight: a.weight,
                        })
                        .collect()
                }),
                pod_anti_affinity: config.pod_anti_affinity.as_ref().map(|anti_affinities| {
                    anti_affinities
                        .iter()
                        .map(|a| PodAntiAffinityRule {
                            label_selector: a.label_selector.clone(),
                            topology_key: a.topology_key.clone(),
                            weight: a.weight,
                        })
                        .collect()
                }),
                pod_security_standard: match config.pod_security_standard {
                    PodSecurityStandard::Restricted => PodSecurityStandard::Restricted,
                    PodSecurityStandard::Baseline => PodSecurityStandard::Baseline,
                    PodSecurityStandard::Privileged => PodSecurityStandard::Privileged,
                },
                security_context: SecurityContextConfig {
                    run_as_non_root: config.security_context.run_as_non_root,
                    run_as_user: config.security_context.run_as_user,
                    run_as_group: config.security_context.run_as_group,
                    read_only_root_filesystem: config.security_context.read_only_root_filesystem,
                    allow_privilege_escalation: config.security_context.allow_privilege_escalation,
                    drop_capabilities: config.security_context.drop_capabilities.clone(),
                    add_capabilities: config.security_context.add_capabilities.clone(),
                    seccomp_profile_type: config.security_context.seccomp_profile_type.clone(),
                },
            },
        }
    }

    /// Construir un Pod completo desde WorkerSpec
    pub fn build_pod(
        &self,
        spec: &WorkerSpec,
        otp_token: Option<&str>,
        namespace: &str,
        provider_id: &str,
    ) -> PodSpecBuildResult {
        let pod_name = self.pod_name(&spec.worker_id);

        // Build labels
        let labels = self.build_labels(spec, provider_id);

        // Build annotations
        let annotations = self.build_annotations(spec);

        // Build environment variables
        let env_vars = self.build_env_vars(spec, otp_token);

        // Build resources
        let resources = self.build_resources(spec);

        // Build node selector
        let node_selector = self.build_node_selector(spec);

        // Build tolerations
        let tolerations = self.build_tolerations(spec);

        // Build volumes
        let volumes = self.build_volumes(spec);

        // Build volume mounts
        let volume_mounts = self.build_volume_mounts(spec);

        // Build affinity
        let affinity = self.build_affinity(spec);

        // Build security context
        let security_context = self.build_security_context();

        // Build container
        let container =
            self.build_main_container(spec, env_vars, resources, volume_mounts, security_context);

        // Build init containers
        let init_containers = self.build_init_containers(spec);

        // Build sidecar containers
        let sidecar_containers = self.build_sidecar_containers(spec);

        // Build image pull secrets
        let image_pull_secrets = self.build_image_pull_secrets();

        // Apply TTL annotation
        let final_annotations = self.apply_ttl_annotation(annotations);

        // Build Pod
        let pod = self.assemble_pod(
            pod_name,
            namespace,
            labels,
            final_annotations,
            container,
            init_containers,
            sidecar_containers,
            node_selector,
            tolerations,
            affinity,
            volumes,
            image_pull_secrets,
            spec,
        );

        let gpu_resource_name = self.get_gpu_resource_name(&spec.resources.gpu_type);

        PodSpecBuildResult {
            pod,
            gpu_resource_name,
        }
    }

    fn pod_name(&self, worker_id: &WorkerId) -> String {
        format!("hodei-worker-{}", worker_id)
    }

    /// Generate pod name (public for use by KubernetesProvider)
    pub fn generate_pod_name(worker_id: &WorkerId) -> String {
        format!("hodei-worker-{}", worker_id)
    }

    fn build_labels(&self, spec: &WorkerSpec, provider_id: &str) -> BTreeMap<String, String> {
        let mut labels: BTreeMap<String, String> = self
            .config
            .base_labels
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        labels.insert("hodei.io/worker-id".to_string(), spec.worker_id.to_string());
        labels.insert("hodei.io/provider-id".to_string(), provider_id.to_string());

        // Add spec labels
        for (k, v) in &spec.labels {
            labels.insert(k.clone(), v.clone());
        }

        // Add custom labels from provider_config (new pattern)
        if let Some(ref config) = spec.provider_config {
            if let Some(k8s_config) = config.as_kubernetes() {
                for (k, v) in &k8s_config.custom_labels {
                    labels.insert(k.clone(), v.clone());
                }
            }
        }

        // Fall back to deprecated spec.kubernetes.custom_labels for backwards compatibility
        for (k, v) in &spec.kubernetes.custom_labels {
            labels.insert(k.clone(), v.clone());
        }

        labels
    }

    fn build_annotations(&self, spec: &WorkerSpec) -> BTreeMap<String, String> {
        let mut annotations: BTreeMap<String, String> = self
            .config
            .base_annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Add custom annotations from provider_config (new pattern)
        if let Some(ref config) = spec.provider_config {
            if let Some(k8s_config) = config.as_kubernetes() {
                for (k, v) in &k8s_config.annotations {
                    annotations.insert(k.clone(), v.clone());
                }
            }
        }

        // Fall back to deprecated spec.kubernetes.annotations for backwards compatibility
        for (k, v) in &spec.kubernetes.annotations {
            annotations.insert(k.clone(), v.clone());
        }

        annotations
    }

    fn build_env_vars(&self, spec: &WorkerSpec, otp_token: Option<&str>) -> Vec<EnvVar> {
        // Generate worker name from worker_id for consistency
        let worker_name = format!("hodei-worker-{}", spec.worker_id);

        // Parse server_address to extract components
        // server_address can be: "http://host:port", "host:port", or "host"
        let (protocol, hostname, port) = parse_server_address(&spec.server_address);

        let mut env_vars = vec![
            EnvVar {
                name: "HODEI_WORKER_ID".to_string(),
                value: Some(spec.worker_id.to_string()),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_WORKER_NAME".to_string(),
                value: Some(worker_name),
                ..Default::default()
            },
            // Worker receives individual components to compose URL
            EnvVar {
                name: "HODEI_SERVER_ADDRESS".to_string(),
                value: Some(hostname),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_SERVER_PROTOCOL".to_string(),
                value: Some(protocol),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_SERVER_PORT".to_string(),
                value: Some(port),
                ..Default::default()
            },
            EnvVar {
                name: "HODEI_WORKER_MODE".to_string(),
                value: Some("ephemeral".to_string()),
                ..Default::default()
            },
        ];

        // Add OTP token if provided and not already in environment
        if let Some(token) = otp_token {
            if !spec.environment.contains_key("HODEI_OTP_TOKEN") {
                env_vars.push(EnvVar {
                    name: "HODEI_OTP_TOKEN".to_string(),
                    value: Some(token.to_string()),
                    ..Default::default()
                });
            }
        }

        // Add spec environment variables
        for (k, v) in &spec.environment {
            env_vars.push(EnvVar {
                name: k.clone(),
                value: Some(v.clone()),
                ..Default::default()
            });
        }

        env_vars
    }

    fn build_resources(&self, spec: &WorkerSpec) -> K8sResourceRequirements {
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

        K8sResourceRequirements {
            claims: None,
            requests: Some(requests),
            limits: Some(limits),
        }
    }

    fn build_node_selector(&self, spec: &WorkerSpec) -> Option<BTreeMap<String, String>> {
        let mut selector: BTreeMap<String, String> = BTreeMap::new();

        // Add base node selector from config
        for (k, v) in &self.config.node_selector {
            selector.insert(k.clone(), v.clone());
        }

        // Add Kubernetes-specific node selector from provider_config (new pattern)
        if let Some(ref config) = spec.provider_config {
            if let Some(k8s_config) = config.as_kubernetes() {
                for (k, v) in &k8s_config.node_selector {
                    selector.insert(k.clone(), v.clone());
                }
            }
        }

        // Fall back to deprecated spec.kubernetes.node_selector for backwards compatibility
        for (k, v) in &spec.kubernetes.node_selector {
            selector.insert(k.clone(), v.clone());
        }

        // Add GPU-specific node selector
        if spec.resources.gpu_count > 0 {
            match spec.resources.gpu_type.as_deref() {
                Some("nvidia-tesla-v100") => {
                    selector.insert("accelerator".to_string(), "nvidia-tesla-v100".to_string());
                    selector.insert("nvidia.com/gpu".to_string(), "1".to_string());
                }
                Some("nvidia-tesla-t4") => {
                    selector.insert("accelerator".to_string(), "nvidia-tesla-t4".to_string());
                    selector.insert("nvidia.com/gpu".to_string(), "1".to_string());
                }
                Some("nvidia-tesla-a100") => {
                    selector.insert("accelerator".to_string(), "nvidia-tesla-a100".to_string());
                    selector.insert("nvidia.com/gpu".to_string(), "1".to_string());
                }
                Some("amd-mi100") => {
                    selector.insert("accelerator".to_string(), "amd-mi100".to_string());
                    selector.insert("amd.com/gpu".to_string(), "1".to_string());
                }
                Some("amd-mi200") => {
                    selector.insert("accelerator".to_string(), "amd-mi200".to_string());
                    selector.insert("amd.com/gpu".to_string(), "1".to_string());
                }
                _ => {
                    selector.insert("accelerator".to_string(), "nvidia-tesla-t4".to_string());
                }
            }
        }

        if selector.is_empty() {
            None
        } else {
            Some(selector)
        }
    }

    fn build_tolerations(
        &self,
        spec: &WorkerSpec,
    ) -> Option<Vec<k8s_openapi::api::core::v1::Toleration>> {
        let mut tolerations: Vec<k8s_openapi::api::core::v1::Toleration> = Vec::new();

        // Add base tolerations from config
        for t in &self.config.tolerations {
            tolerations.push(k8s_openapi::api::core::v1::Toleration {
                key: t.key.clone(),
                operator: t.operator.clone(),
                value: t.value.clone(),
                effect: t.effect.clone(),
                toleration_seconds: t.toleration_seconds,
            });
        }

        // Add GPU-specific tolerations
        if spec.resources.gpu_count > 0 {
            tolerations.push(k8s_openapi::api::core::v1::Toleration {
                key: Some("nvidia.com/gpu".to_string()),
                operator: Some("Equal".to_string()),
                value: Some("true".to_string()),
                effect: Some("NoSchedule".to_string()),
                toleration_seconds: None,
            });
        }

        if tolerations.is_empty() {
            None
        } else {
            Some(tolerations)
        }
    }

    fn build_volumes(&self, spec: &WorkerSpec) -> Option<Vec<k8s_openapi::api::core::v1::Volume>> {
        if spec.volumes.is_empty() {
            return None;
        }

        let volumes: Vec<k8s_openapi::api::core::v1::Volume> = spec
            .volumes
            .iter()
            .map(|v| match v {
                VolumeSpec::Ephemeral {
                    name,
                    size_limit: _,
                } => k8s_openapi::api::core::v1::Volume {
                    name: name.clone(),
                    empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                        medium: None,
                        size_limit: None,
                    }),
                    ..Default::default()
                },
                VolumeSpec::HostPath {
                    name,
                    path,
                    read_only: _,
                } => k8s_openapi::api::core::v1::Volume {
                    name: name.clone(),
                    host_path: Some(k8s_openapi::api::core::v1::HostPathVolumeSource {
                        path: path.clone(),
                        type_: None,
                    }),
                    ..Default::default()
                },
                VolumeSpec::Persistent {
                    name,
                    claim_name,
                    read_only: _,
                } => k8s_openapi::api::core::v1::Volume {
                    name: name.clone(),
                    persistent_volume_claim: Some(
                        k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                            claim_name: claim_name.clone(),
                            ..Default::default()
                        },
                    ),
                    ..Default::default()
                },
            })
            .collect();

        Some(volumes)
    }

    fn build_volume_mounts(
        &self,
        spec: &WorkerSpec,
    ) -> Option<Vec<k8s_openapi::api::core::v1::VolumeMount>> {
        if spec.volumes.is_empty() {
            return None;
        }

        let mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> = spec
            .volumes
            .iter()
            .map(|v| {
                let (name, read_only) = match v {
                    VolumeSpec::Ephemeral { name, .. } => (name, false),
                    VolumeSpec::HostPath {
                        name, read_only, ..
                    } => (name, *read_only),
                    VolumeSpec::Persistent {
                        name, read_only, ..
                    } => (name, *read_only),
                };
                k8s_openapi::api::core::v1::VolumeMount {
                    name: name.clone(),
                    mount_path: format!("/mnt/{}", name),
                    read_only: Some(read_only),
                    ..Default::default()
                }
            })
            .collect();

        Some(mounts)
    }

    fn build_affinity(&self, _spec: &WorkerSpec) -> Option<k8s_openapi::api::core::v1::Affinity> {
        let mut pod_affinity_terms = Vec::new();
        let mut pod_anti_affinity_terms = Vec::new();

        // Build pod affinity terms from config
        if let Some(ref affinities) = self.config.pod_affinity {
            for affinity in affinities {
                let label_selector = Some(
                    k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                        match_labels: Some(affinity.label_selector.clone()),
                        match_expressions: None,
                    },
                );

                let term = k8s_openapi::api::core::v1::PodAffinityTerm {
                    label_selector,
                    namespace_selector: None,
                    namespaces: None,
                    topology_key: affinity.topology_key.clone(),
                    match_label_keys: None,
                    mismatch_label_keys: None,
                };

                pod_affinity_terms.push(k8s_openapi::api::core::v1::WeightedPodAffinityTerm {
                    weight: affinity.weight,
                    pod_affinity_term: term,
                });
            }
        }

        // Build pod anti-affinity terms from config
        if let Some(ref anti_affinities) = self.config.pod_anti_affinity {
            for anti_affinity in anti_affinities {
                let label_selector = Some(
                    k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                        match_labels: Some(anti_affinity.label_selector.clone()),
                        match_expressions: None,
                    },
                );

                let term = k8s_openapi::api::core::v1::PodAffinityTerm {
                    label_selector,
                    namespace_selector: None,
                    namespaces: None,
                    topology_key: anti_affinity.topology_key.clone(),
                    match_label_keys: None,
                    mismatch_label_keys: None,
                };

                pod_anti_affinity_terms.push(k8s_openapi::api::core::v1::WeightedPodAffinityTerm {
                    weight: anti_affinity.weight,
                    pod_affinity_term: term,
                });
            }
        }

        // Only return affinity if we have rules
        if pod_affinity_terms.is_empty() && pod_anti_affinity_terms.is_empty() {
            return None;
        }

        Some(k8s_openapi::api::core::v1::Affinity {
            node_affinity: None,
            pod_affinity: if !pod_affinity_terms.is_empty() {
                Some(k8s_openapi::api::core::v1::PodAffinity {
                    required_during_scheduling_ignored_during_execution: None,
                    preferred_during_scheduling_ignored_during_execution: Some(pod_affinity_terms),
                })
            } else {
                None
            },
            pod_anti_affinity: if !pod_anti_affinity_terms.is_empty() {
                Some(k8s_openapi::api::core::v1::PodAntiAffinity {
                    required_during_scheduling_ignored_during_execution: None,
                    preferred_during_scheduling_ignored_during_execution: Some(
                        pod_anti_affinity_terms,
                    ),
                })
            } else {
                None
            },
        })
    }

    fn build_security_context(&self) -> Option<k8s_openapi::api::core::v1::SecurityContext> {
        let sc = &self.config.security_context;

        // Skip security context if using privileged mode
        if matches!(
            self.config.pod_security_standard,
            PodSecurityStandard::Privileged
        ) {
            return None;
        }

        let mut capabilities = None;
        if !sc.drop_capabilities.is_empty() || !sc.add_capabilities.is_empty() {
            let mut caps = k8s_openapi::api::core::v1::Capabilities {
                drop: None,
                add: None,
            };

            if !sc.drop_capabilities.is_empty() {
                caps.drop = Some(
                    sc.drop_capabilities
                        .iter()
                        .map(|c| c.clone().into())
                        .collect(),
                );
            }

            if !sc.add_capabilities.is_empty() {
                caps.add = Some(
                    sc.add_capabilities
                        .iter()
                        .map(|c| c.clone().into())
                        .collect(),
                );
            }

            capabilities = Some(caps);
        }

        Some(k8s_openapi::api::core::v1::SecurityContext {
            allow_privilege_escalation: Some(sc.allow_privilege_escalation),
            capabilities,
            privileged: Some(false),
            read_only_root_filesystem: Some(sc.read_only_root_filesystem),
            run_as_non_root: Some(sc.run_as_non_root),
            run_as_user: sc.run_as_user,
            run_as_group: sc.run_as_group,
            se_linux_options: None,
            proc_mount: None,
            seccomp_profile: sc.seccomp_profile_type.as_ref().map(|p| {
                k8s_openapi::api::core::v1::SeccompProfile {
                    type_: p.clone(),
                    localhost_profile: None,
                }
            }),
            windows_options: None,
            app_armor_profile: None,
        })
    }

    fn build_main_container(
        &self,
        spec: &WorkerSpec,
        env_vars: Vec<EnvVar>,
        resources: K8sResourceRequirements,
        volume_mounts: Option<Vec<k8s_openapi::api::core::v1::VolumeMount>>,
        security_context: Option<k8s_openapi::api::core::v1::SecurityContext>,
    ) -> Container {
        Container {
            name: "worker".to_string(),
            image: Some(spec.image.clone()),
            env: Some(env_vars),
            resources: Some(resources),
            volume_mounts: volume_mounts,
            security_context,
            image_pull_policy: Some("Always".to_string()),
            ..Default::default()
        }
    }

    fn build_init_containers(&self, spec: &WorkerSpec) -> Vec<Container> {
        spec.kubernetes
            .init_containers
            .iter()
            .map(|k8s_container| self.convert_k8s_container(k8s_container))
            .collect()
    }

    fn build_sidecar_containers(&self, spec: &WorkerSpec) -> Vec<Container> {
        spec.kubernetes
            .sidecar_containers
            .iter()
            .map(|k8s_container| self.convert_k8s_container(k8s_container))
            .collect()
    }

    fn convert_k8s_container(
        &self,
        k8s_container: &hodei_server_domain::workers::KubernetesContainer,
    ) -> Container {
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

        let resources = k8s_container.resources.as_ref().map(|req| {
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

            K8sResourceRequirements {
                claims: None,
                requests: Some(requests),
                limits: Some(limits),
            }
        });

        Container {
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
            env_from: None,
            ports: None,
            resources,
            volume_mounts: if volume_mounts.is_empty() {
                None
            } else {
                Some(volume_mounts)
            },
            security_context: None,
            image_pull_policy: k8s_container.image_pull_policy.clone(),
            ..Default::default()
        }
    }

    fn build_image_pull_secrets(
        &self,
    ) -> Option<Vec<k8s_openapi::api::core::v1::LocalObjectReference>> {
        if self.config.image_pull_secrets.is_empty() {
            None
        } else {
            Some(
                self.config
                    .image_pull_secrets
                    .iter()
                    .map(|s| k8s_openapi::api::core::v1::LocalObjectReference { name: s.clone() })
                    .collect(),
            )
        }
    }

    fn apply_ttl_annotation(
        &self,
        mut annotations: BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        if let Some(ttl) = self.config.ttl_seconds_after_finished {
            annotations.insert("hodei.io/ttl-after-finished".to_string(), ttl.to_string());
        }
        annotations
    }

    fn assemble_pod(
        &self,
        pod_name: String,
        namespace: &str,
        labels: BTreeMap<String, String>,
        annotations: BTreeMap<String, String>,
        container: Container,
        init_containers: Vec<Container>,
        sidecar_containers: Vec<Container>,
        node_selector: Option<BTreeMap<String, String>>,
        tolerations: Option<Vec<k8s_openapi::api::core::v1::Toleration>>,
        affinity: Option<k8s_openapi::api::core::v1::Affinity>,
        volumes: Option<Vec<k8s_openapi::api::core::v1::Volume>>,
        image_pull_secrets: Option<Vec<k8s_openapi::api::core::v1::LocalObjectReference>>,
        spec: &WorkerSpec,
    ) -> Pod {
        let metadata = k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
            name: Some(pod_name),
            namespace: Some(namespace.to_string()),
            labels: Some(labels),
            annotations: Some(annotations),
            ..Default::default()
        };

        Pod {
            metadata,
            spec: Some(PodSpec {
                containers: {
                    let mut containers = Vec::new();
                    containers.extend(sidecar_containers);
                    containers.push(container);
                    containers
                },
                init_containers: Some(init_containers),
                restart_policy: Some("Never".to_string()),
                active_deadline_seconds: Some(spec.max_lifetime.as_secs() as i64),
                service_account_name: spec
                    .kubernetes
                    .service_account
                    .as_ref()
                    .cloned()
                    .or_else(|| self.config.service_account.clone()),
                node_selector,
                tolerations,
                affinity: affinity.map(|a| k8s_openapi::api::core::v1::Affinity {
                    node_affinity: a.node_affinity,
                    pod_affinity: a.pod_affinity,
                    pod_anti_affinity: a.pod_anti_affinity,
                }),
                volumes,
                image_pull_secrets,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Get GPU resource name based on GPU type
    pub fn get_gpu_resource_name(&self, gpu_type: &Option<String>) -> Option<String> {
        match gpu_type.as_deref() {
            Some("nvidia-tesla-v100") | Some("nvidia-tesla-t4") | Some("nvidia-tesla-a100") => {
                Some("nvidia.com/gpu".to_string())
            }
            Some("amd-mi100") | Some("amd-mi200") => Some("amd.com/gpu".to_string()),
            Some("intel-xe") => Some("intel.com/xe".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_spec() -> WorkerSpec {
        WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        )
    }

    #[tokio::test]
    async fn test_factory_config_default() {
        let config = PodSpecFactoryConfig::default();
        assert_eq!(config.default_cpu_request, "100m");
        assert_eq!(config.default_memory_request, "128Mi");
    }

    #[tokio::test]
    async fn test_pod_name_generation() {
        let factory = PodSpecFactory::new(None);
        let worker_id = WorkerId::new();
        let name = factory.pod_name(&worker_id);
        assert!(name.starts_with("hodei-worker-"));
    }

    #[tokio::test]
    async fn test_gpu_resource_name_nvidia() {
        let factory = PodSpecFactory::new(None);
        assert_eq!(
            factory.get_gpu_resource_name(&Some("nvidia-tesla-v100".to_string())),
            Some("nvidia.com/gpu".to_string())
        );
    }

    #[tokio::test]
    async fn test_gpu_resource_name_unknown() {
        let factory = PodSpecFactory::new(None);
        assert_eq!(factory.get_gpu_resource_name(&None), None);
    }

    #[tokio::test]
    async fn test_build_pod_basic() {
        let factory = PodSpecFactory::new(None);
        let spec = make_test_spec();

        let result = factory.build_pod(&spec, Some("test-otp"), "default", "test-provider");

        let pod = &result.pod;
        assert!(
            pod.metadata
                .name
                .as_ref()
                .unwrap()
                .starts_with("hodei-worker-")
        );
        assert_eq!(pod.metadata.namespace, Some("default".to_string()));

        let containers = pod.spec.as_ref().unwrap().containers.len();
        assert_eq!(containers, 1);
    }

    #[tokio::test]
    async fn test_build_pod_with_labels() {
        let mut config = PodSpecFactoryConfig::default();
        config
            .base_labels
            .insert("app".to_string(), "hodei".to_string());

        let factory = PodSpecFactory::new(Some(config));
        let spec = make_test_spec();

        let result = factory.build_pod(&spec, None, "default", "test");
        let labels = result.pod.metadata.labels.as_ref().unwrap();

        assert_eq!(labels.get("app"), Some(&"hodei".to_string()));
        assert!(labels.contains_key("hodei.io/worker-id"));
    }
}

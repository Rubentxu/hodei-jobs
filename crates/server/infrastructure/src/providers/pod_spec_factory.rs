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

use hodei_server_domain::workers::{WorkerSpec, VolumeSpec};
use hodei_server_domain::shared_kernel::WorkerId;

/// Configuración base para construcción de PodSpecs
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
}

impl Default for PodSpecFactoryConfig {
    fn default() -> Self {
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
        let mut labels = self.build_labels(spec, provider_id);

        // Build annotations
        let mut annotations = self.build_annotations(spec);

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
        let container = self.build_main_container(spec, env_vars, resources, volume_mounts, security_context);

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

        PodSpecBuildResult { pod, gpu_resource_name }
    }

    fn pod_name(&self, worker_id: &WorkerId) -> String {
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

        labels
    }

    fn build_annotations(&self, spec: &WorkerSpec) -> BTreeMap<String, String> {
        let mut annotations: BTreeMap<String, String> = self
            .config
            .base_annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Add custom annotations from Kubernetes config
        if let Some(ref config) = spec.provider_config {
            if let Some(k8s_config) = config.as_kubernetes() {
                for (k, v) in &k8s_config.annotations {
                    annotations.insert(k.clone(), v.clone());
                }
            }
        }

        annotations
    }

    fn build_env_vars(&self, spec: &WorkerSpec, otp_token: Option<&str>) -> Vec<EnvVar> {
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

    fn build_node_selector(&self, _spec: &WorkerSpec) -> Option<BTreeMap<String, String>> {
        // Node selector from base config only
        // Advanced node selector would require KubernetesConfigExt extension
        None
    }

    fn build_tolerations(&self, _spec: &WorkerSpec) -> Option<Vec<k8s_openapi::api::core::v1::Toleration>> {
        // Tolerations would require KubernetesConfigExt extension
        None
    }

    fn build_volumes(&self, spec: &WorkerSpec) -> Option<Vec<k8s_openapi::api::core::v1::Volume>> {
        if spec.volumes.is_empty() {
            return None;
        }

        let volumes: Vec<k8s_openapi::api::core::v1::Volume> = spec
            .volumes
            .iter()
            .map(|v| match v {
                VolumeSpec::Ephemeral { name, size_limit: _ } => {
                    k8s_openapi::api::core::v1::Volume {
                        name: name.clone(),
                        empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource {
                            medium: None,
                            size_limit: None,
                        }),
                        ..Default::default()
                    }
                }
                VolumeSpec::HostPath { name, path, read_only: _ } => k8s_openapi::api::core::v1::Volume {
                    name: name.clone(),
                    host_path: Some(k8s_openapi::api::core::v1::HostPathVolumeSource {
                        path: path.clone(),
                        type_: None,
                    }),
                    ..Default::default()
                },
                VolumeSpec::Persistent { name, claim_name, read_only: _ } => {
                    k8s_openapi::api::core::v1::Volume {
                        name: name.clone(),
                        persistent_volume_claim: Some(
                            k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                                claim_name: claim_name.clone(),
                                ..Default::default()
                            },
                        ),
                        ..Default::default()
                    }
                }
            })
            .collect();

        Some(volumes)
    }

    fn build_volume_mounts(&self, spec: &WorkerSpec) -> Option<Vec<k8s_openapi::api::core::v1::VolumeMount>> {
        if spec.volumes.is_empty() {
            return None;
        }

        let mounts: Vec<k8s_openapi::api::core::v1::VolumeMount> = spec
            .volumes
            .iter()
            .map(|v| {
                let (name, read_only) = match v {
                    VolumeSpec::Ephemeral { name, .. } => (name, false),
                    VolumeSpec::HostPath { name, read_only, .. } => (name, *read_only),
                    VolumeSpec::Persistent { name, read_only, .. } => (name, *read_only),
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
        // Affinity configuration would require KubernetesConfigExt extension
        None
    }

    fn build_security_context(&self) -> Option<k8s_openapi::api::core::v1::SecurityContext> {
        Some(k8s_openapi::api::core::v1::SecurityContext {
            run_as_non_root: Some(true),
            run_as_user: Some(1000),
            run_as_group: Some(1000),
            ..Default::default()
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
            image_pull_policy: Some("IfNotPresent".to_string()),
            ..Default::default()
        }
    }

    fn build_init_containers(&self, _spec: &WorkerSpec) -> Vec<Container> {
        // Init containers from config
        vec![]
    }

    fn build_sidecar_containers(&self, _spec: &WorkerSpec) -> Vec<Container> {
        // Sidecar containers from config
        vec![]
    }

    fn build_image_pull_secrets(&self) -> Option<Vec<k8s_openapi::api::core::v1::LocalObjectReference>> {
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
        }
    }

    fn apply_ttl_annotation(&self, mut annotations: BTreeMap<String, String>) -> BTreeMap<String, String> {
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
        assert!(pod.metadata.name.as_ref().unwrap().starts_with("hodei-worker-"));
        assert_eq!(pod.metadata.namespace, Some("default".to_string()));

        let containers = pod.spec.as_ref().unwrap().containers.len();
        assert_eq!(containers, 1);
    }

    #[tokio::test]
    async fn test_build_pod_with_labels() {
        let mut config = PodSpecFactoryConfig::default();
        config.base_labels.insert("app".to_string(), "hodei".to_string());

        let factory = PodSpecFactory::new(Some(config));
        let spec = make_test_spec();

        let result = factory.build_pod(&spec, None, "default", "test");
        let labels = result.pod.metadata.labels.as_ref().unwrap();

        assert_eq!(labels.get("app"), Some(&"hodei".to_string()));
        assert!(labels.contains_key("hodei.io/worker-id"));
    }
}

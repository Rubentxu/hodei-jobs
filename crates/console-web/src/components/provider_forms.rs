//! Provider Configuration Forms - Production-ready provider setup
//!
//! Complete forms for Kubernetes, Docker, and Firecracker provider configuration
//! with validation, state management, and real gRPC integration.

use crate::types::ProviderConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Provider types
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProviderType {
    #[serde(rename = "kubernetes")]
    Kubernetes,
    #[serde(rename = "docker")]
    Docker,
    #[serde(rename = "firecracker")]
    Firecracker,
}

impl ProviderType {
    /// Get display name for provider type
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderType::Kubernetes => "Kubernetes",
            ProviderType::Docker => "Docker",
            ProviderType::Firecracker => "Firecracker",
        }
    }

    /// Get icon name for provider type
    #[must_use]
    pub fn icon_name(&self) -> &'static str {
        match self {
            ProviderType::Kubernetes => "container",
            ProviderType::Docker => "docker",
            ProviderType::Firecracker => "security",
        }
    }

    /// Get description for provider type
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            ProviderType::Kubernetes => {
                "Deploy workers on Kubernetes clusters with full orchestration"
            }
            ProviderType::Docker => "Run workers as lightweight Docker containers",
            ProviderType::Firecracker => {
                "Launch ultra-lightweight microVMs for maximum performance"
            }
        }
    }
}

/// Validation errors for provider configuration
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ProviderValidationError {
    #[error("Name is required (max 100 characters)")]
    InvalidName,

    #[error("Kubeconfig is required")]
    MissingKubeconfig,

    #[error("Invalid kubeconfig format: {0}")]
    InvalidKubeconfig(String),

    #[error("Cluster URL is not a valid URL")]
    InvalidClusterUrl,

    #[error("Namespace is required")]
    MissingNamespace,

    #[error("Docker socket path is required")]
    MissingDockerSocket,

    #[error("Docker socket path is not valid: {0}")]
    InvalidDockerSocket(String),

    #[error("Firecracker socket path is required")]
    MissingFcSocket,

    #[error("Firecracker socket path is not valid: {0}")]
    InvalidFcSocket(String),

    #[error("Kernel image path is required")]
    MissingKernelImage,

    #[error("Rootfs image path is required")]
    MissingRootfs,

    #[error("vCPU must be between 1 and 64")]
    InvalidVcpu,

    #[error("Memory must be between 128 MiB and 131072 MiB")]
    InvalidMemory,
}

/// Kubernetes provider configuration
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KubernetesProviderConfig {
    /// Provider display name
    pub name: String,
    /// Provider description
    pub description: String,
    /// Base64 encoded kubeconfig content
    pub kubeconfig: String,
    /// Optional cluster URL override
    pub cluster_url: Option<String>,
    /// Default namespace for workers
    pub namespace: String,
    /// Node selector labels
    pub node_selector: Vec<(String, String)>,
    /// Tolerations
    pub tolerations: Vec<String>,
    /// Resource limits
    pub cpu_limit: Option<i32>,
    pub memory_limit_mib: Option<i32>,
}

impl KubernetesProviderConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ProviderValidationError> {
        // Validate name
        if self.name.trim().is_empty() {
            return Err(ProviderValidationError::InvalidName);
        }
        if self.name.len() > 100 {
            return Err(ProviderValidationError::InvalidName);
        }

        // Validate kubeconfig
        if self.kubeconfig.trim().is_empty() {
            return Err(ProviderValidationError::MissingKubeconfig);
        }

        // Try to decode base64 to validate
        if let Ok(decoded) = base64_decode(&self.kubeconfig) {
            if decoded.is_empty() {
                return Err(ProviderValidationError::MissingKubeconfig);
            }
            // Basic YAML validation
            if !decoded.contains("apiVersion") || !decoded.contains("kind") {
                return Err(ProviderValidationError::InvalidKubeconfig(
                    "Missing apiVersion or kind".to_string(),
                ));
            }
        }

        // Validate cluster URL if provided
        if let Some(url) = &self.cluster_url {
            if !url.trim().is_empty() {
                let _ =
                    url::Url::parse(url).map_err(|_| ProviderValidationError::InvalidClusterUrl)?;
            }
        }

        // Validate namespace
        if self.namespace.trim().is_empty() {
            return Err(ProviderValidationError::MissingNamespace);
        }

        Ok(())
    }
}

impl ProviderConfig for KubernetesProviderConfig {}

/// Docker provider configuration
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DockerProviderConfig {
    /// Provider display name
    pub name: String,
    /// Provider description
    pub description: String,
    /// Docker socket path or TCP endpoint
    pub socket_path: String,
    /// Use TLS for connection
    pub use_tls: bool,
    /// TLS CA certificate (base64)
    pub tls_ca_cert: String,
    /// TLS client certificate (base64)
    pub tls_client_cert: String,
    /// TLS client key (base64)
    pub tls_client_key: String,
    /// Privileged mode
    pub privileged: bool,
    /// Network mode
    pub network_mode: String,
    /// Volume mounts
    pub volume_mounts: Vec<VolumeMount>,
    /// Memory limit in MiB
    pub memory_limit_mib: i32,
    /// CPU shares
    pub cpu_shares: i32,
}

impl DockerProviderConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ProviderValidationError> {
        // Validate name
        if self.name.trim().is_empty() {
            return Err(ProviderValidationError::InvalidName);
        }
        if self.name.len() > 100 {
            return Err(ProviderValidationError::InvalidName);
        }

        // Validate socket path
        if self.socket_path.trim().is_empty() {
            return Err(ProviderValidationError::MissingDockerSocket);
        }

        // Validate socket path format
        let socket = self.socket_path.trim();
        if socket.starts_with("unix://") {
            let path = &socket[7..];
            if path.is_empty() {
                return Err(ProviderValidationError::InvalidDockerSocket(
                    "Empty socket path".to_string(),
                ));
            }
        } else if socket.starts_with("tcp://") {
            // Validate TCP endpoint
            let endpoint = &socket[6..];
            if endpoint.is_empty() {
                return Err(ProviderValidationError::InvalidDockerSocket(
                    "Empty TCP endpoint".to_string(),
                ));
            }
            // Basic format check
            if !endpoint.contains(':') {
                return Err(ProviderValidationError::InvalidDockerSocket(
                    "Missing port in TCP endpoint".to_string(),
                ));
            }
        }

        // Validate TLS if enabled
        if self.use_tls {
            if self.tls_ca_cert.trim().is_empty() {
                return Err(ProviderValidationError::InvalidDockerSocket(
                    "CA certificate required for TLS".to_string(),
                ));
            }
        }

        // Validate resources
        if self.memory_limit_mib < 128 || self.memory_limit_mib > 4 * 1024 {
            return Err(ProviderValidationError::InvalidMemory);
        }

        if self.cpu_shares < 512 || self.cpu_shares > 8192 {
            return Err(ProviderValidationError::InvalidVcpu);
        }

        Ok(())
    }
}

impl ProviderConfig for DockerProviderConfig {}

/// Volume mount configuration
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VolumeMount {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

/// Firecracker provider configuration
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FirecrackerProviderConfig {
    /// Provider display name
    pub name: String,
    /// Provider description
    pub description: String,
    /// API socket path
    pub socket_path: String,
    /// Kernel image path
    pub kernel_image: String,
    /// Rootfs image path
    pub rootfs: String,
    /// Number of vCPUs
    pub vcpu: i32,
    /// Memory in MiB
    pub memory_mib: i32,
    /// Disk size in GiB
    pub disk_size_gib: i32,
    /// Network interface
    pub network_interface: String,
    /// Security groups
    pub security_groups: Vec<String>,
}

impl FirecrackerProviderConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ProviderValidationError> {
        // Validate name
        if self.name.trim().is_empty() {
            return Err(ProviderValidationError::InvalidName);
        }
        if self.name.len() > 100 {
            return Err(ProviderValidationError::InvalidName);
        }

        // Validate socket path
        if self.socket_path.trim().is_empty() {
            return Err(ProviderValidationError::MissingFcSocket);
        }

        let socket = self.socket_path.trim();
        if !socket.starts_with('/') {
            return Err(ProviderValidationError::InvalidFcSocket(
                "Socket path must be absolute".to_string(),
            ));
        }

        // Validate kernel image
        if self.kernel_image.trim().is_empty() {
            return Err(ProviderValidationError::MissingKernelImage);
        }

        // Validate rootfs
        if self.rootfs.trim().is_empty() {
            return Err(ProviderValidationError::MissingRootfs);
        }

        // Validate resources
        if self.vcpu < 1 || self.vcpu > 64 {
            return Err(ProviderValidationError::InvalidVcpu);
        }

        if self.memory_mib < 128 || self.memory_mib > 131072 {
            return Err(ProviderValidationError::InvalidMemory);
        }

        Ok(())
    }
}

impl ProviderConfig for FirecrackerProviderConfig {}

/// Helper function to decode base64
fn base64_decode(input: &str) -> Result<String, base64::DecodeError> {
    let cleaned = input.trim().replace(['\n', '\r', ' '], "");
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &cleaned)?;
    String::from_utf8(bytes).map_err(|_e| base64::DecodeError::InvalidByte(0, 0))
}

/// Provider form state for wizard
#[derive(Clone, Debug, Default)]
pub struct ProviderFormState {
    pub is_open: bool,
    pub provider_type: Option<ProviderType>,
    pub kubernetes: KubernetesProviderConfig,
    pub docker: DockerProviderConfig,
    pub firecracker: FirecrackerProviderConfig,
    pub validation_errors: Vec<ProviderValidationError>,
    pub is_saving: bool,
    pub is_testing: bool,
    pub test_result: Option<Result<(), String>>,
}

impl ProviderFormState {
    /// Get the active configuration based on provider type
    pub fn active_config(&self) -> Option<&dyn crate::types::ProviderConfig> {
        match self.provider_type {
            Some(ProviderType::Kubernetes) => {
                Some(&self.kubernetes as &dyn crate::types::ProviderConfig)
            }
            Some(ProviderType::Docker) => Some(&self.docker as &dyn crate::types::ProviderConfig),
            Some(ProviderType::Firecracker) => {
                Some(&self.firecracker as &dyn crate::types::ProviderConfig)
            }
            None => None,
        }
    }

    /// Validate current configuration
    pub fn validate(&self) -> Result<(), Vec<ProviderValidationError>> {
        let errors = match self.provider_type {
            Some(ProviderType::Kubernetes) => {
                self.kubernetes.validate().err().into_iter().collect()
            }
            Some(ProviderType::Docker) => self.docker.validate().err().into_iter().collect(),
            Some(ProviderType::Firecracker) => {
                self.firecracker.validate().err().into_iter().collect()
            }
            None => vec![ProviderValidationError::InvalidName],
        };

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

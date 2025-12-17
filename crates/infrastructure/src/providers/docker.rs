//! Docker Worker Provider Implementation
//!
//! Production-ready implementation of WorkerProvider using Docker containers.
//! Uses bollard for Docker API communication.

use async_trait::async_trait;
use bollard::{
    Docker,
    container::LogOutput,
    models::{ContainerCreateBody, ContainerStateStatusEnum, HostConfig},
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, InspectContainerOptions,
        LogsOptionsBuilder, RemoveContainerOptionsBuilder, StartContainerOptions,
        StopContainerOptionsBuilder,
    },
};
use chrono::Utc;
use futures_util::StreamExt;
use hodei_jobs_domain::{
    provider_config::DockerConfig,
    shared_kernel::{DomainError, ProviderId, Result, WorkerState},
    worker::{Architecture, ProviderType, WorkerHandle, WorkerSpec},
    worker_provider::{
        HealthStatus, LogEntry, LogLevel, ProviderCapabilities, ProviderError, ResourceLimits,
        WorkerProvider,
    },
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Docker Provider for creating ephemeral worker containers
///
/// Uses bollard crate for Docker API communication. Each worker is a Docker container
/// that runs the hodei-worker agent image.
#[derive(Clone)]
pub struct DockerProvider {
    /// Unique provider identifier
    provider_id: ProviderId,
    /// Docker client
    client: Docker,
    /// Provider configuration
    config: DockerConfig,
    /// Active workers tracking (provider_resource_id -> WorkerHandle)
    active_workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    /// Provider capabilities
    capabilities: ProviderCapabilities,
}

/// Builder for DockerProvider
pub struct DockerProviderBuilder {
    provider_id: Option<ProviderId>,
    config: Option<DockerConfig>,
    client: Option<Docker>,
    capabilities: Option<ProviderCapabilities>,
}

impl DockerProviderBuilder {
    pub fn new() -> Self {
        Self {
            provider_id: None,
            config: None,
            client: None,
            capabilities: None,
        }
    }

    pub fn with_provider_id(mut self, id: ProviderId) -> Self {
        self.provider_id = Some(id);
        self
    }

    pub fn with_config(mut self, config: DockerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_client(mut self, client: Docker) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Build the DockerProvider
    pub async fn build(self) -> Result<DockerProvider> {
        let config = self.config.unwrap_or_default();
        let provider_id = self.provider_id.unwrap_or_else(ProviderId::new);

        let client = match self.client {
            Some(c) => c,
            None => Self::create_docker_client(&config).await?,
        };

        let capabilities = self.capabilities.unwrap_or_else(Self::default_capabilities);

        Ok(DockerProvider {
            provider_id,
            client,
            config,
            active_workers: Arc::new(RwLock::new(HashMap::new())),
            capabilities,
        })
    }

    async fn create_docker_client(config: &DockerConfig) -> Result<Docker> {
        // Try multiple socket locations in order of preference:
        // 1. Configured socket path
        // 2. DOCKER_HOST environment variable
        // 3. Docker Desktop socket (~/.docker/desktop/docker.sock)
        // 4. Default socket (/var/run/docker.sock)

        let socket_paths = Self::get_socket_paths(config);

        for socket_path in &socket_paths {
            debug!("Trying Docker socket: {}", socket_path);

            let client_result = if socket_path == "/var/run/docker.sock" {
                Docker::connect_with_socket_defaults()
            } else {
                Docker::connect_with_socket(socket_path, 120, bollard::API_DEFAULT_VERSION)
            };

            match client_result {
                Ok(client) => {
                    if client.ping().await.is_ok() {
                        info!("Docker client connected successfully via {}", socket_path);
                        return Ok(client);
                    }
                }
                Err(e) => {
                    debug!("Failed to connect to {}: {}", socket_path, e);
                }
            }
        }

        Err(DomainError::InfrastructureError {
            message: format!(
                "Failed to connect to Docker daemon. Tried sockets: {:?}",
                socket_paths
            ),
        })
    }

    fn get_socket_paths(config: &DockerConfig) -> Vec<String> {
        let mut paths = Vec::new();

        // 1. Configured socket path (if not default)
        if config.socket_path != "/var/run/docker.sock" {
            paths.push(config.socket_path.clone());
        }

        // 2. DOCKER_HOST environment variable
        if let Ok(docker_host) = std::env::var("DOCKER_HOST") {
            if let Some(path) = docker_host.strip_prefix("unix://") {
                paths.push(path.to_string());
            }
        }

        // 3. Docker Desktop socket (Linux)
        if let Ok(home) = std::env::var("HOME") {
            let desktop_socket = format!("{}/.docker/desktop/docker.sock", home);
            if std::path::Path::new(&desktop_socket).exists() {
                paths.push(desktop_socket);
            }
        }

        // 4. Podman socket (rootless)
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let podman_socket = format!("{}/podman/podman.sock", xdg_runtime);
            if std::path::Path::new(&podman_socket).exists() {
                paths.push(podman_socket);
            }
        }

        // 5. Default Docker socket
        paths.push("/var/run/docker.sock".to_string());

        paths
    }

    fn default_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            max_resources: ResourceLimits {
                max_cpu_cores: 16.0,
                max_memory_bytes: 64 * 1024 * 1024 * 1024,
                max_disk_bytes: 500 * 1024 * 1024 * 1024,
                max_gpu_count: 0,
            },
            gpu_support: false,
            gpu_types: vec![],
            architectures: vec![Architecture::Amd64, Architecture::Arm64],
            runtimes: vec![
                "shell".to_string(),
                "python".to_string(),
                "node".to_string(),
            ],
            regions: vec!["local".to_string()],
            max_execution_time: Some(Duration::from_secs(86400)),
            persistent_storage: true,
            custom_networking: true,
            features: HashMap::new(),
        }
    }
}

impl Default for DockerProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerProvider {
    /// Create a new DockerProvider with default configuration
    pub async fn new() -> Result<Self> {
        DockerProviderBuilder::new().build().await
    }

    /// Create a new DockerProvider with custom configuration
    pub async fn with_config(config: DockerConfig) -> Result<Self> {
        DockerProviderBuilder::new()
            .with_config(config)
            .build()
            .await
    }

    /// Create container configuration from WorkerSpec
    fn create_container_config(&self, spec: &WorkerSpec) -> ContainerCreateBody {
        // Collect all environment variables from the spec
        let mut env_vars: Vec<String> = spec
            .environment
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Add Hodei-specific environment variables
        env_vars.push(format!("HODEI_WORKER_ID={}", spec.worker_id.0));
        env_vars.push(format!("HODEI_SERVER_ADDRESS={}", spec.server_address));
        env_vars.push("HODEI_WORKER_MODE=ephemeral".to_string());

        // Add OTP token if present (for authentication)
        if let Some(token) = spec.environment.get("HODEI_OTP_TOKEN") {
            env_vars.push(format!("HODEI_OTP_TOKEN={}", token));
        }

        info!(
            "Creating container with {} environment variables",
            env_vars.len()
        );

        let mut labels = spec.labels.clone();
        labels.insert("hodei.worker.id".to_string(), spec.worker_id.to_string());
        labels.insert("hodei.managed".to_string(), "true".to_string());

        let memory_limit = spec.resources.memory_bytes;
        let cpu_period = 100000i64;
        let cpu_quota = (spec.resources.cpu_cores * cpu_period as f64) as i64;

        let host_config = HostConfig {
            memory: Some(memory_limit),
            cpu_period: Some(cpu_period),
            cpu_quota: Some(cpu_quota),
            network_mode: self.config.network.clone(),
            auto_remove: Some(false),
            ..Default::default()
        };

        ContainerCreateBody {
            image: Some(spec.image.clone()),
            env: Some(env_vars),
            labels: Some(labels),
            host_config: Some(host_config),
            ..Default::default()
        }
    }

    /// Ensure image is available locally
    async fn ensure_image(&self, image: &str) -> Result<()> {
        if self.client.inspect_image(image).await.is_ok() {
            debug!("Image {} already exists locally", image);
            return Ok(());
        }

        info!("Pulling image {}", image);
        let options = CreateImageOptionsBuilder::default()
            .from_image(image)
            .build();

        let mut stream = self.client.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        debug!("Pull status: {}", status);
                    }
                }
                Err(e) => {
                    return Err(DomainError::InfrastructureError {
                        message: format!("Failed to pull image {}: {}", image, e),
                    });
                }
            }
        }

        info!("Image {} pulled successfully", image);
        Ok(())
    }

    /// Map container state to WorkerState
    /// Map container state to WorkerState (PRD v6.0)
    fn map_container_state(status: Option<&ContainerStateStatusEnum>) -> WorkerState {
        match status {
            Some(ContainerStateStatusEnum::CREATED) => WorkerState::Connecting,
            Some(ContainerStateStatusEnum::RUNNING) => WorkerState::Ready,
            Some(ContainerStateStatusEnum::PAUSED) => WorkerState::Draining,
            Some(ContainerStateStatusEnum::RESTARTING) => WorkerState::Connecting,
            Some(ContainerStateStatusEnum::REMOVING) => WorkerState::Terminating,
            Some(ContainerStateStatusEnum::EXITED) => WorkerState::Terminated,
            Some(ContainerStateStatusEnum::DEAD) => WorkerState::Terminated,
            None | Some(ContainerStateStatusEnum::EMPTY) => WorkerState::Creating,
        }
    }
}

#[async_trait]
impl WorkerProvider for DockerProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Docker
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        info!("Creating Docker worker: {}", worker_id);

        self.ensure_image(&spec.image)
            .await
            .map_err(|e| ProviderError::ProvisioningFailed(e.to_string()))?;

        let container_name = format!("hodei-worker-{}", worker_id);
        let config = self.create_container_config(spec);

        let options = CreateContainerOptionsBuilder::default()
            .name(&container_name)
            .build();

        let container = self
            .client
            .create_container(Some(options), config)
            .await
            .map_err(|e| {
                ProviderError::ProvisioningFailed(format!("Failed to create container: {}", e))
            })?;

        let container_id = container.id.clone();
        debug!("Container created: {}", container_id);

        self.client
            .start_container(&container_id, None::<StartContainerOptions>)
            .await
            .map_err(|e| {
                error!("Failed to start container: {}", e);
                ProviderError::ProvisioningFailed(format!("Failed to start container: {}", e))
            })?;

        info!("Worker {} started in container {}", worker_id, container_id);

        let handle = WorkerHandle::new(
            worker_id,
            container_id.clone(),
            ProviderType::Docker,
            self.provider_id.clone(),
        )
        .with_metadata("container_name", serde_json::json!(container_name));

        self.active_workers
            .write()
            .await
            .insert(container_id, handle.clone());

        Ok(handle)
    }

    async fn get_worker_status(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<WorkerState, ProviderError> {
        let container_id = &handle.provider_resource_id;

        let inspect = self
            .client
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| ProviderError::WorkerNotFound(format!("Container not found: {}", e)))?;

        let state = inspect.state.as_ref().and_then(|s| s.status.as_ref());
        Ok(Self::map_container_state(state))
    }

    async fn destroy_worker(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<(), ProviderError> {
        let container_id = &handle.provider_resource_id;
        info!("Destroying worker: {}", handle.worker_id);

        let stop_options = StopContainerOptionsBuilder::default().t(10).build();
        if let Err(e) = self
            .client
            .stop_container(container_id, Some(stop_options))
            .await
        {
            warn!("Failed to stop container gracefully: {}", e);
        }

        let remove_options = RemoveContainerOptionsBuilder::default()
            .force(true)
            .v(true)
            .build();

        self.client
            .remove_container(container_id, Some(remove_options))
            .await
            .map_err(|e| ProviderError::Internal(format!("Failed to remove container: {}", e)))?;

        self.active_workers.write().await.remove(container_id);

        info!("Worker {} destroyed successfully", handle.worker_id);
        Ok(())
    }

    async fn get_worker_logs(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> std::result::Result<Vec<LogEntry>, ProviderError> {
        let container_id = &handle.provider_resource_id;

        let tail_str = tail
            .map(|t| t.to_string())
            .unwrap_or_else(|| "100".to_string());
        let options = LogsOptionsBuilder::default()
            .stdout(true)
            .stderr(true)
            .timestamps(true)
            .tail(&tail_str)
            .build();

        let mut logs = Vec::new();
        let mut stream = self.client.logs(container_id, Some(options));

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    let (level, message) = match &output {
                        LogOutput::StdOut { message } => {
                            (LogLevel::Info, String::from_utf8_lossy(message).to_string())
                        }
                        LogOutput::StdErr { message } => (
                            LogLevel::Error,
                            String::from_utf8_lossy(message).to_string(),
                        ),
                        LogOutput::Console { message } => (
                            LogLevel::Debug,
                            String::from_utf8_lossy(message).to_string(),
                        ),
                        LogOutput::StdIn { .. } => continue,
                    };

                    logs.push(LogEntry {
                        timestamp: Utc::now(),
                        level,
                        message,
                        source: "container".to_string(),
                    });
                }
                Err(e) => {
                    warn!("Error reading logs: {}", e);
                    break;
                }
            }
        }

        Ok(logs)
    }

    async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
        match self.client.ping().await {
            Ok(_) => {
                let _workers = self.active_workers.read().await;
                Ok(HealthStatus::Healthy)
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                reason: format!("Docker daemon unreachable: {}", e),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_provider_builder() {
        let builder = DockerProviderBuilder::new().with_config(DockerConfig::default());
        assert!(builder.config.is_some());
    }

    #[test]
    fn test_default_capabilities() {
        let caps = DockerProviderBuilder::default_capabilities();
        assert!(!caps.gpu_support);
        assert!(caps.max_resources.max_cpu_cores > 0.0);
        assert!(caps.max_resources.max_memory_bytes > 0);
        assert!(!caps.architectures.is_empty());
    }

    #[test]
    fn test_map_container_state() {
        assert!(matches!(
            DockerProvider::map_container_state(Some(&ContainerStateStatusEnum::RUNNING)),
            WorkerState::Ready
        ));
        assert!(matches!(
            DockerProvider::map_container_state(Some(&ContainerStateStatusEnum::EXITED)),
            WorkerState::Terminated
        ));
        assert!(matches!(
            DockerProvider::map_container_state(Some(&ContainerStateStatusEnum::DEAD)),
            WorkerState::Terminated
        ));
        assert!(matches!(
            DockerProvider::map_container_state(None),
            WorkerState::Creating
        ));
        assert!(matches!(
            DockerProvider::map_container_state(Some(&ContainerStateStatusEnum::CREATED)),
            WorkerState::Connecting
        ));
    }
}

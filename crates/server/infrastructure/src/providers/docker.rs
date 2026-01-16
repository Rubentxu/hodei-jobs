//! Docker Worker Provider Implementation
//!
//! Production-ready implementation of WorkerProvider using Docker containers.
//! Uses bollard for Docker API communication.

use async_trait::async_trait;
use bollard::service::{EventMessage, EventMessageTypeEnum};
use bollard::{
    Docker,
    container::LogOutput,
    models::{ContainerCreateBody, ContainerStateStatusEnum, HostConfig},
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, EventsOptions,
        InspectContainerOptions, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
        StartContainerOptions, StopContainerOptionsBuilder,
    },
};
use chrono::Utc;
use futures::Stream;
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::providers::metrics_collector::ProviderMetricsCollector;
use hodei_server_domain::providers::DockerConfig;
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, Result};
use hodei_server_domain::workers::{
    Architecture, CostEstimate, HealthStatus, JobRequirements, LogEntry, LogLevel,
    ProviderCapabilities, ProviderError, ProviderFeature, ProviderPerformanceMetrics, ProviderType,
    ResourceLimits, WorkerCost, WorkerEligibility, WorkerEventSource, WorkerHandle, WorkerHealth,
    WorkerInfrastructureEvent, WorkerLifecycle, WorkerLogs, WorkerMetrics, WorkerProvider,
    WorkerProviderIdentity, WorkerSpec,
};
use hodei_shared::WorkerState;

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
    /// Metrics collector for performance tracking
    metrics_collector: ProviderMetricsCollector,
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
        let provider_id_clone = provider_id.clone();

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
            metrics_collector: ProviderMetricsCollector::new(provider_id_clone),
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
            features: Vec::new(),
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
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
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

    /// Map container state to WorkerState (Crash-Only Design)
    fn map_container_state(status: Option<&ContainerStateStatusEnum>) -> WorkerState {
        match status {
            Some(ContainerStateStatusEnum::CREATED) => WorkerState::Creating,
            Some(ContainerStateStatusEnum::RUNNING) => WorkerState::Ready,
            Some(ContainerStateStatusEnum::PAUSED)
            | Some(ContainerStateStatusEnum::RESTARTING)
            | Some(ContainerStateStatusEnum::REMOVING) => WorkerState::Creating,
            Some(ContainerStateStatusEnum::EXITED) | Some(ContainerStateStatusEnum::DEAD) => {
                WorkerState::Terminated
            }
            None | Some(ContainerStateStatusEnum::EMPTY) => WorkerState::Creating,
        }
    }
}

#[async_trait]
impl WorkerProviderIdentity for DockerProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Docker
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }
}

#[async_trait]
impl WorkerLifecycle for DockerProvider {
    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        info!("Creating Docker worker: {}", worker_id);

        let startup_timer = Instant::now();

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

        let startup_time = startup_timer.elapsed();
        info!(
            "Worker {} started in container {} (startup time: {:?})",
            worker_id, container_id, startup_time
        );

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
}

#[async_trait]
impl WorkerLogs for DockerProvider {
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
}

#[async_trait]
impl WorkerHealth for DockerProvider {
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

impl WorkerCost for DockerProvider {
    fn estimate_cost(&self, _spec: &WorkerSpec, _duration: Duration) -> Option<CostEstimate> {
        None
    }

    fn estimated_startup_time(&self) -> Duration {
        Duration::from_secs(5)
    }
}

impl WorkerEligibility for DockerProvider {
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
            // Check if any typed feature supports the required capability
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

        // Check allowed regions (empty means all regions allowed)
        if !requirements.allowed_regions.is_empty() {
            let provider_region = self
                .capabilities
                .regions
                .first()
                .cloned()
                .unwrap_or_default();
            if !requirements.allowed_regions.contains(&provider_region)
                && !requirements.allowed_regions.contains(&"*".to_string())
            {
                return false;
            }
        }

        true
    }
}

impl WorkerMetrics for DockerProvider {
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
        // Docker: ~$0.05/vCPU/h + $0.05/GB RAM/h
        // Calculate based on average resource usage
        let avg_resources = self.metrics_collector.get_average_resource_usage();
        let cpu_cost = avg_resources.avg_cpu_millicores / 1000.0 * 0.05; // vCPU per hour
        let memory_cost =
            (avg_resources.avg_memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) * 0.05; // GB per hour
        cpu_cost + memory_cost
    }

    fn calculate_health_score(&self) -> f64 {
        self.metrics_collector.calculate_health_score()
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
            WorkerState::Creating // Crash-Only: CREATED mapea a Creating
        ));
    }
}

// Blanket implementation of WorkerProvider trait combining all ISP traits
// This allows DockerProvider to be used as dyn WorkerProvider
#[async_trait]
impl WorkerProvider for DockerProvider {}

// ============================================================================
// Docker Event Stream Implementation (US-10)
// ============================================================================

#[async_trait]
impl WorkerEventSource for DockerProvider {
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
        info!(
            "Subscribing to Docker events for provider {}",
            self.provider_id
        );

        let active_workers = self.active_workers.clone();
        let provider_id = self.provider_id.clone();
        let client = self.client.clone();

        // Create a channel to bridge between stream and our consumers
        let (tx, rx) =
            tokio::sync::mpsc::channel::<std::result::Result<WorkerInfrastructureEvent, ProviderError>>(100);

        // Spawn a task to process events from Docker Events API
        let _handle = tokio::spawn(async move {
            info!("Docker events stream active for provider {}", provider_id);

            // Subscribe to Docker events using bollard API
            let events = client.events(Some(EventsOptions::default()));

            tokio::pin!(events);

            while let Some(event_result) = events.next().await {
                match event_result {
                    Ok(event) => {
                        if let Some(worker_event) =
                            parse_docker_event(&event, &provider_id, &active_workers).await
                        {
                            if tx.send(Ok(worker_event)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Docker event error: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }

            info!("Docker events stream closed for provider {}", provider_id);
        });

        // Return the receiver as a stream using unfold
        Ok(Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Some(result) => Some((result, rx)),
                None => None,
            }
        })))
    }
}

/// Parse Docker events to WorkerInfrastructureEvent
async fn parse_docker_event(
    event: &EventMessage,
    _provider_id: &ProviderId,
    active_workers: &Arc<RwLock<HashMap<String, WorkerHandle>>>,
) -> Option<WorkerInfrastructureEvent> {
    let event_type = event.typ.as_ref()?;
    let action = event.action.as_ref()?;

    if *event_type != EventMessageTypeEnum::CONTAINER {
        return None;
    }

    let actor = event.actor.as_ref()?;
    let attributes = actor.attributes.as_ref()?;
    let container_id = attributes.get("name")?.clone();
    let container_id_short = container_id.chars().take(12).collect::<String>();

    match action.as_str() {
        "start" => {
            let workers = active_workers.read().await;
            if workers.contains_key(&container_id) {
                Some(WorkerInfrastructureEvent::WorkerStarted {
                    provider_resource_id: container_id,
                    timestamp: Utc::now(),
                })
            } else {
                None
            }
        }
        "die" | "kill" => {
            let workers = active_workers.read().await;
            if workers.contains_key(&container_id) {
                let exit_code = attributes.get("exitCode").and_then(|s| s.parse().ok());
                let reason = if action == "kill" {
                    Some("forced".to_string())
                } else {
                    None
                };
                Some(WorkerInfrastructureEvent::WorkerStopped {
                    provider_resource_id: container_id,
                    timestamp: Utc::now(),
                    reason,
                    exit_code,
                })
            } else {
                None
            }
        }
        "health_status" | "healthcheck" => {
            let workers = active_workers.read().await;
            if workers.contains_key(&container_id) {
                let status_str = attributes
                    .get("healthStatus")
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let status = if status_str == "healthy" {
                    HealthStatus::Healthy
                } else if status_str == "unhealthy" {
                    HealthStatus::Unhealthy {
                        reason: format!("Container {} health check failed", container_id_short),
                    }
                } else {
                    HealthStatus::Unknown
                };
                Some(WorkerInfrastructureEvent::WorkerHealthChanged {
                    provider_resource_id: container_id,
                    status,
                    timestamp: Utc::now(),
                })
            } else {
                None
            }
        }
        _ => {
            debug!("Ignored Docker event: {}", action);
            None
        }
    }
}

// ============================================================================
// Docker Provider Builder
// ============================================================================

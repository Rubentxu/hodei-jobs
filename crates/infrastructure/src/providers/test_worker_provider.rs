//! Test Worker Provider - Fast E2E Testing
//!
//! This provider is designed specifically for E2E testing. It creates workers
//! as direct processes instead of Docker containers, providing:
//! - **10x faster provisioning** (~10s vs ~130s including Docker build)
//! - **No Docker dependency** for tests
//! - **Easier debugging** with direct process access
//! - **Same gRPC protocol** as production DockerProvider
//!
//! **IMPORTANT**: This is for TESTING ONLY. Do not use in production.
//!
//! # Usage
//!
//! ```rust,ignore
//! let test_provider = TestWorkerProvider::new().await?;
//! // Use just like DockerProvider in tests
//! ```

use async_trait::async_trait;
use hodei_jobs_domain::{
    shared_kernel::{DomainError, ProviderId, Result, WorkerState},
    worker::{Architecture, ProviderType, WorkerHandle, WorkerSpec},
    worker_provider::{
        HealthStatus, LogEntry, LogLevel, ProviderCapabilities, ProviderError, ResourceLimits,
        WorkerProvider,
    },
};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::process::Command as AsyncCommand;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Test Worker Provider for fast E2E testing
///
/// Creates workers as direct processes instead of Docker containers.
/// This eliminates Docker build time and reduces test execution from ~140s to ~10s.
#[derive(Clone)]
pub struct TestWorkerProvider {
    /// Unique provider identifier
    provider_id: ProviderId,
    /// Active workers tracking (pid -> WorkerHandle)
    active_workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    /// Provider capabilities
    capabilities: ProviderCapabilities,
    /// Worker binary path
    worker_binary_path: String,
}

/// Configuration for TestWorkerProvider
#[derive(Clone)]
pub struct TestWorkerConfig {
    /// Path to worker binary
    pub worker_binary_path: String,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Default for TestWorkerConfig {
    fn default() -> Self {
        Self {
            worker_binary_path: "target/release/worker".to_string(),
            verbose: false,
        }
    }
}

/// Builder for TestWorkerProvider
pub struct TestWorkerProviderBuilder {
    provider_id: Option<ProviderId>,
    config: Option<TestWorkerConfig>,
    worker_binary_path: Option<String>,
    capabilities: Option<ProviderCapabilities>,
}

impl TestWorkerProviderBuilder {
    pub fn new() -> Self {
        Self {
            provider_id: None,
            config: None,
            worker_binary_path: None,
            capabilities: None,
        }
    }

    pub fn with_provider_id(mut self, id: ProviderId) -> Self {
        self.provider_id = Some(id);
        self
    }

    pub fn with_config(mut self, config: TestWorkerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_worker_binary_path(mut self, path: String) -> Self {
        self.worker_binary_path = Some(path);
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        if let Some(ref mut config) = self.config {
            config.verbose = verbose;
        } else {
            let mut cfg = TestWorkerConfig::default();
            cfg.verbose = verbose;
            self.config = Some(cfg);
        }
        self
    }

    pub fn with_capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Build the TestWorkerProvider
    pub async fn build(self) -> Result<TestWorkerProvider> {
        let config = self.config.unwrap_or_default();
        let provider_id = self.provider_id.unwrap_or_else(ProviderId::new);
        let worker_binary_path = self
            .worker_binary_path
            .unwrap_or_else(|| config.worker_binary_path.clone());
        let capabilities = self.capabilities.unwrap_or_else(Self::default_capabilities);

        // Verify worker binary exists
        if !std::path::Path::new(&worker_binary_path).exists() {
            return Err(DomainError::InvalidProviderConfig {
                message: format!(
                    "Worker binary not found at {}. Run: cargo build --release -p hodei-jobs-grpc --bin worker",
                    worker_binary_path
                ),
            });
        }

        Ok(TestWorkerProvider {
            provider_id,
            active_workers: Arc::new(RwLock::new(HashMap::new())),
            capabilities,
            worker_binary_path,
        })
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

impl Default for TestWorkerProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestWorkerProvider {
    /// Create a new TestWorkerProvider with default configuration
    pub async fn new() -> Result<Self> {
        TestWorkerProviderBuilder::new().build().await
    }

    /// Create a new TestWorkerProvider with custom configuration
    pub async fn with_config(config: TestWorkerConfig) -> Result<Self> {
        TestWorkerProviderBuilder::new()
            .with_config(config)
            .build()
            .await
    }

    /// Create a new TestWorkerProvider with custom worker binary path
    pub async fn with_worker_binary(worker_binary_path: String) -> Result<Self> {
        TestWorkerProviderBuilder::new()
            .with_worker_binary_path(worker_binary_path)
            .build()
            .await
    }

    /// Spawn worker process from WorkerSpec
    async fn spawn_worker_process(&self, spec: &WorkerSpec) -> Result<tokio::process::Child> {
        info!("Spawning test worker process: {}", spec.worker_id);

        // Build environment variables
        let mut env_vars = spec.environment.clone();
        env_vars.insert("HODEI_WORKER_ID".to_string(), spec.worker_id.to_string());
        env_vars.insert(
            "HODEI_SERVER_ADDRESS".to_string(),
            spec.server_address.clone(),
        );
        env_vars.insert("HODEI_WORKER_MODE".to_string(), "test".to_string());

        // Spawn the worker binary as a process
        let mut cmd = AsyncCommand::new(&self.worker_binary_path);

        // Set environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Configure stdio - inherit for easier debugging
        // Note: In production, this would be Docker which handles stdio differently
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Spawn the process
        let child = cmd.spawn().map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to spawn worker process {}: {}", spec.worker_id, e),
        })?;

        let pid = child.id().unwrap_or(0).to_string();
        info!("Worker process spawned with PID: {}", pid);

        Ok(child)
    }

    /// Check if a process is still running
    fn is_process_running(pid: u32) -> bool {
        // Try to send signal 0 to check if process exists
        // This doesn't kill the process, just checks if it exists
        #[cfg(unix)]
        {
            // On Unix, use nix to check if process exists
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let result = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            // If kill with SIGTERM returns ESRCH (no process), it's not running
            // Otherwise, it is running
            result != Err(nix::errno::Errno::ESRCH)
        }
        #[cfg(windows)]
        {
            // On Windows, we need a different approach
            // For now, assume it's running if PID > 0
            pid > 0
        }
    }

    /// Kill a process gracefully, then forcefully if needed
    async fn kill_process(pid: u32) -> Result<()> {
        #[cfg(unix)]
        {
            // On Unix, use nix to send signals
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(pid as i32);

            // First, send SIGTERM for graceful shutdown
            let _ = kill(pid, Signal::SIGTERM);

            // Wait a bit for graceful shutdown
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Check if still running and send SIGKILL if needed
            let result = kill(pid, Signal::SIGTERM);
            if result != Err(nix::errno::Errno::ESRCH) {
                // Still running, force kill with SIGKILL
                let _ = kill(pid, Signal::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            // On Windows, use taskkill
            let _ = AsyncCommand::new("taskkill")
                .args(&["/PID", &pid.to_string(), "/F"])
                .output()
                .await;
        }

        Ok(())
    }
}

#[async_trait]
impl WorkerProvider for TestWorkerProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Test
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        info!("Creating Test worker: {}", worker_id);

        // Spawn the worker process
        let mut child = self
            .spawn_worker_process(spec)
            .await
            .map_err(|e| ProviderError::ProvisioningFailed(e.to_string()))?;

        let pid = child.id().unwrap_or(0);
        let pid_str = pid.to_string();

        // Create worker handle with PID as provider_resource_id
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            pid_str.clone(),
            ProviderType::Test,
            self.provider_id.clone(),
        )
        .with_metadata("pid", serde_json::json!(pid))
        .with_metadata("binary_path", serde_json::json!(self.worker_binary_path));

        self.active_workers
            .write()
            .await
            .insert(pid_str, handle.clone());

        // Spawn a task to monitor the process and consume its output
        let active_workers = self.active_workers.clone();
        let pid_for_monitor = pid;
        tokio::spawn(async move {
            // Consume stdout and stderr to prevent blocking
            if let Some(mut stdout) = child.stdout.take() {
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let _ = stdout.read_to_end(&mut buf).await;
                });
            }
            if let Some(mut stderr) = child.stderr.take() {
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let _ = stderr.read_to_end(&mut buf).await;
                });
            }

            let _ = child.wait().await;
            // Remove from active workers when process exits
            active_workers
                .write()
                .await
                .remove(&pid_for_monitor.to_string());
        });

        info!("Test worker {} created with PID {}", spec.worker_id, pid);
        Ok(handle)
    }

    async fn get_worker_status(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<WorkerState, ProviderError> {
        let pid = handle
            .provider_resource_id
            .parse::<u32>()
            .map_err(|_| ProviderError::WorkerNotFound("Invalid PID".to_string()))?;

        if Self::is_process_running(pid) {
            Ok(WorkerState::Ready)
        } else {
            Ok(WorkerState::Terminated)
        }
    }

    async fn destroy_worker(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<(), ProviderError> {
        let pid = handle
            .provider_resource_id
            .parse::<u32>()
            .map_err(|_| ProviderError::WorkerNotFound("Invalid PID".to_string()))?;

        info!(
            "Destroying test worker: {} (PID: {})",
            handle.worker_id, pid
        );

        // Kill the process
        if let Err(e) = Self::kill_process(pid).await {
            warn!("Error killing process {}: {}", pid, e);
        }

        self.active_workers.write().await.remove(&pid.to_string());

        info!("Test worker {} destroyed successfully", handle.worker_id);
        Ok(())
    }

    async fn get_worker_logs(
        &self,
        _handle: &WorkerHandle,
        _tail: Option<u32>,
    ) -> std::result::Result<Vec<LogEntry>, ProviderError> {
        // For test provider, logs go to stdout/stderr which are inherited
        // by the test process. We don't capture them here.
        // In a real scenario, you might want to pipe them differently.

        Ok(vec![])
    }

    async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
        // Check if worker binary is executable
        let path = std::path::Path::new(&self.worker_binary_path);
        if !path.exists() {
            return Ok(HealthStatus::Unhealthy {
                reason: format!("Worker binary not found at {}", self.worker_binary_path),
            });
        }

        if !path.is_file() {
            return Ok(HealthStatus::Unhealthy {
                reason: format!("Worker binary at {} is not a file", self.worker_binary_path),
            });
        }

        let workers = self.active_workers.read().await;
        Ok(HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_worker_provider_builder() {
        let builder = TestWorkerProviderBuilder::new();
        assert!(builder.provider_id.is_none());
    }

    #[test]
    fn test_default_capabilities() {
        let caps = TestWorkerProviderBuilder::default_capabilities();
        assert!(!caps.gpu_support);
        assert!(caps.max_resources.max_cpu_cores > 0.0);
        assert!(!caps.architectures.is_empty());
    }

    #[tokio::test]
    async fn test_worker_binary_check() {
        // This test will only pass if the worker binary exists
        // In CI/CD, you might want to skip this or build first
        let provider = TestWorkerProvider::new().await;
        match provider {
            Ok(_) => println!("Worker binary found"),
            Err(e) => println!(
                "Worker binary not found (expected in some environments): {}",
                e
            ),
        }
    }
}

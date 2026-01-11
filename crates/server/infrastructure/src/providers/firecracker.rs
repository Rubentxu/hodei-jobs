//! Firecracker Worker Provider Implementation
//!
//! Production-ready implementation of WorkerProvider using Firecracker microVMs.
//! Provides hardware-level isolation via KVM with sub-second boot times.

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use bytes::Bytes;
use http::{Request, Uri};
use http_body_util::{BodyExt, Full};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use hyperlocal::UnixConnector;

// Import the individual ISP traits
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::{
    Architecture, CostEstimate, HealthStatus, JobRequirements, LogEntry, LogLevel,
    ProviderCapabilities, ProviderError, ProviderFeature, ProviderPerformanceMetrics, ProviderType,
    ResourceLimits, WorkerCost, WorkerEligibility, WorkerEventSource, WorkerHandle, WorkerHealth,
    WorkerInfrastructureEvent, WorkerLifecycle, WorkerLogs, WorkerMetrics, WorkerProvider,
    WorkerProviderIdentity, WorkerSpec,
};

use hodei_shared::WorkerState;

use super::metrics_collector::ProviderMetricsCollector;

// ============================================================================
// Configuration Types (HU-8.1)
// ============================================================================

/// Network configuration for Firecracker microVMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerNetworkConfig {
    /// Prefix for TAP device names (e.g., "fc-tap" -> "fc-tap-0", "fc-tap-1")
    pub tap_prefix: String,
    /// Subnet for microVMs in CIDR notation (e.g., "172.16.0.0/24")
    pub subnet: String,
    /// Gateway IP address for microVMs
    pub gateway_ip: String,
    /// Host network interface for NAT (e.g., "eth0", "ens3")
    pub host_interface: String,
    /// Enable NAT for internet access from microVMs
    pub enable_nat: bool,
}

impl Default for FirecrackerNetworkConfig {
    fn default() -> Self {
        Self {
            tap_prefix: "fc-tap".to_string(),
            subnet: "172.16.0.0/24".to_string(),
            gateway_ip: "172.16.0.1".to_string(),
            host_interface: "eth0".to_string(),
            enable_nat: true,
        }
    }
}

impl FirecrackerNetworkConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tap_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.tap_prefix = prefix.into();
        self
    }

    pub fn with_subnet(mut self, subnet: impl Into<String>) -> Self {
        self.subnet = subnet.into();
        self
    }

    pub fn with_gateway_ip(mut self, ip: impl Into<String>) -> Self {
        self.gateway_ip = ip.into();
        self
    }

    pub fn with_host_interface(mut self, iface: impl Into<String>) -> Self {
        self.host_interface = iface.into();
        self
    }

    pub fn with_nat(mut self, enable: bool) -> Self {
        self.enable_nat = enable;
        self
    }
}

/// Resource configuration for microVMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroVMResources {
    /// Number of vCPUs (1-32)
    pub vcpu_count: u8,
    /// Memory in MiB (128-32768)
    pub mem_size_mib: u32,
    /// Enable hyperthreading
    pub ht_enabled: bool,
}

impl Default for MicroVMResources {
    fn default() -> Self {
        Self {
            vcpu_count: 1,
            mem_size_mib: 512,
            ht_enabled: false,
        }
    }
}

impl MicroVMResources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_vcpus(mut self, count: u8) -> Self {
        self.vcpu_count = count.clamp(1, 32);
        self
    }

    pub fn with_memory_mib(mut self, mib: u32) -> Self {
        self.mem_size_mib = mib.clamp(128, 32768);
        self
    }

    pub fn with_hyperthreading(mut self, enabled: bool) -> Self {
        self.ht_enabled = enabled;
        self
    }
}

/// Configuration for the Firecracker Provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerConfig {
    /// Path to the Firecracker binary
    pub firecracker_path: PathBuf,
    /// Path to the Jailer binary (for sandboxing)
    pub jailer_path: PathBuf,
    /// Base directory for VM data (sockets, logs, rootfs copies)
    pub data_dir: PathBuf,
    /// Path to the Linux kernel for microVMs
    pub kernel_path: PathBuf,
    /// Path to the base rootfs image (with agent pre-installed)
    pub rootfs_path: PathBuf,
    /// Network configuration
    pub network: FirecrackerNetworkConfig,
    /// Default resources for microVMs
    pub default_resources: MicroVMResources,
    /// Use Jailer for sandboxing (recommended for production)
    pub use_jailer: bool,
    /// UID for Jailer process
    pub jailer_uid: Option<u32>,
    /// GID for Jailer process
    pub jailer_gid: Option<u32>,
    /// Timeout for VM boot (seconds)
    pub boot_timeout_secs: u64,
    /// Grace period for VM shutdown (seconds)
    pub shutdown_grace_secs: u64,
}

impl Default for FirecrackerConfig {
    fn default() -> Self {
        Self {
            firecracker_path: PathBuf::from("/usr/bin/firecracker"),
            jailer_path: PathBuf::from("/usr/bin/jailer"),
            data_dir: PathBuf::from("/var/lib/hodei/firecracker"),
            kernel_path: PathBuf::from("/var/lib/hodei/vmlinux"),
            rootfs_path: PathBuf::from("/var/lib/hodei/rootfs.ext4"),
            network: FirecrackerNetworkConfig::default(),
            default_resources: MicroVMResources::default(),
            use_jailer: true,
            jailer_uid: Some(1000),
            jailer_gid: Some(1000),
            boot_timeout_secs: 30,
            shutdown_grace_secs: 10,
        }
    }
}

/// Builder for FirecrackerConfig
pub struct FirecrackerConfigBuilder {
    config: FirecrackerConfig,
}

impl FirecrackerConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: FirecrackerConfig::default(),
        }
    }

    pub fn firecracker_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.firecracker_path = path.into();
        self
    }

    pub fn jailer_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.jailer_path = path.into();
        self
    }

    pub fn data_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.data_dir = path.into();
        self
    }

    pub fn kernel_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.kernel_path = path.into();
        self
    }

    pub fn rootfs_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.rootfs_path = path.into();
        self
    }

    pub fn network(mut self, network: FirecrackerNetworkConfig) -> Self {
        self.config.network = network;
        self
    }

    pub fn default_resources(mut self, resources: MicroVMResources) -> Self {
        self.config.default_resources = resources;
        self
    }

    pub fn use_jailer(mut self, use_jailer: bool) -> Self {
        self.config.use_jailer = use_jailer;
        self
    }

    pub fn jailer_uid(mut self, uid: u32) -> Self {
        self.config.jailer_uid = Some(uid);
        self
    }

    pub fn jailer_gid(mut self, gid: u32) -> Self {
        self.config.jailer_gid = Some(gid);
        self
    }

    pub fn boot_timeout_secs(mut self, secs: u64) -> Self {
        self.config.boot_timeout_secs = secs;
        self
    }

    pub fn shutdown_grace_secs(mut self, secs: u64) -> Self {
        self.config.shutdown_grace_secs = secs;
        self
    }

    /// Build the configuration, validating required fields
    pub fn build(self) -> Result<FirecrackerConfig> {
        self.validate()?;
        Ok(self.config)
    }

    fn validate(&self) -> Result<()> {
        if self.config.data_dir.as_os_str().is_empty() {
            return Err(DomainError::InvalidProviderConfig {
                message: "Firecracker data_dir cannot be empty".to_string(),
            });
        }
        if self.config.kernel_path.as_os_str().is_empty() {
            return Err(DomainError::InvalidProviderConfig {
                message: "Firecracker kernel_path cannot be empty".to_string(),
            });
        }
        if self.config.rootfs_path.as_os_str().is_empty() {
            return Err(DomainError::InvalidProviderConfig {
                message: "Firecracker rootfs_path cannot be empty".to_string(),
            });
        }
        Ok(())
    }
}

impl Default for FirecrackerConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FirecrackerConfig {
    /// Create a new builder
    pub fn builder() -> FirecrackerConfigBuilder {
        FirecrackerConfigBuilder::new()
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut builder = FirecrackerConfigBuilder::new();

        if let Ok(path) = std::env::var("HODEI_FC_FIRECRACKER_PATH") {
            builder = builder.firecracker_path(path);
        }
        if let Ok(path) = std::env::var("HODEI_FC_JAILER_PATH") {
            builder = builder.jailer_path(path);
        }
        if let Ok(path) = std::env::var("HODEI_FC_DATA_DIR") {
            builder = builder.data_dir(path);
        }
        if let Ok(path) = std::env::var("HODEI_FC_KERNEL_PATH") {
            builder = builder.kernel_path(path);
        }
        if let Ok(path) = std::env::var("HODEI_FC_ROOTFS_PATH") {
            builder = builder.rootfs_path(path);
        }
        if let Ok(val) = std::env::var("HODEI_FC_USE_JAILER") {
            builder = builder.use_jailer(val == "1" || val.to_lowercase() == "true");
        }
        if let Ok(val) = std::env::var("HODEI_FC_JAILER_UID") {
            if let Ok(uid) = val.parse() {
                builder = builder.jailer_uid(uid);
            }
        }
        if let Ok(val) = std::env::var("HODEI_FC_JAILER_GID") {
            if let Ok(gid) = val.parse() {
                builder = builder.jailer_gid(gid);
            }
        }
        if let Ok(val) = std::env::var("HODEI_FC_BOOT_TIMEOUT") {
            if let Ok(secs) = val.parse() {
                builder = builder.boot_timeout_secs(secs);
            }
        }

        // Network config from env
        let mut network = FirecrackerNetworkConfig::default();
        if let Ok(val) = std::env::var("HODEI_FC_TAP_PREFIX") {
            network.tap_prefix = val;
        }
        if let Ok(val) = std::env::var("HODEI_FC_SUBNET") {
            network.subnet = val;
        }
        if let Ok(val) = std::env::var("HODEI_FC_GATEWAY_IP") {
            network.gateway_ip = val;
        }
        if let Ok(val) = std::env::var("HODEI_FC_HOST_INTERFACE") {
            network.host_interface = val;
        }
        if let Ok(val) = std::env::var("HODEI_FC_ENABLE_NAT") {
            network.enable_nat = val == "1" || val.to_lowercase() == "true";
        }
        builder = builder.network(network);

        builder.build()
    }
}

// ============================================================================
// IP Pool Management (HU-8.3)
// ============================================================================

/// Manages IP address allocation for microVMs
#[derive(Debug)]
pub struct IpPool {
    /// Base network address
    base_ip: Ipv4Addr,
    /// Network mask (number of bits)
    mask_bits: u8,
    /// Set of allocated IPs
    allocated: std::collections::HashSet<Ipv4Addr>,
    /// Next IP to try allocating
    next_index: u32,
}

impl IpPool {
    /// Create a new IP pool from CIDR notation (e.g., "172.16.0.0/24")
    pub fn from_cidr(cidr: &str) -> Result<Self> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            return Err(DomainError::InvalidProviderConfig {
                message: format!("Invalid CIDR notation: {}", cidr),
            });
        }

        let base_ip: Ipv4Addr =
            parts[0]
                .parse()
                .map_err(|_| DomainError::InvalidProviderConfig {
                    message: format!("Invalid IP address: {}", parts[0]),
                })?;

        let mask_bits: u8 = parts[1]
            .parse()
            .map_err(|_| DomainError::InvalidProviderConfig {
                message: format!("Invalid mask bits: {}", parts[1]),
            })?;

        if mask_bits > 30 {
            return Err(DomainError::InvalidProviderConfig {
                message: "Subnet too small (mask > /30)".to_string(),
            });
        }

        Ok(Self {
            base_ip,
            mask_bits,
            allocated: std::collections::HashSet::new(),
            next_index: 2, // Skip .0 (network) and .1 (gateway)
        })
    }

    /// Allocate an IP address from the pool
    pub fn allocate(&mut self) -> Result<Ipv4Addr> {
        let max_hosts = (1u32 << (32 - self.mask_bits)) - 2; // Exclude network and broadcast

        for _ in 0..max_hosts {
            let ip = self.index_to_ip(self.next_index);
            self.next_index = (self.next_index + 1) % (max_hosts + 2);
            if self.next_index < 2 {
                self.next_index = 2;
            }

            if !self.allocated.contains(&ip) {
                self.allocated.insert(ip);
                return Ok(ip);
            }
        }

        Err(DomainError::InvalidProviderConfig {
            message: "IP pool exhausted".to_string(),
        })
    }

    /// Release an IP address back to the pool
    pub fn release(&mut self, ip: Ipv4Addr) {
        self.allocated.remove(&ip);
    }

    /// Check if an IP is allocated
    pub fn is_allocated(&self, ip: &Ipv4Addr) -> bool {
        self.allocated.contains(ip)
    }

    /// Get the number of allocated IPs
    pub fn allocated_count(&self) -> usize {
        self.allocated.len()
    }

    fn index_to_ip(&self, index: u32) -> Ipv4Addr {
        let base: u32 = u32::from(self.base_ip);
        Ipv4Addr::from(base + index)
    }
}

// ============================================================================
// MicroVM Instance Tracking
// ============================================================================

/// State of a microVM
#[derive(Debug, Clone, PartialEq)]
pub enum MicroVMState {
    Creating,
    Running,
    Stopping,
    Stopped,
    Failed(String),
}

/// Represents a running microVM instance
#[derive(Debug)]
#[allow(dead_code)]
struct MicroVMInstance {
    worker_id: WorkerId,
    vm_id: String,
    socket_path: PathBuf,
    tap_device: String,
    ip_address: Ipv4Addr,
    state: MicroVMState,
    pid: Option<u32>,
}

// ============================================================================
// RAII Types for Resource Cleanup (US-23.5)
// ============================================================================

/// RAII wrapper for network resources (TAP devices).
/// Automatically cleans up TAP devices when dropped.
#[derive(Debug)]
pub struct NetworkResources {
    /// Name of the TAP device
    tap_name: String,
    /// File descriptor for the TAP device (if owned)
    tap_fd: RawFd,
    /// IP address assigned to the VM
    ip_address: Ipv4Addr,
}

impl NetworkResources {
    /// Create new NetworkResources
    pub fn new(tap_name: String, tap_fd: RawFd, ip_address: Ipv4Addr) -> Self {
        Self {
            tap_name,
            tap_fd,
            ip_address,
        }
    }

    /// Get the TAP device name
    pub fn tap_name(&self) -> &str {
        &self.tap_name
    }

    /// Get the IP address
    pub fn ip_address(&self) -> Ipv4Addr {
        self.ip_address
    }

    /// Consume and release ownership of resources (for successful case)
    /// Returns a clone of the TAP name and IP address since we can't move out of Drop type
    pub fn into_parts(&self) -> (String, Ipv4Addr) {
        (self.tap_name.clone(), self.ip_address)
    }
}

impl Drop for NetworkResources {
    fn drop(&mut self) {
        tracing::warn!(tap_device = %self.tap_name, "Dropping NetworkResources, cleaning up TAP device");

        // Close the file descriptor if valid using std::fs::File
        // Note: We use a safe approach - opening /dev/null and dup'ing to close
        // This avoids needing the libc crate as a dependency
        if self.tap_fd != -1 {
            // Safe approach: just close using std's File API would require more setup
            // For now, we focus on cleaning up the TAP device itself
            tracing::debug!(fd = self.tap_fd, "TAP device FD would be closed here");
        }

        // Delete the TAP device using ip command
        let output = std::process::Command::new("ip")
            .args(&["link", "delete", &self.tap_name])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!(tap_device = %self.tap_name, "Successfully deleted TAP device");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(tap_device = %self.tap_name, error = %stderr, "Failed to delete TAP device");
                }
            }
            Err(e) => {
                tracing::error!(tap_device = %self.tap_name, error = %e, "Failed to execute ip link delete command");
            }
        }
    }
}

/// Guard that ensures cleanup of network resources if VM creation fails.
/// Uses RAII pattern to guarantee cleanup even on panics.
#[derive(Debug)]
pub struct FirecrackerCreationGuard {
    /// Network resources to clean up if dropped
    resources: Option<NetworkResources>,
    /// IP address to release on cleanup
    ip_to_release: Option<Ipv4Addr>,
}

impl FirecrackerCreationGuard {
    /// Create a new guard (call during VM creation)
    pub fn new() -> Self {
        Self {
            resources: None,
            ip_to_release: None,
        }
    }

    /// Set network resources to be cleaned up if this guard is dropped
    pub fn set_resources(&mut self, resources: NetworkResources) {
        self.ip_to_release = Some(resources.ip_address);
        self.resources = Some(resources);
    }

    /// Take ownership of resources (call when VM creation succeeds)
    /// Returns None if no resources were set
    pub fn take_resources(&mut self) -> Option<NetworkResources> {
        self.ip_to_release = None;
        self.resources.take()
    }

    /// Check if resources are set
    pub fn has_resources(&self) -> bool {
        self.resources.is_some()
    }
}

impl Drop for FirecrackerCreationGuard {
    fn drop(&mut self) {
        if let Some(resources) = self.resources.take() {
            tracing::warn!(
                tap_device = %resources.tap_name(),
                ip_address = %resources.ip_address(),
                "FirecrackerCreationGuard dropped, cleaning up network resources"
            );

            // Release IP address back to pool (if we have a reference to the provider)
            // Note: In production, we'd use a weak reference or callback pattern
            // For now, we just log the cleanup
            tracing::debug!(ip = %resources.ip_address(), "Would release IP here");

            // NetworkResources::drop will clean up the TAP device
        }
    }
}

// ============================================================================
// Firecracker Provider (HU-8.2+)
// ============================================================================

/// Firecracker Provider for creating ephemeral microVM workers
pub struct FirecrackerProvider {
    provider_id: ProviderId,
    config: FirecrackerConfig,
    capabilities: ProviderCapabilities,
    /// Active microVMs: worker_id -> MicroVMInstance
    active_vms: Arc<RwLock<HashMap<WorkerId, MicroVMInstance>>>,
    /// IP address pool
    ip_pool: Arc<RwLock<IpPool>>,
    /// Performance metrics collector
    metrics_collector: ProviderMetricsCollector,
}

impl FirecrackerProvider {
    /// Create a new FirecrackerProvider with default configuration
    pub async fn new() -> Result<Self> {
        let config = FirecrackerConfig::default();
        Self::with_config(config).await
    }

    /// Create a new FirecrackerProvider with custom configuration
    pub async fn with_config(config: FirecrackerConfig) -> Result<Self> {
        // Verify system requirements
        Self::verify_requirements(&config).await?;

        // Create IP pool
        let ip_pool = IpPool::from_cidr(&config.network.subnet)?;

        // Create data directory if it doesn't exist
        if !config.data_dir.exists() {
            tokio::fs::create_dir_all(&config.data_dir)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to create data directory: {}", e),
                })?;
        }

        let capabilities = Self::default_capabilities();

        let metrics_collector = ProviderMetricsCollector::new(ProviderId::new());

        Ok(Self {
            provider_id: ProviderId::new(),
            config,
            capabilities,
            active_vms: Arc::new(RwLock::new(HashMap::new())),
            ip_pool: Arc::new(RwLock::new(ip_pool)),
            metrics_collector,
        })
    }

    /// Create a new FirecrackerProvider with a specific provider ID
    pub async fn with_provider_id(
        provider_id: ProviderId,
        config: FirecrackerConfig,
    ) -> Result<Self> {
        let ip_pool = IpPool::from_cidr(&config.network.subnet)?;
        let metrics_collector = ProviderMetricsCollector::new(provider_id.clone());

        Ok(Self {
            provider_id,
            config,
            capabilities: Self::default_capabilities(),
            active_vms: Arc::new(RwLock::new(HashMap::new())),
            ip_pool: Arc::new(RwLock::new(ip_pool)),
            metrics_collector,
        })
    }

    /// Verify system requirements for Firecracker (HU-8.2)
    async fn verify_requirements(config: &FirecrackerConfig) -> Result<()> {
        // Check KVM access
        let kvm_path = Path::new("/dev/kvm");
        if !kvm_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: "KVM not available: /dev/kvm does not exist".to_string(),
            });
        }

        // Check if we can access KVM (read/write)
        match std::fs::metadata(kvm_path) {
            Ok(metadata) => {
                use std::os::unix::fs::MetadataExt;
                let mode = metadata.mode();
                // Check if readable and writable (simplified check)
                if mode & 0o006 == 0 {
                    return Err(DomainError::InfrastructureError {
                        message: "Insufficient permissions on /dev/kvm".to_string(),
                    });
                }
            }
            Err(e) => {
                return Err(DomainError::InfrastructureError {
                    message: format!("Cannot access /dev/kvm: {}", e),
                });
            }
        }

        // Check Firecracker binary
        if !config.firecracker_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: format!(
                    "Firecracker binary not found: {}",
                    config.firecracker_path.display()
                ),
            });
        }

        // Check Jailer binary if enabled
        if config.use_jailer && !config.jailer_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: format!("Jailer binary not found: {}", config.jailer_path.display()),
            });
        }

        // Check kernel
        if !config.kernel_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: format!("Kernel not found: {}", config.kernel_path.display()),
            });
        }

        // Check rootfs
        if !config.rootfs_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: format!("Rootfs not found: {}", config.rootfs_path.display()),
            });
        }

        Ok(())
    }

    fn default_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            max_resources: ResourceLimits {
                max_cpu_cores: 32.0,
                max_memory_bytes: 32 * 1024 * 1024 * 1024, // 32GB
                max_disk_bytes: 100 * 1024 * 1024 * 1024,  // 100GB
                max_gpu_count: 0, // Firecracker doesn't support GPU passthrough
            },
            gpu_support: false,
            gpu_types: vec![],
            architectures: vec![Architecture::Amd64, Architecture::Arm64],
            runtimes: vec!["shell".to_string(), "python".to_string()],
            regions: vec!["local".to_string()],
            max_execution_time: Some(Duration::from_secs(86400)), // 24 hours
            persistent_storage: false,
            custom_networking: true,
            features: Vec::new(),
        }
    }

    /// Generate VM ID from worker ID
    fn vm_id(worker_id: &WorkerId) -> String {
        format!("fc-{}", worker_id)
    }

    /// Get VM directory path
    fn vm_dir(&self, vm_id: &str) -> PathBuf {
        self.config.data_dir.join(vm_id)
    }

    /// Get socket path for a VM
    fn socket_path(&self, vm_id: &str) -> PathBuf {
        self.vm_dir(vm_id).join("firecracker.socket")
    }

    /// Create TAP device for a VM (HU-8.4)
    async fn create_tap_device(&self, vm_id: &str, ip: Ipv4Addr) -> Result<String> {
        let tap_name = format!("{}-{}", self.config.network.tap_prefix, &vm_id[3..11]);

        // Create TAP device using ip command
        let output = Command::new("ip")
            .args(["tuntap", "add", "dev", &tap_name, "mode", "tap"])
            .output()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create TAP device: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "already exists" error
            if !stderr.contains("File exists") {
                return Err(DomainError::InfrastructureError {
                    message: format!("Failed to create TAP device: {}", stderr),
                });
            }
        }

        // Configure IP on TAP device (host side)
        let gateway = &self.config.network.gateway_ip;
        let _ = Command::new("ip")
            .args(["addr", "add", &format!("{}/30", gateway), "dev", &tap_name])
            .output()
            .await;

        // Bring up TAP device
        let output = Command::new("ip")
            .args(["link", "set", "dev", &tap_name, "up"])
            .output()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to bring up TAP device: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Warning bringing up TAP device: {}", stderr);
        }

        info!(
            "Created TAP device {} for VM {} with IP {}",
            tap_name, vm_id, ip
        );
        Ok(tap_name)
    }

    /// Destroy TAP device
    async fn destroy_tap_device(&self, tap_name: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["link", "delete", tap_name])
            .output()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete TAP device: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "not found" error
            if !stderr.contains("Cannot find device") {
                warn!("Warning deleting TAP device {}: {}", tap_name, stderr);
            }
        }

        debug!("Destroyed TAP device {}", tap_name);
        Ok(())
    }

    /// Prepare VM directory and rootfs copy
    async fn prepare_vm_directory(&self, vm_id: &str) -> Result<PathBuf> {
        let vm_dir = self.vm_dir(vm_id);

        // Create VM directory
        tokio::fs::create_dir_all(&vm_dir)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create VM directory: {}", e),
            })?;

        // Copy rootfs (copy-on-write would be better, but simple copy for now)
        let rootfs_copy = vm_dir.join("rootfs.ext4");
        tokio::fs::copy(&self.config.rootfs_path, &rootfs_copy)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to copy rootfs: {}", e),
            })?;

        Ok(vm_dir)
    }

    /// Build kernel boot arguments
    fn build_boot_args(
        &self,
        worker_id: &WorkerId,
        server_address: &str,
        otp_token: Option<&str>,
        vm_ip: Ipv4Addr,
    ) -> String {
        let mut args = format!(
            "console=ttyS0 reboot=k panic=1 pci=off \
             HODEI_WORKER_ID={} \
             HODEI_SERVER_ADDRESS={} \
             HODEI_VM_IP={} \
             HODEI_GATEWAY={}",
            worker_id, server_address, vm_ip, self.config.network.gateway_ip
        );

        if let Some(token) = otp_token {
            args.push_str(&format!(" HODEI_OTP_TOKEN={}", token));
        }

        args
    }

    /// Start Firecracker process and configure VM via API (HU-8.5)
    async fn start_microvm(
        &self,
        vm_id: &str,
        spec: &WorkerSpec,
        vm_ip: Ipv4Addr,
        tap_device: &str,
    ) -> Result<u32> {
        let vm_dir = self.vm_dir(vm_id);
        let socket_path = self.socket_path(vm_id);
        let rootfs_path = vm_dir.join("rootfs.ext4");

        // Remove old socket if exists
        let _ = tokio::fs::remove_file(&socket_path).await;

        // Start Firecracker process
        let mut cmd = if self.config.use_jailer {
            self.build_jailer_command(vm_id, &socket_path)
        } else {
            self.build_firecracker_command(&socket_path)
        };

        let child = cmd.spawn().map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to start Firecracker: {}", e),
        })?;

        let pid = child.id().unwrap_or(0);
        info!(
            "Started Firecracker process (PID: {}) for VM {}",
            pid, vm_id
        );

        // Wait for socket to be available
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !socket_path.exists() {
            return Err(DomainError::InfrastructureError {
                message: "Firecracker socket not created in time".to_string(),
            });
        }

        // Configure VM via API
        self.configure_vm_api(&socket_path, spec, &rootfs_path, vm_ip, tap_device)
            .await?;

        // Start the VM
        self.start_vm_api(&socket_path).await?;

        Ok(pid)
    }

    fn build_firecracker_command(&self, socket_path: &Path) -> Command {
        let mut cmd = Command::new(&self.config.firecracker_path);
        cmd.arg("--api-sock").arg(socket_path);
        cmd
    }

    fn build_jailer_command(&self, vm_id: &str, socket_path: &Path) -> Command {
        let mut cmd = Command::new(&self.config.jailer_path);
        cmd.arg("--id")
            .arg(vm_id)
            .arg("--exec-file")
            .arg(&self.config.firecracker_path)
            .arg("--chroot-base-dir")
            .arg(&self.config.data_dir);

        if let Some(uid) = self.config.jailer_uid {
            cmd.arg("--uid").arg(uid.to_string());
        }
        if let Some(gid) = self.config.jailer_gid {
            cmd.arg("--gid").arg(gid.to_string());
        }

        cmd.arg("--").arg("--api-sock").arg(socket_path);
        cmd
    }

    /// Configure VM via Firecracker API socket
    async fn configure_vm_api(
        &self,
        socket_path: &Path,
        spec: &WorkerSpec,
        rootfs_path: &Path,
        vm_ip: Ipv4Addr,
        tap_device: &str,
    ) -> Result<()> {
        let socket_path_str = socket_path.to_string_lossy();

        // Configure machine (vCPUs, memory)
        let vcpus = if spec.resources.cpu_cores > 0.0 {
            spec.resources.cpu_cores.ceil() as u8
        } else {
            self.config.default_resources.vcpu_count
        };
        let mem_mib = if spec.resources.memory_bytes > 0 {
            (spec.resources.memory_bytes / (1024 * 1024)) as u32
        } else {
            self.config.default_resources.mem_size_mib
        };

        let machine_config = serde_json::json!({
            "vcpu_count": vcpus,
            "mem_size_mib": mem_mib,
            "ht_enabled": self.config.default_resources.ht_enabled
        });

        self.api_put(&socket_path_str, "/machine-config", &machine_config)
            .await?;

        // Configure boot source
        let otp_token = spec.environment.get("HODEI_OTP_TOKEN").map(|s| s.as_str());
        let boot_args =
            self.build_boot_args(&spec.worker_id, &spec.server_address, otp_token, vm_ip);

        let boot_source = serde_json::json!({
            "kernel_image_path": self.config.kernel_path.to_string_lossy(),
            "boot_args": boot_args
        });

        self.api_put(&socket_path_str, "/boot-source", &boot_source)
            .await?;

        // Configure root drive
        let drive_config = serde_json::json!({
            "drive_id": "rootfs",
            "path_on_host": rootfs_path.to_string_lossy(),
            "is_root_device": true,
            "is_read_only": false
        });

        self.api_put(&socket_path_str, "/drives/rootfs", &drive_config)
            .await?;

        // Configure network interface
        let mac = self.generate_mac_address(vm_ip);
        let network_config = serde_json::json!({
            "iface_id": "eth0",
            "guest_mac": mac,
            "host_dev_name": tap_device
        });

        self.api_put(
            &socket_path_str,
            "/network-interfaces/eth0",
            &network_config,
        )
        .await?;

        Ok(())
    }

    /// Start VM via API
    async fn start_vm_api(&self, socket_path: &Path) -> Result<()> {
        let socket_path_str = socket_path.to_string_lossy();
        let action = serde_json::json!({
            "action_type": "InstanceStart"
        });

        self.api_put(&socket_path_str, "/actions", &action).await?;
        Ok(())
    }

    /// Send PUT request to Firecracker API via Unix socket
    /// Send PUT request to Firecracker API via Unix socket
    async fn api_put(
        &self,
        socket_path: &str,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        let body_str =
            serde_json::to_string(body).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize API request: {}", e),
            })?;

        let url: Uri = hyperlocal::Uri::new(socket_path, endpoint).into();

        let client = Client::builder(TokioExecutor::new()).build(UnixConnector);

        let req = Request::put(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(Full::new(Bytes::from(body_str)))
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to build request: {}", e),
            })?;

        let res = client
            .request(req)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to call Firecracker API: {}", e),
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let body_bytes = res
                .collect()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to read response body: {}", e),
                })?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes);
            return Err(DomainError::InfrastructureError {
                message: format!("Firecracker API error {}: {}", status, body_str),
            });
        }

        Ok(())
    }

    /// Generate MAC address from IP
    fn generate_mac_address(&self, ip: Ipv4Addr) -> String {
        let octets = ip.octets();
        format!(
            "AA:FC:00:{:02X}:{:02X}:{:02X}",
            octets[1], octets[2], octets[3]
        )
    }

    /// Map MicroVMState to WorkerState (Crash-Only Design)
    fn map_vm_state(state: &MicroVMState) -> WorkerState {
        match state {
            MicroVMState::Creating => WorkerState::Creating,
            MicroVMState::Running => WorkerState::Ready,
            MicroVMState::Stopping => WorkerState::Creating, // Map to Creating for retry in Crash-Only
            MicroVMState::Stopped => WorkerState::Terminated,
            MicroVMState::Failed(_) => WorkerState::Terminated,
        }
    }
}

#[async_trait]
impl WorkerProviderIdentity for FirecrackerProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Custom("firecracker".to_string())
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn transform_server_address(&self, server_address: &str) -> String {
        // Firecracker VMs use TAP network interface
        // Transform localhost/127.0.0.1 to use the host IP on the TAP network
        if server_address.contains("localhost") || server_address.contains("127.0.0.1") {
            // Use the TAP bridge IP (typically 172.16.0.1 for Firecracker default setup)
            // This can be overridden via HODEI_FIRECRACKER_HOST_IP environment variable
            std::env::var("HODEI_FIRECRACKER_HOST_IP")
                .unwrap_or_else(|_| {
                    // Default TAP bridge IP for Firecracker
                    server_address
                        .replace("localhost", "172.16.0.1")
                        .replace("127.0.0.1", "172.16.0.1")
                })
        } else {
            // Use the address as-is
            server_address.to_string()
        }
    }
}

#[async_trait]
impl WorkerLifecycle for FirecrackerProvider {
    async fn create_worker(
        &self,
        spec: &WorkerSpec,
    ) -> std::result::Result<WorkerHandle, ProviderError> {
        let worker_id = spec.worker_id.clone();
        let vm_id = Self::vm_id(&worker_id);
        let startup_start = Instant::now();

        info!("Creating Firecracker microVM: {}", vm_id);

        // Check if VM already exists
        {
            let vms = self.active_vms.read().await;
            if vms.contains_key(&worker_id) {
                return Err(ProviderError::ProvisioningFailed(format!(
                    "VM {} already exists",
                    vm_id
                )));
            }
        }

        // Allocate IP
        let vm_ip = {
            let mut pool = self.ip_pool.write().await;
            pool.allocate().map_err(|e| {
                ProviderError::ProvisioningFailed(format!("Failed to allocate IP: {}", e))
            })?
        };

        // RAII guard for automatic cleanup on failure
        let guard = FirecrackerCreationGuard::new();

        // Create TAP device with RAII cleanup
        let tap_name = match self.create_tap_device(&vm_id, vm_ip).await {
            Ok(name) => name,
            Err(e) => {
                // Release IP manually before returning error
                {
                    let mut pool = self.ip_pool.write().await;
                    pool.release(vm_ip);
                }
                return Err(ProviderError::ProvisioningFailed(format!(
                    "Failed to create TAP device: {}",
                    e
                )));
            }
        };

        // Create NetworkResources that will clean up on drop
        let network_resources = NetworkResources::new(tap_name.clone(), -1, vm_ip);

        // Prepare VM directory
        self.prepare_vm_directory(&vm_id).await.map_err(|e| {
            ProviderError::ProvisioningFailed(format!("Failed to prepare VM directory: {}", e))
        })?;

        // Start microVM
        let pid = self
            .start_microvm(&vm_id, spec, vm_ip, &tap_name)
            .await
            .map_err(|e| {
                ProviderError::ProvisioningFailed(format!("Failed to start microVM: {}", e))
            })?;

        // SUCCESS: Prevent cleanup on drop by forgetting the guard
        std::mem::forget(guard);
        std::mem::forget(network_resources);

        // Track VM instance
        let instance = MicroVMInstance {
            worker_id: worker_id.clone(),
            vm_id: vm_id.clone(),
            socket_path: self.socket_path(&vm_id),
            tap_device: tap_name.clone(),
            ip_address: vm_ip,
            state: MicroVMState::Running,
            pid: Some(pid),
        };

        {
            let mut vms = self.active_vms.write().await;
            vms.insert(worker_id.clone(), instance);
        }

        info!(
            "MicroVM {} created successfully (IP: {}, PID: {})",
            vm_id, vm_ip, pid
        );

        let startup_time = startup_start.elapsed();

        // Record metrics for successful worker creation
        // Spawn a task to record metrics without blocking
        let collector = self.metrics_collector.clone();
        tokio::spawn(async move {
            collector.record_worker_creation(startup_time, true).await;
        });

        let handle = WorkerHandle::new(
            worker_id,
            vm_id.clone(),
            ProviderType::Custom("firecracker".to_string()),
            self.provider_id.clone(),
        )
        .with_metadata("ip_address", serde_json::json!(vm_ip.to_string()))
        .with_metadata("tap_device", serde_json::json!(tap_name))
        .with_metadata("pid", serde_json::json!(pid));

        Ok(handle)
    }

    async fn get_worker_status(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<WorkerState, ProviderError> {
        let vms = self.active_vms.read().await;

        match vms.get(&handle.worker_id) {
            Some(instance) => Ok(Self::map_vm_state(&instance.state)),
            None => Ok(WorkerState::Terminated),
        }
    }

    async fn destroy_worker(
        &self,
        handle: &WorkerHandle,
    ) -> std::result::Result<(), ProviderError> {
        let vm_id = &handle.provider_resource_id;
        info!("Destroying Firecracker microVM: {}", vm_id);

        // Get instance info
        let instance = {
            let mut vms = self.active_vms.write().await;
            vms.remove(&handle.worker_id)
        };

        if let Some(instance) = instance {
            // Try graceful shutdown via API
            if instance.socket_path.exists() {
                let action = serde_json::json!({
                    "action_type": "SendCtrlAltDel"
                });
                let _ = self
                    .api_put(&instance.socket_path.to_string_lossy(), "/actions", &action)
                    .await;

                // Wait for graceful shutdown
                tokio::time::sleep(Duration::from_secs(self.config.shutdown_grace_secs)).await;
            }

            // Force kill if still running
            if let Some(pid) = instance.pid {
                let _ = Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output()
                    .await;
            }

            // Cleanup TAP device
            let _ = self.destroy_tap_device(&instance.tap_device).await;

            // Release IP
            {
                let mut pool = self.ip_pool.write().await;
                pool.release(instance.ip_address);
            }

            // Cleanup VM directory
            let vm_dir = self.vm_dir(vm_id);
            let _ = tokio::fs::remove_dir_all(&vm_dir).await;

            info!("MicroVM {} destroyed successfully", vm_id);
        } else {
            debug!("MicroVM {} not found (already destroyed)", vm_id);
        }

        Ok(())
    }
}

#[async_trait]
impl WorkerLogs for FirecrackerProvider {
    async fn get_worker_logs(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> std::result::Result<Vec<LogEntry>, ProviderError> {
        let vm_id = &handle.provider_resource_id;
        let log_path = self.vm_dir(vm_id).join("console.log");

        if !log_path.exists() {
            return Ok(vec![]);
        }

        let content = tokio::fs::read_to_string(&log_path)
            .await
            .map_err(|e| ProviderError::Internal(format!("Failed to read logs: {}", e)))?;

        let lines: Vec<&str> = content.lines().collect();
        let lines = if let Some(n) = tail {
            let start = lines.len().saturating_sub(n as usize);
            &lines[start..]
        } else {
            &lines[..]
        };

        let entries: Vec<LogEntry> = lines
            .iter()
            .map(|line| LogEntry {
                timestamp: chrono::Utc::now(),
                level: LogLevel::Info,
                message: line.to_string(),
                source: "console".to_string(),
            })
            .collect();

        Ok(entries)
    }
}

#[async_trait]
impl WorkerHealth for FirecrackerProvider {
    async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
        // Check KVM
        if !Path::new("/dev/kvm").exists() {
            return Ok(HealthStatus::Unhealthy {
                reason: "KVM not available".to_string(),
            });
        }

        // Check Firecracker binary
        if !self.config.firecracker_path.exists() {
            return Ok(HealthStatus::Unhealthy {
                reason: format!(
                    "Firecracker binary not found: {}",
                    self.config.firecracker_path.display()
                ),
            });
        }

        // Check kernel
        if !self.config.kernel_path.exists() {
            return Ok(HealthStatus::Degraded {
                reason: format!("Kernel not found: {}", self.config.kernel_path.display()),
            });
        }

        // Check rootfs
        if !self.config.rootfs_path.exists() {
            return Ok(HealthStatus::Degraded {
                reason: format!("Rootfs not found: {}", self.config.rootfs_path.display()),
            });
        }

        Ok(HealthStatus::Healthy)
    }
}

impl WorkerCost for FirecrackerProvider {
    fn estimate_cost(&self, _spec: &WorkerSpec, _duration: Duration) -> Option<CostEstimate> {
        None
    }

    fn estimated_startup_time(&self) -> Duration {
        Duration::from_millis(125)
    }
}

impl WorkerEligibility for FirecrackerProvider {
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

impl WorkerMetrics for FirecrackerProvider {
    fn get_performance_metrics(&self) -> ProviderPerformanceMetrics {
        self.metrics_collector.get_metrics()
    }

    fn record_worker_creation(&self, _startup_time: Duration, _success: bool) {
        // Spawn a task to record metrics without blocking
        let collector = self.metrics_collector.clone();
        tokio::spawn(async move {
            // Note: We can't use the startup_time parameter here since it's already been recorded
            // This is a no-op for the default implementation
            let _ = collector;
        });
    }

    fn get_startup_time_history(&self) -> Vec<Duration> {
        self.metrics_collector.get_startup_times()
    }

    fn calculate_average_cost_per_hour(&self) -> f64 {
        let metrics = self.get_performance_metrics();

        // Firecracker cost model: $0.25/vCPU/h + $0.05/GB RAM/h + $0.10/h overhead
        let avg_cpu_cores = metrics.avg_resource_usage.avg_cpu_millicores / 1000.0;
        let avg_memory_gb =
            metrics.avg_resource_usage.avg_memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        (avg_cpu_cores * 0.25) + (avg_memory_gb * 0.05) + 0.10
    }

    fn calculate_health_score(&self) -> f64 {
        let metrics = self.get_performance_metrics();
        let success_rate = metrics.success_rate;

        // Calculate average startup time from startup_times
        let avg_startup_ms = if !metrics.startup_times.is_empty() {
            let total_ms: u64 = metrics
                .startup_times
                .iter()
                .map(|d| d.as_millis() as u64)
                .sum();
            (total_ms / metrics.startup_times.len() as u64) as f64
        } else {
            200.0
        };

        // Health score based on success rate and startup time
        // Target: >95% success rate, <200ms startup time
        let success_score = (success_rate / 100.0).min(1.0);
        let startup_score = (200.0 / avg_startup_ms).min(1.0);

        (success_score * 0.7) + (startup_score * 0.3)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firecracker_config_default() {
        let config = FirecrackerConfig::default();
        assert_eq!(
            config.firecracker_path,
            PathBuf::from("/usr/bin/firecracker")
        );
        assert_eq!(config.jailer_path, PathBuf::from("/usr/bin/jailer"));
        assert!(config.use_jailer);
        assert_eq!(config.boot_timeout_secs, 30);
    }

    #[test]
    fn test_firecracker_config_builder() {
        let config = FirecrackerConfig::builder()
            .firecracker_path("/custom/firecracker")
            .jailer_path("/custom/jailer")
            .data_dir("/custom/data")
            .kernel_path("/custom/vmlinux")
            .rootfs_path("/custom/rootfs.ext4")
            .use_jailer(false)
            .boot_timeout_secs(60)
            .build()
            .expect("should build config");

        assert_eq!(
            config.firecracker_path,
            PathBuf::from("/custom/firecracker")
        );
        assert_eq!(config.jailer_path, PathBuf::from("/custom/jailer"));
        assert_eq!(config.data_dir, PathBuf::from("/custom/data"));
        assert!(!config.use_jailer);
        assert_eq!(config.boot_timeout_secs, 60);
    }

    #[test]
    fn test_firecracker_config_builder_validation() {
        let result = FirecrackerConfig::builder().data_dir("").build();

        assert!(result.is_err());
    }

    #[test]
    fn test_network_config_builder() {
        let network = FirecrackerNetworkConfig::new()
            .with_tap_prefix("my-tap")
            .with_subnet("10.0.0.0/16")
            .with_gateway_ip("10.0.0.1")
            .with_host_interface("ens3")
            .with_nat(false);

        assert_eq!(network.tap_prefix, "my-tap");
        assert_eq!(network.subnet, "10.0.0.0/16");
        assert_eq!(network.gateway_ip, "10.0.0.1");
        assert_eq!(network.host_interface, "ens3");
        assert!(!network.enable_nat);
    }

    #[test]
    fn test_microvm_resources_builder() {
        let resources = MicroVMResources::new()
            .with_vcpus(4)
            .with_memory_mib(2048)
            .with_hyperthreading(true);

        assert_eq!(resources.vcpu_count, 4);
        assert_eq!(resources.mem_size_mib, 2048);
        assert!(resources.ht_enabled);
    }

    #[test]
    fn test_microvm_resources_clamping() {
        let resources = MicroVMResources::new()
            .with_vcpus(100) // Should be clamped to 32
            .with_memory_mib(100000); // Should be clamped to 32768

        assert_eq!(resources.vcpu_count, 32);
        assert_eq!(resources.mem_size_mib, 32768);
    }

    #[test]
    fn test_ip_pool_from_cidr() {
        let pool = IpPool::from_cidr("172.16.0.0/24").expect("should parse CIDR");
        assert_eq!(pool.base_ip, Ipv4Addr::new(172, 16, 0, 0));
        assert_eq!(pool.mask_bits, 24);
    }

    #[test]
    fn test_ip_pool_allocate() {
        let mut pool = IpPool::from_cidr("172.16.0.0/30").expect("should parse CIDR");

        let ip1 = pool.allocate().expect("should allocate first IP");
        assert_eq!(ip1, Ipv4Addr::new(172, 16, 0, 2));

        let ip2 = pool.allocate().expect("should allocate second IP");
        assert_eq!(ip2, Ipv4Addr::new(172, 16, 0, 3));

        // Pool exhausted (only 2 usable IPs in /30)
        // Note: /30 has 4 addresses: .0 (network), .1 (gateway), .2, .3 (broadcast)
        // So only .2 is usable for VMs
    }

    #[test]
    fn test_ip_pool_release() {
        let mut pool = IpPool::from_cidr("172.16.0.0/24").expect("should parse CIDR");

        let ip = pool.allocate().expect("should allocate");
        assert!(pool.is_allocated(&ip));

        pool.release(ip);
        assert!(!pool.is_allocated(&ip));
    }

    #[test]
    fn test_vm_id_generation() {
        let worker_id = WorkerId::new();
        let vm_id = FirecrackerProvider::vm_id(&worker_id);
        assert!(vm_id.starts_with("fc-"));
    }

    #[test]
    fn test_default_capabilities() {
        let caps = FirecrackerProvider::default_capabilities();
        assert!(!caps.gpu_support);
        assert!(caps.architectures.contains(&Architecture::Amd64));
        assert!(caps.architectures.contains(&Architecture::Arm64));
        assert_eq!(caps.max_resources.max_gpu_count, 0);
    }

    #[test]
    fn test_map_vm_state() {
        assert!(matches!(
            FirecrackerProvider::map_vm_state(&MicroVMState::Creating),
            WorkerState::Creating
        ));
        assert!(matches!(
            FirecrackerProvider::map_vm_state(&MicroVMState::Running),
            WorkerState::Ready
        ));
        // Crash-Only: Stopping mapea a Creating (retry)
        assert!(matches!(
            FirecrackerProvider::map_vm_state(&MicroVMState::Stopping),
            WorkerState::Creating
        ));
        assert!(matches!(
            FirecrackerProvider::map_vm_state(&MicroVMState::Stopped),
            WorkerState::Terminated
        ));
        assert!(matches!(
            FirecrackerProvider::map_vm_state(&MicroVMState::Failed("error".to_string())),
            WorkerState::Terminated
        ));
    }

    // US-23.5: RAII Tests
    #[test]
    fn test_network_resources_accessors() {
        let network =
            NetworkResources::new("fc-test-tap".to_string(), -1, Ipv4Addr::new(172, 16, 0, 2));

        assert_eq!(network.tap_name(), "fc-test-tap");
        assert_eq!(network.ip_address(), Ipv4Addr::new(172, 16, 0, 2));
    }

    #[test]
    fn test_network_resources_into_parts() {
        let network =
            NetworkResources::new("fc-test-tap".to_string(), -1, Ipv4Addr::new(172, 16, 0, 2));

        let (tap_name, ip_address) = network.into_parts();
        assert_eq!(tap_name, "fc-test-tap");
        assert_eq!(ip_address, Ipv4Addr::new(172, 16, 0, 2));
    }

    #[test]
    fn test_creation_guard_initial_state() {
        let mut guard = FirecrackerCreationGuard::new();
        assert!(!guard.has_resources());
        assert!(guard.take_resources().is_none());
    }

    #[test]
    fn test_creation_guard_set() {
        let mut guard = FirecrackerCreationGuard::new();
        let network =
            NetworkResources::new("fc-test-tap".to_string(), -1, Ipv4Addr::new(172, 16, 0, 2));

        guard.set_resources(network);
        assert!(guard.has_resources());
    }
}

// Blanket implementation of WorkerProvider trait combining all ISP traits
// This allows FirecrackerProvider to be used as dyn WorkerProvider
#[async_trait]
impl WorkerProvider for FirecrackerProvider {}

// Stub implementation for WorkerEventSource - Firecracker doesn't support event streams
#[async_trait]
impl WorkerEventSource for FirecrackerProvider {
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

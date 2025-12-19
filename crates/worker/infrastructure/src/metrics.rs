use hodei_jobs::ResourceUsage;
use std::time::{Duration, Instant};
use sysinfo::{Disks, System};

/// T3.6: Metrics cache TTL (Time To Live) in seconds
/// Cache is considered fresh for 35s (5s buffer beyond 30s collection interval)
pub const METRICS_CACHE_TTL_SECS: u64 = 35;

/// T3.6: Cached resource metrics with timestamp for TTL validation
#[derive(Clone, Debug)]
pub struct CachedResourceUsage {
    pub usage: ResourceUsage,
    pub timestamp: Instant,
}

impl CachedResourceUsage {
    /// Create a new cached metrics entry
    pub fn new(usage: ResourceUsage) -> Self {
        Self {
            timestamp: Instant::now(),
            usage,
        }
    }

    /// Check if cache is still fresh (within TTL)
    pub fn is_fresh(&self) -> bool {
        self.timestamp.elapsed() < Duration::from_secs(METRICS_CACHE_TTL_SECS)
    }

    /// Get the underlying usage
    pub fn get_usage(&self) -> &ResourceUsage {
        &self.usage
    }
}

pub struct MetricsCollector {
    sys: System,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn collect(&mut self) -> ResourceUsage {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();

        let cpu_usage = self.sys.global_cpu_usage() as f64 / 100.0;
        let memory_bytes = self.sys.used_memory() as i64;

        // Disk usage (root usually)
        let disks = Disks::new_with_refreshed_list();
        let disk_bytes = disks
            .iter()
            .find(|d| d.mount_point().to_str() == Some("/"))
            .map(|d| (d.total_space() - d.available_space()) as i64)
            .unwrap_or(0);

        ResourceUsage {
            cpu_cores: cpu_usage,
            memory_bytes,
            disk_bytes,
            gpu_count: 0,
            custom_usage: std::collections::HashMap::new(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

pub fn get_system_memory() -> i64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.total_memory() as i64
}

pub fn get_disk_space() -> i64 {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .find(|d| d.mount_point().to_str() == Some("/"))
        .map(|d| d.total_space() as i64)
        .unwrap_or(100 * 1024 * 1024 * 1024)
}

/// Shared application metrics
pub struct WorkerMetrics {
    /// Counter for logs dropped due to backpressure
    pub dropped_logs: std::sync::atomic::AtomicU64,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self {
            dropped_logs: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

use hodei_jobs::ResourceUsage;
use std::time::{Duration, Instant};

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

    /// Get the age of the cache in seconds
    pub fn age_secs(&self) -> u64 {
        self.timestamp.elapsed().as_secs()
    }

    /// Get the underlying usage
    pub fn get_usage(&self) -> &ResourceUsage {
        &self.usage
    }
}

pub fn get_system_memory() -> i64 {
    // Try to get actual system memory, fallback to 8GB
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb) = line.split_whitespace().nth(1) {
                        if let Ok(kb_val) = kb.parse::<i64>() {
                            return kb_val * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
    }
    8 * 1024 * 1024 * 1024 // 8GB default
}

pub fn get_disk_space() -> i64 {
    // Default to 100GB
    100 * 1024 * 1024 * 1024
}

//! Metrics for Global Log Buffer
//!
//! Collects and tracks metrics for the global log buffer including
//! memory usage, backpressure events, and performance statistics.

use crate::shared_kernel::JobId;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics collector for the global log buffer
#[derive(Debug, Default)]
pub struct LogBufferMetrics {
    /// Total bytes pushed to buffer
    total_bytes_pushed: AtomicU64,
    /// Total entries pushed
    total_entries_pushed: AtomicU64,
    /// Current buffer size
    current_buffer_size: AtomicU64,
    /// Number of backpressure events
    backpressure_events: AtomicU64,
    /// Total bytes evicted
    total_bytes_evicted: AtomicU64,
    /// Number of evictions
    evictions_count: AtomicU64,
    /// Number of times buffer was cleared
    clears_count: AtomicU64,
    /// Number of get_logs calls
    get_logs_calls: AtomicU64,
    /// Peak buffer usage
    peak_buffer_usage: AtomicU64,
    /// Average entry size
    average_entry_size: AtomicU64,
    /// Number of samples for average calculation
    average_samples: AtomicU64,
}

impl LogBufferMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a push operation
    pub fn record_push(&self, entry_size: u64, current_total: u64) {
        self.total_bytes_pushed
            .fetch_add(entry_size, Ordering::Relaxed);
        self.total_entries_pushed.fetch_add(1, Ordering::Relaxed);
        self.current_buffer_size
            .store(current_total, Ordering::Relaxed);

        // Update peak usage
        let mut current_peak = self.peak_buffer_usage.load(Ordering::Relaxed);
        while current_total > current_peak {
            match self.peak_buffer_usage.compare_exchange_weak(
                current_peak,
                current_total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(val) => current_peak = val,
            }
        }

        // Update average entry size
        let current_avg = self.average_entry_size.load(Ordering::Relaxed);
        let samples = self.average_samples.load(Ordering::Relaxed) + 1;
        let new_avg = (current_avg * samples + entry_size) / samples;
        self.average_entry_size.store(new_avg, Ordering::Relaxed);
        self.average_samples.store(samples, Ordering::Relaxed);
    }

    /// Record a backpressure event
    pub fn record_backpressure_event(&self) {
        self.backpressure_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an eviction operation
    pub fn record_eviction(&self, evicted_bytes: u64) {
        self.total_bytes_evicted
            .fetch_add(evicted_bytes, Ordering::Relaxed);
        self.evictions_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a clear operation
    pub fn record_clear(&self, job_id: &JobId, cleared_bytes: u64) {
        let _ = job_id; // Suppress unused warning in metrics
        self.clears_count.fetch_add(1, Ordering::Relaxed);
        self.current_buffer_size
            .fetch_sub(cleared_bytes, Ordering::Relaxed);
    }

    /// Record a get_logs call
    pub fn record_get_logs(&self, job_id: &JobId, entry_count: usize) {
        let _ = job_id; // Suppress unused warning in metrics
        self.get_logs_calls.fetch_add(1, Ordering::Relaxed);
        let _ = entry_count;
    }

    /// Get total bytes pushed
    pub fn total_bytes_pushed(&self) -> u64 {
        self.total_bytes_pushed.load(Ordering::Relaxed)
    }

    /// Get total entries pushed
    pub fn total_entries_pushed(&self) -> u64 {
        self.total_entries_pushed.load(Ordering::Relaxed)
    }

    /// Get current buffer size
    pub fn current_buffer_size(&self) -> u64 {
        self.current_buffer_size.load(Ordering::Relaxed)
    }

    /// Get backpressure events count
    pub fn backpressure_events(&self) -> u64 {
        self.backpressure_events.load(Ordering::Relaxed)
    }

    /// Get total bytes evicted
    pub fn total_bytes_evicted(&self) -> u64 {
        self.total_bytes_evicted.load(Ordering::Relaxed)
    }

    /// Get evictions count
    pub fn evictions_count(&self) -> u64 {
        self.evictions_count.load(Ordering::Relaxed)
    }

    /// Get clears count
    pub fn clears_count(&self) -> u64 {
        self.clears_count.load(Ordering::Relaxed)
    }

    /// Get get_logs calls count
    pub fn get_logs_calls(&self) -> u64 {
        self.get_logs_calls.load(Ordering::Relaxed)
    }

    /// Get peak buffer usage
    pub fn peak_buffer_usage(&self) -> u64 {
        self.peak_buffer_usage.load(Ordering::Relaxed)
    }

    /// Get average entry size
    pub fn average_entry_size(&self) -> u64 {
        self.average_entry_size.load(Ordering::Relaxed)
    }

    /// Get buffer utilization percentage
    pub fn utilization_percent(&self, max_bytes: u64) -> f64 {
        let current = self.current_buffer_size();
        if max_bytes > 0 {
            (current as f64 / max_bytes as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Reset all metrics (useful for testing)
    pub fn reset(&self) {
        self.total_bytes_pushed.store(0, Ordering::Relaxed);
        self.total_entries_pushed.store(0, Ordering::Relaxed);
        self.current_buffer_size.store(0, Ordering::Relaxed);
        self.backpressure_events.store(0, Ordering::Relaxed);
        self.total_bytes_evicted.store(0, Ordering::Relaxed);
        self.evictions_count.store(0, Ordering::Relaxed);
        self.clears_count.store(0, Ordering::Relaxed);
        self.get_logs_calls.store(0, Ordering::Relaxed);
        self.peak_buffer_usage.store(0, Ordering::Relaxed);
        self.average_entry_size.store(0, Ordering::Relaxed);
        self.average_samples.store(0, Ordering::Relaxed);
    }
}

/// Snapshot of metrics for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_bytes_pushed: u64,
    pub total_entries_pushed: u64,
    pub current_buffer_size: u64,
    pub backpressure_events: u64,
    pub total_bytes_evicted: u64,
    pub evictions_count: u64,
    pub clears_count: u64,
    pub get_logs_calls: u64,
    pub peak_buffer_usage: u64,
    pub average_entry_size: u64,
    pub utilization_percent: f64,
}

impl From<&LogBufferMetrics> for MetricsSnapshot {
    fn from(metrics: &LogBufferMetrics) -> Self {
        Self {
            total_bytes_pushed: metrics.total_bytes_pushed(),
            total_entries_pushed: metrics.total_entries_pushed(),
            current_buffer_size: metrics.current_buffer_size(),
            backpressure_events: metrics.backpressure_events(),
            total_bytes_evicted: metrics.total_bytes_evicted(),
            evictions_count: metrics.evictions_count(),
            clears_count: metrics.clears_count(),
            get_logs_calls: metrics.get_logs_calls(),
            peak_buffer_usage: metrics.peak_buffer_usage(),
            average_entry_size: metrics.average_entry_size(),
            utilization_percent: 0.0, // Will be filled by caller with max_bytes
        }
    }
}

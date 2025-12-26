//! Global Log Buffer Implementation
//!
//! Provides a global log buffer with memory limits and backpressure control.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{Duration, timeout};
use tracing::{debug, error, info, warn};

use crate::logging::backpressure::BackpressureStrategy;
use crate::logging::log_buffer::{LogBuffer, LogEntry};
use crate::logging::metrics::LogBufferMetrics;
use crate::shared_kernel::{DomainError, JobId};

/// Error types for GlobalLogBuffer operations
#[derive(Debug, thiserror::Error)]
pub enum GlobalLogBufferError {
    #[error("Backpressure timeout: buffer full and eviction took too long")]
    BackpressureTimeout,

    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl From<GlobalLogBufferError> for DomainError {
    fn from(err: GlobalLogBufferError) -> Self {
        match err {
            GlobalLogBufferError::BackpressureTimeout => DomainError::InfrastructureError {
                message: "Log buffer backpressure timeout".to_string(),
            },
            GlobalLogBufferError::Domain(e) => e,
            GlobalLogBufferError::Internal { message } => {
                DomainError::InfrastructureError { message }
            }
        }
    }
}

/// Global Log Buffer with memory limit and backpressure
///
/// This buffer enforces a global memory limit across all job logs and implements
/// backpressure when the limit is reached. It uses LRU eviction to remove old logs.
pub struct GlobalLogBuffer {
    /// Maximum allowed memory in bytes
    max_bytes: u64,
    /// Current total memory usage
    total_bytes: AtomicU64,
    /// Per-job log buffers
    buffers: Arc<dashmap::DashMap<JobId, LogBuffer>>,
    /// Backpressure strategy
    backpressure_strategy: BackpressureStrategy,
    /// Metrics collector
    metrics: Arc<LogBufferMetrics>,
}

impl std::fmt::Debug for GlobalLogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalLogBuffer")
            .field("max_bytes", &self.max_bytes)
            .field("total_bytes", &self.total_bytes.load(Ordering::Relaxed))
            .field("buffers", &self.buffers.len())
            .finish()
    }
}

impl GlobalLogBuffer {
    /// Create a new GlobalLogBuffer with the specified memory limit
    pub fn new(
        max_bytes: u64,
        backpressure_strategy: BackpressureStrategy,
        metrics: Arc<LogBufferMetrics>,
    ) -> Self {
        info!(
            max_bytes_mb = max_bytes / (1024 * 1024),
            "Initializing GlobalLogBuffer"
        );

        Self {
            max_bytes,
            total_bytes: AtomicU64::new(0),
            buffers: Arc::new(dashmap::DashMap::new()),
            backpressure_strategy,
            metrics,
        }
    }

    /// Push a log entry to the buffer
    ///
    /// This method implements backpressure:
    /// 1. Check if adding the entry would exceed the limit
    /// 2. If so, evict oldest entries until there's space
    /// 3. Add the entry to the appropriate buffer
    pub async fn push(&self, job_id: &JobId, entry: LogEntry) -> Result<(), GlobalLogBufferError> {
        let entry_size = entry.size_bytes();
        let current_total = self.total_bytes.load(Ordering::Relaxed);

        // Check if we need to evict
        if current_total + entry_size > self.max_bytes {
            self.handle_backpressure(entry_size, job_id).await?;
        }

        // Get or create buffer for this job
        let buffer = self
            .buffers
            .entry(job_id.clone())
            .or_insert_with(|| LogBuffer::new(job_id.clone()));

        // Try to push to buffer
        let added_size = buffer
            .push(entry)
            .await
            .map_err(|e| GlobalLogBufferError::Internal {
                message: format!("Failed to push to buffer: {}", e),
            })?;

        // Update total bytes
        self.total_bytes.fetch_add(added_size, Ordering::Relaxed);

        // Update metrics
        self.metrics
            .record_push(added_size, self.total_bytes.load(Ordering::Relaxed));

        debug!(
            job_id = %job_id,
            entry_size = entry_size,
            total_bytes = self.total_bytes.load(Ordering::Relaxed),
            "Log entry pushed to global buffer"
        );

        Ok(())
    }

    /// Handle backpressure when buffer is full
    async fn handle_backpressure(
        &self,
        required_size: u64,
        _current_job_id: &JobId,
    ) -> Result<(), GlobalLogBufferError> {
        let target_size = self.total_bytes.load(Ordering::Relaxed) + required_size;
        let mut evicted_bytes = 0u64;

        // Track backpressure events for metrics
        self.metrics.record_backpressure_event();

        // Try to evict oldest entries until we have enough space
        let timeout_duration = match self.backpressure_strategy.timeout() {
            Some(t) => t,
            None => Duration::from_secs(5), // Default timeout
        };

        let evict_result = timeout(timeout_duration, async {
            while self.total_bytes.load(Ordering::Relaxed) + required_size > self.max_bytes
                && evicted_bytes < target_size
            {
                // Find the oldest buffer
                let oldest_entry = self
                    .buffers
                    .iter()
                    .min_by_key(|entry| entry.value().last_access());

                if let Some(entry) = oldest_entry {
                    let buffer = entry.value();
                    let flushed_size = buffer.flush_to_persister().await;

                    // Remove the buffer from the map
                    let job_id = entry.key().clone();
                    self.buffers.remove(&job_id);

                    evicted_bytes += flushed_size;

                    // Update total bytes
                    self.total_bytes.fetch_sub(flushed_size, Ordering::Relaxed);

                    // Update metrics
                    self.metrics.record_eviction(flushed_size);

                    warn!(
                        job_id = %job_id,
                        evicted_bytes = flushed_size,
                        "Evicted log buffer due to backpressure"
                    );
                } else {
                    // No buffers to evict
                    break;
                }
            }
        })
        .await;

        if evict_result.is_err() {
            error!("Backpressure eviction timed out");
            return Err(GlobalLogBufferError::BackpressureTimeout);
        }

        info!(
            required_bytes = required_size,
            evicted_bytes = evicted_bytes,
            current_total = self.total_bytes.load(Ordering::Relaxed),
            "Backpressure handled"
        );

        Ok(())
    }

    /// Get logs for a specific job
    pub async fn get_logs(&self, job_id: &JobId, limit: Option<usize>) -> Vec<LogEntry> {
        if let Some(buffer) = self.buffers.get(job_id) {
            let logs = buffer.get_logs(limit).await;
            self.metrics.record_get_logs(job_id, logs.len());
            logs
        } else {
            Vec::new()
        }
    }

    /// Clear logs for a specific job
    pub async fn clear_logs(&self, job_id: &JobId) -> Result<u64, GlobalLogBufferError> {
        if let Some((_, buffer)) = self.buffers.remove(job_id) {
            let size = buffer.size_bytes();
            self.total_bytes.fetch_sub(size, Ordering::Relaxed);
            self.metrics.record_clear(job_id, size);
            Ok(size)
        } else {
            Ok(0)
        }
    }

    /// Get current memory usage
    pub fn current_usage(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// Get buffer utilization percentage
    pub fn utilization(&self) -> f64 {
        let current = self.total_bytes.load(Ordering::Relaxed) as f64;
        (current / self.max_bytes as f64) * 100.0
    }

    /// Get number of buffered jobs
    pub fn buffered_jobs_count(&self) -> usize {
        self.buffers.len()
    }

    /// Flush all buffers to persistence
    pub async fn flush_all(&self) -> Result<u64, GlobalLogBufferError> {
        let mut total_flushed = 0u64;

        let job_ids: Vec<JobId> = self.buffers.iter().map(|e| e.key().clone()).collect();

        for job_id in job_ids {
            if let Some((_, buffer)) = self.buffers.remove(&job_id) {
                let size = buffer.flush_to_persister().await;
                total_flushed += size;
                self.total_bytes.fetch_sub(size, Ordering::Relaxed);
            }
        }

        info!(flushed_bytes = total_flushed, "Flushed all log buffers");
        Ok(total_flushed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::backpressure::BackpressureStrategy;
    use crate::logging::log_buffer::LogEntry;
    use crate::logging::metrics::LogBufferMetrics;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Create a test log entry with specified size
    fn create_test_entry(size: usize, job_id: &JobId) -> LogEntry {
        LogEntry::new(
            job_id.clone(),
            format!("{:width$}", "test message", width = size.saturating_sub(20)),
            false, // is_stderr
            chrono::Utc::now(),
            0, // sequence
        )
    }

    fn create_job_id() -> JobId {
        JobId(uuid::Uuid::new_v4())
    }

    fn create_metrics() -> Arc<LogBufferMetrics> {
        Arc::new(LogBufferMetrics::new())
    }

    #[tokio::test]
    async fn test_global_log_buffer_basic_operations() {
        let metrics = create_metrics();
        let buffer = GlobalLogBuffer::new(
            1024 * 1024, // 1MB limit
            BackpressureStrategy::default(),
            metrics.clone(),
        );

        let job_id = create_job_id();

        // Push some entries
        for i in 0..10 {
            let entry = create_test_entry(100, &job_id);
            buffer.push(&job_id, entry).await.unwrap();
        }

        // Verify state
        assert!(buffer.current_usage() > 0);
        assert_eq!(buffer.buffered_jobs_count(), 1);

        // Get logs back
        let logs = buffer.get_logs(&job_id, None).await;
        assert_eq!(logs.len(), 10);

        // Clear logs
        buffer.clear_logs(&job_id).await.unwrap();
        assert_eq!(buffer.current_usage(), 0);
    }

    #[tokio::test]
    async fn test_global_log_buffer_multiple_jobs() {
        let metrics = create_metrics();
        let buffer = GlobalLogBuffer::new(
            1024 * 1024, // 1MB limit
            BackpressureStrategy::default(),
            metrics.clone(),
        );

        let job_ids: Vec<JobId> = (0..5).map(|_| create_job_id()).collect();

        // Add entries to multiple jobs
        for job_id in &job_ids {
            for i in 0..10 {
                let entry = create_test_entry(100, job_id);
                buffer.push(job_id, entry).await.unwrap();
            }
        }

        assert_eq!(buffer.buffered_jobs_count(), 5);

        // Verify each job has logs
        for job_id in &job_ids {
            let logs = buffer.get_logs(job_id, None).await;
            assert_eq!(logs.len(), 10);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_global_log_buffer_backpressure() {
        // Create a small buffer to force backpressure
        let metrics = create_metrics();
        let buffer = GlobalLogBuffer::new(
            1024, // 1KB limit - very small to trigger backpressure
            BackpressureStrategy::with_timeout(std::time::Duration::from_millis(10)), // Short timeout for test
            metrics.clone(),
        );

        let job_id = create_job_id();

        // Fill the buffer - with small timeout, backpressure will trigger quickly
        let mut count = 0;
        while count < 50 {
            let entry = create_test_entry(100, &job_id); // 100 bytes each
            match buffer.push(&job_id, entry).await {
                Ok(_) => count += 1,
                Err(_) => break, // Backpressure kicked in
            }
        }

        // Should have encountered backpressure
        assert!(
            count < 50,
            "Should have encountered backpressure before pushing 50 entries"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_global_log_buffer_lru_eviction() {
        let metrics = create_metrics();
        let buffer = Arc::new(GlobalLogBuffer::new(
            300, // 300 bytes limit - very small
            BackpressureStrategy::with_timeout(std::time::Duration::from_millis(100)),
            metrics.clone(),
        ));

        // Create multiple jobs to trigger LRU eviction
        let job_ids: Vec<JobId> = (0..5).map(|_| create_job_id()).collect();

        // Push entries and check for errors
        for (i, job_id) in job_ids.iter().enumerate() {
            let entry = create_test_entry(50, job_id); // Smaller entries
            let result = buffer.push(job_id, entry).await;

            // First few should succeed, later ones may fail due to backpressure
            if result.is_err() {
                // Backpressure kicked in, stop pushing
                break;
            }

            // Access older jobs to make them "recent"
            if i > 0 {
                let _ = buffer.get_logs(&job_ids[0], None).await;
            }
        }

        // Verify the buffer is working (may have evicted some entries)
        let usage = buffer.current_usage();
        let buffered_count = buffer.buffered_jobs_count();

        // Usage should not exceed limit (with some tolerance for overhead)
        assert!(
            usage <= 300 + 100,
            "Usage {} should not exceed 300 bytes significantly",
            usage
        );
        // Some jobs may have been evicted
        assert!(
            buffered_count <= 5,
            "Buffered jobs should not exceed initial jobs"
        );
    }

    #[tokio::test]
    async fn test_global_log_buffer_concurrent_access() {
        let metrics = create_metrics();
        let buffer = Arc::new(GlobalLogBuffer::new(
            1024 * 1024, // 1MB limit
            BackpressureStrategy::default(),
            metrics.clone(),
        ));

        let num_jobs = 100;
        let entries_per_job = 50;

        // Spawn concurrent tasks
        let handles: Vec<_> = (0..num_jobs)
            .map(|i| {
                let buffer = buffer.clone();
                let job_id = create_job_id();
                tokio::spawn(async move {
                    for j in 0..entries_per_job {
                        let entry = create_test_entry(50, &job_id);
                        buffer.push(&job_id, entry).await.unwrap();
                    }
                    job_id
                })
            })
            .collect();

        let job_ids: Vec<JobId> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Verify all jobs are buffered
        assert_eq!(buffer.buffered_jobs_count(), num_jobs);

        // Verify total entries using concurrent access
        // Clone JobId for each spawn since tokio::spawn requires 'static lifetime
        let log_futures: Vec<_> = job_ids
            .into_iter()
            .map(|job_id| {
                let buffer = buffer.clone();
                tokio::spawn(async move { buffer.get_logs(&job_id, None).await.len() })
            })
            .collect();

        let entries_per_job_vec: Vec<usize> = futures::future::join_all(log_futures)
            .await
            .into_iter()
            .filter_map(Result::ok) // Handle any spawn errors gracefully
            .collect();
        let total_entries: usize = entries_per_job_vec.into_iter().sum();

        assert_eq!(total_entries, num_jobs * 50); // 50 entries per job as defined above
    }

    /// Stress test: Simulate 5000 concurrent jobs with high log volume
    /// This test verifies the buffer can handle production-scale loads
    #[tokio::test]
    #[ignore]
    async fn test_global_log_buffer_stress_5000_jobs() {
        let metrics = create_metrics();
        let buffer = Arc::new(GlobalLogBuffer::new(
            1024 * 1024 * 1024, // 1GB limit for stress test
            BackpressureStrategy::default(),
            metrics.clone(),
        ));

        let num_jobs = 5000;
        let entries_per_job = 100;
        let entry_size = 100; // bytes

        let start_time = std::time::Instant::now();

        // Create a barrier to start all tasks simultaneously
        let barrier = Arc::new(tokio::sync::Barrier::new(num_jobs));

        let handles: Vec<_> = (0..num_jobs)
            .map(|i| {
                let buffer = buffer.clone();
                let barrier = barrier.clone();
                let job_id = create_job_id();
                tokio::spawn(async move {
                    barrier.wait().await;
                    for _ in 0..entries_per_job {
                        let entry = create_test_entry(entry_size, &job_id);
                        if let Err(e) = buffer.push(&job_id, entry).await {
                            // Log but don't fail - backpressure may cause some rejections
                            tracing::warn!("Push failed for job {}: {}", job_id, e);
                        }
                    }
                    job_id
                })
            })
            .collect();

        let job_ids: Vec<JobId> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let elapsed = start_time.elapsed();

        // Verify buffer state
        let buffered_count = buffer.buffered_jobs_count();
        let total_usage = buffer.current_usage();
        let utilization = buffer.utilization();

        tracing::info!(
            jobs_buffered = buffered_count,
            total_bytes = total_usage,
            utilization_percent = utilization,
            elapsed_secs = elapsed.as_secs_f64(),
            "Stress test completed"
        );

        // With 1GB limit and 5000 jobs * 100 entries * 100 bytes = 50MB
        // We should be well within limits
        assert!(
            total_usage < 1024 * 1024 * 1024,
            "Total usage should be less than 1GB"
        );
        assert!(
            buffered_count <= num_jobs,
            "Buffered jobs should not exceed total jobs"
        );

        // Performance assertion: should complete in reasonable time
        assert!(
            elapsed < std::time::Duration::from_secs(60),
            "Stress test should complete within 60 seconds, took {}s",
            elapsed.as_secs_f64()
        );
    }

    /// Test that verifies backpressure strategy with timeout
    #[tokio::test]
    #[ignore]
    async fn test_global_log_buffer_backpressure_with_timeout() {
        let metrics = create_metrics();
        let strategy = BackpressureStrategy::with_timeout(std::time::Duration::from_millis(100));

        let buffer = Arc::new(GlobalLogBuffer::new(
            100, // 100 bytes - very small
            strategy,
            metrics.clone(),
        ));

        let job_id = create_job_id();

        // Try to fill buffer quickly
        let mut failures = 0;
        for i in 0..1000 {
            let entry = create_test_entry(50, &job_id);
            if buffer.push(&job_id, entry).await.is_err() {
                failures += 1;
            }
            if failures > 10 {
                // Should have triggered backpressure early
                break;
            }
        }

        // Should have encountered backpressure
        assert!(failures > 0, "Should have encountered backpressure");
    }

    #[tokio::test]
    async fn test_global_log_buffer_utilization() {
        let metrics = create_metrics();
        let buffer = GlobalLogBuffer::new(
            1000, // 1000 bytes
            BackpressureStrategy::default(),
            metrics.clone(),
        );

        let job_id = create_job_id();

        // Initially 0%
        assert_eq!(buffer.utilization(), 0.0);

        // Add some entries (each entry is ~180 bytes due to overhead)
        for _ in 0..3 {
            let entry = create_test_entry(100, &job_id);
            buffer.push(&job_id, entry).await.unwrap();
        }

        // Should be > 0% and < 100%
        let utilization = buffer.utilization();
        assert!(
            utilization > 0.0 && utilization < 100.0,
            "Utilization should be between 0% and 100%, got {}%",
            utilization
        );
    }
}

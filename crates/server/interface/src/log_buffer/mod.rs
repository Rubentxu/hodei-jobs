//! Global Log Buffer with Backpressure
//!
//! Provides a memory-bounded log buffer system that prevents OOM in high-load scenarios.
//! Implements LRU eviction and backpressure mechanisms.

use dashmap::DashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use super::log_persistence::LogStorage;
use prost_types::Timestamp;
use std::collections::VecDeque;

/// Configuration for the Global Log Buffer
#[derive(Debug, Clone)]
pub struct GlobalLogBufferConfig {
    /// Maximum total bytes in memory for all log buffers
    pub max_bytes: u64,
    /// Maximum entries per individual job buffer
    pub max_entries_per_job: usize,
    /// Backpressure timeout - how long to wait before forcing eviction
    pub backpressure_timeout: Duration,
    /// How often to proactively flush oldest buffers
    pub proactive_flush_interval: Duration,
    /// Target percentage of max_bytes to trigger proactive eviction
    pub eviction_threshold_percent: u8,
}

impl Default for GlobalLogBufferConfig {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024 * 1024, // 1GB default
            max_entries_per_job: 1000,
            backpressure_timeout: Duration::from_secs(5),
            proactive_flush_interval: Duration::from_secs(30),
            eviction_threshold_percent: 80,
        }
    }
}

/// A single log entry with size tracking
#[derive(Debug, Clone)]
pub struct LogBufferEntry {
    pub line: String,
    pub is_stderr: bool,
    pub timestamp: Option<i64>,
    pub sequence: u64,
    /// Size of this entry in bytes
    pub size_bytes: u64,
}

impl LogBufferEntry {
    /// Create a new entry and calculate its size
    pub fn new(line: String, is_stderr: bool, timestamp: Option<i64>, sequence: u64) -> Self {
        let size_bytes = line.len() as u64 + 1; // +1 for newline
        Self {
            line,
            is_stderr,
            timestamp,
            sequence,
            size_bytes,
        }
    }
}

/// Per-job log buffer with LRU tracking
#[derive(Debug)]
pub struct LogBuffer {
    pub job_id: String,
    entries: tokio::sync::RwLock<VecDeque<LogBufferEntry>>,
    bytes_count: AtomicU64,
    last_access: AtomicU64,
}

impl LogBuffer {
    /// Create a new log buffer for a job
    pub fn new(job_id: String) -> Self {
        Self {
            job_id,
            entries: tokio::sync::RwLock::new(VecDeque::new()),
            bytes_count: AtomicU64::new(0),
            last_access: AtomicU64::new(Self::now_millis()),
        }
    }

    fn now_millis() -> u64 {
        Instant::now().elapsed().as_millis() as u64
    }

    /// Push a new entry to the buffer
    pub async fn push(&self, entry: LogBufferEntry, max_entries: usize) -> u64 {
        // Update last access
        self.last_access
            .store(Self::now_millis(), Ordering::Release);

        let entry_size = entry.size_bytes;
        let mut entries = self.entries.write().await;

        // Remove oldest if at capacity
        if entries.len() >= max_entries {
            if let Some(oldest) = entries.pop_front() {
                self.bytes_count
                    .fetch_sub(oldest.size_bytes, Ordering::Release);
            }
        }

        entries.push_back(entry);
        self.bytes_count.fetch_add(entry_size, Ordering::Release);

        self.bytes_count.load(Ordering::Acquire)
    }

    /// Get all entries (for flushing/persistence)
    pub async fn get_all_entries(&self) -> Vec<LogBufferEntry> {
        let entries = self.entries.read().await;
        entries.iter().cloned().collect()
    }

    /// Get current byte count
    pub fn bytes_count(&self) -> u64 {
        self.bytes_count.load(Ordering::Acquire)
    }

    /// Get last access timestamp
    pub fn last_access(&self) -> u64 {
        self.last_access.load(Ordering::Acquire)
    }

    /// Clear the buffer and return total bytes removed
    pub async fn clear(&self) -> u64 {
        let mut entries = self.entries.write().await;
        let bytes = self.bytes_count.load(Ordering::Acquire);
        entries.clear();
        self.bytes_count.store(0, Ordering::Release);
        bytes
    }

    /// Get entry count
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Check if buffer is empty
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

/// Metrics for the Global Log Buffer
#[derive(Debug, Default)]
pub struct GlobalLogBufferMetrics {
    pub total_bytes: AtomicU64,
    pub total_entries: AtomicU64,
    pub buffer_count: AtomicU64,
    pub evictions_total: AtomicU64,
    pub backpressure_events: AtomicU64,
    pub bytes_evicted: AtomicU64,
    pub flush_operations: AtomicU64,
}

impl GlobalLogBufferMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> GlobalLogBufferMetricsSnapshot {
        GlobalLogBufferMetricsSnapshot {
            total_bytes: self.total_bytes.load(Ordering::Acquire),
            total_entries: self.total_entries.load(Ordering::Acquire),
            buffer_count: self.buffer_count.load(Ordering::Acquire),
            evictions_total: self.evictions_total.load(Ordering::Acquire),
            backpressure_events: self.backpressure_events.load(Ordering::Acquire),
            bytes_evicted: self.bytes_evicted.load(Ordering::Acquire),
            flush_operations: self.flush_operations.load(Ordering::Acquire),
        }
    }
}

/// Snapshot of Global Log Buffer metrics
#[derive(Debug, Clone)]
pub struct GlobalLogBufferMetricsSnapshot {
    pub total_bytes: u64,
    pub total_entries: u64,
    pub buffer_count: u64,
    pub evictions_total: u64,
    pub backpressure_events: u64,
    pub bytes_evicted: u64,
    pub flush_operations: u64,
}

impl fmt::Display for GlobalLogBufferMetricsSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GlobalLogBuffer Metrics:
  Total Bytes: {}
  Total Entries: {}
  Buffer Count: {}
  Total Evictions: {}
  Backpressure Events: {}
  Bytes Evicted: {}
  Flush Operations: {}",
            self.total_bytes,
            self.total_entries,
            self.buffer_count,
            self.evictions_total,
            self.backpressure_events,
            self.bytes_evicted,
            self.flush_operations,
        )
    }
}

/// Error types for Global Log Buffer operations
#[derive(Debug, thiserror::Error)]
pub enum GlobalLogBufferError {
    #[error("Backpressure timeout exceeded")]
    BackpressureTimeout,

    #[error("Buffer full: current={current_bytes}, max={max_bytes}")]
    BufferFull { current_bytes: u64, max_bytes: u64 },

    #[error("Job buffer not found: {job_id}")]
    JobBufferNotFound { job_id: String },

    #[error("Flush operation cancelled")]
    FlushCancelled,
}

/// Command for the background persister task
#[derive(Debug)]
enum PersisterCommand {
    Flush {
        job_id: String,
        entries: Vec<LogBufferEntry>,
        respond_to: oneshot::Sender<Result<u64, GlobalLogBufferError>>,
    },
    Shutdown,
}

/// Global Log Buffer with memory bounds and backpressure
///
/// This struct provides a singleton-like log buffer that:
/// - Limits total memory usage to a configurable maximum
/// - Implements LRU eviction when memory is exhausted
/// - Provides backpressure signals when buffer is near capacity
/// - Supports background persistence to external storage
#[derive(Debug)]
pub struct GlobalLogBuffer {
    max_bytes: u64,
    max_entries_per_job: usize,
    eviction_threshold_bytes: u64,
    buffers: DashMap<String, Arc<LogBuffer>>,
    total_bytes: AtomicU64,
    total_entries: AtomicU64,
    metrics: std::sync::Arc<std::sync::Mutex<GlobalLogBufferMetrics>>,
    persister_tx: Option<mpsc::Sender<PersisterCommand>>,
    backpressure_timeout: Duration,
}

impl GlobalLogBuffer {
    /// Create a new Global Log Buffer
    pub fn new(
        config: GlobalLogBufferConfig,
        persister_tx: Option<mpsc::Sender<PersisterCommand>>,
    ) -> Self {
        let eviction_threshold =
            (config.max_bytes * config.eviction_threshold_percent as u64) / 100;

        Self {
            max_bytes: config.max_bytes,
            max_entries_per_job: config.max_entries_per_job,
            eviction_threshold_bytes: eviction_threshold,
            buffers: DashMap::new(),
            total_bytes: AtomicU64::new(0),
            total_entries: AtomicU64::new(0),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(GlobalLogBufferMetrics::new())),
            persister_tx,
            backpressure_timeout: config.backpressure_timeout,
        }
    }

    /// Create a new Global Log Buffer with default configuration
    #[allow(dead_code)]
    pub fn with_default_config() -> Self {
        Self::new(GlobalLogBufferConfig::default(), None)
    }

    /// Get or create a buffer for a job
    fn get_or_create_buffer(&self, job_id: &str) -> Arc<LogBuffer> {
        self.buffers
            .entry(job_id.to_string())
            .or_insert_with(|| Arc::new(LogBuffer::new(job_id.to_string())))
            .value()
            .clone()
    }

    /// Push a log entry to the buffer with backpressure handling
    ///
    /// # Arguments
    /// * `job_id` - The job this log belongs to
    /// * `entry` - The log entry to push
    ///
    /// # Returns
    /// * `Result<u64, GlobalLogBufferError>` - New total bytes or error
    pub async fn push(
        &self,
        job_id: &str,
        entry: LogBufferEntry,
    ) -> Result<u64, GlobalLogBufferError> {
        // Check if we need backpressure
        let current_total = self.total_bytes.load(Ordering::Acquire);
        let entry_size = entry.size_bytes;

        if current_total + entry_size > self.max_bytes {
            // Try to evict oldest buffers
            self.trigger_eviction().await;

            // Re-check after eviction
            let new_total = self.total_bytes.load(Ordering::Acquire);
            if new_total + entry_size > self.max_bytes {
                // Still over capacity - increment backpressure counter
                self.metrics
                    .lock()
                    .unwrap()
                    .backpressure_events
                    .fetch_add(1, Ordering::Relaxed);

                // Wait for backpressure timeout then try once more
                tokio::time::sleep(self.backpressure_timeout).await;

                let final_total = self.total_bytes.load(Ordering::Acquire);
                if final_total + entry_size > self.max_bytes {
                    return Err(GlobalLogBufferError::BufferFull {
                        current_bytes: final_total,
                        max_bytes: self.max_bytes,
                    });
                }
            }
        }

        // Check proactive eviction threshold
        let current_total = self.total_bytes.load(Ordering::Acquire);
        if current_total >= self.eviction_threshold_bytes {
            self.trigger_eviction().await;
        }

        // Get or create buffer for this job
        let buffer = self.get_or_create_buffer(job_id);

        // Push to buffer
        let _new_bytes = buffer.push(entry, self.max_entries_per_job).await;

        // Update totals atomically
        self.total_bytes.fetch_add(entry_size, Ordering::Release);
        self.total_entries.fetch_add(1, Ordering::Release);

        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.total_bytes.fetch_add(entry_size, Ordering::Relaxed);
            metrics.total_entries.fetch_add(1, Ordering::Relaxed);
            if self.buffers.len() as u64 > metrics.buffer_count.load(Ordering::Relaxed) {
                metrics
                    .buffer_count
                    .store(self.buffers.len() as u64, Ordering::Relaxed);
            }
        }

        Ok(self.total_bytes.load(Ordering::Acquire))
    }

    /// Trigger eviction of oldest buffers
    async fn trigger_eviction(&self) {
        let mut evicted_count = 0u64;
        let mut evicted_bytes = 0u64;

        // Find and evict oldest buffers until we're under threshold
        let target_bytes = self.max_bytes / 2; // Evict down to 50%

        while self.total_bytes.load(Ordering::Acquire) > target_bytes {
            // Find oldest buffer
            let oldest = self
                .buffers
                .iter()
                .min_by_key(|entry| entry.value().last_access());

            if let Some(entry) = oldest {
                let buffer = entry.value();
                let job_id = buffer.job_id.clone();

                // Flush to persister if available
                if let Some(ref tx) = self.persister_tx {
                    let entries = buffer.get_all_entries().await;
                    if !entries.is_empty() {
                        let (resp_tx, resp_rx) = oneshot::channel();
                        let cmd = PersisterCommand::Flush {
                            job_id: job_id.clone(),
                            entries,
                            respond_to: resp_tx,
                        };

                        if tx.send(cmd).await.is_ok() {
                            // Wait for flush to complete (or timeout)
                            let _ = tokio::time::timeout(Duration::from_secs(10), resp_rx).await;
                        }
                    }
                }

                // Clear the buffer
                let _cleared_bytes = buffer.clear().await;

                // Remove from map
                self.buffers.remove(&job_id);

                evicted_count += 1;
                evicted_bytes += buffer.bytes_count();
            } else {
                break;
            }

            // Prevent infinite loop
            if evicted_count > 100 {
                warn!("Eviction limit reached, forcing cleanup");
                break;
            }
        }

        // Update totals
        self.total_bytes.fetch_sub(evicted_bytes, Ordering::Release);

        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics
                .evictions_total
                .fetch_add(evicted_count, Ordering::Relaxed);
            metrics
                .bytes_evicted
                .fetch_add(evicted_bytes, Ordering::Relaxed);
        }

        if evicted_count > 0 {
            info!(
                "Evicted {} buffers, {} bytes reclaimed",
                evicted_count, evicted_bytes
            );
        }
    }

    /// Get logs for a specific job
    pub async fn get_logs(&self, job_id: &str) -> Option<Vec<LogBufferEntry>> {
        if let Some(buffer) = self.buffers.get(job_id) {
            Some(buffer.value().get_all_entries().await)
        } else {
            None
        }
    }

    /// Get logs for a job with sequence filtering
    pub async fn get_logs_since(
        &self,
        job_id: &str,
        since_sequence: u64,
    ) -> Option<Vec<LogBufferEntry>> {
        if let Some(buffer) = self.buffers.get(job_id) {
            let entries = buffer.value().get_all_entries().await;
            Some(
                entries
                    .into_iter()
                    .filter(|e| e.sequence > since_sequence)
                    .collect(),
            )
        } else {
            None
        }
    }

    /// Remove a job's buffer (after job completion)
    pub async fn remove_job(&self, job_id: &str) -> Option<u64> {
        if let Some((_, buffer)) = self.buffers.remove(job_id) {
            let bytes = buffer.clear().await;
            self.total_bytes.fetch_sub(bytes, Ordering::Release);
            self.total_entries
                .fetch_sub(buffer.len().await as u64, Ordering::Release);
            Some(bytes)
        } else {
            None
        }
    }

    /// Get current metrics
    pub fn metrics(&self) -> GlobalLogBufferMetricsSnapshot {
        self.metrics.lock().unwrap().snapshot()
    }

    /// Get current total bytes
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Acquire)
    }

    /// Get current buffer count
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Get buffer utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        let total = self.total_bytes.load(Ordering::Acquire);
        (total as f64 / self.max_bytes as f64) * 100.0
    }

    /// Force flush all buffers (for shutdown)
    pub async fn flush_all(&self) {
        self.metrics
            .lock()
            .unwrap()
            .flush_operations
            .fetch_add(1, Ordering::Relaxed);

        // Collect all job IDs first (can't iterate while modifying)
        let job_ids: Vec<_> = self.buffers.iter().map(|e| e.key().clone()).collect();

        for job_id in job_ids {
            if let Some((_, buffer)) = self.buffers.remove(&job_id) {
                let entries = buffer.get_all_entries().await;
                if !entries.is_empty() {
                    // Send to persister
                    if let Some(ref tx) = self.persister_tx {
                        let (resp_tx, resp_rx) = oneshot::channel();
                        let cmd = PersisterCommand::Flush {
                            job_id: job_id.clone(),
                            entries,
                            respond_to: resp_tx,
                        };

                        let _ = tx.send(cmd).await;
                        let _ = tokio::time::timeout(Duration::from_secs(10), resp_rx).await;
                    }
                }

                self.total_bytes
                    .fetch_sub(buffer.bytes_count(), Ordering::Release);
            }
        }
    }
}

/// Convert i64 timestamp to prost_types::Timestamp
fn to_prost_timestamp(ts: Option<i64>) -> Option<Timestamp> {
    ts.map(|s| Timestamp {
        seconds: s,
        nanos: 0,
    })
}

/// Background task for persisting logs
///
/// This task receives flush commands and persists log entries to external storage.
/// It runs continuously and handles graceful shutdown.
pub async fn run_log_persister<M: LogStorage + Send + Sync + 'static>(
    mut receiver: mpsc::Receiver<PersisterCommand>,
    storage: Arc<M>,
) {
    info!("Log persister started");

    while let Some(cmd) = receiver.recv().await {
        match cmd {
            PersisterCommand::Flush {
                job_id,
                entries,
                respond_to,
            } => {
                let mut bytes_persisted = 0u64;

                // Persist each entry
                for entry in &entries {
                    let ts = to_prost_timestamp(entry.timestamp);
                    if let Err(e) = storage
                        .append_log(&job_id, &entry.line, entry.is_stderr, ts.as_ref())
                        .await
                    {
                        warn!("Failed to persist log for job {}: {}", job_id, e);
                    } else {
                        bytes_persisted += entry.size_bytes;
                    }
                }

                // Finalize the job log
                if let Err(e) = storage.finalize_job_log(&job_id).await {
                    warn!("Failed to finalize log for job {}: {}", job_id, e);
                }

                // Signal completion
                let _ = respond_to.send(Ok(bytes_persisted));
            }
            PersisterCommand::Shutdown => {
                info!("Log persister shutting down");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_push_and_get() {
        let buffer = GlobalLogBuffer::with_default_config();

        let entry = LogBufferEntry::new(
            "Test log line".to_string(),
            false,
            Some(chrono::Utc::now().timestamp()),
            1,
        );

        buffer.push("job-1", entry).await.unwrap();

        let logs = buffer.get_logs("job-1").await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].line, "Test log line");
    }

    #[tokio::test]
    async fn test_multiple_jobs() {
        let buffer = GlobalLogBuffer::with_default_config();

        for i in 0..5 {
            let entry = LogBufferEntry::new(
                format!("Line {}", i),
                false,
                Some(chrono::Utc::now().timestamp()),
                i as u64,
            );
            buffer.push("job-1", entry).await.unwrap();
        }

        for i in 0..3 {
            let entry = LogBufferEntry::new(
                format!("Error {}", i),
                true,
                Some(chrono::Utc::now().timestamp()),
                i as u64,
            );
            buffer.push("job-2", entry).await.unwrap();
        }

        assert_eq!(buffer.buffer_count(), 2);
        assert_eq!(buffer.get_logs("job-1").await.unwrap().len(), 5);
        assert_eq!(buffer.get_logs("job-2").await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_remove_job() {
        let buffer = GlobalLogBuffer::with_default_config();

        let entry = LogBufferEntry::new("Test".to_string(), false, None, 1);
        buffer.push("job-1", entry).await.unwrap();

        assert_eq!(buffer.buffer_count(), 1);
        assert!(buffer.get_logs("job-1").await.is_some());

        buffer.remove_job("job-1").await;

        assert_eq!(buffer.buffer_count(), 0);
        assert!(buffer.get_logs("job-1").await.is_none());
    }

    #[tokio::test]
    async fn test_circular_buffer_per_job() {
        let config = GlobalLogBufferConfig {
            max_entries_per_job: 10,
            ..Default::default()
        };
        let buffer = GlobalLogBuffer::new(config, None);

        // Push more than max entries per job
        for i in 0..15 {
            let entry = LogBufferEntry::new(format!("Line {}", i), false, None, i as u64);
            buffer.push("job-1", entry).await.unwrap();
        }

        // Should have only last 10 entries
        let logs = buffer.get_logs("job-1").await.unwrap();
        assert_eq!(logs.len(), 10);
        assert_eq!(logs[0].line, "Line 5"); // First 5 were evicted
        assert_eq!(logs[9].line, "Line 14");
    }

    #[tokio::test]
    async fn test_metrics() {
        let buffer = GlobalLogBuffer::with_default_config();

        for i in 0..10 {
            let entry = LogBufferEntry::new(format!("Line {}", i), false, None, i as u64);
            buffer.push("job-1", entry).await.unwrap();
        }

        let metrics = buffer.metrics();
        assert!(metrics.total_bytes > 0);
        assert_eq!(metrics.total_entries, 10);
        assert_eq!(metrics.buffer_count, 1);
    }

    #[tokio::test]
    async fn test_sequence_filtering() {
        let buffer = GlobalLogBuffer::with_default_config();

        for i in 0..10 {
            let entry = LogBufferEntry::new(format!("Line {}", i), false, None, i as u64);
            buffer.push("job-1", entry).await.unwrap();
        }

        let logs = buffer.get_logs_since("job-1", 5).await.unwrap();
        assert_eq!(logs.len(), 4); // Lines 6, 7, 8, 9
        assert_eq!(logs[0].line, "Line 6");
    }

    #[tokio::test]
    async fn test_utilization() {
        let config = GlobalLogBufferConfig {
            max_bytes: 1000,
            backpressure_timeout: Duration::ZERO, // Disable long wait in tests
            ..Default::default()
        };
        let buffer = GlobalLogBuffer::new(config, None);

        // Push entries until we reach capacity
        let mut pushed = 0;
        while let Ok(_) = buffer
            .push(
                "job-1",
                LogBufferEntry::new("x".repeat(100), false, None, pushed as u64),
            )
            .await
        {
            pushed += 1;
            if pushed > 100 {
                break; // Safety limit
            }
        }

        let utilization = buffer.utilization_percent();
        assert!(utilization >= 0.0 && utilization <= 100.0);
    }
}

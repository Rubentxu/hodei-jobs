//! Log Buffer for Individual Jobs
//!
//! Manages log entries for a single job with size tracking and access time.

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::debug;

use crate::shared_kernel::JobId;

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub job_id: JobId,
    pub line: String,
    pub is_stderr: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub sequence: u64,
}

impl LogEntry {
    pub fn new(
        job_id: JobId,
        line: String,
        is_stderr: bool,
        timestamp: chrono::DateTime<chrono::Utc>,
        sequence: u64,
    ) -> Self {
        Self {
            job_id,
            line,
            is_stderr,
            timestamp,
            sequence,
        }
    }

    /// Calculate the size of this entry in bytes
    pub fn size_bytes(&self) -> u64 {
        // Estimate size: job_id (16 bytes UUID) + line length + overhead
        16 + self.line.len() as u64 + 64 // 64 bytes for other fields
    }
}

/// Buffer for storing log entries of a single job
pub struct LogBuffer {
    job_id: JobId,
    entries: RwLock<Vec<LogEntry>>,
    bytes_count: AtomicU64,
    last_access: AtomicU64,
}

impl std::fmt::Debug for LogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogBuffer")
            .field("job_id", &self.job_id)
            .field("entries_count", &self.entries.blocking_read().len())
            .field("bytes_count", &self.bytes_count.load(Ordering::Relaxed))
            .finish()
    }
}

impl LogBuffer {
    /// Create a new log buffer for a job
    pub fn new(job_id: JobId) -> Self {
        Self {
            job_id,
            entries: RwLock::new(Vec::new()),
            bytes_count: AtomicU64::new(0),
            last_access: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        }
    }

    /// Push a new log entry to the buffer
    ///
    /// Returns the size of the added entry
    pub async fn push(
        &self,
        entry: LogEntry,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let entry_size = entry.size_bytes();

        let mut entries = self.entries.write().await;
        entries.push(entry);

        self.bytes_count.fetch_add(entry_size, Ordering::Relaxed);
        self.update_last_access();

        Ok(entry_size)
    }

    /// Get log entries from the buffer
    pub async fn get_logs(&self, limit: Option<usize>) -> Vec<LogEntry> {
        self.update_last_access();

        let entries = self.entries.read().await;
        if let Some(limit) = limit {
            entries.iter().rev().take(limit).rev().cloned().collect()
        } else {
            entries.clone()
        }
    }

    /// Get the current size of the buffer in bytes
    pub fn size_bytes(&self) -> u64 {
        self.bytes_count.load(Ordering::Relaxed)
    }

    /// Get the number of entries in the buffer
    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }

    /// Check if buffer is empty
    pub async fn is_empty(&self) -> bool {
        let entries = self.entries.read().await;
        entries.is_empty()
    }

    /// Get last access timestamp
    pub fn last_access(&self) -> u64 {
        self.last_access.load(Ordering::Relaxed)
    }

    /// Update last access time
    fn update_last_access(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_access.store(now, Ordering::Relaxed);
    }

    /// Flush buffer to persistence layer
    ///
    /// This is a placeholder implementation. In production, this would
    /// send the logs to a persistent storage backend.
    pub async fn flush_to_persister(&self) -> u64 {
        let size = self.size_bytes();
        let entry_count = {
            let entries = self.entries.read().await;
            entries.len()
        };

        debug!(
            job_id = %self.job_id,
            bytes = size,
            entries = entry_count,
            "Flushing log buffer to persistence"
        );

        // In a real implementation, this would:
        // 1. Serialize the logs
        // 2. Send to S3, PostgreSQL, or other persistent storage
        // 3. Return the size flushed

        size
    }
}

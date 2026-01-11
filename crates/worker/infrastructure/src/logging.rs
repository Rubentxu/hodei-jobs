use hodei_jobs::{LogEntry, WorkerMessage, worker_message::Payload as WorkerPayload};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{warn, info, debug, error};

/// FileLogger for local job log persistence
#[derive(Clone)]
pub struct FileLogger {
    log_dir: PathBuf,
}

impl FileLogger {
    pub fn new<P: AsRef<Path>>(log_dir: P) -> Self {
        Self {
            log_dir: log_dir.as_ref().to_path_buf(),
        }
    }

    /// Appends a log entry to a job-specific log file
    pub async fn log(&self, entry: &LogEntry) -> std::io::Result<()> {
        if !self.log_dir.exists() {
            tokio::fs::create_dir_all(&self.log_dir).await?;
        }

        let file_path = self.log_dir.join(format!("job-{}.log", entry.job_id));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .await?;

        let timestamp = entry
            .timestamp
            .as_ref()
            .map(|t| format!("{}.{:03}Z", t.seconds, t.nanos / 1_000_000))
            .unwrap_or_else(|| "unknown".to_string());

        let prefix = if entry.is_stderr { "[ERR]" } else { "[OUT]" };
        let line = format!("{} {} {}\n", timestamp, prefix, entry.line);

        file.write_all(line.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }
}

/// LogBatcher for efficient log streaming with batching
/// Reduces overhead by sending logs in batches instead of one by one
pub struct LogBatcher {
    /// Channel to send WorkerMessage
    tx: mpsc::Sender<WorkerMessage>,
    /// Buffer to accumulate log entries
    buffer: Vec<LogEntry>,
    /// Maximum number of entries before flush
    capacity: usize,
    /// Time interval for automatic flush
    flush_interval: Duration,
    /// Timestamp of last flush
    last_flush: Instant,
}

impl LogBatcher {
    /// Create a new LogBatcher
    pub fn new(tx: mpsc::Sender<WorkerMessage>, capacity: usize, flush_interval: Duration) -> Self {
        Self {
            tx,
            buffer: Vec::with_capacity(capacity),
            capacity,
            flush_interval,
            last_flush: Instant::now(),
        }
    }

    /// Push a log entry to the batcher
    /// Flushes when buffer is full or interval elapsed
    pub async fn push(&mut self, entry: LogEntry) {
        debug!(
            job_id = %entry.job_id,
            buffer_size = self.buffer.len(),
            capacity = self.capacity,
            "LogBatcher: Pushing log entry"
        );

        self.buffer.push(entry);

        // Flush when buffer reaches capacity
        if self.buffer.len() >= self.capacity {
            info!(
                buffer_size = self.buffer.len(),
                "LogBatcher: Buffer full, triggering flush"
            );
            self.flush().await;
        }
    }

    /// Flush the buffer to the channel (blocking)
    /// Returns true if flush succeeded, false if failed
    pub async fn flush(&mut self) -> bool {
        if self.buffer.is_empty() {
            debug!("LogBatcher: Buffer empty, skipping flush");
            return true;
        }

        // Take the buffer contents
        let batch = std::mem::take(&mut self.buffer);
        let entry_count = batch.len();

        // Create LogBatch message
        let job_id = batch[0].job_id.clone();

        info!(
            job_id = %job_id,
            entry_count = entry_count,
            "LogBatcher: Flushing batch to server"
        );

        let msg = WorkerMessage {
            payload: Some(WorkerPayload::LogBatch(hodei_jobs::LogBatch {
                job_id: job_id.clone(),
                entries: batch,
            })),
        };

        // Send (blocking with timeout to avoid hanging)
        debug!(
            job_id = %job_id,
            "LogBatcher: Attempting to send batch via channel"
        );

        match tokio::time::timeout(Duration::from_secs(5), self.tx.send(msg)).await {
            Ok(result) => match result {
                Ok(_) => {
                    info!(
                        job_id = %job_id,
                        entry_count = entry_count,
                        "✅ LogBatcher: Batch sent successfully"
                    );
                    self.last_flush = Instant::now();
                    true
                }
                Err(e) => {
                    error!(
                        error = %e,
                        job_id = %job_id,
                        entry_count = entry_count,
                        "❌ LogBatcher: Failed to send log batch - channel error"
                    );
                    false
                }
            },
            Err(_) => {
                error!(
                    job_id = %job_id,
                    timeout_sec = 5,
                    entry_count = entry_count,
                    "❌ LogBatcher: Send timed out after 5 seconds"
                );
                false
            }
        }
    }

    /// Get the buffer size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Manually trigger flush if time interval elapsed
    pub async fn flush_if_needed(&mut self) -> bool {
        if self.last_flush.elapsed() >= self.flush_interval && !self.buffer.is_empty() {
            self.flush().await
        } else {
            true
        }
    }
}

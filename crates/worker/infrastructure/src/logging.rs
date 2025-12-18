use hodei_jobs::{LogEntry, WorkerMessage, worker_message::Payload as WorkerPayload};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::warn;

/// Log batching buffer capacity
pub const LOG_BATCHER_CAPACITY: usize = 100;

/// Log flush interval in milliseconds
pub const LOG_FLUSH_INTERVAL_MS: u64 = 100;

fn current_timestamp() -> prost_types::Timestamp {
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0));
    prost_types::Timestamp {
        seconds: since_epoch.as_secs() as i64,
        nanos: since_epoch.subsec_nanos() as i32,
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
    /// Automatically flushes when capacity is reached
    pub async fn push(&mut self, entry: LogEntry) {
        self.buffer.push(entry);

        // Flush if capacity reached
        if self.buffer.len() >= self.capacity {
            self.flush().await;
        }
    }

    /// Flush the buffer to the channel (non-blocking)
    /// Returns true if flush succeeded, false if dropped due to backpressure
    pub async fn flush(&mut self) -> bool {
        if self.buffer.is_empty() {
            return true;
        }

        // Take the buffer contents
        let batch = std::mem::take(&mut self.buffer);

        // Create LogBatch message
        let job_id = batch[0].job_id.clone();
        let msg = WorkerMessage {
            payload: Some(WorkerPayload::LogBatch(hodei_jobs::LogBatch {
                job_id,
                entries: batch,
            })),
        };

        // Try to send (non-blocking for backpressure)
        let result = self.tx.try_send(msg);

        match result {
            Ok(_) => {
                self.last_flush = Instant::now();
                true
            }
            Err(_) => {
                // On backpressure, logs are dropped to prioritize job execution
                // This is acceptable in high-performance scenarios
                warn!("Log batch dropped due to backpressure");
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

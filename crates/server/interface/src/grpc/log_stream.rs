//! Log Streaming Service with Persistent Storage
//!
//! Provides real-time log streaming for job execution monitoring with optional
//! persistent storage to files (local, S3, etc.)
//!
//! Features:
//! - Buffered log storage (circular buffer per job)
//! - Real-time streaming to subscribed clients
//! - Persistent log storage with pluggable backends
//! - Historical log retrieval from persistent storage

use dashmap::DashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

use crate::log_persistence::{LogStorage, LogStorageRef};
use hodei_jobs::{
    GetLogsRequest, GetLogsResponse, JobLogEntry, LogEntry, SubscribeLogsRequest,
    log_stream_service_server::LogStreamService as LogStreamServiceTrait,
};
use prost_types;

/// Buffer size for log entries per job (increased to 1000 for longer jobs)
const LOG_BUFFER_SIZE: usize = 1000;

/// Log entry with metadata
#[derive(Debug, Clone)]
pub struct BufferedLogEntry {
    pub job_id: String,
    pub line: String,
    pub is_stderr: bool,
    pub timestamp: Option<prost_types::Timestamp>,
    pub sequence: u64,
}

/// Manages log buffers and subscribers for jobs with persistent storage
pub struct LogStreamService {
    /// EPIC-42: Buffered logs per job (circular buffer) - DashMap para concurrencia sin bloqueos
    logs: Arc<DashMap<String, Vec<BufferedLogEntry>>>,
    /// EPIC-42: Active subscribers per job - DashMap para concurrencia sin bloqueos
    subscribers: Arc<DashMap<String, Vec<mpsc::Sender<LogEntry>>>>,
    /// EPIC-42: Sequence counter per job - DashMap para concurrencia sin bloqueos
    sequences: Arc<DashMap<String, u64>>,
    /// Persistent log storage backend (optional)
    storage: Option<Box<dyn LogStorage>>,
    /// Callback for when a job log is finalized (to store in DB)
    on_log_finalized: Option<Arc<dyn Fn(LogStorageRef) + Send + Sync>>,
}

impl std::fmt::Debug for LogStreamService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogStreamService")
            .field("logs", &"<DashMap>")
            .field("subscribers", &"<DashMap>")
            .field("sequences", &"<DashMap>")
            .field("storage", &self.storage.as_ref().map(|_| "<LogStorage>"))
            .field(
                "on_log_finalized",
                &self.on_log_finalized.as_ref().map(|_| "<Callback>"),
            )
            .finish()
    }
}

impl Default for LogStreamService {
    fn default() -> Self {
        Self::new()
    }
}

impl LogStreamService {
    /// Create new service without persistence
    pub fn new() -> Self {
        Self {
            logs: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            sequences: Arc::new(DashMap::new()),
            storage: None,
            on_log_finalized: None,
        }
    }

    /// Create new service with persistent storage
    pub fn with_storage(
        storage: Box<dyn LogStorage>,
        on_log_finalized: Option<Arc<dyn Fn(LogStorageRef) + Send + Sync>>,
    ) -> Self {
        info!("LogStreamService initialized with persistent storage");
        Self {
            logs: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            sequences: Arc::new(DashMap::new()),
            storage: Some(storage),
            on_log_finalized,
        }
    }

    /// Append a log entry for a job
    pub async fn append_log(&self, entry: LogEntry) {
        let job_id = entry.job_id.clone();

        // EPIC-42: DashMap para incremento de secuencia concurrente
        let sequence = {
            let mut seq = self.sequences.entry(job_id.clone()).or_insert(0);
            *seq += 1;
            *seq
        };

        let buffered = BufferedLogEntry {
            job_id: job_id.clone(),
            line: entry.line.clone(),
            is_stderr: entry.is_stderr,
            timestamp: entry.timestamp.clone(),
            sequence,
        };

        // EPIC-42: DashMap para buffer circular concurrente
        {
            let mut buffer = self.logs.entry(job_id.clone()).or_insert_with(Vec::new);

            if buffer.len() >= LOG_BUFFER_SIZE {
                buffer.remove(0);
            }
            buffer.push(buffered);
        }

        // Notify subscribers
        {
            if let Some(job_subs) = self.subscribers.get(&job_id) {
                for tx in job_subs.value().iter() {
                    if let Err(e) = tx.send(entry.clone()).await {
                        warn!(
                            error = %e,
                            job_id = %job_id,
                            "Failed to send log to subscriber, entry will be delivered on next subscribe"
                        );
                    }
                }
            }
        }

        // Persist to storage backend (if configured)
        if let Some(storage) = &self.storage {
            if let Err(e) = storage
                .append_log(
                    &job_id,
                    &entry.line,
                    entry.is_stderr,
                    entry.timestamp.as_ref(),
                )
                .await
            {
                warn!("Failed to persist log for job {}: {}", job_id, e);
            }
        }

        debug!("Log appended for job {}: {}", job_id, entry.line);
    }

    /// Subscribe to logs for a specific job
    pub async fn subscribe(&self, job_id: &str) -> Pin<Box<dyn Stream<Item = LogEntry> + Send>> {
        let (tx, rx) = mpsc::channel::<LogEntry>(100);

        // Send buffered logs first
        {
            if let Some(buffer) = self.logs.get(job_id) {
                for entry in buffer.value() {
                    let log_entry = LogEntry {
                        job_id: entry.job_id.clone(),
                        line: entry.line.clone(),
                        is_stderr: entry.is_stderr,
                        timestamp: entry.timestamp.clone(),
                    };
                    let _ = tx.send(log_entry).await;
                }
            }
        }

        // Register subscriber for new logs
        {
            // EPIC-42: DashMap para registro de subscribers concurrente
            self.subscribers
                .entry(job_id.to_string())
                .or_insert_with(Vec::new)
                .push(tx);
        }

        info!("New subscriber for job {}", job_id);
        Box::pin(ReceiverStream::new(rx))
    }

    /// Get historical logs for a job
    pub async fn get_logs(&self, job_id: &str, limit: Option<usize>) -> Vec<LogEntry> {
        // EPIC-42: DashMap para lectura concurrente
        if let Some(buffer) = self.logs.get(job_id) {
            let limit = limit.unwrap_or(buffer.len());
            buffer
                .iter()
                .rev()
                .take(limit)
                .rev()
                .map(|e| LogEntry {
                    job_id: e.job_id.clone(),
                    line: e.line.clone(),
                    is_stderr: e.is_stderr,
                    timestamp: e.timestamp.clone(),
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Clear logs for a completed job
    pub async fn clear_logs(&self, job_id: &str) {
        // EPIC-42: DashMap para limpieza concurrente sin bloqueos
        self.logs.remove(job_id);
        self.subscribers.remove(job_id);
        self.sequences.remove(job_id);
        debug!("Logs cleared for job {}", job_id);
    }

    /// Finalize and persist log file for a completed job
    pub async fn finalize_job_log(
        &self,
        job_id: &str,
    ) -> Result<Option<LogStorageRef>, Box<dyn std::error::Error + Send + Sync>> {
        // Finalize persistent log storage
        let log_ref = if let Some(storage) = &self.storage {
            match storage.finalize_job_log(job_id).await {
                Ok(log_ref) => {
                    info!(
                        "Finalized persistent log for job {}: {} bytes",
                        job_id, log_ref.size_bytes
                    );
                    Some(log_ref)
                }
                Err(e) => {
                    warn!(
                        "Failed to finalize persistent log for job {}: {}",
                        job_id, e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Clear in-memory buffers
        self.clear_logs(job_id).await;

        // Notify via callback (e.g., to store in database)
        if let Some(log_ref) = &log_ref {
            if let Some(callback) = &self.on_log_finalized {
                callback(log_ref.clone());
            }
        }

        Ok(log_ref)
    }

    /// Unsubscribe all subscribers for a job (usually when job completes)
    pub async fn close_subscribers(&self, job_id: &str) {
        // EPIC-42: DashMap para cierre de subscribers concurrente
        self.subscribers.remove(job_id);
    }

    /// Get buffered logs with sequence info
    pub async fn get_logs_with_sequence(
        &self,
        job_id: &str,
        limit: Option<usize>,
        since_sequence: Option<u64>,
    ) -> Vec<BufferedLogEntry> {
        // EPIC-42: DashMap para lectura concurrente
        if let Some(buffer) = self.logs.get(job_id) {
            let filtered: Vec<_> = buffer
                .iter()
                .filter(|e| since_sequence.map_or(true, |seq| e.sequence > seq))
                .cloned()
                .collect();

            let limit = limit.unwrap_or(filtered.len());
            filtered.into_iter().rev().take(limit).rev().collect()
        } else {
            Vec::new()
        }
    }
}

// =============================================================================
// gRPC SERVICE IMPLEMENTATION (PRD v6.0 Section 10.8)
// =============================================================================

/// Wrapper for gRPC service
#[derive(Clone)]
pub struct LogStreamServiceGrpc {
    inner: Arc<LogStreamService>,
}

impl std::fmt::Debug for LogStreamServiceGrpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogStreamServiceGrpc").finish()
    }
}

impl LogStreamServiceGrpc {
    pub fn new(service: Arc<LogStreamService>) -> Self {
        Self { inner: service }
    }

    pub fn inner(&self) -> &LogStreamService {
        &self.inner
    }
}

#[tonic::async_trait]
impl LogStreamServiceTrait for LogStreamServiceGrpc {
    type SubscribeLogsStream = Pin<Box<dyn Stream<Item = Result<JobLogEntry, Status>> + Send>>;

    /// Subscribe to real-time logs for a job
    async fn subscribe_logs(
        &self,
        request: Request<SubscribeLogsRequest>,
    ) -> Result<Response<Self::SubscribeLogsStream>, Status> {
        let req = request.into_inner();
        let job_id = req.job_id;

        info!("Client subscribing to logs for job: {}", job_id);

        let (tx, rx) = mpsc::channel::<Result<JobLogEntry, Status>>(100);

        // Send buffered logs first if requested
        if req.include_history {
            let limit = if req.tail_lines > 0 {
                Some(req.tail_lines as usize)
            } else {
                None
            };
            let buffered = self
                .inner
                .get_logs_with_sequence(&job_id, limit, None)
                .await;

            for entry in buffered {
                let log_entry = JobLogEntry {
                    job_id: entry.job_id,
                    line: entry.line,
                    is_stderr: entry.is_stderr,
                    timestamp: entry.timestamp,
                    sequence: entry.sequence as i64,
                };
                if tx.send(Ok(log_entry)).await.is_err() {
                    return Err(Status::internal("Failed to send buffered logs"));
                }
            }
        }

        // Subscribe to new logs
        let mut log_stream = self.inner.subscribe(&job_id).await;
        let sequences = self.inner.sequences.clone();

        // Forward new logs to client
        tokio::spawn(async move {
            while let Some(entry) = log_stream.next().await {
                // Get sequence
                let seq = {
                    sequences
                        .get(&entry.job_id)
                        .map(|s| *s.value())
                        .unwrap_or(0)
                };

                let log_entry = JobLogEntry {
                    job_id: entry.job_id,
                    line: entry.line,
                    is_stderr: entry.is_stderr,
                    timestamp: entry.timestamp,
                    sequence: seq as i64,
                };

                if tx.send(Ok(log_entry)).await.is_err() {
                    debug!("Log subscriber disconnected");
                    break;
                }
            }
        });

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    /// Get historical logs (non-streaming)
    async fn get_logs(
        &self,
        request: Request<GetLogsRequest>,
    ) -> Result<Response<GetLogsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit > 0 {
            Some(req.limit as usize)
        } else {
            None
        };
        let since = if req.since_sequence > 0 {
            Some(req.since_sequence as u64)
        } else {
            None
        };

        let entries = self
            .inner
            .get_logs_with_sequence(&req.job_id, limit, since)
            .await;

        let response = GetLogsResponse {
            entries: entries
                .into_iter()
                .map(|e| JobLogEntry {
                    job_id: e.job_id,
                    line: e.line,
                    is_stderr: e.is_stderr,
                    timestamp: e.timestamp,
                    sequence: e.sequence as i64,
                })
                .collect(),
            has_more: false, // TODO: Implement pagination
        };

        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_append_and_get_logs() {
        let service = LogStreamService::new();

        let entry1 = LogEntry {
            job_id: "job-1".to_string(),
            line: "Hello world".to_string(),
            is_stderr: false,
            timestamp: None,
        };

        let entry2 = LogEntry {
            job_id: "job-1".to_string(),
            line: "Error occurred".to_string(),
            is_stderr: true,
            timestamp: None,
        };

        service.append_log(entry1).await;
        service.append_log(entry2).await;

        let logs = service.get_logs("job-1", None).await;
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].line, "Hello world");
        assert_eq!(logs[1].line, "Error occurred");
        assert!(logs[1].is_stderr);
    }

    #[tokio::test]
    async fn test_subscribe_receives_buffered_logs() {
        let service = LogStreamService::new();

        // Add some logs first
        for i in 0..5 {
            service
                .append_log(LogEntry {
                    job_id: "job-2".to_string(),
                    line: format!("Line {}", i),
                    is_stderr: false,
                    timestamp: None,
                })
                .await;
        }

        // Subscribe
        let mut stream = service.subscribe("job-2").await;

        // Should receive buffered logs
        let mut received = Vec::new();
        for _ in 0..5 {
            if let Some(log) = stream.next().await {
                received.push(log.line);
            }
        }

        assert_eq!(received.len(), 5);
        assert_eq!(received[0], "Line 0");
        assert_eq!(received[4], "Line 4");
    }

    #[tokio::test]
    async fn test_subscriber_receives_new_logs() {
        let service = Arc::new(LogStreamService::new());
        let service_clone = service.clone();

        // Subscribe first
        let mut stream = service.subscribe("job-3").await;

        // Append new log in background
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            service_clone
                .append_log(LogEntry {
                    job_id: "job-3".to_string(),
                    line: "New log entry".to_string(),
                    is_stderr: false,
                    timestamp: None,
                })
                .await;
        });

        // Should receive the new log
        let timeout = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next()).await;

        assert!(timeout.is_ok());
        let log = timeout.unwrap().unwrap();
        assert_eq!(log.line, "New log entry");
    }

    #[tokio::test]
    async fn test_circular_buffer() {
        let service = LogStreamService::new();

        // Add more than buffer size
        for i in 0..(LOG_BUFFER_SIZE + 100) {
            service
                .append_log(LogEntry {
                    job_id: "job-4".to_string(),
                    line: format!("Line {}", i),
                    is_stderr: false,
                    timestamp: None,
                })
                .await;
        }

        let logs = service.get_logs("job-4", None).await;
        assert_eq!(logs.len(), LOG_BUFFER_SIZE);
        // First entry should be line 100 (oldest ones removed)
        assert_eq!(logs[0].line, "Line 100");
    }

    #[tokio::test]
    async fn test_clear_logs() {
        let service = LogStreamService::new();

        service
            .append_log(LogEntry {
                job_id: "job-5".to_string(),
                line: "Test".to_string(),
                is_stderr: false,
                timestamp: None,
            })
            .await;

        assert_eq!(service.get_logs("job-5", None).await.len(), 1);

        service.clear_logs("job-5").await;

        assert_eq!(service.get_logs("job-5", None).await.len(), 0);
    }
}

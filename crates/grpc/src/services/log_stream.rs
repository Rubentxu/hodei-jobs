//! Log Streaming Service (PRD v6.0)
//!
//! Provides real-time log streaming for job execution monitoring.
//!
//! Features:
//! - Buffered log storage (circular buffer per job)
//! - Real-time streaming to subscribed clients
//! - Historical log retrieval

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use hodei_jobs::{
    GetLogsRequest, GetLogsResponse, JobLogEntry, LogEntry, SubscribeLogsRequest,
    log_stream_service_server::LogStreamService as LogStreamServiceTrait,
};

/// Buffer size for log entries per job
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

/// Manages log buffers and subscribers for jobs
#[derive(Debug, Clone)]
pub struct LogStreamService {
    /// Buffered logs per job (circular buffer)
    logs: Arc<RwLock<HashMap<String, Vec<BufferedLogEntry>>>>,
    /// Active subscribers per job
    subscribers: Arc<RwLock<HashMap<String, Vec<mpsc::Sender<LogEntry>>>>>,
    /// Sequence counter per job
    sequences: Arc<RwLock<HashMap<String, u64>>>,
}

impl Default for LogStreamService {
    fn default() -> Self {
        Self::new()
    }
}

impl LogStreamService {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            sequences: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Append a log entry for a job
    pub async fn append_log(&self, entry: LogEntry) {
        let job_id = entry.job_id.clone();

        // Get and increment sequence
        let sequence = {
            let mut seqs = self.sequences.write().await;
            let seq = seqs.entry(job_id.clone()).or_insert(0);
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

        // Add to buffer (circular)
        {
            let mut logs = self.logs.write().await;
            let buffer = logs.entry(job_id.clone()).or_insert_with(Vec::new);

            if buffer.len() >= LOG_BUFFER_SIZE {
                buffer.remove(0);
            }
            buffer.push(buffered);
        }

        // Notify subscribers
        {
            let subs = self.subscribers.read().await;
            if let Some(job_subs) = subs.get(&job_id) {
                for tx in job_subs {
                    let _ = tx.send(entry.clone()).await;
                }
            }
        }

        debug!("Log appended for job {}: {}", job_id, entry.line);
    }

    /// Subscribe to logs for a specific job
    pub async fn subscribe(&self, job_id: &str) -> Pin<Box<dyn Stream<Item = LogEntry> + Send>> {
        let (tx, rx) = mpsc::channel::<LogEntry>(100);

        // Send buffered logs first
        {
            let logs = self.logs.read().await;
            if let Some(buffer) = logs.get(job_id) {
                for entry in buffer {
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
            let mut subs = self.subscribers.write().await;
            subs.entry(job_id.to_string())
                .or_insert_with(Vec::new)
                .push(tx);
        }

        info!("New subscriber for job {}", job_id);
        Box::pin(ReceiverStream::new(rx))
    }

    /// Get historical logs for a job
    pub async fn get_logs(&self, job_id: &str, limit: Option<usize>) -> Vec<LogEntry> {
        let logs = self.logs.read().await;

        if let Some(buffer) = logs.get(job_id) {
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
        self.logs.write().await.remove(job_id);
        self.subscribers.write().await.remove(job_id);
        self.sequences.write().await.remove(job_id);
        debug!("Logs cleared for job {}", job_id);
    }

    /// Unsubscribe all subscribers for a job (usually when job completes)
    pub async fn close_subscribers(&self, job_id: &str) {
        self.subscribers.write().await.remove(job_id);
    }

    /// Get buffered logs with sequence info
    pub async fn get_logs_with_sequence(
        &self,
        job_id: &str,
        limit: Option<usize>,
        since_sequence: Option<u64>,
    ) -> Vec<BufferedLogEntry> {
        let logs = self.logs.read().await;

        if let Some(buffer) = logs.get(job_id) {
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
#[derive(Debug, Clone)]
pub struct LogStreamServiceGrpc {
    inner: LogStreamService,
}

impl LogStreamServiceGrpc {
    pub fn new(service: LogStreamService) -> Self {
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
                    let seqs = sequences.read().await;
                    seqs.get(&entry.job_id).copied().unwrap_or(0)
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

        let output_stream = ReceiverStream::new(rx);
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
        let service = LogStreamService::new();
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

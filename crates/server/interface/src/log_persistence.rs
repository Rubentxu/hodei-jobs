//! Log Persistence Service - Storage-Agnostic Design
//!
//! Provides persistent log storage with pluggable backends (local files, S3, etc.)
//! This design allows switching storage backends without changing client code.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Configuration for log persistence
#[derive(Debug, Clone)]
pub struct LogPersistenceConfig {
    pub enabled: bool,
    pub storage_backend: StorageBackend,
    pub ttl_hours: u64,
}

impl LogPersistenceConfig {
    pub fn new(enabled: bool, storage_backend: StorageBackend, ttl_hours: u64) -> Self {
        Self {
            enabled,
            storage_backend,
            ttl_hours,
        }
    }
}

/// Storage backend configuration
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Local(LocalStorageConfig),
    // Future: S3(S3StorageConfig),
    // Future: AzureBlob(AzureBlobStorageConfig),
    // Future: Gcs(GcsStorageConfig),
}

/// Local file storage configuration
#[derive(Debug, Clone)]
pub struct LocalStorageConfig {
    pub base_path: PathBuf,
}

/// S3 storage configuration (for future use)
#[derive(Debug, Clone)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub prefix: Option<String>,
}

/// Reference to a persistent log storage (storage-agnostic)
/// Could be a local file, S3 object, GCS blob, etc.
#[derive(Debug, Clone)]
pub struct LogStorageRef {
    pub job_id: String,
    pub storage_uri: String,
    pub size_bytes: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub entry_count: u64,
}

/// Storage backend trait - pluggable storage implementations
#[async_trait::async_trait]
pub trait LogStorage: Send + Sync {
    /// Clone the storage instance
    fn clone_box(&self) -> Box<dyn LogStorage>;

    /// Append a log entry
    async fn append_log(
        &self,
        job_id: &str,
        line: &str,
        is_stderr: bool,
        timestamp: Option<&prost_types::Timestamp>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Finalize and close log storage for a completed job
    async fn finalize_job_log(
        &self,
        job_id: &str,
    ) -> Result<LogStorageRef, Box<dyn std::error::Error + Send + Sync>>;

    /// Clean up old log files based on TTL
    async fn cleanup_old_logs(
        &self,
        ttl_hours: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Local file system storage implementation
#[derive(Debug)]
pub struct LocalFileStorage {
    base_path: PathBuf,
    /// Cache of open file writers per job
    writers: Arc<std::sync::Mutex<HashMap<String, BufWriter>>>,
}

impl LocalFileStorage {
    pub fn new(config: LocalStorageConfig) -> Self {
        // Ensure base directory exists
        std::fs::create_dir_all(&config.base_path).expect("Failed to create log directory");
        info!("Local file storage initialized at: {:?}", config.base_path);

        Self {
            base_path: config.base_path,
            writers: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Get the file path for a job
    fn get_job_log_path(&self, job_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.log", job_id))
    }
}

#[async_trait::async_trait]
impl LogStorage for LocalFileStorage {
    fn clone_box(&self) -> Box<dyn LogStorage> {
        Box::new(LocalFileStorage {
            base_path: self.base_path.clone(),
            writers: Arc::new(std::sync::Mutex::new(HashMap::new())), // New writers cache
        })
    }

    async fn append_log(
        &self,
        job_id: &str,
        line: &str,
        is_stderr: bool,
        timestamp: Option<&prost_types::Timestamp>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get or create writer for this job
        let mut writers = self.writers.lock().unwrap();
        let writer = writers.entry(job_id.to_string()).or_insert_with(|| {
            let path = self.get_job_log_path(job_id);
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap_or_else(|e| panic!("Failed to open log file for job {}: {}", job_id, e));
            BufWriter::new(file)
        });

        // Format log entry
        let timestamp_str = timestamp
            .and_then(|t| {
                chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            })
            .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());

        let log_line = if is_stderr {
            format!("[{}] [ERROR] {}\n", timestamp_str, line)
        } else {
            format!("[{}] [INFO] {}\n", timestamp_str, line)
        };

        writer.write_all(log_line.as_bytes())?;
        writer.flush()?;

        Ok(())
    }

    async fn finalize_job_log(
        &self,
        job_id: &str,
    ) -> Result<LogStorageRef, Box<dyn std::error::Error + Send + Sync>> {
        // Remove from writers cache
        let mut writers = self.writers.lock().unwrap();
        writers.remove(job_id);

        // Get file metadata
        let path = self.get_job_log_path(job_id);
        let metadata = std::fs::metadata(&path)?;

        let line_count = Self::count_lines(&path)?;

        let log_ref = LogStorageRef {
            job_id: job_id.to_string(),
            storage_uri: format!("file://{}", path.to_string_lossy()),
            size_bytes: metadata.len(),
            created_at: chrono::Utc::now(),
            entry_count: line_count,
        };

        info!(
            "Finalized log file for job {}: {} bytes, {} lines",
            job_id, log_ref.size_bytes, log_ref.entry_count
        );
        Ok(log_ref)
    }

    async fn cleanup_old_logs(
        &self,
        ttl_hours: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let mut removed_files = Vec::new();
        let ttl_duration = chrono::Duration::hours(ttl_hours as i64);
        let cutoff_time = chrono::Utc::now() - ttl_duration;

        let entries = std::fs::read_dir(&self.base_path)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                let metadata = entry.metadata()?;
                let modified = metadata.modified()?;

                // Convert to chrono
                let modified_time = chrono::DateTime::<chrono::Utc>::from(modified);

                if modified_time < cutoff_time {
                    let file_name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    std::fs::remove_file(&path)?;
                    removed_files.push(file_name);
                    info!("Removed old log file: {:?}", path);
                }
            }
        }

        if !removed_files.is_empty() {
            info!("Cleaned up {} old log files", removed_files.len());
        }

        Ok(removed_files)
    }
}

impl LocalFileStorage {
    fn count_lines(path: &Path) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let content = std::fs::read_to_string(path)?;
        Ok(content.lines().count() as u64)
    }
}

/// Helper struct for buffered writing
#[derive(Debug)]
struct BufWriter {
    buffer: Vec<u8>,
    writer: std::fs::File,
}

impl BufWriter {
    fn new(writer: std::fs::File) -> Self {
        Self {
            buffer: Vec::with_capacity(8192),
            writer,
        }
    }
}

impl std::io::Write for BufWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        if self.buffer.len() >= 8192 {
            self.flush()?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.write_all(&self.buffer)?;
        self.buffer.clear();
        self.writer.flush()?;
        Ok(())
    }
}

impl Drop for BufWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Storage factory - creates appropriate storage backend from config
pub struct LogStorageFactory;

impl LogStorageFactory {
    /// Create storage backend from configuration
    pub fn create(config: &LogPersistenceConfig) -> Box<dyn LogStorage> {
        if !config.enabled {
            // Return a no-op storage that does nothing
            return Box::new(DisabledStorage);
        }

        match &config.storage_backend {
            StorageBackend::Local(local_config) => {
                Box::new(LocalFileStorage::new(local_config.clone()))
            } // StorageBackend::S3(s3_config) => {
              //     Box::new(S3Storage::new(s3_config.clone()))
              // }
        }
    }
}

/// Disabled storage (no-op) for when persistence is disabled
#[derive(Debug, Clone)]
pub struct DisabledStorage;

#[async_trait::async_trait]
impl LogStorage for DisabledStorage {
    fn clone_box(&self) -> Box<dyn LogStorage> {
        Box::new(DisabledStorage)
    }

    async fn append_log(
        &self,
        _job_id: &str,
        _line: &str,
        _is_stderr: bool,
        _timestamp: Option<&prost_types::Timestamp>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn finalize_job_log(
        &self,
        _job_id: &str,
    ) -> Result<LogStorageRef, Box<dyn std::error::Error + Send + Sync>> {
        Err("Log persistence disabled".into())
    }

    async fn cleanup_old_logs(
        &self,
        _ttl_hours: u64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hodei-log-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_test_dir(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_local_storage() {
        let temp_dir = create_test_dir();
        let local_config = LocalStorageConfig {
            base_path: temp_dir.clone(),
        };
        let storage: LocalFileStorage = LocalFileStorage::new(local_config);

        // Append logs
        storage
            .append_log("job-123", "Test log line", false, None)
            .await
            .unwrap();
        storage
            .append_log("job-123", "Error occurred", true, None)
            .await
            .unwrap();

        // Finalize job log
        let log_ref = storage.finalize_job_log("job-123").await.unwrap();

        assert_eq!(log_ref.job_id, "job-123");
        assert!(log_ref.size_bytes > 0);
        assert_eq!(log_ref.entry_count, 2);
        assert!(log_ref.storage_uri.starts_with("file://"));

        // Verify file exists
        let log_path = temp_dir.join("job-123.log");
        assert!(log_path.exists());

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("Test log line"));
        assert!(content.contains("Error occurred"));

        cleanup_test_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_storage_factory() {
        let temp_dir = create_test_dir();
        let local_config = LocalStorageConfig {
            base_path: temp_dir.clone(),
        };
        let config = LogPersistenceConfig {
            enabled: true,
            storage_backend: StorageBackend::Local(local_config),
            ttl_hours: 24,
        };

        let storage = LogStorageFactory::create(&config);
        storage
            .append_log("job-1", "Test", false, None)
            .await
            .unwrap();
        let result = storage.finalize_job_log("job-1").await.unwrap();
        assert_eq!(result.job_id, "job-1");

        cleanup_test_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_disabled_storage() {
        let config = LogPersistenceConfig {
            enabled: false,
            storage_backend: StorageBackend::Local(LocalStorageConfig {
                base_path: PathBuf::from("/tmp"),
            }),
            ttl_hours: 24,
        };

        let storage = LogStorageFactory::create(&config);
        storage
            .append_log("job-1", "Test", false, None)
            .await
            .unwrap();
        assert!(storage.finalize_job_log("job-1").await.is_err());
    }
}

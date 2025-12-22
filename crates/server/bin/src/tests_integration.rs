//! Integration Tests for Log Persistence Architecture
//!
//! Tests verify the complete integration of storage-agnostic log persistence
//! from configuration through storage backend to database.

#[cfg(test)]
mod log_persistence_integration_tests {
    use crate::config::ServerConfig;
    use hodei_server_interface::log_persistence::{
        LocalStorageConfig, LogPersistenceConfig, LogStorageFactory, StorageBackend,
    };
    use std::path::PathBuf;

    fn create_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hodei-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_test_dir(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Test 1: Configuration conversion to storage config
    #[tokio::test]
    async fn test_config_conversion_to_storage_config() {
        let config = ServerConfig {
            port: 50051,
            database_url: Some("postgres://test".to_string()),
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/test-logs".to_string(),
            log_ttl_hours: 168,
        };

        let persistence_config = config.to_log_persistence_config();

        assert!(persistence_config.enabled);
        assert_eq!(persistence_config.ttl_hours, 168);

        match &persistence_config.storage_backend {
            StorageBackend::Local(local) => {
                assert_eq!(local.base_path, PathBuf::from("/tmp/test-logs"));
            }
            _ => panic!("Expected Local storage backend"),
        }
    }

    /// Test 2: S3 backend configuration (future use)
    #[tokio::test]
    async fn test_s3_backend_config_defaults_to_local() {
        let config = ServerConfig {
            port: 50051,
            database_url: Some("postgres://test".to_string()),
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "s3".to_string(), // Unknown backend
            log_persistence_path: "/tmp/test-logs".to_string(),
            log_ttl_hours: 168,
        };

        let persistence_config = config.to_log_persistence_config();

        // Should default to local when unknown backend is specified
        match &persistence_config.storage_backend {
            StorageBackend::Local(_) => {
                // Correctly defaulted to local
            }
            _ => panic!("Should default to Local for unknown backend"),
        }
    }

    /// Test 3: Disabled persistence
    #[tokio::test]
    async fn test_disabled_persistence() {
        let config = ServerConfig {
            port: 50051,
            database_url: Some("postgres://test".to_string()),
            log_level: "info".to_string(),
            log_persistence_enabled: false,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/test-logs".to_string(),
            log_ttl_hours: 168,
        };

        let persistence_config = config.to_log_persistence_config();
        assert!(!persistence_config.enabled);

        // Create storage - should return DisabledStorage
        let storage = LogStorageFactory::create(&persistence_config);

        // Disabled storage should work but do nothing
        let result = storage.append_log("job-1", "test", false, None).await;
        assert!(result.is_ok());

        let result = storage.finalize_job_log("job-1").await;
        assert!(result.is_err()); // Should error as disabled
    }

    /// Test 4: Local file storage integration
    #[tokio::test]
    async fn test_local_storage_integration() {
        let temp_dir = create_test_dir();
        let storage_config = LogPersistenceConfig {
            enabled: true,
            storage_backend: StorageBackend::Local(LocalStorageConfig {
                base_path: temp_dir.clone(),
            }),
            ttl_hours: 24,
        };

        let storage = LogStorageFactory::create(&storage_config);

        // Append logs
        storage
            .append_log("job-123", "Line 1", false, None)
            .await
            .unwrap();
        storage
            .append_log("job-123", "Line 2", true, None)
            .await
            .unwrap();
        storage
            .append_log("job-123", "Line 3", false, None)
            .await
            .unwrap();

        // Finalize
        let log_ref = storage.finalize_job_log("job-123").await.unwrap();

        assert_eq!(log_ref.job_id, "job-123");
        assert!(log_ref.storage_uri.starts_with("file://"));
        assert!(log_ref.size_bytes > 0);
        assert_eq!(log_ref.entry_count, 3);

        // Verify file exists
        let path_str = log_ref.storage_uri.strip_prefix("file://").unwrap();
        assert!(PathBuf::from(path_str).exists());

        cleanup_test_dir(&temp_dir);
    }

    /// Test 5: Complete integration flow
    #[tokio::test]
    async fn test_complete_integration_flow() {
        let temp_dir = create_test_dir();

        // 1. Create config
        let config = ServerConfig {
            port: 50051,
            database_url: Some("postgres://test".to_string()),
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: temp_dir.to_str().unwrap().to_string(),
            log_ttl_hours: 24,
        };

        // 2. Convert to persistence config
        let persistence_config = config.to_log_persistence_config();
        assert!(persistence_config.enabled);

        // 3. Create storage
        let storage = LogStorageFactory::create(&persistence_config);

        // 4. Simulate log flow
        let job_id = "test-job-456";

        // Append multiple logs
        for i in 0..10 {
            storage
                .append_log(
                    job_id,
                    &format!("Log message {}", i),
                    i % 3 == 0, // stderr for every 3rd message
                    None,
                )
                .await
                .unwrap();
        }

        // 5. Finalize job log
        let log_ref = storage.finalize_job_log(job_id).await.unwrap();

        // Verify finalization
        assert_eq!(log_ref.job_id, job_id);
        assert!(log_ref.storage_uri.contains(job_id));
        assert_eq!(log_ref.entry_count, 10);

        // 6. Verify file was created
        let path_str = log_ref.storage_uri.strip_prefix("file://").unwrap();
        let file_path = PathBuf::from(path_str);
        assert!(file_path.exists());

        // 7. Read and verify content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Log message 0"));
        assert!(content.contains("Log message 9"));

        cleanup_test_dir(&temp_dir);
    }

    /// Test 6: Multiple jobs with isolated storage
    #[tokio::test]
    async fn test_multiple_jobs_isolated_storage() {
        let temp_dir = create_test_dir();

        let storage_config = LogPersistenceConfig {
            enabled: true,
            storage_backend: StorageBackend::Local(LocalStorageConfig {
                base_path: temp_dir.clone(),
            }),
            ttl_hours: 24,
        };

        let storage = LogStorageFactory::create(&storage_config);

        // Create logs for multiple jobs
        let job_ids = vec!["job-a", "job-b", "job-c"];

        for job_id in &job_ids {
            for i in 0..5 {
                storage
                    .append_log(job_id, &format!("{} - line {}", job_id, i), false, None)
                    .await
                    .unwrap();
            }
        }

        // Finalize each job and verify
        for job_id in &job_ids {
            let log_ref = storage.finalize_job_log(job_id).await.unwrap();
            assert_eq!(log_ref.job_id, *job_id);
            assert_eq!(log_ref.entry_count, 5);

            // Verify file exists
            let path_str = log_ref.storage_uri.strip_prefix("file://").unwrap();
            assert!(PathBuf::from(path_str).exists());
        }

        cleanup_test_dir(&temp_dir);
    }
}

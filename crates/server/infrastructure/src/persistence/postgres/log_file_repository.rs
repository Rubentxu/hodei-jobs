//! PostgreSQL Log Storage Repository
//!
//! Stores references to persistent log storage (local files, S3, GCS, etc.)
//! This repository is storage-agnostic and works with any storage backend.

use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::postgres::PgPool;
use sqlx::{Row, Transaction};
use uuid::Uuid;

/// Reference to a persistent log storage (agnostic to storage type)
/// Could be a local file path, S3 URI, GCS URL, Azure Blob URL, etc.
#[derive(Debug, Clone)]
pub struct LogStorageReference {
    pub id: Uuid,
    pub job_id: JobId,
    /// URI identifying the storage location (e.g., "file:///path/to/log", "s3://bucket/log", etc.)
    pub storage_uri: String,
    /// Size in bytes of the stored logs
    pub size_bytes: i64,
    /// Number of log entries (lines, records, etc.)
    pub entry_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl LogStorageReference {
    pub fn new(
        job_id: JobId,
        storage_uri: String,
        size_bytes: u64,
        entry_count: u64,
        ttl_hours: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::hours(ttl_hours as i64);

        Self {
            id: Uuid::new_v4(),
            job_id,
            storage_uri,
            size_bytes: size_bytes as i64,
            entry_count: entry_count as i32,
            created_at: now,
            expires_at,
        }
    }
}

/// Repository for log storage references (agnostic to storage type)
#[derive(Clone)]
pub struct LogStorageRepository {
    pool: PgPool,
}

impl LogStorageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Save a log storage reference
    pub async fn save(&self, log_ref: &LogStorageReference) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO job_log_files
                (id, job_id, storage_uri, size_bytes, entry_count, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (job_id) DO UPDATE SET
                storage_uri = EXCLUDED.storage_uri,
                size_bytes = EXCLUDED.size_bytes,
                entry_count = EXCLUDED.entry_count,
                created_at = EXCLUDED.created_at,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(log_ref.id)
        .bind(log_ref.job_id.0)
        .bind(&log_ref.storage_uri)
        .bind(log_ref.size_bytes)
        .bind(log_ref.entry_count)
        .bind(log_ref.created_at)
        .bind(log_ref.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save log storage reference: {}", e),
        })?;

        Ok(())
    }

    /// Get log storage reference by job ID
    pub async fn find_by_job_id(&self, job_id: &JobId) -> Result<Option<LogStorageReference>> {
        let row = sqlx::query(
            r#"
            SELECT id, job_id, storage_uri, size_bytes, entry_count, created_at, expires_at
            FROM job_log_files
            WHERE job_id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch log storage reference: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(LogStorageReference {
                id: row.get("id"),
                job_id: JobId(row.get("job_id")),
                storage_uri: row.get("storage_uri"),
                size_bytes: row.get("size_bytes"),
                entry_count: row.get("entry_count"),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete expired log storage references
    pub async fn delete_expired(&self) -> Result<Vec<LogStorageReference>> {
        let now = chrono::Utc::now();

        let rows = sqlx::query(
            r#"
            DELETE FROM job_log_files
            WHERE expires_at < $1
            RETURNING id, job_id, storage_uri, size_bytes, entry_count, created_at, expires_at
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to delete expired log storage references: {}", e),
        })?;

        let deleted: Vec<LogStorageReference> = rows
            .into_iter()
            .map(|row| LogStorageReference {
                id: row.get("id"),
                job_id: JobId(row.get("job_id")),
                storage_uri: row.get("storage_uri"),
                size_bytes: row.get("size_bytes"),
                entry_count: row.get("entry_count"),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            })
            .collect();

        Ok(deleted)
    }

    /// Clean up old log storage references from database
    /// Returns list of storage URIs that should be deleted from storage
    pub async fn cleanup_expired_with_uris(&self) -> Result<Vec<String>> {
        let now = chrono::Utc::now();

        let rows = sqlx::query(
            r#"
            DELETE FROM job_log_files
            WHERE expires_at < $1
            RETURNING storage_uri
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to cleanup expired log storage references: {}", e),
        })?;

        let uris: Vec<String> = rows.into_iter().map(|row| row.get("storage_uri")).collect();

        Ok(uris)
    }

    /// Get all log storage references expiring before a given time
    pub async fn find_expiring_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<LogStorageReference>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, storage_uri, size_bytes, entry_count, created_at, expires_at
            FROM job_log_files
            WHERE expires_at < $1
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch expiring log storage references: {}", e),
        })?;

        let refs: Vec<LogStorageReference> = rows
            .into_iter()
            .map(|row| LogStorageReference {
                id: row.get("id"),
                job_id: JobId(row.get("job_id")),
                storage_uri: row.get("storage_uri"),
                size_bytes: row.get("size_bytes"),
                entry_count: row.get("entry_count"),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            })
            .collect();

        Ok(refs)
    }

    /// Get total size of all log storage
    pub async fn get_total_size(&self) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(size_bytes), 0) as total
            FROM job_log_files
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get total log storage size: {}", e),
        })?;

        Ok(row.get("total"))
    }

    /// Get count of log storage references
    pub async fn count(&self) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM job_log_files
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count log storage references: {}", e),
        })?;

        Ok(row.get("count"))
    }
}

// Legacy type alias for backward compatibility
pub type LogFileRepository = LogStorageRepository;
pub type LogFileReference = LogStorageReference;

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_save_and_find_log_storage_reference() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        let repo = LogStorageRepository::new(pool.clone());
        repo.pool
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS job_log_files (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    storage_uri TEXT NOT NULL,
                    size_bytes BIGINT NOT NULL,
                    entry_count INTEGER NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    expires_at TIMESTAMPTZ NOT NULL,
                    UNIQUE(job_id)
                );
                "#,
            )
            .await
            .expect("Failed to create test table");

        // Create and save a log storage reference
        let job_id = JobId(uuid::Uuid::new_v4());
        let log_ref = LogStorageReference::new(
            job_id.clone(),
            "s3://my-bucket/logs/job-123".to_string(), // S3 URI example
            1024,
            100,
            168, // 7 days
        );

        repo.save(&log_ref).await.expect("Failed to save log ref");

        // Retrieve and verify
        let found = repo
            .find_by_job_id(&job_id)
            .await
            .expect("Failed to find log ref")
            .expect("Log ref not found");

        assert_eq!(found.job_id, job_id);
        assert_eq!(found.storage_uri, "s3://my-bucket/logs/job-123");
        assert_eq!(found.size_bytes, 1024);
        assert_eq!(found.entry_count, 100);

        // Cleanup
        sqlx::query("DELETE FROM job_log_files WHERE job_id = $1")
            .bind(job_id.0)
            .execute(&pool)
            .await
            .expect("Failed to cleanup");

        pool.close().await;
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_cleanup_expired() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        let repo = LogStorageRepository::new(pool.clone());
        repo.pool
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS job_log_files (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    storage_uri TEXT NOT NULL,
                    size_bytes BIGINT NOT NULL,
                    entry_count INTEGER NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    expires_at TIMESTAMPTZ NOT NULL,
                    UNIQUE(job_id)
                );
                "#,
            )
            .await
            .expect("Failed to create test table");

        // Create expired log refs
        let job_id_1 = JobId(uuid::Uuid::new_v4());
        let job_id_2 = JobId(uuid::Uuid::new_v4());

        let mut expired_ref_1 = LogStorageReference::new(
            job_id_1.clone(),
            "file:///expired1.log".to_string(),
            100,
            10,
            1,
        );
        expired_ref_1.expires_at = chrono::Utc::now() - chrono::Duration::hours(2); // Make it expired

        let mut expired_ref_2 = LogStorageReference::new(
            job_id_2.clone(),
            "gcs://bucket/expired2.log".to_string(), // GCS URI example
            200,
            20,
            1,
        );
        expired_ref_2.expires_at = chrono::Utc::now() - chrono::Duration::hours(2); // Make it expired

        repo.save(&expired_ref_1)
            .await
            .expect("Failed to save ref 1");
        repo.save(&expired_ref_2)
            .await
            .expect("Failed to save ref 2");

        // Cleanup expired
        let deleted_uris = repo
            .cleanup_expired_with_uris()
            .await
            .expect("Failed to cleanup expired");

        assert_eq!(deleted_uris.len(), 2);
        assert!(deleted_uris.contains(&"file:///expired1.log".to_string()));
        assert!(deleted_uris.contains(&"gcs://bucket/expired2.log".to_string()));

        // Verify they're gone
        assert!(repo.find_by_job_id(&job_id_1).await.unwrap().is_none());
        assert!(repo.find_by_job_id(&job_id_2).await.unwrap().is_none());

        pool.close().await;
    }
}

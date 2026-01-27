//! Queue Job with Transactional Outbox
//!
//! Implementation of atomic job queueing using the Transactional Outbox Pattern.
//! This ensures that either both the Job and the JobQueued event are persisted,
//! or neither (atomicity guarantee).
//!
//! EPIC-43: Pure EDA & Saga Orchestration - Sprint 1
//! US-EDA-102: Refactorizar queue_job() con atomicidad

use hodei_server_domain::jobs::{Job, JobRepositoryTx, JobSpec};
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::{PgPool, PgTransaction};
use std::sync::Arc;
use uuid::Uuid;

/// DTOs for Queue Job with Transactional Outbox
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueJobRequest {
    pub spec: JobSpecRequest,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    /// Optional job_id provided by client. If None, a new UUID will be generated.
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpecRequest {
    pub command: Vec<String>,
    pub image: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub working_dir: Option<String>,
    pub cpu_cores: Option<f64>,
    pub memory_bytes: Option<i64>,
    pub disk_bytes: Option<i64>,
    /// Scheduling preferences for provider selection
    pub preferred_provider: Option<String>,
    pub preferred_region: Option<String>,
    pub required_labels: Option<std::collections::HashMap<String, String>>,
    pub required_annotations: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueJobResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

/// Transaction-aware operations for outbox
///
/// This trait abstracts the transaction-aware operations needed by QueueJobUseCase.
/// It allows the use case to work with any implementation that supports transactions.
#[async_trait::async_trait]
pub trait TransactionalOutbox {
    /// Insert events into the outbox within a transaction
    async fn insert_events_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        events: &[OutboxEventInsert],
    ) -> Result<()>;
}

/// Use Case: Queue Job with Transactional Outbox (EPIC-43 US-EDA-102)
///
/// This use case implements atomic job queueing using the Transactional Outbox Pattern.
/// Unlike the previous implementation, this guarantees that:
/// 1. The Job is persisted in the database
/// 2. The JobQueued event is persisted in the outbox table
/// 3. Both operations happen in a single transaction (atomic)
/// 4. No direct event bus publishing (OutboxRelay handles that)
///
/// This eliminates the dual-write problem where:
/// - Previously: save() could succeed but publish() could fail
/// - Now: Either both succeed, or both fail together
pub struct QueueJobUseCase {
    /// Transaction-aware job repository
    job_repo: Arc<dyn JobRepositoryTx>,
    /// Transaction-aware outbox operations
    outbox_tx: Arc<dyn TransactionalOutbox>,
    /// Database connection pool for starting transactions
    pool: PgPool,
}

impl QueueJobUseCase {
    /// Create a new QueueJobUseCase
    ///
    /// # Arguments
    /// * `job_repo` - Transaction-aware job repository
    /// * `outbox_tx` - Transaction-aware outbox operations
    /// * `pool` - PostgreSQL connection pool
    pub fn new(
        job_repo: Arc<dyn JobRepositoryTx>,
        outbox_tx: Arc<dyn TransactionalOutbox>,
        pool: PgPool,
    ) -> Self {
        Self {
            job_repo,
            outbox_tx,
            pool,
        }
    }

    /// Execute the queue job use case with atomicity guarantee
    ///
    /// This method:
    /// 1. Starts a database transaction
    /// 2. Persists the Job
    /// 3. Persists the JobQueued event in the outbox
    /// 4. Commits the transaction
    ///
    /// If any step fails, the entire transaction is rolled back.
    ///
    /// # Arguments
    /// * `request` - The queue job request containing job specification
    ///
    /// # Returns
    /// * `Result<QueueJobResponse>` - The queued job response
    ///
    /// # Errors
    /// * Validation errors (invalid job spec)
    /// * Database errors (transaction failures)
    /// * Duplicate job_id (if provided)
    pub async fn execute(&self, request: QueueJobRequest) -> Result<QueueJobResponse> {
        // 1. Convertir request a JobSpec
        let job_spec = self.convert_to_job_spec(request.spec.clone());

        // 2. Validar JobSpec en el dominio
        job_spec.validate().map_err(|e| {
            tracing::error!("JobSpec validation failed: {}", e);
            e
        })?;

        // 3. Use provided JobId or generate new one
        let job_id = if let Some(id_str) = &request.job_id {
            let uuid = Uuid::parse_str(id_str).map_err(|_| DomainError::InvalidProviderConfig {
                message: format!("Invalid UUID format for job_id: {}", id_str),
            })?;
            JobId(uuid)
        } else {
            JobId::new()
        };

        // 4. Crear Job aggregate
        let mut job = Job::new(job_id.clone(), "queued-job".to_string(), job_spec.clone());

        // Store correlation details in metadata
        if let Some(correlation_id) = &request.correlation_id {
            job.metadata_mut()
                .insert("correlation_id".to_string(), correlation_id.clone());
        }
        if let Some(actor) = &request.actor {
            job.metadata_mut()
                .insert("actor".to_string(), actor.clone());
        }

        // 5. Generate idempotency key for the event
        let idempotency_key = format!("job-queued-{}", job_id);

        // 6. Create the JobQueued event for the outbox
        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobQueued".to_string(),
            serde_json::json!({
                "job_id": job_id.to_string(),
                "spec": {
                    "command": job_spec.command_vec(),
                    "timeout_ms": job_spec.timeout_ms,
                    "preferred_provider": job_spec.preferences.preferred_provider,
                    "preferred_region": job_spec.preferences.preferred_region,
                    "required_labels": job_spec.preferences.required_labels,
                    "required_annotations": job_spec.preferences.required_annotations,
                },
                "queued_at": chrono::Utc::now().to_rfc3339(),
            }),
            Some(serde_json::json!({
                "source": "QueueJobUseCase",
                "correlation_id": request.correlation_id.clone().unwrap_or_else(|| job_id.to_string()),
                "actor": request.actor.clone().unwrap_or_else(|| "system".to_string()),
            })),
            Some(idempotency_key),
        );

        // 7. ATOMIC OPERATION: Begin transaction
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to begin transaction: {}", e),
            })?;

        // 8. Persist Job within transaction
        tracing::info!("Saving job {} within transaction", job_id);
        self.job_repo
            .save_with_tx(&mut tx, &job)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to save job: {}", e),
            })?;

        // 9. Persist JobQueued event within same transaction
        tracing::info!("Inserting JobQueued event into outbox within transaction");
        self.outbox_tx
            .insert_events_with_tx(&mut tx, &[event])
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to insert outbox event: {}", e),
            })?;

        // 10. Commit transaction (both job and event are persisted atomically)
        tracing::info!("Committing transaction for job {}", job_id);
        tx.commit()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to commit transaction: {}", e),
            })?;

        tracing::info!(
            "✅ Job {} queued successfully with atomic outbox event",
            job_id
        );

        Ok(QueueJobResponse {
            job_id: job_id.to_string(),
            status: "PENDING".to_string(),
            message: "Job queued successfully with transactional outbox".to_string(),
        })
    }

    /// Builder Pattern: Convierte JobSpecRequest a JobSpec
    fn convert_to_job_spec(&self, request: JobSpecRequest) -> JobSpec {
        let mut spec = JobSpec::new(request.command);

        if let Some(image) = request.image {
            spec.image = Some(image);
        }

        if let Some(env) = request.env {
            spec.env = env;
        }

        if let Some(timeout) = request.timeout_ms {
            spec.timeout_ms = timeout;
        }

        if let Some(working_dir) = request.working_dir {
            spec.working_dir = Some(working_dir);
        }

        if let Some(cpu_cores) = request.cpu_cores {
            if cpu_cores > 0.0 {
                spec.resources.cpu_cores = cpu_cores as f32;
            }
        }

        if let Some(memory_bytes) = request.memory_bytes {
            if memory_bytes > 0 {
                spec.resources.memory_mb = (memory_bytes / (1024 * 1024)) as u64;
            }
        }

        if let Some(disk_bytes) = request.disk_bytes {
            if disk_bytes > 0 {
                spec.resources.storage_mb = (disk_bytes / (1024 * 1024)) as u64;
            }
        }

        if let Some(provider) = request.preferred_provider {
            if !provider.is_empty() {
                spec.preferences.preferred_provider = Some(provider);
            }
        }

        if let Some(region) = request.preferred_region {
            if !region.is_empty() {
                spec.preferences.preferred_region = Some(region);
            }
        }

        if let Some(labels) = request.required_labels {
            if !labels.is_empty() {
                spec.preferences.required_labels = labels;
            }
        }

        if let Some(annotations) = request.required_annotations {
            if !annotations.is_empty() {
                spec.preferences.required_annotations = annotations;
            }
        }

        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_server_domain::jobs::{Job, JobRepository, JobRepositoryTx, JobsFilter};
    use hodei_server_domain::shared_kernel::{JobState, WorkerId};
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    /// Mock implementation of JobRepositoryTx for testing
    struct MockJobRepositoryTx;

    #[async_trait::async_trait]
    impl JobRepository for MockJobRepositoryTx {
        async fn save(&self, _job: &Job) -> Result<()> {
            Ok(())
        }

        async fn find_by_id(&self, _job_id: &JobId) -> Result<Option<Job>> {
            Ok(None)
        }

        async fn find(&self, _filter: JobsFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn count_by_state(&self, _state: &JobState) -> Result<u64> {
            Ok(0)
        }

        async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn find_pending(&self) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn find_all(&self, _limit: usize, _offset: usize) -> Result<(Vec<Job>, usize)> {
            Ok((vec![], 0))
        }

        async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>> {
            Ok(None)
        }

        async fn delete(&self, _job_id: &JobId) -> Result<()> {
            Ok(())
        }

        async fn update(&self, _job: &Job) -> Result<()> {
            Ok(())
        }

        async fn update_state(&self, _job_id: &JobId, _new_state: JobState) -> Result<()> {
            Ok(())
        }

        async fn assign_worker(&self, _job_id: &JobId, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        fn supports_job_assigned(&self) -> bool {
            true
        }
    }


    #[async_trait::async_trait]
    impl JobRepositoryTx for MockJobRepositoryTx {
        async fn save_with_tx(&self, _tx: &mut sqlx::PgTransaction<'_>, _job: &Job) -> Result<()> {
            Ok(())
        }

        async fn find_by_id_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _job_id: &JobId,
        ) -> Result<Option<Job>> {
            Ok(None)
        }

        async fn update_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _job: &Job,
        ) -> Result<()> {
            Ok(())
        }

        async fn update_status_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _job_id: &JobId,
            _new_status: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn find_by_state_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _state: &JobState,
        ) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn update_state_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _job_id: &JobId,
            _state: JobState,
        ) -> Result<()> {
            Ok(())
        }
    }

    /// Mock implementation of TransactionalOutbox for testing
    struct MockTransactionalOutbox;

    #[async_trait::async_trait]
    impl TransactionalOutbox for MockTransactionalOutbox {
        async fn insert_events_with_tx(
            &self,
            _tx: &mut sqlx::PgTransaction<'_>,
            _events: &[OutboxEventInsert],
        ) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL. Run with DATABASE_URL set and migrations applied"]
    async fn test_queue_job_with_tx() {
        // Create a test pool (will fail without real DB, but tests the logic)
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/hodei_test")
            .ok();

        if pool.is_none() {
            // Skip if no DB available
            return;
        }

        let pool = pool.unwrap();
        let job_repo: Arc<dyn JobRepositoryTx> = Arc::new(MockJobRepositoryTx);
        let outbox_tx: Arc<dyn TransactionalOutbox> = Arc::new(MockTransactionalOutbox);

        let use_case = QueueJobUseCase::new(job_repo, outbox_tx, pool);

        let request = QueueJobRequest {
            spec: JobSpecRequest {
                command: vec!["echo".to_string(), "test".to_string()],
                image: None,
                env: None,
                timeout_ms: Some(300_000),
                working_dir: None,
                cpu_cores: Some(1.0),
                memory_bytes: Some(512 * 1024 * 1024),
                disk_bytes: Some(1024 * 1024 * 1024),
                preferred_provider: None,
                preferred_region: None,
                required_labels: None,
                required_annotations: None,
            },
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("test-actor".to_string()),
            job_id: Some(Uuid::new_v4().to_string()),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "PENDING");
        assert!(response.job_id.parse::<Uuid>().is_ok());
    }

    #[tokio::test]
    async fn test_queue_job_validates_spec() {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/hodei_test")
            .ok();

        if pool.is_none() {
            return;
        }

        let pool = pool.unwrap();
        let job_repo: Arc<dyn JobRepositoryTx> = Arc::new(MockJobRepositoryTx);
        let outbox_tx: Arc<dyn TransactionalOutbox> = Arc::new(MockTransactionalOutbox);

        let use_case = QueueJobUseCase::new(job_repo, outbox_tx, pool);

        // Request with invalid timeout (0)
        let request = QueueJobRequest {
            spec: JobSpecRequest {
                command: vec!["echo".to_string()],
                image: None,
                env: None,
                timeout_ms: Some(0), // Invalid
                working_dir: None,
                cpu_cores: Some(1.0),
                memory_bytes: Some(512 * 1024 * 1024),
                disk_bytes: Some(1024 * 1024 * 1024),
                preferred_provider: None,
                preferred_region: None,
                required_labels: None,
                required_annotations: None,
            },
            correlation_id: None,
            actor: None,
            job_id: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }
}

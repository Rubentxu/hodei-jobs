// Persistence Layer
// Adaptadores de persistencia para bases de datos, archivos, etc.

use hodei_jobs_domain::job_execution::{Job, JobQueue, JobRepository, JobSpec};
use hodei_jobs_domain::job_template::{
    JobTemplate, JobTemplateId, JobTemplateRepository, JobTemplateStatus,
};
use hodei_jobs_domain::provider_config::{ProviderConfig, ProviderConfigRepository};
use hodei_jobs_domain::shared_kernel::{DomainError, JobId, ProviderId, ProviderStatus, Result};
use hodei_jobs_domain::worker::ProviderType;
use serde::{Deserialize, Serialize};
use sqlx::{
    Row,
    postgres::{PgPool, PgPoolOptions},
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Configuración de persistencia
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    pub data_directory: String,
    pub backup_enabled: bool,
    pub auto_compact: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            data_directory: "./data".to_string(),
            backup_enabled: true,
            auto_compact: false,
        }
    }
}

/// Adaptador de persistencia basado en archivos
#[derive(Clone)]
pub struct FileBasedPersistence {
    config: PersistenceConfig,
}

impl FileBasedPersistence {
    pub fn new(config: PersistenceConfig) -> Self {
        // Crear directorio si no existe
        if !Path::new(&config.data_directory).exists() {
            fs::create_dir_all(&config.data_directory).unwrap_or_default();
        }
        Self { config }
    }

    fn jobs_file_path(&self) -> String {
        format!("{}/jobs.json", self.config.data_directory)
    }

    fn providers_file_path(&self) -> String {
        format!("{}/providers.json", self.config.data_directory)
    }
}

/// Repositorio persistente para Jobs
#[derive(Clone)]
pub struct PersistentJobRepository {
    persistence: FileBasedPersistence,
}

impl PersistentJobRepository {
    pub fn new(persistence: FileBasedPersistence) -> Self {
        Self { persistence }
    }
}

#[async_trait::async_trait]
impl JobRepository for PersistentJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        // Cargar jobs existentes
        let mut jobs = self.load_jobs().await?;

        // Agregar/actualizar job
        jobs.insert(job.id.clone(), job.clone());

        // Guardar
        self.save_jobs(&jobs).await?;
        Ok(())
    }

    async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
        let jobs = self.load_jobs().await?;
        Ok(jobs.get(job_id).cloned())
    }

    async fn find_by_state(
        &self,
        state: &hodei_jobs_domain::shared_kernel::JobState,
    ) -> Result<Vec<Job>> {
        let jobs = self.load_jobs().await?;
        Ok(jobs
            .values()
            .filter(|job| &job.state == state)
            .cloned()
            .collect())
    }

    async fn find_pending(&self) -> Result<Vec<Job>> {
        let jobs = self.load_jobs().await?;
        Ok(jobs
            .values()
            .filter(|job| {
                matches!(
                    job.state,
                    hodei_jobs_domain::shared_kernel::JobState::Pending
                )
            })
            .cloned()
            .collect())
    }

    async fn find_all(&self, limit: usize, offset: usize) -> Result<(Vec<Job>, usize)> {
        let jobs = self.load_jobs().await?;
        let total = jobs.len();
        let mut sorted_jobs: Vec<Job> = jobs.values().cloned().collect();
        // Sort by created_at desc
        sorted_jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok((
            sorted_jobs.into_iter().skip(offset).take(limit).collect(),
            total,
        ))
    }

    async fn find_by_execution_id(&self, execution_id: &str) -> Result<Option<Job>> {
        let jobs = self.load_jobs().await?;
        Ok(jobs
            .values()
            .find(|job| {
                job.execution_context
                    .as_ref()
                    .map(|ctx| ctx.provider_execution_id == execution_id)
                    .unwrap_or(false)
            })
            .cloned())
    }

    async fn delete(&self, job_id: &JobId) -> Result<()> {
        let mut jobs = self.load_jobs().await?;
        jobs.remove(job_id);
        self.save_jobs(&jobs).await?;
        Ok(())
    }

    async fn update(&self, job: &Job) -> Result<()> {
        self.save(job).await
    }
}

impl PersistentJobRepository {
    async fn load_jobs(&self) -> Result<std::collections::HashMap<JobId, Job>> {
        let file_path = self.persistence.jobs_file_path();

        if !Path::new(&file_path).exists() {
            return Ok(std::collections::HashMap::new());
        }

        let content = fs::read_to_string(&file_path).map_err(|e| {
            hodei_jobs_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to read jobs file: {}", e),
            }
        })?;

        let jobs: std::collections::HashMap<JobId, Job> =
            serde_json::from_str(&content).map_err(|e| {
                hodei_jobs_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to parse jobs file: {}", e),
                }
            })?;

        Ok(jobs)
    }

    async fn save_jobs(&self, jobs: &std::collections::HashMap<JobId, Job>) -> Result<()> {
        let file_path = self.persistence.jobs_file_path();

        let content = serde_json::to_string_pretty(jobs).map_err(|e| {
            hodei_jobs_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to serialize jobs: {}", e),
            }
        })?;

        fs::write(&file_path, content).map_err(|e| {
            hodei_jobs_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to write jobs file: {}", e),
            }
        })?;

        Ok(())
    }
}

// ============================================================================
// Provider Config Repository - File-based persistence
// ============================================================================

/// Repositorio persistente para ProviderConfig basado en archivos JSON
#[derive(Clone)]
pub struct FileBasedProviderConfigRepository {
    persistence: FileBasedPersistence,
    cache: Arc<RwLock<HashMap<ProviderId, ProviderConfig>>>,
}

impl FileBasedProviderConfigRepository {
    pub fn new(persistence: FileBasedPersistence) -> Self {
        Self {
            persistence,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Cargar configuraciones desde archivo
    async fn load_configs(&self) -> Result<HashMap<ProviderId, ProviderConfig>> {
        let file_path = self.persistence.providers_file_path();

        if !Path::new(&file_path).exists() {
            return Ok(HashMap::new());
        }

        let content =
            fs::read_to_string(&file_path).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to read providers file: {}", e),
            })?;

        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let configs: HashMap<ProviderId, ProviderConfig> =
            serde_json::from_str(&content).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to parse providers file: {}", e),
            })?;

        Ok(configs)
    }

    /// Guardar configuraciones a archivo
    async fn save_configs(&self, configs: &HashMap<ProviderId, ProviderConfig>) -> Result<()> {
        let file_path = self.persistence.providers_file_path();

        let content = serde_json::to_string_pretty(configs).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize providers: {}", e),
            }
        })?;

        fs::write(&file_path, content).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to write providers file: {}", e),
        })?;

        Ok(())
    }

    /// Inicializar cache desde archivo
    pub async fn initialize(&self) -> Result<()> {
        let configs = self.load_configs().await?;
        let mut cache = self.cache.write().await;
        *cache = configs;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProviderConfigRepository for FileBasedProviderConfigRepository {
    async fn save(&self, config: &ProviderConfig) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.insert(config.id.clone(), config.clone());
        self.save_configs(&cache).await
    }

    async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache.get(id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache.values().find(|c| c.name == name).cloned())
    }

    async fn find_by_type(&self, provider_type: &ProviderType) -> Result<Vec<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache
            .values()
            .filter(|c| &c.provider_type == provider_type)
            .cloned()
            .collect())
    }

    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache
            .values()
            .filter(|c| c.status == ProviderStatus::Active)
            .cloned()
            .collect())
    }

    async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache
            .values()
            .filter(|c| c.status == ProviderStatus::Active && c.has_capacity())
            .cloned()
            .collect())
    }

    async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
        let cache = self.cache.read().await;
        Ok(cache.values().cloned().collect())
    }

    async fn update(&self, config: &ProviderConfig) -> Result<()> {
        let mut cache = self.cache.write().await;
        if cache.contains_key(&config.id) {
            cache.insert(config.id.clone(), config.clone());
            drop(cache);
            let cache = self.cache.read().await;
            self.save_configs(&cache).await
        } else {
            Err(DomainError::ProviderNotFound {
                provider_id: config.id.clone(),
            })
        }
    }

    async fn delete(&self, id: &ProviderId) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.remove(id);
        self.save_configs(&cache).await
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let cache = self.cache.read().await;
        Ok(cache.values().any(|c| c.name == name))
    }
}

// ============================================================================
// PostgreSQL Job Repository
// ============================================================================

/// Repositorio persistente para Jobs basado en PostgreSQL
#[derive(Clone)]
pub struct PostgresJobRepository {
    pool: PgPool,
}

impl PostgresJobRepository {
    /// Crear nuevo repositorio con pool existente
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Crear repositorio conectando a la base de datos
    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    /// Ejecutar migraciones para crear tablas de jobs
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id UUID PRIMARY KEY,
                spec JSONB NOT NULL,
                state VARCHAR(50) NOT NULL,
                selected_provider_id UUID,
                execution_context JSONB,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                created_at TIMESTAMPTZ NOT NULL,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                result JSONB,
                error_message TEXT,
                metadata JSONB NOT NULL DEFAULT '{}'
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create jobs table: {}", e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create jobs state index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create jobs created_at index: {}", e),
            })?;

        Ok(())
    }

    fn state_to_string(state: &hodei_jobs_domain::shared_kernel::JobState) -> String {
        match state {
            hodei_jobs_domain::shared_kernel::JobState::Pending => "PENDING".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Scheduled => "SCHEDULED".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Running => "RUNNING".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Succeeded => "SUCCEEDED".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Failed => "FAILED".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Cancelled => "CANCELLED".to_string(),
            hodei_jobs_domain::shared_kernel::JobState::Timeout => "TIMEOUT".to_string(),
        }
    }

    fn string_to_state(s: &str) -> hodei_jobs_domain::shared_kernel::JobState {
        match s {
            "PENDING" => hodei_jobs_domain::shared_kernel::JobState::Pending,
            "SCHEDULED" => hodei_jobs_domain::shared_kernel::JobState::Scheduled,
            "RUNNING" => hodei_jobs_domain::shared_kernel::JobState::Running,
            "SUCCEEDED" => hodei_jobs_domain::shared_kernel::JobState::Succeeded,
            "FAILED" => hodei_jobs_domain::shared_kernel::JobState::Failed,
            "CANCELLED" => hodei_jobs_domain::shared_kernel::JobState::Cancelled,
            "TIMEOUT" => hodei_jobs_domain::shared_kernel::JobState::Timeout,
            _ => hodei_jobs_domain::shared_kernel::JobState::Failed, // Default safe fallback
        }
    }
}

#[async_trait::async_trait]
impl JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        let spec_json =
            serde_json::to_value(&job.spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            })?;

        let context_json = if let Some(ctx) = &job.execution_context {
            Some(
                serde_json::to_value(ctx).map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to serialize execution context: {}", e),
                })?,
            )
        } else {
            None
        };

        let result_json = if let Some(res) = &job.result {
            Some(
                serde_json::to_value(res).map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to serialize result: {}", e),
                })?,
            )
        } else {
            None
        };

        let metadata_json =
            serde_json::to_value(&job.metadata).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize metadata: {}", e),
            })?;

        let provider_id = job.selected_provider.as_ref().map(|p| *p.as_uuid());

        sqlx::query(
            r#"
            INSERT INTO jobs 
                (id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                 created_at, started_at, completed_at, result, error_message, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                spec = EXCLUDED.spec,
                state = EXCLUDED.state,
                selected_provider_id = EXCLUDED.selected_provider_id,
                execution_context = EXCLUDED.execution_context,
                attempts = EXCLUDED.attempts,
                max_attempts = EXCLUDED.max_attempts,
                created_at = EXCLUDED.created_at,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                result = EXCLUDED.result,
                error_message = EXCLUDED.error_message,
                metadata = EXCLUDED.metadata
            "#,
        )
        .bind(job.id.0)
        .bind(spec_json)
        .bind(Self::state_to_string(&job.state))
        .bind(provider_id)
        .bind(context_json)
        .bind(job.attempts as i32)
        .bind(job.max_attempts as i32)
        .bind(job.created_at)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(result_json)
        .bind(&job.error_message)
        .bind(metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job: {}", e),
        })?;

        Ok(())
    }

    async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job by id: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(map_row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_state(
        &self,
        state: &hodei_jobs_domain::shared_kernel::JobState,
    ) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE state = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(Self::state_to_string(state))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find jobs by state: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_row_to_job(row)?);
        }
        Ok(jobs)
    }

    async fn find_pending(&self) -> Result<Vec<Job>> {
        self.find_by_state(&hodei_jobs_domain::shared_kernel::JobState::Pending)
            .await
    }

    async fn find_all(&self, limit: usize, offset: usize) -> Result<(Vec<Job>, usize)> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to count jobs: {}", e),
            })?;

        let rows = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find all jobs: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_row_to_job(row)?);
        }
        Ok((jobs, count as usize))
    }

    async fn find_by_execution_id(&self, execution_id: &str) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE execution_context ->> 'provider_execution_id' = $1
            "#,
        )
        .bind(execution_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job by execution id: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(map_row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, job_id: &JobId) -> Result<()> {
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(job_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete job: {}", e),
            })?;
        Ok(())
    }

    async fn update(&self, job: &Job) -> Result<()> {
        self.save(job).await
    }
}

fn map_row_to_job(row: sqlx::postgres::PgRow) -> Result<Job> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| DomainError::InfrastructureError {
            message: e.to_string(),
        })?;
    let spec_value: serde_json::Value =
        row.try_get("spec")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let state_str: String = row
        .try_get("state")
        .map_err(|e| DomainError::InfrastructureError {
            message: e.to_string(),
        })?;
    let provider_id_uuid: Option<uuid::Uuid> =
        row.try_get("selected_provider_id")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let context_value: Option<serde_json::Value> =
        row.try_get("execution_context")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let attempts: i32 = row
        .try_get("attempts")
        .map_err(|e| DomainError::InfrastructureError {
            message: e.to_string(),
        })?;
    let max_attempts: i32 =
        row.try_get("max_attempts")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let started_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("started_at")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let completed_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("completed_at")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let result_value: Option<serde_json::Value> =
        row.try_get("result")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let error_message: Option<String> =
        row.try_get("error_message")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
    let metadata_value: serde_json::Value =
        row.try_get("metadata")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

    let spec =
        serde_json::from_value(spec_value).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to deserialize job spec: {}", e),
        })?;

    let execution_context = if let Some(v) = context_value {
        Some(
            serde_json::from_value(v).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize execution context: {}", e),
            })?,
        )
    } else {
        None
    };

    let result = if let Some(v) = result_value {
        Some(
            serde_json::from_value(v).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize result: {}", e),
            })?,
        )
    } else {
        None
    };

    let metadata =
        serde_json::from_value(metadata_value).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to deserialize metadata: {}", e),
        })?;

    Ok(Job {
        id: JobId(id),
        spec,
        state: PostgresJobRepository::string_to_state(&state_str),
        selected_provider: provider_id_uuid.map(ProviderId),
        execution_context,
        attempts: attempts as u32,
        max_attempts: max_attempts as u32,
        created_at,
        started_at,
        completed_at,
        result,
        error_message,
        metadata,
    })
}

#[derive(Clone)]
pub struct PostgresJobQueue {
    pool: PgPool,
}

impl PostgresJobQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_queue (
                id BIGSERIAL PRIMARY KEY,
                job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create job_queue table: {}", e),
        })?;

        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS uq_job_queue_job_id ON job_queue(job_id);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create job_queue unique index: {}", e),
            })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_job_queue_enqueued_at ON job_queue(enqueued_at);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create job_queue enqueued_at index: {}", e),
        })?;

        Ok(())
    }

    async fn fetch_job_by_id<'e, E>(&self, executor: E, job_id: uuid::Uuid) -> Result<Job>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(executor)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch job for queue: {}", e),
        })?;

        map_row_to_job(row)
    }
}

#[async_trait::async_trait]
impl JobQueue for PostgresJobQueue {
    async fn enqueue(&self, job: Job) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO job_queue (job_id)
            VALUES ($1)
            ON CONFLICT (job_id) DO NOTHING
            "#,
        )
        .bind(job.id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to enqueue job: {}", e),
        })?;

        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to begin transaction: {}", e),
            })?;

        loop {
            let item = sqlx::query(
                r#"
                SELECT q.id, q.job_id
                FROM job_queue q
                ORDER BY q.enqueued_at ASC, q.id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
                "#,
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to dequeue job: {}", e),
            })?;

            let Some(item) = item else {
                tx.commit()
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to commit transaction: {}", e),
                    })?;
                return Ok(None);
            };

            let queue_id: i64 =
                item.try_get("id")
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to read queue id: {}", e),
                    })?;
            let job_id: uuid::Uuid =
                item.try_get("job_id")
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to read job_id from queue: {}", e),
                    })?;

            let state_row = sqlx::query("SELECT state FROM jobs WHERE id = $1")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to read job state for queue: {}", e),
                })?;

            let Some(state_row) = state_row else {
                sqlx::query("DELETE FROM job_queue WHERE id = $1")
                    .bind(queue_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to cleanup missing job from queue: {}", e),
                    })?;
                continue;
            };

            let state_str: String =
                state_row
                    .try_get("state")
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to read job state: {}", e),
                    })?;

            if state_str != "PENDING" {
                sqlx::query("DELETE FROM job_queue WHERE id = $1")
                    .bind(queue_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to cleanup non-pending job from queue: {}", e),
                    })?;
                continue;
            }

            sqlx::query("DELETE FROM job_queue WHERE id = $1")
                .bind(queue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to remove queue item: {}", e),
                })?;

            let job = self.fetch_job_by_id(&mut *tx, job_id).await?;

            tx.commit()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to commit transaction: {}", e),
                })?;

            return Ok(Some(job));
        }
    }

    async fn peek(&self) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT q.job_id
            FROM job_queue q
            JOIN jobs j ON j.id = q.job_id
            WHERE j.state = 'PENDING'
            ORDER BY q.enqueued_at ASC, q.id ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to peek job queue: {}", e),
        })?;

        let Some(row) = row else {
            return Ok(None);
        };

        let job_id: uuid::Uuid =
            row.try_get("job_id")
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to read job_id from queue: {}", e),
                })?;

        let job = self.fetch_job_by_id(&self.pool, job_id).await?;
        Ok(Some(job))
    }

    async fn len(&self) -> Result<usize> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM job_queue q
            JOIN jobs j ON j.id = q.job_id
            WHERE j.state = 'PENDING'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get job queue length: {}", e),
        })?;

        let count: i64 = row
            .try_get("count")
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to read job queue count: {}", e),
            })?;

        Ok(count.max(0) as usize)
    }

    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM job_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to clear job queue: {}", e),
            })?;

        Ok(())
    }
}

// ============================================================================
// PostgreSQL Worker Registry
// ============================================================================

use hodei_jobs_domain::shared_kernel::{WorkerId, WorkerState};
use hodei_jobs_domain::worker::{Worker, WorkerHandle, WorkerSpec};
use hodei_jobs_domain::worker_registry::{WorkerFilter, WorkerRegistry, WorkerRegistryStats};

// =========================================================================
// PostgreSQL Worker Bootstrap Token Store (OTP)
// =========================================================================

use hodei_jobs_domain::otp_token_store::{OtpToken, WorkerBootstrapTokenStore};
use std::time::Duration;

#[derive(Clone)]
pub struct PostgresWorkerBootstrapTokenStore {
    pool: PgPool,
}

impl PostgresWorkerBootstrapTokenStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS worker_bootstrap_tokens (
                token UUID PRIMARY KEY,
                worker_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create worker_bootstrap_tokens table: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_worker_bootstrap_tokens_worker_id ON worker_bootstrap_tokens(worker_id);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create worker_bootstrap_tokens worker_id index: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_worker_bootstrap_tokens_expires_at ON worker_bootstrap_tokens(expires_at);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create worker_bootstrap_tokens expires_at index: {}", e),
        })?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkerBootstrapTokenStore for PostgresWorkerBootstrapTokenStore {
    async fn issue(&self, worker_id: &WorkerId, ttl: Duration) -> Result<OtpToken> {
        let token = OtpToken::new();
        let expires_at = chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(token.0)
        .bind(worker_id.0)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to issue OTP token: {}", e),
        })?;

        Ok(token)
    }

    async fn consume(&self, token: &OtpToken, worker_id: &WorkerId) -> Result<()> {
        let now = chrono::Utc::now();

        let res = sqlx::query(
            r#"
            UPDATE worker_bootstrap_tokens
            SET consumed_at = $1
            WHERE token = $2
              AND worker_id = $3
              AND consumed_at IS NULL
              AND expires_at > $1
            "#,
        )
        .bind(now)
        .bind(token.0)
        .bind(worker_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to consume OTP token: {}", e),
        })?;

        if res.rows_affected() == 1 {
            return Ok(());
        }

        let row = sqlx::query(
            "SELECT expires_at, consumed_at, worker_id FROM worker_bootstrap_tokens WHERE token = $1",
        )
        .bind(token.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to lookup OTP token: {}", e),
        })?;

        let Some(row) = row else {
            return Err(DomainError::InvalidOtpToken {
                message: "Token not found".to_string(),
            });
        };

        let db_worker_id: Uuid =
            row.try_get("worker_id")
                .map_err(|e| DomainError::InfrastructureError {
                    message: e.to_string(),
                })?;
        if db_worker_id != worker_id.0 {
            return Err(DomainError::InvalidOtpToken {
                message: "Token does not match worker_id".to_string(),
            });
        }

        let consumed_at: Option<chrono::DateTime<chrono::Utc>> = row
            .try_get("consumed_at")
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;
        if consumed_at.is_some() {
            return Err(DomainError::InvalidOtpToken {
                message: "Token already used".to_string(),
            });
        }

        let expires_at: chrono::DateTime<chrono::Utc> =
            row.try_get("expires_at")
                .map_err(|e| DomainError::InfrastructureError {
                    message: e.to_string(),
                })?;
        if expires_at <= now {
            return Err(DomainError::InvalidOtpToken {
                message: "Token expired".to_string(),
            });
        }

        Err(DomainError::InvalidOtpToken {
            message: "Token invalid".to_string(),
        })
    }

    async fn cleanup_expired(&self) -> Result<u64> {
        let now = chrono::Utc::now();
        let res = sqlx::query(
            r#"
            DELETE FROM worker_bootstrap_tokens
            WHERE expires_at <= $1 OR consumed_at IS NOT NULL
            "#,
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to cleanup OTP tokens: {}", e),
        })?;

        Ok(res.rows_affected())
    }
}

/// Repositorio persistente para Workers basado en PostgreSQL
#[derive(Clone)]
pub struct PostgresWorkerRegistry {
    pool: PgPool,
    heartbeat_timeout: std::time::Duration,
}

impl PostgresWorkerRegistry {
    /// Crear nuevo repositorio con pool existente
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            heartbeat_timeout: std::time::Duration::from_secs(60),
        }
    }

    pub fn with_heartbeat_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    /// Crear repositorio conectando a la base de datos
    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self {
            pool,
            heartbeat_timeout: std::time::Duration::from_secs(60),
        })
    }

    /// Ejecutar migraciones para crear tablas de workers
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workers (
                id UUID PRIMARY KEY,
                provider_id UUID NOT NULL,
                provider_type VARCHAR(50) NOT NULL,
                handle JSONB NOT NULL,
                spec JSONB NOT NULL,
                state VARCHAR(50) NOT NULL,
                current_job_id UUID,
                jobs_executed INTEGER NOT NULL DEFAULT 0,
                last_heartbeat TIMESTAMPTZ NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create workers table: {}", e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_workers_provider_id ON workers(provider_id);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create workers provider_id index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_workers_state ON workers(state);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create workers state index: {}", e),
            })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_workers_last_heartbeat ON workers(last_heartbeat);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create workers last_heartbeat index: {}", e),
        })?;

        Ok(())
    }

    fn state_to_string(state: &WorkerState) -> String {
        match state {
            WorkerState::Creating => "CREATING".to_string(),
            WorkerState::Connecting => "CONNECTING".to_string(),
            WorkerState::Ready => "READY".to_string(),
            WorkerState::Busy => "BUSY".to_string(),
            WorkerState::Draining => "DRAINING".to_string(),
            WorkerState::Terminating => "TERMINATING".to_string(),
            WorkerState::Terminated => "TERMINATED".to_string(),
        }
    }

    fn string_to_state(s: &str) -> WorkerState {
        match s {
            "CREATING" => WorkerState::Creating,
            "CONNECTING" => WorkerState::Connecting,
            "READY" => WorkerState::Ready,
            "BUSY" => WorkerState::Busy,
            "DRAINING" => WorkerState::Draining,
            "TERMINATING" => WorkerState::Terminating,
            "TERMINATED" => WorkerState::Terminated,
            _ => WorkerState::Terminated, // Safe fallback
        }
    }
}

#[async_trait::async_trait]
impl WorkerRegistry for PostgresWorkerRegistry {
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec) -> Result<Worker> {
        let worker = Worker::new(handle.clone(), spec.clone());
        let worker_id = worker.id();

        let handle_json =
            serde_json::to_value(&handle).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize worker handle: {}", e),
            })?;

        let spec_json =
            serde_json::to_value(&spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize worker spec: {}", e),
            })?;

        sqlx::query(
            r#"
            INSERT INTO workers 
                (id, provider_id, provider_type, handle, spec, state, current_job_id, 
                 jobs_executed, last_heartbeat, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(worker.id().0)
        .bind(worker.provider_id().as_uuid())
        .bind(PostgresProviderConfigRepository::provider_type_to_string(
            worker.provider_type(),
        ))
        .bind(handle_json)
        .bind(spec_json)
        .bind(Self::state_to_string(worker.state()))
        .bind(worker.current_job_id().map(|id| id.0))
        .bind(worker.jobs_executed() as i32)
        .bind(worker.last_heartbeat())
        .bind(worker.created_at())
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                DomainError::WorkerAlreadyExists {
                    worker_id: worker_id.clone(),
                }
            } else {
                DomainError::InfrastructureError {
                    message: format!("Failed to register worker: {}", e),
                }
            }
        })?;

        Ok(worker)
    }

    async fn unregister(&self, worker_id: &WorkerId) -> Result<()> {
        let result = sqlx::query("DELETE FROM workers WHERE id = $1")
            .bind(worker_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to unregister worker: {}", e),
            })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }

        Ok(())
    }

    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>> {
        let row = sqlx::query(
            r#"
            SELECT id, handle, spec, state, current_job_id, jobs_executed, 
                   last_heartbeat, created_at, updated_at
            FROM workers WHERE id = $1
            "#,
        )
        .bind(worker_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get worker: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(map_row_to_worker(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find(&self, filter: &WorkerFilter) -> Result<Vec<Worker>> {
        // Basic query construction (this could be optimized with a query builder)
        let mut query = String::from(
            "SELECT id, handle, spec, state, current_job_id, jobs_executed, last_heartbeat, created_at, updated_at FROM workers WHERE 1=1",
        );

        // TODO: Implement actual filtering logic by extending the query dynamically
        // For MVP/Test simplicity, we'll fetch all and filter in memory if needed,
        // but for production query builder is better.
        // Let's implement partial filtering at SQL level for efficiency.

        if let Some(states) = &filter.states {
            let state_strings: Vec<String> = states.iter().map(Self::state_to_string).collect();
            let states_list = state_strings.join("','");
            query.push_str(&format!(" AND state IN ('{}')", states_list));
        }

        if let Some(provider_id) = &filter.provider_id {
            query.push_str(&format!(" AND provider_id = '{}'", provider_id.as_uuid()));
        }

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to find workers: {}", e),
            })?;

        let mut workers = Vec::new();
        for row in rows {
            let worker = map_row_to_worker(row)?;

            // Apply memory filters for complex logic not easy in SQL (like idle_for based on last_heartbeat)
            // This is a hybrid approach.
            if let Some(idle_duration) = filter.idle_for {
                let since_heartbeat = chrono::Utc::now()
                    .signed_duration_since(worker.last_heartbeat())
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO);
                if since_heartbeat < idle_duration {
                    continue;
                }
            }

            if let Some(can_accept) = filter.can_accept_jobs {
                if worker.state().can_accept_jobs() != can_accept {
                    continue;
                }
            }

            workers.push(worker);
        }

        Ok(workers)
    }

    async fn find_available(&self) -> Result<Vec<Worker>> {
        self.find(&WorkerFilter::new().accepting_jobs()).await
    }

    async fn find_by_provider(&self, provider_id: &ProviderId) -> Result<Vec<Worker>> {
        self.find(&WorkerFilter::new().with_provider_id(provider_id.clone()))
            .await
    }

    async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()> {
        let result = sqlx::query("UPDATE workers SET state = $1, updated_at = $2 WHERE id = $3")
            .bind(Self::state_to_string(&state))
            .bind(chrono::Utc::now())
            .bind(worker_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to update worker state: {}", e),
            })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }

        Ok(())
    }

    async fn heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        let result =
            sqlx::query("UPDATE workers SET last_heartbeat = $1, updated_at = $1 WHERE id = $2")
                .bind(chrono::Utc::now())
                .bind(worker_id.0)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to update heartbeat: {}", e),
                })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }

        Ok(())
    }

    async fn assign_to_job(&self, worker_id: &WorkerId, job_id: JobId) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workers SET current_job_id = $1, state = 'BUSY', updated_at = $2 WHERE id = $3",
        )
        .bind(job_id.0)
        .bind(chrono::Utc::now())
        .bind(worker_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to assign job to worker: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }

        Ok(())
    }

    async fn release_from_job(&self, worker_id: &WorkerId) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workers SET current_job_id = NULL, state = 'READY', jobs_executed = jobs_executed + 1, updated_at = $1 WHERE id = $2"
        )
        .bind(chrono::Utc::now())
        .bind(worker_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to release worker from job: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }

        Ok(())
    }

    async fn find_unhealthy(&self, timeout: std::time::Duration) -> Result<Vec<Worker>> {
        // We can do this efficiently in SQL
        let timeout_seconds = timeout.as_secs() as i64;
        let rows = sqlx::query(
            r#"
            SELECT id, handle, spec, state, current_job_id, jobs_executed, 
                   last_heartbeat, created_at, updated_at
            FROM workers 
            WHERE state != 'TERMINATED' 
            AND last_heartbeat < NOW() - make_interval(secs => $1)
            "#,
        )
        .bind(timeout_seconds as f64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find unhealthy workers: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }
        Ok(workers)
    }

    async fn find_for_termination(&self) -> Result<Vec<Worker>> {
        // Implementation that fetches all active workers and checks logic in memory
        // or complex SQL query. For now, fetch active and filter.
        let rows = sqlx::query(
            r#"
            SELECT id, handle, spec, state, current_job_id, jobs_executed, 
                   last_heartbeat, created_at, updated_at
            FROM workers 
            WHERE state != 'TERMINATED'
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch workers for termination check: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            let worker = map_row_to_worker(row)?;
            if worker.is_idle_timeout() || worker.is_lifetime_exceeded() {
                workers.push(worker);
            }
        }
        Ok(workers)
    }

    async fn stats(&self) -> Result<WorkerRegistryStats> {
        // This should be done with aggregations in SQL for performance
        let rows = sqlx::query("SELECT state, provider_type, provider_id FROM workers")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to fetch stats: {}", e),
            })?;

        let mut stats = WorkerRegistryStats::default();
        stats.total_workers = rows.len();

        for row in rows {
            let state_str: String = row.get("state");
            let state = PostgresWorkerRegistry::string_to_state(&state_str);
            let provider_type_str: String = row.get("provider_type");
            let provider_id_uuid: uuid::Uuid = row.get("provider_id");

            match state {
                WorkerState::Ready => stats.ready_workers += 1,
                WorkerState::Busy => stats.busy_workers += 1,
                WorkerState::Draining => stats.idle_workers += 1,
                WorkerState::Terminating => stats.terminating_workers += 1,
                _ => {}
            }

            *stats
                .workers_by_type
                .entry(PostgresProviderConfigRepository::string_to_provider_type(
                    &provider_type_str,
                ))
                .or_insert(0) += 1;
            *stats
                .workers_by_provider
                .entry(ProviderId::from_uuid(provider_id_uuid))
                .or_insert(0) += 1;
        }

        Ok(stats)
    }

    async fn count(&self) -> Result<usize> {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workers")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to count workers: {}", e),
            })?;
        Ok(result.0 as usize)
    }
}

fn map_row_to_worker(row: sqlx::postgres::PgRow) -> Result<Worker> {
    // We need to reconstruct the Worker.
    // Worker struct has private fields but provides a constructor `new` that takes Handle and Spec.
    // However, `new` sets state to Creating and timestamps to now.
    // We need to bypass this or use reflection/unsafe, BUT since we are in infrastructure layer,
    // we should ideally have a method in Domain to reconstruct from persistence, or `Worker` fields should be accessible.
    // Given the `Worker` struct in `crates/domain/src/worker.rs` does NOT have a reconstructor,
    // we might need to modify `Worker` or use a workaround.
    //
    // Best practice: Add `Worker::rehydrate` or similar to domain.
    // For now, assuming we can't change domain easily (as per instructions "No se toca las interfaces y ports a la ligera"),
    // but `Worker` is a Domain Entity, not a Port. Adding a reconstruction method is standard DDD.
    //
    // WAIT, I see `Worker` fields are private. I cannot set them directly.
    // I will use `serde_json::from_value` trick if `Worker` derives Deserialize!
    // Yes, `Worker` derives `Deserialize`.

    let id: uuid::Uuid = row.get("id");
    let handle_val: serde_json::Value = row.get("handle");
    let spec_val: serde_json::Value = row.get("spec");
    let state_str: String = row.get("state");
    let current_job_id_uuid: Option<uuid::Uuid> = row.get("current_job_id");
    let jobs_executed: i32 = row.get("jobs_executed");
    let last_heartbeat: chrono::DateTime<chrono::Utc> = row.get("last_heartbeat");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    // Construct a JSON object that matches Worker structure and deserialize it
    // This relies on the internal structure of Worker, which is a bit brittle but works without changing domain code.
    let worker_json = serde_json::json!({
        "id": id,
        "handle": handle_val,
        "spec": spec_val,
        "state": PostgresWorkerRegistry::string_to_state(&state_str),
        "current_job_id": current_job_id_uuid,
        "jobs_executed": jobs_executed,
        "last_heartbeat": last_heartbeat,
        "created_at": created_at,
        "updated_at": updated_at
    });

    let worker: Worker =
        serde_json::from_value(worker_json).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to deserialize worker from DB row: {}", e),
        })?;

    Ok(worker)
}

/// Configuración de la base de datos PostgreSQL
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: std::time::Duration,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://localhost/hodei".to_string(),
            max_connections: 10,
            connection_timeout: std::time::Duration::from_secs(30),
        }
    }
}

/// Repositorio persistente para ProviderConfig basado en PostgreSQL
#[derive(Clone)]
pub struct PostgresProviderConfigRepository {
    pool: PgPool,
}

impl PostgresProviderConfigRepository {
    /// Crear nuevo repositorio con pool existente
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Crear repositorio conectando a la base de datos
    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    /// Ejecutar migraciones para crear tablas necesarias
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS provider_configs (
                id UUID PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                provider_type VARCHAR(50) NOT NULL,
                status VARCHAR(50) NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                max_workers INTEGER NOT NULL DEFAULT 10,
                active_workers INTEGER NOT NULL DEFAULT 0,
                capabilities JSONB NOT NULL,
                type_config JSONB NOT NULL,
                tags TEXT[] NOT NULL DEFAULT '{}',
                metadata JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create provider_configs table: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_provider_configs_name ON provider_configs(name);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create provider_configs name index: {}", e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_provider_configs_type ON provider_configs(provider_type);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create provider_configs type index: {}", e),
            })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_provider_configs_status ON provider_configs(status);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create provider_configs status index: {}", e),
        })?;

        Ok(())
    }

    /// Convertir ProviderType a string para BD
    pub fn provider_type_to_string(pt: &ProviderType) -> String {
        match pt {
            ProviderType::Docker => "docker".to_string(),
            ProviderType::Kubernetes => "kubernetes".to_string(),
            ProviderType::Fargate => "fargate".to_string(),
            ProviderType::CloudRun => "cloud_run".to_string(),
            ProviderType::ContainerApps => "container_apps".to_string(),
            ProviderType::Lambda => "lambda".to_string(),
            ProviderType::CloudFunctions => "cloud_functions".to_string(),
            ProviderType::AzureFunctions => "azure_functions".to_string(),
            ProviderType::EC2 => "ec2".to_string(),
            ProviderType::ComputeEngine => "compute_engine".to_string(),
            ProviderType::AzureVMs => "azure_vms".to_string(),
            ProviderType::BareMetal => "bare_metal".to_string(),
            ProviderType::Custom(name) => format!("custom:{}", name),
        }
    }

    /// Convertir string de BD a ProviderType
    pub fn string_to_provider_type(s: &str) -> ProviderType {
        match s {
            "docker" => ProviderType::Docker,
            "kubernetes" => ProviderType::Kubernetes,
            "fargate" => ProviderType::Fargate,
            "cloud_run" => ProviderType::CloudRun,
            "container_apps" => ProviderType::ContainerApps,
            "lambda" => ProviderType::Lambda,
            "cloud_functions" => ProviderType::CloudFunctions,
            "azure_functions" => ProviderType::AzureFunctions,
            "ec2" => ProviderType::EC2,
            "compute_engine" => ProviderType::ComputeEngine,
            "azure_vms" => ProviderType::AzureVMs,
            "bare_metal" => ProviderType::BareMetal,
            s if s.starts_with("custom:") => {
                ProviderType::Custom(s.strip_prefix("custom:").unwrap_or("").to_string())
            }
            _ => ProviderType::Custom(s.to_string()),
        }
    }

    /// Convertir ProviderStatus a string
    pub fn status_to_string(status: &ProviderStatus) -> String {
        match status {
            ProviderStatus::Active => "active".to_string(),
            ProviderStatus::Maintenance => "maintenance".to_string(),
            ProviderStatus::Disabled => "disabled".to_string(),
            ProviderStatus::Overloaded => "overloaded".to_string(),
            ProviderStatus::Unhealthy => "unhealthy".to_string(),
            ProviderStatus::Degraded => "degraded".to_string(),
        }
    }

    /// Convertir string a ProviderStatus
    pub fn string_to_status(s: &str) -> ProviderStatus {
        match s {
            "active" => ProviderStatus::Active,
            "maintenance" => ProviderStatus::Maintenance,
            "disabled" => ProviderStatus::Disabled,
            "overloaded" => ProviderStatus::Overloaded,
            "unhealthy" => ProviderStatus::Unhealthy,
            "degraded" => ProviderStatus::Degraded,
            _ => ProviderStatus::Disabled,
        }
    }
}

#[async_trait::async_trait]
impl ProviderConfigRepository for PostgresProviderConfigRepository {
    async fn save(&self, config: &ProviderConfig) -> Result<()> {
        let capabilities_json = serde_json::to_value(&config.capabilities).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize capabilities: {}", e),
            }
        })?;

        let type_config_json = serde_json::to_value(&config.type_config).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize type_config: {}", e),
            }
        })?;

        let metadata_json = serde_json::to_value(&config.metadata).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize metadata: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO provider_configs 
                (id, name, provider_type, status, priority, max_workers, active_workers,
                 capabilities, type_config, tags, metadata, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                provider_type = EXCLUDED.provider_type,
                status = EXCLUDED.status,
                priority = EXCLUDED.priority,
                max_workers = EXCLUDED.max_workers,
                active_workers = EXCLUDED.active_workers,
                capabilities = EXCLUDED.capabilities,
                type_config = EXCLUDED.type_config,
                tags = EXCLUDED.tags,
                metadata = EXCLUDED.metadata,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(config.id.as_uuid())
        .bind(&config.name)
        .bind(Self::provider_type_to_string(&config.provider_type))
        .bind(Self::status_to_string(&config.status))
        .bind(config.priority)
        .bind(config.max_workers as i32)
        .bind(config.active_workers as i32)
        .bind(&capabilities_json)
        .bind(&type_config_json)
        .bind(&config.tags)
        .bind(&metadata_json)
        .bind(config.created_at)
        .bind(config.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save provider config: {}", e),
        })?;

        Ok(())
    }

    async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider config: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(self.row_to_config(&row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider config by name: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(self.row_to_config(&row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_type(&self, provider_type: &ProviderType) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs WHERE provider_type = $1
            ORDER BY priority DESC
            "#,
        )
        .bind(Self::provider_type_to_string(provider_type))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find providers by type: {}", e),
        })?;

        rows.iter().map(|row| self.row_to_config(row)).collect()
    }

    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs WHERE status = 'active'
            ORDER BY priority DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find enabled providers: {}", e),
        })?;

        rows.iter().map(|row| self.row_to_config(row)).collect()
    }

    async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs 
            WHERE status = 'active' AND active_workers < max_workers
            ORDER BY priority DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find providers with capacity: {}", e),
        })?;

        rows.iter().map(|row| self.row_to_config(row)).collect()
    }

    async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, status, priority, max_workers, active_workers,
                   capabilities, type_config, tags, metadata, created_at, updated_at
            FROM provider_configs
            ORDER BY priority DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find all providers: {}", e),
        })?;

        rows.iter().map(|row| self.row_to_config(row)).collect()
    }

    async fn update(&self, config: &ProviderConfig) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE provider_configs SET
                name = $2,
                provider_type = $3,
                status = $4,
                priority = $5,
                max_workers = $6,
                active_workers = $7,
                capabilities = $8,
                type_config = $9,
                tags = $10,
                metadata = $11,
                updated_at = $12
            WHERE id = $1
            "#,
        )
        .bind(config.id.as_uuid())
        .bind(&config.name)
        .bind(Self::provider_type_to_string(&config.provider_type))
        .bind(Self::status_to_string(&config.status))
        .bind(config.priority)
        .bind(config.max_workers as i32)
        .bind(config.active_workers as i32)
        .bind(serde_json::to_value(&config.capabilities).unwrap())
        .bind(serde_json::to_value(&config.type_config).unwrap())
        .bind(&config.tags)
        .bind(serde_json::to_value(&config.metadata).unwrap())
        .bind(config.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to update provider config: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ProviderNotFound {
                provider_id: config.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &ProviderId) -> Result<()> {
        sqlx::query("DELETE FROM provider_configs WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete provider config: {}", e),
            })?;

        Ok(())
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let result: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM provider_configs WHERE name = $1")
                .bind(name)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to check if provider exists: {}", e),
                })?;

        Ok(result.0 > 0)
    }
}

impl PostgresProviderConfigRepository {
    fn row_to_config(&self, row: &sqlx::postgres::PgRow) -> Result<ProviderConfig> {
        use hodei_jobs_domain::provider_config::ProviderTypeConfig;
        use hodei_jobs_domain::worker_provider::ProviderCapabilities;

        let id: uuid::Uuid = row.get("id");
        let name: String = row.get("name");
        let provider_type_str: String = row.get("provider_type");
        let status_str: String = row.get("status");
        let priority: i32 = row.get("priority");
        let max_workers: i32 = row.get("max_workers");
        let active_workers: i32 = row.get("active_workers");
        let capabilities_json: serde_json::Value = row.get("capabilities");
        let type_config_json: serde_json::Value = row.get("type_config");
        let tags: Vec<String> = row.get("tags");
        let metadata_json: serde_json::Value = row.get("metadata");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        let capabilities: ProviderCapabilities = serde_json::from_value(capabilities_json)
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize capabilities: {}", e),
            })?;

        let type_config: ProviderTypeConfig =
            serde_json::from_value(type_config_json).map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to deserialize type_config: {}", e),
                }
            })?;

        let metadata: HashMap<String, String> =
            serde_json::from_value(metadata_json).map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to deserialize metadata: {}", e),
                }
            })?;

        Ok(ProviderConfig {
            id: ProviderId::from_uuid(id),
            name,
            provider_type: Self::string_to_provider_type(&provider_type_str),
            status: Self::string_to_status(&status_str),
            capabilities,
            type_config,
            priority,
            max_workers: max_workers as u32,
            active_workers: active_workers as u32,
            created_at,
            updated_at,
            tags,
            metadata,
        })
    }
}

// ============================================================================
// PostgresJobTemplateRepository - HU-6.7
// ============================================================================

/// Postgres implementation of JobTemplateRepository
pub struct PostgresJobTemplateRepository {
    pool: PgPool,
}

impl PostgresJobTemplateRepository {
    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_templates (
                id UUID PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                description TEXT,
                spec JSONB NOT NULL,
                status VARCHAR(50) NOT NULL DEFAULT 'active',
                version INTEGER NOT NULL DEFAULT 1,
                labels JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                created_by VARCHAR(255),
                run_count BIGINT NOT NULL DEFAULT 0,
                success_count BIGINT NOT NULL DEFAULT 0,
                failure_count BIGINT NOT NULL DEFAULT 0
            );
            
            CREATE INDEX IF NOT EXISTS idx_job_templates_name ON job_templates(name);
            CREATE INDEX IF NOT EXISTS idx_job_templates_status ON job_templates(status);
            CREATE INDEX IF NOT EXISTS idx_job_templates_labels ON job_templates USING GIN(labels);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to run job_templates migrations: {}", e),
        })?;

        Ok(())
    }

    fn status_to_string(status: &JobTemplateStatus) -> &'static str {
        match status {
            JobTemplateStatus::Active => "active",
            JobTemplateStatus::Disabled => "disabled",
            JobTemplateStatus::Archived => "archived",
        }
    }

    fn string_to_status(s: &str) -> JobTemplateStatus {
        match s {
            "active" => JobTemplateStatus::Active,
            "disabled" => JobTemplateStatus::Disabled,
            "archived" => JobTemplateStatus::Archived,
            _ => JobTemplateStatus::Active,
        }
    }

    fn row_to_template(&self, row: &sqlx::postgres::PgRow) -> Result<JobTemplate> {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let description: Option<String> = row.get("description");
        let spec_json: serde_json::Value = row.get("spec");
        let status_str: String = row.get("status");
        let version: i32 = row.get("version");
        let labels_json: serde_json::Value = row.get("labels");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
        let created_by: Option<String> = row.get("created_by");
        let run_count: i64 = row.get("run_count");
        let success_count: i64 = row.get("success_count");
        let failure_count: i64 = row.get("failure_count");

        let spec: JobSpec =
            serde_json::from_value(spec_json).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize job spec: {}", e),
            })?;

        let labels: HashMap<String, String> =
            serde_json::from_value(labels_json).unwrap_or_default();

        Ok(JobTemplate {
            id: JobTemplateId::from_uuid(id),
            name,
            description,
            spec,
            status: Self::string_to_status(&status_str),
            version: version as u32,
            labels,
            created_at,
            updated_at,
            created_by,
            run_count: run_count as u64,
            success_count: success_count as u64,
            failure_count: failure_count as u64,
        })
    }
}

#[async_trait::async_trait]
impl JobTemplateRepository for PostgresJobTemplateRepository {
    async fn save(&self, template: &JobTemplate) -> Result<()> {
        let spec_json =
            serde_json::to_value(&template.spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            })?;

        let labels_json = serde_json::to_value(&template.labels).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize labels: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO job_templates (
                id, name, description, spec, status, version, labels,
                created_at, updated_at, created_by, run_count, success_count, failure_count
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(&template.description)
        .bind(&spec_json)
        .bind(Self::status_to_string(&template.status))
        .bind(template.version as i32)
        .bind(&labels_json)
        .bind(template.created_at)
        .bind(template.updated_at)
        .bind(&template.created_by)
        .bind(template.run_count as i64)
        .bind(template.success_count as i64)
        .bind(template.failure_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job template: {}", e),
        })?;

        Ok(())
    }

    async fn update(&self, template: &JobTemplate) -> Result<()> {
        let spec_json =
            serde_json::to_value(&template.spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            })?;

        let labels_json = serde_json::to_value(&template.labels).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize labels: {}", e),
            }
        })?;

        let result = sqlx::query(
            r#"
            UPDATE job_templates SET
                name = $2,
                description = $3,
                spec = $4,
                status = $5,
                version = $6,
                labels = $7,
                updated_at = $8,
                run_count = $9,
                success_count = $10,
                failure_count = $11
            WHERE id = $1
            "#,
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(&template.description)
        .bind(&spec_json)
        .bind(Self::status_to_string(&template.status))
        .bind(template.version as i32)
        .bind(&labels_json)
        .bind(template.updated_at)
        .bind(template.run_count as i64)
        .bind(template.success_count as i64)
        .bind(template.failure_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to update job template: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::JobNotFound {
                job_id: JobId(template.id.0),
            });
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &JobTemplateId) -> Result<Option<JobTemplate>> {
        let row = sqlx::query("SELECT * FROM job_templates WHERE id = $1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to find job template: {}", e),
            })?;

        match row {
            Some(r) => Ok(Some(self.row_to_template(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<JobTemplate>> {
        let row = sqlx::query("SELECT * FROM job_templates WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to find job template by name: {}", e),
            })?;

        match row {
            Some(r) => Ok(Some(self.row_to_template(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_active(&self) -> Result<Vec<JobTemplate>> {
        let rows = sqlx::query("SELECT * FROM job_templates WHERE status = 'active' ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to list active job templates: {}", e),
            })?;

        rows.iter().map(|r| self.row_to_template(r)).collect()
    }

    async fn find_by_label(&self, key: &str, value: &str) -> Result<Vec<JobTemplate>> {
        let rows = sqlx::query("SELECT * FROM job_templates WHERE labels->>$1 = $2 ORDER BY name")
            .bind(key)
            .bind(value)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to find job templates by label: {}", e),
            })?;

        rows.iter().map(|r| self.row_to_template(r)).collect()
    }

    async fn delete(&self, id: &JobTemplateId) -> Result<()> {
        sqlx::query("DELETE FROM job_templates WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete job template: {}", e),
            })?;

        Ok(())
    }
}

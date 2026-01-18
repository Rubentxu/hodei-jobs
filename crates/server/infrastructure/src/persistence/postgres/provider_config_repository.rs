//! PostgreSQL Provider Config Repository
//!
//! Persistent repository implementation for provider configurations based on PostgreSQL
//!
//! # Pool Management
//!
//! This repository expects a `PgPool` to be passed in via `new()`.
//! The pool should be created using `DatabasePool` for consistent configuration.

use hodei_server_domain::providers::config::{
    ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
};
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, ProviderStatus, Result};
use sqlx::Row;
use sqlx::postgres::PgPool;

/// PostgreSQL Provider Config Repository
#[derive(Clone)]
pub struct PostgresProviderConfigRepository {
    pool: PgPool,
}

impl PostgresProviderConfigRepository {
    /// Create new repository with existing pool
    ///
    /// The pool should be created centrally (e.g., using `DatabasePool`)
    /// to ensure consistent configuration across all repositories.
    #[inline]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run migrations to create provider config tables
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<()> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProviderConfigRepository for PostgresProviderConfigRepository {
    async fn save(&self, provider: &ProviderConfig) -> Result<()> {
        let config_json = serde_json::to_value(&provider.type_config).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize provider config: {}", e),
            }
        })?;

        let tags_json =
            serde_json::to_value(&provider.tags).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize provider tags: {}", e),
            })?;

        let metadata_json = serde_json::to_value(&provider.metadata).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize provider metadata: {}", e),
            }
        })?;

        // EPIC-86: Serialize new filtering fields
        let preferred_region_json = serde_json::to_value(&provider.preferred_region).ok();
        let allowed_regions_json =
            serde_json::to_value(&provider.allowed_regions).map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to serialize allowed regions: {}", e),
                }
            })?;
        let required_labels_json =
            serde_json::to_value(&provider.required_labels).map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to serialize required labels: {}", e),
                }
            })?;
        let annotations_json = serde_json::to_value(&provider.annotations).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize annotations: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO provider_configs
                (id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                 preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (name) DO UPDATE SET
                config = EXCLUDED.config,
                status = EXCLUDED.status,
                priority = EXCLUDED.priority,
                max_workers = EXCLUDED.max_workers,
                tags = EXCLUDED.tags,
                metadata = EXCLUDED.metadata,
                preferred_region = EXCLUDED.preferred_region,
                allowed_regions = EXCLUDED.allowed_regions,
                required_labels = EXCLUDED.required_labels,
                annotations = EXCLUDED.annotations,
                updated_at = now()
            "#,
        )
        .bind(provider.id.as_uuid())
        .bind(&provider.name)
        .bind(provider.provider_type.to_string())
        .bind(config_json)
        .bind(provider.status.to_string())
        .bind(provider.priority as i32)
        .bind(provider.max_workers as i32)
        .bind(tags_json)
        .bind(metadata_json)
        .bind(preferred_region_json)
        .bind(allowed_regions_json)
        .bind(required_labels_json)
        .bind(annotations_json)
        .bind(provider.created_at)
        .bind(provider.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save provider config: {}", e),
        })?;

        Ok(())
    }

    async fn find_by_id(&self, provider_id: &ProviderId) -> Result<Option<ProviderConfig>> {
        let row_opt = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            WHERE id = $1
            "#,
        )
        .bind(provider_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider config by id: {}", e),
        })?;

        if let Some(row) = row_opt {
            Ok(Some(map_row_to_provider_config(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
        let row_opt = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider config by name: {}", e),
        })?;

        if let Some(row) = row_opt {
            Ok(Some(map_row_to_provider_config(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            ORDER BY priority DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find all provider configs: {}", e),
        })?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(map_row_to_provider_config(row)?);
        }

        Ok(providers)
    }

    async fn find_by_type(
        &self,
        provider_type: &hodei_server_domain::workers::ProviderType,
    ) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            WHERE provider_type = $1
            ORDER BY priority DESC, name ASC
            "#,
        )
        .bind(provider_type.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider configs by type: {}", e),
        })?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(map_row_to_provider_config(row)?);
        }

        Ok(providers)
    }

    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            WHERE status = 'ACTIVE'
            ORDER BY priority DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find enabled provider configs: {}", e),
        })?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(map_row_to_provider_config(row)?);
        }

        Ok(providers)
    }

    async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata,
                   preferred_region, allowed_regions, required_labels, annotations, created_at, updated_at
            FROM provider_configs
            WHERE status = 'ACTIVE'
            ORDER BY priority DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find provider configs with capacity: {}", e),
        })?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(map_row_to_provider_config(row)?);
        }

        Ok(providers)
    }

    async fn update(&self, config: &ProviderConfig) -> Result<()> {
        self.save(config).await
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        // Use query() instead of query_as!() to avoid compile-time schema verification
        let row = sqlx::query("SELECT COUNT(*) as count FROM provider_configs WHERE name = $1")
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to check provider config existence: {}", e),
            })?;

        let count: i64 = row.try_get("count").unwrap_or(0);
        Ok(count > 0)
    }

    async fn delete(&self, provider_id: &ProviderId) -> Result<()> {
        sqlx::query("DELETE FROM provider_configs WHERE id = $1")
            .bind(provider_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete provider config: {}", e),
            })?;

        Ok(())
    }
}

fn map_row_to_provider_config(row: sqlx::postgres::PgRow) -> Result<ProviderConfig> {
    let id: uuid::Uuid = row.get("id");
    let name: String = row.get("name");
    let provider_type_str: String = row.get("provider_type");
    let config_json: serde_json::Value = row.get("config");
    let status_str: String = row.get("status");
    let priority: i32 = row.get("priority");
    let max_workers: i32 = row.get("max_workers");
    let tags_json: serde_json::Value = row.get("tags");
    let metadata_json: serde_json::Value = row.get("metadata");
    // EPIC-86: New filtering fields
    // preferred_region is VARCHAR in DB, not JSONB
    let preferred_region: Option<String> = row.try_get("preferred_region")?;
    let allowed_regions_json: serde_json::Value = row.get("allowed_regions");
    let required_labels_json: serde_json::Value = row.get("required_labels");
    let annotations_json: serde_json::Value = row.get("annotations");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    let provider_type = match provider_type_str.to_uppercase().as_str() {
        "DOCKER" => hodei_server_domain::workers::ProviderType::Docker,
        "KUBERNETES" => hodei_server_domain::workers::ProviderType::Kubernetes,
        "FARGATE" => hodei_server_domain::workers::ProviderType::Fargate,
        "CLOUDRUN" => hodei_server_domain::workers::ProviderType::CloudRun,
        "CONTAINERAPPS" => hodei_server_domain::workers::ProviderType::ContainerApps,
        "LAMBDA" => hodei_server_domain::workers::ProviderType::Lambda,
        "CLOUDFUNCTIONS" => hodei_server_domain::workers::ProviderType::CloudFunctions,
        "AZUREFUNCTIONS" => hodei_server_domain::workers::ProviderType::AzureFunctions,
        "EC2" => hodei_server_domain::workers::ProviderType::EC2,
        "COMPUTEENGINE" => hodei_server_domain::workers::ProviderType::ComputeEngine,
        "AZUREVMS" => hodei_server_domain::workers::ProviderType::AzureVMs,
        "TEST" => hodei_server_domain::workers::ProviderType::Test,
        "BAREMETAL" => hodei_server_domain::workers::ProviderType::BareMetal,
        "CUSTOM" => hodei_server_domain::workers::ProviderType::Custom("custom".to_string()),
        _ => hodei_server_domain::workers::ProviderType::Docker,
    };

    let config: ProviderTypeConfig = ProviderTypeConfig::deserialize_from_json(config_json)
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to deserialize provider config: {}", e),
        })?;

    let status = match status_str.as_str() {
        "ACTIVE" => ProviderStatus::Active,
        "MAINTENANCE" => ProviderStatus::Maintenance,
        "DISABLED" => ProviderStatus::Disabled,
        "OVERLOADED" => ProviderStatus::Overloaded,
        "UNHEALTHY" => ProviderStatus::Unhealthy,
        _ => ProviderStatus::Disabled,
    };

    let tags: Vec<String> = serde_json::from_value(tags_json).unwrap_or_default();
    let metadata: std::collections::HashMap<String, String> =
        serde_json::from_value(metadata_json).unwrap_or_default();

    // EPIC-86: Deserialize new filtering fields (preferred_region is already String from VARCHAR)
    let allowed_regions: Vec<String> =
        serde_json::from_value(allowed_regions_json).unwrap_or_default();
    let required_labels: std::collections::HashMap<String, String> =
        serde_json::from_value(required_labels_json).unwrap_or_default();
    let annotations: std::collections::HashMap<String, String> =
        serde_json::from_value(annotations_json).unwrap_or_default();

    Ok(ProviderConfig {
        id: ProviderId(id),
        name,
        provider_type,
        status,
        capabilities: hodei_server_domain::workers::ProviderCapabilities::default(),
        type_config: config,
        priority,
        max_workers: max_workers as u32,
        active_workers: 0,
        tags,
        metadata,
        created_at,
        updated_at,
        // EPIC-86: New filtering fields
        preferred_region,
        allowed_regions,
        required_labels,
        annotations,
    })
}

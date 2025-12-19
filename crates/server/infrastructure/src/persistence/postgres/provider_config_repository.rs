//! PostgreSQL Provider Config Repository
//!
//! Persistent repository implementation for provider configurations based on PostgreSQL

use hodei_server_domain::providers::config::{
    ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
};
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, ProviderStatus, Result};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use super::DatabaseConfig;

/// PostgreSQL Provider Config Repository
#[derive(Clone)]
pub struct PostgresProviderConfigRepository {
    pool: PgPool,
}

impl PostgresProviderConfigRepository {
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
            CREATE TABLE IF NOT EXISTS provider_configs (
                id UUID PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                provider_type VARCHAR(50) NOT NULL,
                config JSONB NOT NULL,
                status VARCHAR(50) NOT NULL DEFAULT 'ACTIVE',
                priority INTEGER NOT NULL DEFAULT 0,
                max_workers INTEGER NOT NULL DEFAULT 10,
                tags JSONB NOT NULL DEFAULT '[]',
                metadata JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
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

        sqlx::query(
            r#"
            INSERT INTO provider_configs
                (id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (name) DO UPDATE SET
                config = EXCLUDED.config,
                status = EXCLUDED.status,
                priority = EXCLUDED.priority,
                max_workers = EXCLUDED.max_workers,
                tags = EXCLUDED.tags,
                metadata = EXCLUDED.metadata,
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
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
            SELECT id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at
            FROM provider_configs
            WHERE status = 'ACTIVE' AND active_workers < max_workers
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
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) as count FROM provider_configs WHERE name = $1",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to check provider config existence: {}", e),
        })?;

        Ok(row.0 > 0)
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
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    let provider_type = match provider_type_str.as_str() {
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

    let config: ProviderTypeConfig =
        serde_json::from_value(config_json).map_err(|e| DomainError::InfrastructureError {
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
    })
}

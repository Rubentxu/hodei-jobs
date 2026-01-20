//!
//! # PostgreSQL Migration Helper
//!
//! Production-ready implementation of SagaMigrationHelper using PostgreSQL
//! for persistent storage of migration configuration and decisions.
//!

use async_trait::async_trait;
use sqlx::postgres::PgPool;
use sqlx::{Executor, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use hodei_server_application::saga::ports::migration_helper::{
    MigrationDecision, MigrationHelperConfig, MigrationStrategy, SagaMigrationHelper, SagaTypeInfo,
    SagaTypeRegistry, SagaTypeRegistryTrait,
};

/// SQL statements for migration helper tables
const MIGRATION_CONFIG_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS saga_migration_config (
        id TEXT PRIMARY KEY DEFAULT 'global',
        v4_enabled BOOLEAN NOT NULL DEFAULT false,
        default_strategy TEXT NOT NULL DEFAULT 'KeepLegacy',
        strategy_overrides JSONB NOT NULL DEFAULT '{}',
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )
"#;

const MIGRATION_DECISIONS_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS saga_migration_decisions (
        id BIGSERIAL PRIMARY KEY,
        saga_type TEXT NOT NULL,
        strategy TEXT NOT NULL,
        should_use_v4 BOOLEAN NOT NULL,
        reason TEXT NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )
"#;

const SAGA_TYPE_INFO_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS saga_type_info (
        name TEXT PRIMARY KEY,
        display_name TEXT NOT NULL,
        description TEXT NOT NULL,
        is_migrated BOOLEAN NOT NULL DEFAULT false,
        migrated_at TIMESTAMPTZ,
        migration_notes TEXT,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )
"#;

const MIGRATION_DECISIONS_INDEX: &str = r#"
    CREATE INDEX IF NOT EXISTS idx_saga_migration_decisions_saga_type
    ON saga_migration_decisions(saga_type, created_at DESC)
"#;

/// Errors from PostgreSQL migration helper operations.
#[derive(Debug, thiserror::Error)]
pub enum PostgresMigrationHelperError {
    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl From<sqlx::Error> for PostgresMigrationHelperError {
    fn from(e: sqlx::Error) -> Self {
        Self::DatabaseError(e.to_string())
    }
}

/// PostgreSQL-backed migration helper for production use.
///
/// This implementation provides persistent storage for migration configuration,
/// decision history, and saga type information using PostgreSQL.
#[derive(Debug)]
pub struct PostgresMigrationHelper {
    pool: Arc<PgPool>,
    config: Arc<RwLock<MigrationHelperConfig>>,
    decision_history: Arc<RwLock<Vec<MigrationDecision>>>,
}

impl PostgresMigrationHelper {
    /// Creates a new PostgresMigrationHelper.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool.
    ///
    /// # Returns
    ///
    /// A new instance, after running migrations.
    pub async fn new(pool: Arc<PgPool>) -> Result<Self, PostgresMigrationHelperError> {
        Self::with_config(pool, MigrationHelperConfig::default()).await
    }

    /// Creates a new PostgresMigrationHelper with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool.
    /// * `config` - Initial migration helper configuration.
    ///
    /// # Returns
    ///
    /// A new instance with custom config, after running migrations.
    pub async fn with_config(
        pool: Arc<PgPool>,
        config: MigrationHelperConfig,
    ) -> Result<Self, PostgresMigrationHelperError> {
        // Run migrations
        pool.execute(MIGRATION_CONFIG_TABLE).await?;
        pool.execute(MIGRATION_DECISIONS_TABLE).await?;
        pool.execute(SAGA_TYPE_INFO_TABLE).await?;
        pool.execute(MIGRATION_DECISIONS_INDEX).await?;

        let helper = Self {
            pool: pool.clone(),
            config: Arc::new(RwLock::new(config)),
            decision_history: Arc::new(RwLock::new(Vec::new())),
        };

        // Load existing config from database
        helper.load_config().await?;

        Ok(helper)
    }

    /// Loads configuration from the database.
    async fn load_config(&self) -> Result<(), PostgresMigrationHelperError> {
        let result = sqlx::query(
            "SELECT v4_enabled, default_strategy, strategy_overrides FROM saga_migration_config WHERE id = 'global'",
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = result {
            let v4_enabled: bool = row.try_get("v4_enabled")?;
            let default_strategy: String = row.try_get("default_strategy")?;
            let strategy_overrides: serde_json::Value = row.try_get("strategy_overrides")?;

            let strategy = match default_strategy.as_str() {
                "KeepLegacy" => MigrationStrategy::KeepLegacy,
                "UseV4" => MigrationStrategy::UseV4,
                "DualWrite" => MigrationStrategy::DualWrite,
                "DualWriteReadV4" => MigrationStrategy::DualWriteReadV4,
                "GradualMigration" => {
                    let percentage = strategy_overrides
                        .as_object()
                        .and_then(|o| o.get("v4_percentage"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(50);
                    let seed = strategy_overrides
                        .as_object()
                        .and_then(|o| o.get("seed"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(12345);
                    MigrationStrategy::GradualMigration {
                        v4_percentage: percentage as u32,
                        seed,
                    }
                }
                _ => MigrationStrategy::KeepLegacy,
            };

            let mut config = self.config.write().await;
            config.v4_enabled = v4_enabled;
            config.default_strategy = strategy;

            // Parse strategy overrides
            if let Some(overrides) = strategy_overrides.as_object() {
                for (saga_type, strategy_val) in overrides {
                    let s = match strategy_val.as_str() {
                        Some("KeepLegacy") => MigrationStrategy::KeepLegacy,
                        Some("UseV4") => MigrationStrategy::UseV4,
                        Some("DualWrite") => MigrationStrategy::DualWrite,
                        Some("DualWriteReadV4") => MigrationStrategy::DualWriteReadV4,
                        _ => MigrationStrategy::KeepLegacy,
                    };
                    config.strategy_overrides.insert(saga_type.clone(), s);
                }
            }
        } else {
            sqlx::query(
                "INSERT INTO saga_migration_config (id, v4_enabled, default_strategy, strategy_overrides)
                 VALUES ('global', false, 'KeepLegacy', '{}')
                 ON CONFLICT (id) DO NOTHING",
            )
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Saves the current configuration to the database.
    async fn save_config(&self) -> Result<(), PostgresMigrationHelperError> {
        let config = self.config.read().await;
        let (default_strategy, overrides_json) = match &config.default_strategy {
            MigrationStrategy::KeepLegacy => ("KeepLegacy", serde_json::json!({})),
            MigrationStrategy::UseV4 => ("UseV4", serde_json::json!({})),
            MigrationStrategy::DualWrite => ("DualWrite", serde_json::json!({})),
            MigrationStrategy::DualWriteReadV4 => ("DualWriteReadV4", serde_json::json!({})),
            MigrationStrategy::GradualMigration {
                v4_percentage,
                seed,
            } => (
                "GradualMigration",
                serde_json::json!({
                    "v4_percentage": v4_percentage,
                    "seed": seed
                }),
            ),
        };

        sqlx::query(
            "UPDATE saga_migration_config SET v4_enabled = $1, default_strategy = $2, strategy_overrides = $3, updated_at = NOW() WHERE id = 'global'",
        )
        .bind(config.v4_enabled)
        .bind(default_strategy)
        .bind(overrides_json)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl SagaMigrationHelper for PostgresMigrationHelper {
    async fn decide(&self, saga_type: &str) -> MigrationDecision {
        let config = self.config.read().await;
        let strategy = config
            .strategy_overrides
            .get(saga_type)
            .cloned()
            .unwrap_or_else(|| config.default_strategy.clone());

        let (should_use_v4, reason) = match &strategy {
            MigrationStrategy::KeepLegacy => (false, "Strategy: KeepLegacy".to_string()),
            MigrationStrategy::UseV4 => (true, "Strategy: UseV4".to_string()),
            MigrationStrategy::DualWrite => {
                (false, "Strategy: DualWrite (read from legacy)".to_string())
            }
            MigrationStrategy::DualWriteReadV4 => (true, "Strategy: DualWriteReadV4".to_string()),
            MigrationStrategy::GradualMigration {
                v4_percentage,
                seed,
            } => {
                let use_v4 = self.should_route_to_v4(saga_type, *v4_percentage, *seed);
                if use_v4 {
                    (
                        true,
                        format!("GradualMigration: {}% routing to v4", v4_percentage),
                    )
                } else {
                    (
                        false,
                        format!(
                            "GradualMigration: {}% routing to legacy",
                            100 - v4_percentage
                        ),
                    )
                }
            }
        };

        let decision = MigrationDecision {
            saga_type: saga_type.to_string(),
            strategy: strategy.clone(),
            should_use_v4,
            reason: reason.clone(),
        };

        // Record decision to database
        let strategy_str = match &strategy {
            MigrationStrategy::KeepLegacy => "KeepLegacy",
            MigrationStrategy::UseV4 => "UseV4",
            MigrationStrategy::DualWrite => "DualWrite",
            MigrationStrategy::DualWriteReadV4 => "DualWriteReadV4",
            MigrationStrategy::GradualMigration { .. } => "GradualMigration",
        };

        if let Err(e) = sqlx::query(
            "INSERT INTO saga_migration_decisions (saga_type, strategy, should_use_v4, reason)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(saga_type)
        .bind(strategy_str)
        .bind(should_use_v4)
        .bind(&reason)
        .execute(&*self.pool)
        .await
        {
            warn!(error = %e, "Failed to record migration decision");
        }

        let mut history = self.decision_history.write().await;
        history.push(decision.clone());

        decision
    }

    async fn get_strategy(&self, saga_type: &str) -> MigrationStrategy {
        let config = self.config.read().await;
        config
            .strategy_overrides
            .get(saga_type)
            .cloned()
            .unwrap_or_else(|| config.default_strategy.clone())
    }

    async fn set_strategy(&self, saga_type: &str, strategy: MigrationStrategy) {
        let mut config = self.config.write().await;
        config
            .strategy_overrides
            .insert(saga_type.to_string(), strategy);

        if let Err(e) = self.save_config().await {
            warn!(error = %e, "Failed to save migration config");
        }
    }

    fn is_v4_enabled(&self) -> bool {
        self.config
            .try_read()
            .map(|config| config.v4_enabled)
            .unwrap_or(false)
    }

    fn set_v4_enabled(&self, enabled: bool) {
        if let Ok(mut config) = self.config.try_write() {
            config.v4_enabled = enabled;
        }
    }
}

impl PostgresMigrationHelper {
    /// Determine if a workflow should route to v4 based on gradual migration strategy.
    fn should_route_to_v4(&self, saga_type: &str, v4_percentage: u32, seed: u64) -> bool {
        if v4_percentage == 0 {
            return false;
        }
        if v4_percentage >= 100 {
            return true;
        }

        let hash = self.create_hash(saga_type, seed);
        let bucket = hash % 100;

        bucket < v4_percentage as u64
    }

    /// Simple hash function for deterministic routing.
    fn create_hash(&self, input: &str, seed: u64) -> u64 {
        let mut hash = seed;
        for byte in input.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    /// Persists the current configuration to the database asynchronously.
    pub async fn persist_config(&self) -> Result<(), PostgresMigrationHelperError> {
        self.save_config().await
    }

    /// Gets recent migration decisions from the database.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of decisions to retrieve.
    ///
    /// # Returns
    ///
    /// A vector of recent migration decisions.
    pub async fn get_recent_decisions(
        &self,
        limit: i64,
    ) -> Result<Vec<MigrationDecision>, PostgresMigrationHelperError> {
        let rows = sqlx::query(
            "SELECT saga_type, strategy, should_use_v4, reason, created_at
             FROM saga_migration_decisions
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut decisions = Vec::new();
        for row in rows {
            let strategy: String = row.try_get("strategy")?;
            let strategy = match strategy.as_str() {
                "KeepLegacy" => MigrationStrategy::KeepLegacy,
                "UseV4" => MigrationStrategy::UseV4,
                "DualWrite" => MigrationStrategy::DualWrite,
                "DualWriteReadV4" => MigrationStrategy::DualWriteReadV4,
                _ => MigrationStrategy::KeepLegacy,
            };

            decisions.push(MigrationDecision {
                saga_type: row.try_get("saga_type")?,
                strategy,
                should_use_v4: row.try_get("should_use_v4")?,
                reason: row.try_get("reason")?,
            });
        }

        Ok(decisions)
    }
}

/// PostgreSQL-backed saga type registry.
///
/// This implementation provides persistent storage for saga type information.
#[derive(Debug)]
pub struct PostgresSagaTypeRegistry {
    pool: Arc<PgPool>,
}

impl PostgresSagaTypeRegistry {
    /// Creates a new PostgresSagaTypeRegistry.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool.
    ///
    /// # Returns
    ///
    /// A new instance.
    pub async fn new(pool: Arc<PgPool>) -> Result<Self, PostgresMigrationHelperError> {
        pool.execute(SAGA_TYPE_INFO_TABLE).await?;
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl SagaTypeRegistryTrait for PostgresSagaTypeRegistry {
    async fn register(&self, saga_type: SagaTypeInfo) {
        let notes = saga_type.migration_notes.clone();
        let migrated_at = saga_type.migrated_at.map(|dt| dt.timestamp_millis());

        let result = sqlx::query(
            "INSERT INTO saga_type_info (name, display_name, description, is_migrated, migrated_at, migration_notes, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             ON CONFLICT (name) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                description = EXCLUDED.description,
                is_migrated = EXCLUDED.is_migrated,
                migrated_at = EXCLUDED.migrated_at,
                migration_notes = EXCLUDED.migration_notes,
                updated_at = NOW()",
        )
        .bind(&saga_type.name)
        .bind(&saga_type.display_name)
        .bind(&saga_type.description)
        .bind(saga_type.is_migrated)
        .bind(migrated_at)
        .bind(notes)
        .execute(&*self.pool)
        .await;

        if let Err(e) = result {
            warn!(error = %e, saga_type = %saga_type.name, "Failed to register saga type");
        } else {
            info!(saga_type = %saga_type.name, "Registered saga type");
        }
    }

    async fn get(&self, name: &str) -> Option<SagaTypeInfo> {
        let result = sqlx::query(
            "SELECT name, display_name, description, is_migrated, migrated_at, migration_notes
             FROM saga_type_info WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&*self.pool)
        .await
        .ok()?;

        let row = result?;
        let migrated_at: Option<i64> = row.try_get("migrated_at").ok().flatten();

        Some(SagaTypeInfo {
            name: row.try_get("name").ok()?,
            display_name: row.try_get("display_name").ok()?,
            description: row.try_get("description").ok()?,
            is_migrated: row.try_get("is_migrated").ok()?,
            migrated_at: migrated_at
                .map(|ts| chrono::DateTime::from_timestamp_millis(ts))
                .flatten(),
            migration_notes: row.try_get("migration_notes").ok()?,
        })
    }

    async fn get_all(&self) -> Vec<SagaTypeInfo> {
        let rows = sqlx::query(
            "SELECT name, display_name, description, is_migrated, migrated_at, migration_notes
             FROM saga_type_info ORDER BY name",
        )
        .fetch_all(&*self.pool)
        .await
        .ok()
        .unwrap_or_default();

        rows.into_iter()
            .filter_map(|row| {
                let name: String = row.try_get("name").ok()?;
                let display_name: String = row.try_get("display_name").ok()?;
                let description: String = row.try_get("description").ok()?;
                let is_migrated: bool = row.try_get("is_migrated").ok()?;
                let migrated_at: Option<i64> = row.try_get("migrated_at").ok().flatten();
                let migration_notes: Option<String> = row.try_get("migration_notes").ok()?;

                Some(SagaTypeInfo {
                    name,
                    display_name,
                    description,
                    is_migrated,
                    migrated_at: migrated_at
                        .map(|ts| chrono::DateTime::from_timestamp_millis(ts))
                        .flatten(),
                    migration_notes,
                })
            })
            .collect()
    }

    async fn is_registered(&self, name: &str) -> bool {
        sqlx::query("SELECT 1 FROM saga_type_info WHERE name = $1")
            .bind(name)
            .fetch_optional(&*self.pool)
            .await
            .ok()
            .flatten()
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    #[ignore]
    async fn test_postgres_migration_helper() {
        let pool = PgPoolOptions::new()
            .connect("postgres://user:pass@localhost/test")
            .await
            .unwrap();

        let helper = PostgresMigrationHelper::new(Arc::new(pool)).await.unwrap();
        let decision = helper.decide("provisioning").await;
        assert!(!decision.should_use_v4);
    }

    #[tokio::test]
    #[ignore]
    async fn test_gradual_migration_deterministic() {
        let pool = PgPoolOptions::new()
            .connect("postgres://user:pass@localhost/test")
            .await
            .unwrap();

        let helper = PostgresMigrationHelper::with_config(
            Arc::new(pool),
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::GradualMigration {
                    v4_percentage: 50,
                    seed: 12345,
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let first = helper.decide("deterministic-saga").await;
        let second = helper.decide("deterministic-saga").await;
        assert_eq!(first.should_use_v4, second.should_use_v4);
    }
}

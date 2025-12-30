//! Centralized database migration service for Hodei Jobs Platform
//!
//! This module provides a unified migration system that:
//! - Executes SQL migrations from the migrations/ directory
//! - Supports embedded migrations as fallback
//! - Provides history, validation, and rollback capabilities
//!
//! # Usage
//!
//! ```rust
//! use hodei_server_infrastructure::persistence::postgres::migrations::MigrationService;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), anyhow::Error> {
//!     let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
//!
//!     let service = MigrationService::new(pool, MigrationConfig::default());
//!     service.run_all().await?;
//!
//!     Ok(())
//! }
//! ```

use sqlx::Executor;
use sqlx::migrate::{Migrate, MigrateError};
use sqlx::postgres::PgPool;
use std::path::Path;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Configuration for the migration service
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Path to the migrations directory
    pub migrations_path: String,
    /// Enable embedded migrations as fallback
    pub embed_fallback: bool,
    /// Log each migration execution
    pub log_migrations: bool,
    /// Panic on missing migrations
    pub fail_on_missing: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            migrations_path: "migrations".to_string(),
            embed_fallback: true,
            log_migrations: true,
            fail_on_missing: false,
        }
    }
}

impl MigrationConfig {
    /// Create a new configuration with the specified migrations path
    pub fn new<P: Into<String>>(migrations_path: P) -> Self {
        Self {
            migrations_path: migrations_path.into(),
            ..Default::default()
        }
    }

    /// Set the database URL for testing
    #[allow(dead_code)]
    pub fn with_database_url(mut self, _url: &str) -> Self {
        // This is handled at the pool level, not config
        self
    }
}

/// Result of a migration execution
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// Version number (from filename)
    pub version: i64,
    /// Description (from filename)
    pub description: String,
    /// Timestamp when executed
    pub executed_at: chrono::DateTime<chrono::Utc>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether migration was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Errors that can occur during migration operations
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Database migration error: {source}")]
    Migration { source: MigrateError },

    #[error("Migration directory not found: {path}")]
    DirectoryNotFound { path: String },

    #[error("Migration file not found: {name}")]
    FileNotFound { name: String },

    #[error("Invalid migration file format: {name}")]
    InvalidFormat { name: String },

    #[error("Migration version conflict: {version}")]
    VersionConflict { version: String },

    #[error("No migrations found in {path}")]
    NoMigrations { path: String },

    #[error("Database connection error: {source}")]
    Connection { source: sqlx::Error },
}

/// Centralized migration service
#[derive(Clone)]
pub struct MigrationService {
    pool: PgPool,
    config: MigrationConfig,
}

impl MigrationService {
    /// Create a new migration service
    pub fn new(pool: PgPool, config: MigrationConfig) -> Self {
        Self { pool, config }
    }

    /// Run all pending migrations
    pub async fn run_all(&self) -> Result<Vec<MigrationResult>, MigrationError> {
        let mut results = Vec::new();

        // Run migrations from files
        let file_results = self.run_file_migrations().await?;
        results.extend(file_results);

        // Run embedded migrations as fallback
        if self.config.embed_fallback {
            let embed_results = self.run_embedded_migrations().await?;
            results.extend(embed_results);
        }

        Ok(results)
    }

    /// Run migrations from SQL files in the migrations directory
    async fn run_file_migrations(&self) -> Result<Vec<MigrationResult>, MigrationError> {
        let path = Path::new(&self.config.migrations_path);

        if !path.exists() {
            if self.config.fail_on_missing {
                return Err(MigrationError::DirectoryNotFound {
                    path: self.config.migrations_path.clone(),
                });
            }
            if self.config.log_migrations {
                warn!(
                    "Migration directory not found: {}, skipping file migrations",
                    self.config.migrations_path
                );
            }
            return Ok(Vec::new());
        }

        // Collect migration files
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| MigrationError::DirectoryNotFound {
                path: e.to_string(),
            })?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().map(|e| e == "sql").unwrap_or(false))
            .collect();

        if entries.is_empty() {
            if self.config.log_migrations {
                warn!(
                    "No SQL migration files found in {}",
                    self.config.migrations_path
                );
            }
            return Ok(Vec::new());
        }

        // Sort by filename (which includes version)
        entries.sort();

        // Group by migration (up.sql + optional down.sql)
        let migrations = Self::group_migrations(&entries);

        if self.config.log_migrations {
            info!("Found {} migration(s) to apply", migrations.len());
        }

        let mut results = Vec::new();

        for (name, _up_path, _down_path) in migrations {
            let result = self.apply_migration(&name).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Group migration files by migration name
    fn group_migrations(
        entries: &[std::path::PathBuf],
    ) -> Vec<(String, std::path::PathBuf, Option<std::path::PathBuf>)> {
        let mut migrations = Vec::new();

        for entry in entries {
            let filename = entry.file_stem().unwrap_or_default().to_string_lossy();
            let extension = entry.extension().unwrap_or_default().to_string_lossy();

            // Only process files ending in _up.sql or files without _up/_down suffix
            if extension == "sql" {
                let is_up = filename.ends_with("_up");
                let base_name = if is_up {
                    &filename[..filename.len() - 3] // Remove "_up"
                } else {
                    &filename
                };

                // Check if this is a paired migration
                let up_path = if is_up {
                    entry.clone()
                } else {
                    std::path::PathBuf::from(format!("{}_up.sql", base_name))
                };

                let down_path = std::path::PathBuf::from(format!("{}_down.sql", base_name));

                // Only add if up.sql exists
                if up_path.exists() {
                    migrations.push((base_name.to_string(), up_path, Some(down_path)));
                }
            }
        }

        migrations
    }

    /// Apply a single migration
    async fn apply_migration(&self, name: &str) -> Result<MigrationResult, MigrationError> {
        let up_path = format!("{}/{}_up.sql", self.config.migrations_path, name);
        let path = Path::new(&up_path);

        if !path.exists() {
            return Err(MigrationError::FileNotFound { name: up_path });
        }

        let start = std::time::Instant::now();
        let sql = std::fs::read_to_string(path)
            .map_err(|e| MigrationError::FileNotFound { name: up_path })?;

        // Extract version from name
        let version = Self::extract_version(name);

        // Execute the migration - use as_str() to avoid sqlx Execute trait issues
        match self.pool.execute(sql.as_str()).await {
            Ok(_) => {
                let duration = start.elapsed();

                if self.config.log_migrations {
                    info!(
                        "Migration {} applied successfully in {}ms",
                        name,
                        duration.as_millis()
                    );
                }

                Ok(MigrationResult {
                    version,
                    description: name.to_string(),
                    executed_at: chrono::Utc::now(),
                    duration_ms: duration.as_millis() as u64,
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                let duration = start.elapsed();

                if self.config.log_migrations {
                    warn!(
                        "Migration {} failed after {}ms: {}",
                        name,
                        duration.as_millis(),
                        e
                    );
                }

                Ok(MigrationResult {
                    version,
                    description: name.to_string(),
                    executed_at: chrono::Utc::now(),
                    duration_ms: duration.as_millis() as u64,
                    success: false,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    /// Run embedded migrations (defined in Rust code as fallback)
    async fn run_embedded_migrations(&self) -> Result<Vec<MigrationResult>, MigrationError> {
        // For now, we don't have embedded migrations
        // This can be extended to include critical migrations
        // that must always run regardless of file presence
        Ok(Vec::new())
    }

    /// Extract version number from migration filename
    fn extract_version(name: &str) -> i64 {
        // Format: YYYYMMDDHHMMSS_description
        let parts: Vec<&str> = name.split('_').collect();
        if parts.is_empty() {
            return 0;
        }

        let version_str = parts[0];
        version_str.parse().unwrap_or(0)
    }

    /// Get migration history from the database
    pub async fn history(&self) -> Result<Vec<MigrationResult>, MigrationError> {
        // Use query_as without ! macro to avoid sqlx offline compilation issues
        #[derive(sqlx::FromRow, Debug)]
        struct MigrationRow {
            version: i64,
            description: Option<String>,
            installed_on: chrono::DateTime<chrono::Utc>,
            execution_time: Option<i64>,
            success: Option<bool>,
        }

        let rows: Vec<MigrationRow> = sqlx::query_as(
            r#"
            SELECT version, description, installed_on, execution_time, success
            FROM __sqlx_migrations
            ORDER BY installed_on ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MigrationError::Migration { source: e.into() })?;

        Ok(rows
            .into_iter()
            .map(|row| MigrationResult {
                version: row.version,
                description: row.description.unwrap_or_default(),
                executed_at: row.installed_on,
                duration_ms: row.execution_time.unwrap_or(0) as u64,
                success: row.success.unwrap_or(true),
                error: if row.success == Some(false) {
                    Some("Migration failed".to_string())
                } else {
                    None
                },
            })
            .collect())
    }

    /// Validate that all expected migrations have been applied
    pub async fn validate(&self) -> Result<SchemaValidationResult, MigrationError> {
        let history = self.history().await?;
        let expected_count = self.count_expected_migrations()?;

        Ok(SchemaValidationResult {
            applied_migrations: history.len(),
            expected_migrations: expected_count,
            pending_migrations: expected_count.saturating_sub(history.len()),
            failed_migrations: history.iter().filter(|h| !h.success).count(),
            last_migration: history.last().cloned(),
            is_valid: history.len() == expected_count && history.iter().all(|h| h.success),
        })
    }

    /// Count expected migrations from the migrations directory
    fn count_expected_migrations(&self) -> Result<usize, MigrationError> {
        let path = Path::new(&self.config.migrations_path);

        if !path.exists() {
            return Ok(0);
        }

        let count = std::fs::read_dir(path)
            .map_err(|e| MigrationError::DirectoryNotFound {
                path: e.to_string(),
            })?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().map(|e| e == "sql").unwrap_or(false)
                    && p.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .ends_with("_up")
            })
            .count();

        Ok(count)
    }

    /// Revert the last applied migration
    pub async fn rollback(&self) -> Result<(), MigrationError> {
        // Get the last applied migration
        let history = self.history().await?;
        let last = history.last().ok_or_else(|| MigrationError::NoMigrations {
            path: self.config.migrations_path.clone(),
        })?;

        // Look for down.sql
        let down_path = format!(
            "{}/{}_down.sql",
            self.config.migrations_path, last.description
        );

        let path = Path::new(&down_path);
        if !path.exists() {
            return Err(MigrationError::FileNotFound {
                name: format!("{}_down.sql", last.description),
            });
        }

        let sql = std::fs::read_to_string(path)
            .map_err(|e| MigrationError::FileNotFound { name: down_path })?;

        // Execute rollback - use as_str() to avoid sqlx Execute trait issues
        self.pool
            .execute(sql.as_str())
            .await
            .map_err(|e| MigrationError::Migration { source: e.into() })?;

        if self.config.log_migrations {
            info!("Rolled back migration: {}", last.description);
        }

        Ok(())
    }

    /// Drop all tables and re-apply migrations (DANGEROUS!)
    pub async fn reset(&self) -> Result<(), MigrationError> {
        // First, revert all migrations
        let history = self.history().await?;

        for migration in history.iter().rev() {
            let down_path = format!(
                "{}/{}_down.sql",
                self.config.migrations_path, migration.description
            );

            let path = Path::new(&down_path);
            if path.exists() {
                let sql = std::fs::read_to_string(path)
                    .map_err(|e| MigrationError::FileNotFound { name: down_path })?;

                // Execute rollback - use as_str() to avoid sqlx Execute trait issues
                self.pool
                    .execute(sql.as_str())
                    .await
                    .map_err(|e| MigrationError::Migration { source: e.into() })?;

                if self.config.log_migrations {
                    info!("Rolled back: {}", migration.description);
                }
            }
        }

        // Run all migrations again
        self.run_all().await?;

        Ok(())
    }

    /// Check if database is ready for migrations
    pub async fn check_connection(&self) -> Result<bool, MigrationError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| true)
            .map_err(|e| MigrationError::Connection { source: e })
    }
}

/// Result of schema validation
#[derive(Debug, Clone)]
pub struct SchemaValidationResult {
    /// Number of migrations applied
    pub applied_migrations: usize,
    /// Number of expected migrations
    pub expected_migrations: usize,
    /// Number of pending migrations
    pub pending_migrations: usize,
    /// Number of failed migrations
    pub failed_migrations: usize,
    /// Last applied migration
    pub last_migration: Option<MigrationResult>,
    /// Whether schema is valid
    pub is_valid: bool,
}

impl SchemaValidationResult {
    /// Returns a summary string
    pub fn summary(&self) -> String {
        if self.is_valid {
            format!(
                "Schema valid: {}/{} migrations applied",
                self.applied_migrations, self.expected_migrations
            )
        } else {
            format!(
                "Schema INVALID: {}/{} applied, {} pending, {} failed",
                self.applied_migrations,
                self.expected_migrations,
                self.pending_migrations,
                self.failed_migrations
            )
        }
    }
}

/// Helper function to run migrations on application startup
pub async fn run_migrations(pool: &PgPool) -> Result<SchemaValidationResult, MigrationError> {
    let config = MigrationConfig::default();
    let service = MigrationService::new(pool.clone(), config);

    // Check connection first
    if !service.check_connection().await? {
        return Err(MigrationError::Connection {
            source: sqlx::Error::Configuration("Database not connected".into()),
        });
    }

    // Run all migrations
    let results = service.run_all().await?;

    // Log results
    for result in &results {
        if result.success {
            debug!("Migration {} applied", result.description);
        } else {
            warn!(
                "Migration {} failed: {:?}",
                result.description, result.error
            );
        }
    }

    // Validate schema
    let validation = service.validate().await?;

    if validation.is_valid {
        info!("{}", validation.summary());
    } else {
        warn!("{}", validation.summary());
    }

    Ok(validation)
}

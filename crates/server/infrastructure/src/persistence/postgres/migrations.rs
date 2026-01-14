//! Centralized database migration service for Hodei Jobs Platform
//!
//! This module provides a unified migration system that:
//! - Executes SQL migrations from the migrations/ directory
//! - Supports embedded migrations as fallback
//! - Provides history, validation, and rollback capabilities
//!
//! # Usage
//!
//! ```ignore
//! use hodei_server_infrastructure::persistence::postgres::migrations::MigrationService;
//! use hodei_server_infrastructure::persistence::MigrationConfig;
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
    /// Paths to the migrations directories (comma-separated or multiple)
    pub migrations_paths: Vec<String>,
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
            migrations_paths: vec!["migrations".to_string()],
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
            migrations_paths: vec![migrations_path.into()],
            ..Default::default()
        }
    }

    /// Add an additional migrations path
    pub fn with_additional_path<P: Into<String>>(mut self, path: P) -> Self {
        self.migrations_paths.push(path.into());
        self
    }

    /// Create with multiple migrations paths
    pub fn with_paths(paths: Vec<String>) -> Self {
        Self {
            migrations_paths: paths,
            ..Default::default()
        }
    }

    /// Load migrations path from environment variable
    pub fn from_env() -> Self {
        let env_paths = std::env::var("HODEI_MIGRATIONS_PATH")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        if env_paths.is_empty() {
            Self::default()
        } else {
            Self {
                migrations_paths: env_paths,
                ..Default::default()
            }
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

    /// Run migrations from SQL files in all configured migration directories
    async fn run_file_migrations(&self) -> Result<Vec<MigrationResult>, MigrationError> {
        let mut all_entries: Vec<std::path::PathBuf> = Vec::new();

        // Collect migration files from all configured paths
        for migrations_path in &self.config.migrations_paths {
            let path = Path::new(migrations_path);

            tracing::debug!(
                "Checking migration path: {} (exists: {})",
                migrations_path,
                path.exists()
            );

            if !path.exists() {
                if self.config.fail_on_missing {
                    return Err(MigrationError::DirectoryNotFound {
                        path: migrations_path.clone(),
                    });
                }
                if self.config.log_migrations {
                    warn!(
                        "Migration directory not found: {}, skipping",
                        migrations_path
                    );
                }
                continue;
            }

            let entries: Vec<_> = std::fs::read_dir(path)
                .map_err(|e| MigrationError::DirectoryNotFound {
                    path: e.to_string(),
                })?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().map(|e| e == "sql").unwrap_or(false))
                .collect();

            info!(
                "Read {} entries from {}, first 5: {:?}",
                entries.len(),
                migrations_path,
                entries
                    .iter()
                    .take(5)
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            );

            if self.config.log_migrations && !entries.is_empty() {
                info!("Found {} SQL files in {}", entries.len(), migrations_path);
            }

            all_entries.extend(entries);
        }

        if all_entries.is_empty() {
            if self.config.log_migrations {
                warn!("No SQL migration files found in any configured path");
            }
            return Ok(Vec::new());
        }

        // Sort by filename (which includes version) to ensure correct order
        all_entries.sort_by(|a, b| {
            a.file_name()
                .unwrap_or_default()
                .cmp(b.file_name().unwrap_or_default())
        });

        // Apply each migration file directly
        info!("Applying {} migration files directly", all_entries.len());

        let mut results = Vec::new();

        for file_path in &all_entries {
            let name = file_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            info!("Applying migration: {} from {}", name, file_path.display());

            // Skip if file doesn't exist
            if !file_path.exists() {
                warn!("Migration file not found: {}", file_path.display());
                continue;
            }

            // Read and execute SQL
            let sql = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "Failed to read migration file {}: {}",
                        file_path.display(),
                        e
                    );
                    continue;
                }
            };

            let start = std::time::Instant::now();
            match self.pool.execute(sql.as_str()).await {
                Ok(_) => {
                    let duration = start.elapsed();
                    info!(
                        "Migration {} applied successfully in {}ms",
                        name,
                        duration.as_millis()
                    );
                    results.push(MigrationResult {
                        version: Self::extract_version(&name),
                        description: name,
                        executed_at: chrono::Utc::now(),
                        duration_ms: duration.as_millis() as u64,
                        success: true,
                        error: None,
                    });
                }
                Err(e) => {
                    let duration = start.elapsed();
                    warn!(
                        "Migration {} failed after {}ms: {}",
                        name,
                        duration.as_millis(),
                        e
                    );
                    results.push(MigrationResult {
                        version: Self::extract_version(&name),
                        description: name,
                        executed_at: chrono::Utc::now(),
                        duration_ms: duration.as_millis() as u64,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Group migration files by migration name
    fn group_migrations(
        entries: &[std::path::PathBuf],
    ) -> Vec<(String, std::path::PathBuf, Option<std::path::PathBuf>)> {
        info!("group_migrations called with {} entries", entries.len());

        let mut migrations = Vec::new();

        for (i, entry) in entries.iter().enumerate() {
            let path_str = entry.display().to_string();
            let filename_os = entry.file_name();
            let filename = filename_os.unwrap_or_default().to_string_lossy();
            let stem = entry.file_stem().unwrap_or_default().to_string_lossy();
            let extension = entry.extension().unwrap_or_default().to_string_lossy();

            info!(
                "Entry {}: path={}, filename={}, stem={}, ext={}",
                i, path_str, filename, stem, extension
            );

            // Only process .sql files
            if extension != "sql" {
                info!("  -> Skipping (not .sql)");
                continue;
            }

            // Check if this is a paired migration (xxx_up.sql + xxx_down.sql)
            // or a standalone migration (xxx.sql)
            let ends_up = stem.ends_with("_up");
            let ends_down = stem.ends_with("_down");

            info!("  -> ends_up={}, ends_down={}", ends_up, ends_down);

            if ends_up {
                // This is an explicit _up migration, look for _down pair
                let base_name = &stem[..stem.len() - 3]; // Remove "_up"
                let up_path = entry.clone();
                let down_path = std::path::PathBuf::from(format!("{}_down.sql", base_name));
                migrations.push((base_name.to_string(), up_path, Some(down_path)));
                info!("  -> Added as up/down pair: {}", base_name);
            } else if !ends_down {
                // This is a standalone migration (no _up or _down suffix)
                let up_path = entry.clone();
                let down_path = std::path::PathBuf::from(format!("{}_down.sql", stem));
                migrations.push((stem.to_string(), up_path, Some(down_path)));
                info!("  -> Added as standalone: {}", stem);
            } else {
                info!("  -> Skipping _down file");
            }
        }

        info!("group_migrations returning {} migrations", migrations.len());
        migrations
    }

    /// Apply a single migration from a specific file path
    async fn apply_migration_from_path(
        &self,
        name: &str,
        file_path: &std::path::PathBuf,
    ) -> Result<MigrationResult, MigrationError> {
        if !file_path.exists() {
            return Err(MigrationError::FileNotFound {
                name: file_path.display().to_string(),
            });
        }

        let start = std::time::Instant::now();
        let sql = std::fs::read_to_string(file_path).map_err(|_| MigrationError::FileNotFound {
            name: file_path.display().to_string(),
        })?;

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

        // Get migration history - if table doesn't exist, return empty (first run)
        // Note: The migrations table is created by the first migration (00000000000000)
        let result = sqlx::query_as::<_, MigrationRow>(
            r#"
            SELECT version, description, installed_on, execution_time, success
            FROM __sqlx_migrations
            ORDER BY installed_on ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await;

        let rows = match result {
            Ok(rows) => rows,
            Err(sqlx::Error::Database(e)) if e.message().contains("does not exist") => {
                // Table doesn't exist yet (first run), return empty history
                vec![]
            }
            Err(e) => return Err(MigrationError::Migration { source: e.into() }),
        };

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

    /// Count expected migrations from all configured migration directories
    fn count_expected_migrations(&self) -> Result<usize, MigrationError> {
        let mut total_count = 0;

        for migrations_path in &self.config.migrations_paths {
            let path = Path::new(migrations_path);

            if !path.exists() {
                continue;
            }

            let count = std::fs::read_dir(path)
                .map_err(|e| MigrationError::DirectoryNotFound {
                    path: e.to_string(),
                })?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().map(|e| e == "sql").unwrap_or(false))
                .count();

            total_count += count;
        }

        Ok(total_count)
    }

    /// Find a migration file across all configured paths
    fn find_migration_file(&self, filename: &str) -> Option<std::path::PathBuf> {
        for migrations_path in &self.config.migrations_paths {
            let file_path = Path::new(migrations_path).join(filename);
            if file_path.exists() {
                return Some(file_path);
            }
        }
        None
    }

    /// Revert the last applied migration
    pub async fn rollback(&self) -> Result<(), MigrationError> {
        // Get the last applied migration
        let history = self.history().await?;
        let last = history.last().ok_or_else(|| MigrationError::NoMigrations {
            path: self.config.migrations_paths.join(", "),
        })?;

        // Look for down.sql in all configured paths
        let down_filename = format!("{}_down.sql", last.description);
        let down_path = self.find_migration_file(&down_filename).ok_or_else(|| {
            MigrationError::FileNotFound {
                name: down_filename.clone(),
            }
        })?;

        let sql =
            std::fs::read_to_string(&down_path).map_err(|_| MigrationError::FileNotFound {
                name: down_path.display().to_string(),
            })?;

        // Execute rollback
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
            let down_filename = format!("{}_down.sql", migration.description);

            if let Some(down_path) = self.find_migration_file(&down_filename) {
                let sql = std::fs::read_to_string(&down_path).map_err(|_| {
                    MigrationError::FileNotFound {
                        name: down_path.display().to_string(),
                    }
                })?;

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

/// Helper function to run migrations on application startup.
/// Uses `HODEI_MIGRATIONS_PATH` environment variable if set,
/// otherwise falls back to default "migrations" path.
pub async fn run_migrations(pool: &PgPool) -> Result<SchemaValidationResult, MigrationError> {
    let config = MigrationConfig::from_env();
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

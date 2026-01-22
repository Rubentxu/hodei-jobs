use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use tracing::warn;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server_port")]
    pub port: u16,
    #[serde(default = "default_database_url")]
    pub database_url: Option<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_persistence_enabled")]
    pub log_persistence_enabled: bool,
    #[serde(default = "default_log_storage_backend")]
    pub log_storage_backend: String,
    #[serde(default = "default_log_persistence_path")]
    pub log_persistence_path: String,
    #[serde(default = "default_log_ttl_hours")]
    pub log_ttl_hours: u64,
    /// Feature flag for SagaContext V2 migration
    #[serde(default = "default_saga_v2_enabled")]
    pub saga_v2_enabled: bool,
    /// Percentage of sagas to use V2 (0-100) for gradual rollout
    #[serde(default = "default_saga_v2_percentage")]
    pub saga_v2_percentage: u8,
}

fn default_server_port() -> u16 {
    50051
}

fn default_database_url() -> Option<String> {
    None
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_persistence_enabled() -> bool {
    true
}

fn default_log_storage_backend() -> String {
    "local".to_string()
}

fn default_log_persistence_path() -> String {
    "/tmp/hodei-logs".to_string()
}

fn default_log_ttl_hours() -> u64 {
    168 // 7 days
}

fn default_saga_v2_enabled() -> bool {
    false // Disabled by default for gradual rollout
}

fn default_saga_v2_percentage() -> u8 {
    0 // 0% initially, will increase during migration
}

impl ServerConfig {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, config::ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = config::Config::builder()
            // Start with default values
            .set_default("port", 50051)?
            .set_default("database_url", None::<String>)?
            .set_default("log_level", "info")?
            .set_default("log_persistence_enabled", true)?
            .set_default("log_storage_backend", "local")?
            .set_default("log_persistence_path", "/tmp/hodei-logs")?
            .set_default("log_ttl_hours", 168)?
            // Feature flags defaults
            .set_default("saga_v2_enabled", false)?
            .set_default("saga_v2_percentage", 0u8)?
            // Merge with config file (if exists)
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name(&format!("config/{}", run_mode)).required(false))
            // Merge with environment variables (SERVER_...)
            .add_source(config::Environment::with_prefix("SERVER"))
            .build()?;

        s.try_deserialize()
    }

    /// Convert to log persistence configuration
    pub fn to_log_persistence_config(
        &self,
    ) -> hodei_server_interface::log_persistence::LogPersistenceConfig {
        use hodei_server_interface::log_persistence::{
            LocalStorageConfig, LogPersistenceConfig, StorageBackend,
        };

        let storage_backend = match self.log_storage_backend.to_lowercase().as_str() {
            "local" | "file" | "filesystem" => StorageBackend::Local(LocalStorageConfig {
                base_path: PathBuf::from(self.log_persistence_path.clone()),
            }),
            // Future implementations:
            // "s3" => StorageBackend::S3(S3StorageConfig { ... }),
            // "azure" => StorageBackend::AzureBlob(AzureBlobConfig { ... }),
            // "gcs" => StorageBackend::Gcs(GcsConfig { ... }),
            _ => {
                // Default to local if unknown backend specified
                warn!(
                    "Unknown log storage backend '{}', defaulting to 'local'",
                    self.log_storage_backend
                );
                StorageBackend::Local(LocalStorageConfig {
                    base_path: PathBuf::from(self.log_persistence_path.clone()),
                })
            }
        };

        LogPersistenceConfig::new(
            self.log_persistence_enabled,
            storage_backend,
            self.log_ttl_hours,
        )
    }

    /// Check if SagaContext V2 should be used for a specific saga
    ///
    /// Uses consistent hashing based on saga ID to determine whether to use V2.
    /// This ensures that the same saga always uses the same version during
    /// gradual rollout.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga ID to check
    ///
    /// # Returns
    ///
    /// * `true` if V2 should be used, `false` for V1
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::collections::hash_map::DefaultHasher;
    /// use std::hash::{Hash, Hasher};
    ///
    /// let saga_id = "saga-123";
    /// if config.should_use_saga_v2(saga_id) {
    ///     // Use SagaContextV2
    /// } else {
    ///     // Use legacy SagaContext
    /// }
    /// ```
    pub fn should_use_saga_v2(&self, saga_id: &str) -> bool {
        if !self.saga_v2_enabled {
            return false;
        }

        if self.saga_v2_percentage == 0 {
            return false;
        }

        if self.saga_v2_percentage >= 100 {
            return true;
        }

        // Use consistent hashing for gradual rollout
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        saga_id.hash(&mut hasher);
        let hash = hasher.finish();

        // Map hash to 0-99 range
        let bucket = (hash % 100) as u8;
        bucket < self.saga_v2_percentage
    }
}

// =============================================================================
// Migration Config Implementation
// =============================================================================

/// Implement MigrationConfig trait for ServerConfig
///
/// This allows ServerConfig to be used with the saga context migration module
/// for gradual rollout of SagaContext V2.
impl hodei_server_domain::saga::context_migration::MigrationConfig for ServerConfig {
    fn should_use_saga_v2(&self, saga_id: &str) -> bool {
        self.should_use_saga_v2(saga_id)
    }

    fn v2_percentage(&self) -> u8 {
        self.saga_v2_percentage
    }
}

// =============================================================================
// Feature Flags Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_saga_v2_disabled() {
        let config = ServerConfig {
            port: 50051,
            database_url: None,
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/hodei-logs".to_string(),
            log_ttl_hours: 168,
            saga_v2_enabled: false,
            saga_v2_percentage: 0,
        };

        assert!(!config.should_use_saga_v2("saga-123"));
    }

    #[test]
    fn test_should_use_saga_v2_zero_percentage() {
        let config = ServerConfig {
            port: 50051,
            database_url: None,
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/hodei-logs".to_string(),
            log_ttl_hours: 168,
            saga_v2_enabled: true,
            saga_v2_percentage: 0,
        };

        assert!(!config.should_use_saga_v2("saga-123"));
    }

    #[test]
    fn test_should_use_saga_v2_hundred_percentage() {
        let config = ServerConfig {
            port: 50051,
            database_url: None,
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/hodei-logs".to_string(),
            log_ttl_hours: 168,
            saga_v2_enabled: true,
            saga_v2_percentage: 100,
        };

        assert!(config.should_use_saga_v2("saga-123"));
    }

    #[test]
    fn test_should_use_saga_v2_consistent_hashing() {
        let config = ServerConfig {
            port: 50051,
            database_url: None,
            log_level: "info".to_string(),
            log_persistence_enabled: true,
            log_storage_backend: "local".to_string(),
            log_persistence_path: "/tmp/hodei-logs".to_string(),
            log_ttl_hours: 168,
            saga_v2_enabled: true,
            saga_v2_percentage: 50,
        };

        // Same saga ID should always produce the same result
        let result1 = config.should_use_saga_v2("saga-123");
        let result2 = config.should_use_saga_v2("saga-123");
        assert_eq!(result1, result2);

        // Different saga IDs may produce different results
        let result3 = config.should_use_saga_v2("saga-456");
        // We can't assert the exact value, but we can verify it's deterministic
        let result4 = config.should_use_saga_v2("saga-456");
        assert_eq!(result3, result4);
    }
}

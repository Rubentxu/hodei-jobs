use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use tracing::warn;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server_port")]
    pub port: u16,
    pub database_url: String,
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
}

#[allow(dead_code)]
fn default_server_port() -> u16 {
    50051
}

#[allow(dead_code)]
fn default_log_level() -> String {
    "info".to_string()
}

#[allow(dead_code)]
fn default_log_persistence_enabled() -> bool {
    true
}

#[allow(dead_code)]
fn default_log_storage_backend() -> String {
    "local".to_string()
}

#[allow(dead_code)]
fn default_log_persistence_path() -> String {
    "/var/log/hodei/jobs".to_string()
}

#[allow(dead_code)]
fn default_log_ttl_hours() -> u64 {
    168 // 7 days
}

impl ServerConfig {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, config::ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = config::Config::builder()
            // Start with default values
            .set_default("port", 50051)?
            .set_default("log_level", "info")?
            .set_default("log_persistence_enabled", true)?
            .set_default("log_storage_backend", "local")?
            .set_default("log_persistence_path", "/var/log/hodei/jobs")?
            .set_default("log_ttl_hours", 168)?
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
}

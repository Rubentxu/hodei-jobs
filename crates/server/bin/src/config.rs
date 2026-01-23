//! Server Configuration
//!
//! This module provides server configuration using the centralized configuration
//! from the `hodei-shared` crate. It wraps `ServerConfigDto` and provides
//! convenience methods for backward compatibility.

use std::path::PathBuf;
use tracing::warn;

use hodei_shared::config::{FeatureFlags, ServerConfigDto as SharedServerConfigDto};

/// Server configuration
///
/// This wraps the shared `ServerConfigDto` and provides convenience methods
/// for backward compatibility with existing code.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The underlying shared configuration DTO
    pub inner: SharedServerConfigDto,
}

impl ServerConfig {
    /// Load server configuration from environment variables
    ///
    /// This method uses the centralized ConfigLoader from hodei-shared.
    /// It reads configuration from:
    /// 1. Optional .env file (if HODEI_CONFIG_FILE is set)
    /// 2. Environment variables (HODEI_*)
    ///
    /// # Returns
    ///
    /// `Ok(ServerConfig)` if configuration is valid and complete
    /// `Err(ConfigError)` if required configuration is missing or invalid
    ///
    /// # Example
    ///
    /// ```ignore
    /// use hodei_server_bin::config::ServerConfig;
    ///
    /// // Set required environment variables first
    /// let config = ServerConfig::load()?;
    /// println!("Database: {}", config.inner.database.url);
    /// ```
    pub fn load() -> Result<Self, hodei_shared::ConfigError> {
        // Check for optional .env file path
        let env_file = std::env::var("HODEI_CONFIG_FILE").ok().map(PathBuf::from);

        let loader = hodei_shared::ConfigLoader::new(env_file);
        let dto = loader.load_server_config()?;

        Ok(Self { inner: dto })
    }

    /// Convert to log persistence configuration
    ///
    /// This method converts the logging configuration to the format expected
    /// by the log persistence service.
    pub fn to_log_persistence_config(
        &self,
    ) -> hodei_server_interface::log_persistence::LogPersistenceConfig {
        use hodei_server_interface::log_persistence::{
            LocalStorageConfig, LogPersistenceConfig, StorageBackend,
        };

        let storage_backend = match self.inner.logging.storage_backend.to_lowercase().as_str() {
            "local" | "file" | "filesystem" => StorageBackend::Local(LocalStorageConfig {
                base_path: self.inner.logging.persistence_path.clone(),
            }),
            // Future implementations:
            // "s3" => StorageBackend::S3(S3StorageConfig { ... }),
            // "azure" => StorageBackend::AzureBlob(AzureBlobConfig { ... }),
            // "gcs" => StorageBackend::Gcs(GcsConfig { ... }),
            _ => {
                // Default to local if unknown backend specified
                warn!(
                    "Unknown log storage backend '{}', defaulting to 'local'",
                    self.inner.logging.storage_backend
                );
                StorageBackend::Local(LocalStorageConfig {
                    base_path: self.inner.logging.persistence_path.clone(),
                })
            }
        };

        LogPersistenceConfig::new(
            self.inner.logging.persistence_enabled,
            storage_backend,
            self.inner.logging.ttl_hours,
        )
    }

    /// Check if SagaContext V2 should be used for a specific saga
    ///
    /// Uses consistent hashing based on saga ID to determine whether to use V2.
    /// Get the gRPC bind address
    pub fn grpc_bind_address(&self) -> &std::net::SocketAddr {
        &self.inner.grpc.bind_address
    }

    /// Get the worker connect URL
    pub fn worker_connect_url(&self) -> &str {
        &self.inner.grpc.worker_connect_url
    }

    /// Get the database URL
    pub fn database_url(&self) -> &str {
        &self.inner.database.url
    }

    /// Get the database pool size
    pub fn db_pool_size(&self) -> u32 {
        self.inner.database.pool_size
    }

    /// Get the database min idle connections
    pub fn db_min_idle(&self) -> u32 {
        self.inner.database.min_idle
    }

    /// Get the database connect timeout (seconds)
    pub fn db_connect_timeout_secs(&self) -> u64 {
        self.inner.database.connect_timeout_secs
    }

    /// Get the database idle timeout (seconds)
    pub fn db_idle_timeout_secs(&self) -> u64 {
        self.inner.database.idle_timeout_secs
    }

    /// Get the database max lifetime (seconds)
    pub fn db_max_lifetime_secs(&self) -> u64 {
        self.inner.database.max_lifetime_secs
    }

    /// Get the NATS URLs
    pub fn nats_urls(&self) -> &[String] {
        &self.inner.nats.urls
    }

    /// Get the NATS timeout (seconds)
    pub fn nats_timeout_secs(&self) -> u64 {
        self.inner.nats.timeout_secs
    }

    /// Get the worker image
    pub fn worker_image(&self) -> &str {
        &self.inner.workers.default_image
    }

    /// Get the OTP TTL (seconds)
    pub fn worker_otp_ttl_secs(&self) -> u64 {
        self.inner.workers.otp_ttl_secs
    }

    /// Check if provisioning is enabled
    pub fn provisioning_enabled(&self) -> bool {
        self.inner.workers.enabled
    }

    /// Get the log level
    pub fn log_level(&self) -> &str {
        &self.inner.logging.level
    }

    /// Check if dev mode is enabled
    pub fn dev_mode(&self) -> bool {
        self.inner.features.dev_mode
    }

    /// Get feature flags
    pub fn features(&self) -> &FeatureFlags {
        &self.inner.features
    }
}

// =============================================================================
// Feature Flags Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
}

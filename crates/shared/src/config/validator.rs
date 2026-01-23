//! Configuration validation
//!
//! This module provides validation logic for configuration DTOs.

use super::dto::ServerConfigDto;
use super::error::{ConfigError, Result};
use std::net::SocketAddr;

/// Validate a database URL format
///
/// # Arguments
///
/// * `url` - The database URL to validate
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_database_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(ConfigError::InvalidDatabaseUrl(
            "Database URL cannot be empty".to_string(),
        ));
    }

    // Check for postgres:// or postgresql:// prefix
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(ConfigError::InvalidDatabaseUrl(format!(
            "Database URL must start with postgres:// or postgresql://, got: {}",
            url
        )));
    }

    // Extract host:port part and validate port
    // URLs can be:
    // - postgresql://user:pass@host:port/db
    // - postgresql://host:port/db (no credentials)
    // - postgresql:///db (Unix socket)
    let host_part = if let Some(at_idx) = url.find('@') {
        // Has credentials: extract after @
        &url[at_idx + 1..]
    } else {
        // No credentials: extract after ://
        &url[url.find("://").unwrap() + 3..]
    };

    // Unix socket paths (start with /) are valid without port
    if host_part.starts_with('/') {
        return Ok(());
    }

    // For network URLs, require port
    if !host_part.contains(':') {
        return Err(ConfigError::InvalidDatabaseUrl(format!(
            "Database URL must include port for network connections, got: {}",
            url
        )));
    }

    if let Some(port_str) = host_part.split(':').nth(1) {
        if let Some(port) = port_str.split('/').next() {
            if let Err(_) = port.parse::<u16>() {
                return Err(ConfigError::InvalidDatabaseUrl(format!(
                    "Invalid port in database URL: {}",
                    port
                )));
            }
        }
    }

    Ok(())
}

/// Validate a gRPC bind address
///
/// # Arguments
///
/// * `addr` - The socket address to validate
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_bind_address(addr: &SocketAddr) -> Result<()> {
    // Check that port is not 0
    if addr.port() == 0 {
        return Err(ConfigError::Validation(
            "Bind address port cannot be 0".to_string(),
        ));
    }

    // Check that port is not in privileged range (1-1023) unless explicitly allowed
    if addr.port() < 1024 {
        return Err(ConfigError::Validation(format!(
            "Bind address port {} is in privileged range (1-1023). \
             Use a non-privileged port (1024-65535)",
            addr.port()
        )));
    }

    // Check that port is not in ephemeral range (49152-65535)
    // While technically valid, it's not recommended for server binding
    // Note: Ephemeral port warning removed to avoid tracing dependency

    Ok(())
}

/// Validate NATS URLs
///
/// # Arguments
///
/// * `urls` - Slice of NATS URLs to validate
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_nats_urls(urls: &[String]) -> Result<()> {
    if urls.is_empty() {
        return Err(ConfigError::Validation(
            "NATS URLs cannot be empty".to_string(),
        ));
    }

    for url in urls {
        if url.is_empty() {
            return Err(ConfigError::Validation(
                "NATS URL cannot be empty".to_string(),
            ));
        }

        // Must start with nats:// or tls://
        if !url.starts_with("nats://") && !url.starts_with("tls://") {
            return Err(ConfigError::Validation(format!(
                "NATS URL must start with nats:// or tls://, got: {}",
                url
            )));
        }

        // Must contain host:port
        let url_without_scheme = url
            .trim_start_matches("nats://")
            .trim_start_matches("tls://");

        if !url_without_scheme.contains(':') {
            return Err(ConfigError::Validation(format!(
                "NATS URL must include port (e.g., nats://host:4222), got: {}",
                url
            )));
        }
    }

    Ok(())
}

/// Validate database pool configuration
///
/// # Arguments
///
/// * `pool_size` - Maximum pool size
/// * `min_idle` - Minimum idle connections
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_pool_config(pool_size: u32, min_idle: u32) -> Result<()> {
    if pool_size == 0 {
        return Err(ConfigError::Validation(
            "Database pool size must be greater than 0".to_string(),
        ));
    }

    if min_idle > pool_size {
        return Err(ConfigError::Validation(format!(
            "Min idle connections ({}) cannot be greater than pool size ({})",
            min_idle, pool_size
        )));
    }

    // Note: Pool size warnings removed to avoid tracing dependency

    Ok(())
}

/// Validate worker provisioning configuration
///
/// # Arguments
///
/// * `default_image` - Default worker image
/// * `otp_ttl_secs` - OTP token TTL in seconds
/// * `max_workers` - Maximum workers per provider
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_worker_provisioning_config(
    default_image: &str,
    otp_ttl_secs: u64,
    max_workers: usize,
) -> Result<()> {
    if default_image.is_empty() {
        return Err(ConfigError::Validation(
            "Worker default image cannot be empty".to_string(),
        ));
    }

    if otp_ttl_secs < 60 {
        return Err(ConfigError::Validation(format!(
            "OTP TTL ({}) is too short. Minimum is 60 seconds.",
            otp_ttl_secs
        )));
    }

    // Note: TTL warning removed to avoid tracing dependency

    if max_workers == 0 {
        return Err(ConfigError::Validation(
            "Max workers per provider must be greater than 0".to_string(),
        ));
    }

    // Note: Max workers warning removed to avoid tracing dependency

    Ok(())
}

/// Validate the complete server configuration
///
/// # Arguments
///
/// * `config` - The server configuration to validate
///
/// # Returns
///
/// Ok(()) if valid, Err(ConfigError) if invalid
pub fn validate_server_config(config: &ServerConfigDto) -> Result<()> {
    // Validate database
    validate_database_url(&config.database.url)?;
    validate_pool_config(config.database.pool_size, config.database.min_idle)?;

    // Validate gRPC
    validate_bind_address(&config.grpc.bind_address)?;

    // Validate NATS
    validate_nats_urls(&config.nats.urls)?;

    // Validate worker provisioning
    validate_worker_provisioning_config(
        &config.workers.default_image,
        config.workers.otp_ttl_secs,
        config.workers.max_workers_per_provider,
    )?;

    // Validate logging
    if config.logging.persistence_enabled {
        if config.logging.persistence_path.as_os_str().is_empty() {
            return Err(ConfigError::Validation(
                "Log persistence path cannot be empty when persistence is enabled".to_string(),
            ));
        }

        if config.logging.ttl_hours == 0 {
            return Err(ConfigError::Validation(
                "Log TTL hours cannot be 0 when persistence is enabled".to_string(),
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::dto::{DatabaseConfig, GrpcServerConfig};
    use std::path::PathBuf;

    #[test]
    fn test_validate_database_url_valid() {
        assert!(validate_database_url("postgresql://localhost:5432/test").is_ok());
        assert!(validate_database_url("postgres://user:pass@host:5432/db").is_ok());
        assert!(validate_database_url("postgresql:///db_name").is_ok()); // Unix socket
    }

    #[test]
    fn test_validate_database_url_invalid() {
        assert!(validate_database_url("").is_err());
        assert!(validate_database_url("http://localhost/test").is_err());
        assert!(validate_database_url("postgresql://localhost").is_err()); // No port
    }

    #[test]
    fn test_validate_bind_address_valid() {
        let addr: SocketAddr = "0.0.0.0:9090".parse().unwrap();
        assert!(validate_bind_address(&addr).is_ok());

        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert!(validate_bind_address(&addr).is_ok());
    }

    #[test]
    fn test_validate_bind_address_invalid() {
        let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        assert!(validate_bind_address(&addr).is_err()); // Port 0

        let addr: SocketAddr = "0.0.0.0:1023".parse().unwrap();
        assert!(validate_bind_address(&addr).is_err()); // Privileged port
    }

    #[test]
    fn test_validate_nats_urls_valid() {
        assert!(validate_nats_urls(&["nats://localhost:4222".to_string()]).is_ok());
        assert!(
            validate_nats_urls(&[
                "nats://nats1:4222".to_string(),
                "nats://nats2:4222".to_string()
            ])
            .is_ok()
        );
    }

    #[test]
    fn test_validate_nats_urls_invalid() {
        assert!(validate_nats_urls(&[]).is_err());
        assert!(validate_nats_urls(&["".to_string()]).is_err());
        assert!(validate_nats_urls(&["http://localhost:4222".to_string()]).is_err());
    }

    #[test]
    fn test_validate_pool_config_valid() {
        assert!(validate_pool_config(50, 10).is_ok());
        assert!(validate_pool_config(10, 5).is_ok());
    }

    #[test]
    fn test_validate_pool_config_invalid() {
        assert!(validate_pool_config(0, 0).is_err()); // Pool size 0
        assert!(validate_pool_config(10, 20).is_err()); // Min > pool size
    }

    #[test]
    fn test_validate_worker_provisioning_config_valid() {
        assert!(validate_worker_provisioning_config("worker:latest", 300, 10).is_ok());
    }

    #[test]
    fn test_validate_worker_provisioning_config_invalid() {
        assert!(validate_worker_provisioning_config("", 300, 10).is_err()); // Empty image
        assert!(validate_worker_provisioning_config("worker:latest", 30, 10).is_err()); // TTL too short
        assert!(validate_worker_provisioning_config("worker:latest", 300, 0).is_err()); // Max workers 0
    }

    #[test]
    fn test_validate_server_config() {
        let config = ServerConfigDto {
            database: DatabaseConfig {
                url: "postgresql://localhost:5432/test".to_string(),
                pool_size: 50,
                min_idle: 10,
                connect_timeout_secs: 30,
                idle_timeout_secs: 600,
                max_lifetime_secs: 1800,
            },
            grpc: GrpcServerConfig {
                bind_address: "0.0.0.0:9090".parse().unwrap(),
                worker_connect_url: "http://localhost:9090".to_string(),
                enable_cors: true,
                enable_grpc_web: true,
            },
            workers: crate::config::dto::WorkerProvisioningConfig {
                default_image: "worker:latest".to_string(),
                otp_ttl_secs: 300,
                max_workers_per_provider: 10,
                enabled: true,
            },
            nats: crate::config::dto::NatsConfig {
                urls: vec!["nats://localhost:4222".to_string()],
                timeout_secs: 10,
                max_reconnects: Some(5),
            },
            logging: crate::config::dto::LoggingConfig {
                level: "info".to_string(),
                persistence_enabled: true,
                storage_backend: "local".to_string(),
                persistence_path: PathBuf::from("/tmp/logs"),
                ttl_hours: 168,
            },
            features: crate::config::dto::FeatureFlags { dev_mode: false },
        };

        assert!(validate_server_config(&config).is_ok());
    }

    #[test]
    fn test_validate_server_config_invalid() {
        let config = ServerConfigDto {
            database: DatabaseConfig {
                url: "invalid".to_string(), // Invalid
                pool_size: 50,
                min_idle: 10,
                connect_timeout_secs: 30,
                idle_timeout_secs: 600,
                max_lifetime_secs: 1800,
            },
            grpc: GrpcServerConfig {
                bind_address: "0.0.0.0:9090".parse().unwrap(),
                worker_connect_url: "http://localhost:9090".to_string(),
                enable_cors: true,
                enable_grpc_web: true,
            },
            workers: crate::config::dto::WorkerProvisioningConfig {
                default_image: "worker:latest".to_string(),
                otp_ttl_secs: 300,
                max_workers_per_provider: 10,
                enabled: true,
            },
            nats: crate::config::dto::NatsConfig {
                urls: vec!["nats://localhost:4222".to_string()],
                timeout_secs: 10,
                max_reconnects: Some(5),
            },
            logging: crate::config::dto::LoggingConfig {
                level: "info".to_string(),
                persistence_enabled: true,
                storage_backend: "local".to_string(),
                persistence_path: PathBuf::from("/tmp/logs"),
                ttl_hours: 168,
            },
            features: crate::config::dto::FeatureFlags { dev_mode: false },
        };

        assert!(validate_server_config(&config).is_err());
    }
}

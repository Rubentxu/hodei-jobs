//! Configuration Data Transfer Objects (DTOs)
//!
//! This module defines immutable configuration DTOs that are used throughout
//! the Hodei Jobs Platform. These DTOs provide a single source of truth for
//! all configuration and are passed to services via dependency injection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

// ============================================================================
// Server Configuration DTOs
// ============================================================================

/// Configuration DTO for Hodei Server
///
/// This is the single source of truth for all server configuration.
/// It is loaded once at startup and passed to all services via AppState.
///
/// # Example
///
/// ```ignore
/// use hodei_shared::config::ServerConfigDto;
///
/// let config = ServerConfigDto::from_env()?;
/// println!("Server will bind to: {}", config.grpc.bind_address);
/// println!("Workers should connect to: {}", config.grpc.worker_connect_url);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfigDto {
    /// Database configuration
    pub database: DatabaseConfig,

    /// gRPC server configuration
    pub grpc: GrpcServerConfig,

    /// Worker provisioning configuration
    pub workers: WorkerProvisioningConfig,

    /// NATS messaging configuration
    pub nats: NatsConfig,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// Feature flags
    pub features: FeatureFlags,
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection string
    /// Example: `postgresql://user:pass@host:5432/dbname`
    pub url: String,

    /// Maximum number of connections in the pool
    pub pool_size: u32,

    /// Minimum number of idle connections to maintain
    pub min_idle: u32,

    /// Timeout for establishing a new connection (seconds)
    pub connect_timeout_secs: u64,

    /// Timeout for idle connections before being closed (seconds)
    pub idle_timeout_secs: u64,

    /// Maximum lifetime of a connection before being closed (seconds)
    pub max_lifetime_secs: u64,
}

/// gRPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcServerConfig {
    /// Bind address for the gRPC server (e.g., "0.0.0.0:9090")
    /// This is the address the server listens on
    pub bind_address: SocketAddr,

    /// External address that workers should connect to
    /// Format: "http://host:port" or "https://host:port"
    /// This is the address workers use to connect to the server
    pub worker_connect_url: String,

    /// Enable CORS for web clients
    pub enable_cors: bool,

    /// Enable gRPC-Web for browser clients
    pub enable_grpc_web: bool,
}

/// Worker provisioning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProvisioningConfig {
    /// Default Docker image for workers
    /// Example: "hodei-jobs-worker:latest"
    pub default_image: String,

    /// OTP token TTL for worker authentication (seconds)
    /// Default: 300 (5 minutes)
    pub otp_ttl_secs: u64,

    /// Maximum workers per provider
    pub max_workers_per_provider: usize,

    /// Enable automatic worker provisioning
    pub enabled: bool,
}

/// NATS messaging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsConfig {
    /// NATS connection URLs
    /// Can be multiple URLs for clustering
    pub urls: Vec<String>,

    /// Connection timeout (seconds)
    pub timeout_secs: u64,

    /// Maximum number of reconnection attempts
    /// None = infinite retries
    pub max_reconnects: Option<u32>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,

    /// Enable log persistence to storage backend
    pub persistence_enabled: bool,

    /// Storage backend for logs (local, s3, azure, gcs)
    pub storage_backend: String,

    /// Path for local log storage
    pub persistence_path: PathBuf,

    /// Time-to-live for log files (hours)
    /// Default: 168 (7 days)
    pub ttl_hours: u64,
}

/// Feature flags for conditional behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// Development mode (enables additional debugging)
    pub dev_mode: bool,
}

// ============================================================================
// Worker Configuration DTOs
// ============================================================================

/// Configuration DTO for Worker
///
/// Workers receive their configuration via environment variables
/// that are injected by the server during provisioning.
/// This DTO is NOT loaded by workers - it's used by the server
/// to construct the WorkerSpec with proper environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigDto {
    /// Worker unique identifier
    pub worker_id: String,

    /// Worker display name
    pub worker_name: String,

    /// Server address (with http:// prefix) for gRPC connection
    pub server_address: String,

    /// OTP authentication token
    pub auth_token: String,

    /// Worker capabilities (e.g., ["shell", "docker"])
    pub capabilities: Vec<String>,

    /// Number of log entries to batch before sending
    pub log_batch_size: usize,

    /// Interval between log flushes (milliseconds)
    pub log_flush_interval_ms: u64,

    /// Cleanup timeout after job completion (milliseconds)
    /// 0 = disable auto-termination
    pub cleanup_timeout_ms: u64,

    /// TLS configuration (optional)
    pub tls: Option<TlsConfig>,
}

impl WorkerConfigDto {
    /// Convert to environment variable map for WorkerSpec
    ///
    /// This method generates the environment variables that will be
    /// injected into the worker container/pod by the provider.
    ///
    /// The worker receives:
    /// - HODEI_SERVER_ADDRESS: Server hostname for connection
    /// - HODEI_SERVER_PROTOCOL: Protocol (http/https)
    /// - HODEI_SERVER_PORT: Port extracted from bind address
    ///
    /// The worker composes: {protocol}://{hostname}:{port}
    ///
    /// # Returns
    ///
    /// A vector of (variable_name, value) tuples
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        // Extract components from server_address URL (e.g., "http://host:port")
        let (protocol, hostname, port) = parse_server_address(&self.server_address);

        let mut vars = vec![
            ("HODEI_WORKER_ID".to_string(), self.worker_id.clone()),
            ("HODEI_WORKER_NAME".to_string(), self.worker_name.clone()),
            // Worker receives individual components to compose URL
            ("HODEI_SERVER_ADDRESS".to_string(), hostname),
            ("HODEI_SERVER_PROTOCOL".to_string(), protocol),
            ("HODEI_SERVER_PORT".to_string(), port),
            ("HODEI_OTP_TOKEN".to_string(), self.auth_token.clone()),
            (
                "HODEI_LOG_BATCH_SIZE".to_string(),
                self.log_batch_size.to_string(),
            ),
            (
                "HODEI_LOG_FLUSH_INTERVAL".to_string(),
                self.log_flush_interval_ms.to_string(),
            ),
            (
                "HODEI_CLEANUP_TIMEOUT_MS".to_string(),
                self.cleanup_timeout_ms.to_string(),
            ),
        ];

        // Add TLS environment variables if configured
        if let Some(tls) = &self.tls {
            vars.push(("HODEI_CLIENT_CERT_PATH".to_string(), tls.cert_path.clone()));
            vars.push(("HODEI_CLIENT_KEY_PATH".to_string(), tls.key_path.clone()));
            vars.push(("HODEI_CA_CERT_PATH".to_string(), tls.ca_cert_path.clone()));
        }

        vars
    }

    /// Convert to HashMap for environment injection
    pub fn to_env_map(&self) -> HashMap<String, String> {
        self.to_env_vars().into_iter().collect()
    }
}

/// Parse server address URL into components
///
/// Extracts protocol, hostname, and port from a URL like "http://host:9090"
fn parse_server_address(url: &str) -> (String, String, String) {
    let mut protocol = "http".to_string();
    let mut hostname = url.to_string();
    let mut port = "9090".to_string();

    // Extract protocol
    if let Some(colon_pos) = url.find("://") {
        protocol = url[..colon_pos].to_string();
        hostname = url[colon_pos + 3..].to_string();
    }

    // Extract port from hostname:port
    if let Some(colon_pos) = hostname.find(':') {
        port = hostname[colon_pos + 1..].to_string();
        hostname = hostname[..colon_pos].to_string();
    }

    (protocol, hostname, port)
}

/// TLS configuration for mTLS worker connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to client certificate file
    pub cert_path: String,

    /// Path to client private key file
    pub key_path: String,

    /// Path to CA certificate file
    pub ca_cert_path: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config_to_env_vars() {
        let dto = WorkerConfigDto {
            worker_id: "worker-123".to_string(),
            worker_name: "Test Worker".to_string(),
            server_address: "http://server:9090".to_string(),
            auth_token: "otp-token-456".to_string(),
            capabilities: vec!["shell".to_string()],
            log_batch_size: 100,
            log_flush_interval_ms: 250,
            cleanup_timeout_ms: 30000,
            tls: None,
        };

        let vars = dto.to_env_vars();

        assert!(vars.contains(&("HODEI_WORKER_ID".to_string(), "worker-123".to_string())));
        // Worker receives hostname only (protocol and port are separate)
        assert!(vars.contains(&("HODEI_SERVER_ADDRESS".to_string(), "server".to_string())));
        assert!(vars.contains(&("HODEI_SERVER_PROTOCOL".to_string(), "http".to_string())));
        assert!(vars.contains(&("HODEI_SERVER_PORT".to_string(), "9090".to_string())));
        assert!(vars.contains(&("HODEI_OTP_TOKEN".to_string(), "otp-token-456".to_string())));
    }

    #[test]
    fn test_worker_config_with_tls() {
        let dto = WorkerConfigDto {
            worker_id: "worker-123".to_string(),
            worker_name: "Test Worker".to_string(),
            server_address: "http://server:9090".to_string(),
            auth_token: "otp-token-456".to_string(),
            capabilities: vec!["shell".to_string()],
            log_batch_size: 100,
            log_flush_interval_ms: 250,
            cleanup_timeout_ms: 30000,
            tls: Some(TlsConfig {
                cert_path: "/certs/client.crt".to_string(),
                key_path: "/certs/client.key".to_string(),
                ca_cert_path: "/certs/ca.crt".to_string(),
            }),
        };

        let vars = dto.to_env_vars();

        assert!(vars.contains(&(
            "HODEI_CLIENT_CERT_PATH".to_string(),
            "/certs/client.crt".to_string()
        )));
        assert!(vars.contains(&(
            "HODEI_CLIENT_KEY_PATH".to_string(),
            "/certs/client.key".to_string()
        )));
        assert!(vars.contains(&(
            "HODEI_CA_CERT_PATH".to_string(),
            "/certs/ca.crt".to_string()
        )));
    }

    #[test]
    fn test_worker_config_to_env_map() {
        let dto = WorkerConfigDto {
            worker_id: "worker-123".to_string(),
            worker_name: "Test Worker".to_string(),
            server_address: "http://server:9090".to_string(),
            auth_token: "otp-token-456".to_string(),
            capabilities: vec!["shell".to_string()],
            log_batch_size: 100,
            log_flush_interval_ms: 250,
            cleanup_timeout_ms: 30000,
            tls: None,
        };

        let map = dto.to_env_map();

        assert_eq!(map.get("HODEI_WORKER_ID"), Some(&"worker-123".to_string()));
        // Worker receives hostname only (protocol and port are separate env vars)
        assert_eq!(map.get("HODEI_SERVER_ADDRESS"), Some(&"server".to_string()));
        assert_eq!(map.get("HODEI_SERVER_PROTOCOL"), Some(&"http".to_string()));
        assert_eq!(map.get("HODEI_SERVER_PORT"), Some(&"9090".to_string()));
    }

    #[test]
    fn test_server_config_dto_serialization() {
        let config = ServerConfigDto {
            database: DatabaseConfig {
                url: "postgresql://localhost/test".to_string(),
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
            workers: WorkerProvisioningConfig {
                default_image: "worker:latest".to_string(),
                otp_ttl_secs: 300,
                max_workers_per_provider: 10,
                enabled: true,
            },
            nats: NatsConfig {
                urls: vec!["nats://localhost:4222".to_string()],
                timeout_secs: 10,
                max_reconnects: Some(5),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                persistence_enabled: true,
                storage_backend: "local".to_string(),
                persistence_path: PathBuf::from("/tmp/logs"),
                ttl_hours: 168,
            },
            features: FeatureFlags { dev_mode: false },
        };

        // Test serialization
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("postgresql://localhost/test"));

        // Test deserialization
        let deserialized: ServerConfigDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.database.url, "postgresql://localhost/test");
    }
}

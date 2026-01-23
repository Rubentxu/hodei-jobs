//! Configuration loader
//!
//! This module provides the ConfigLoader which is responsible for loading
//! configuration from .env files and environment variables.

use std::path::Path;

use super::dto::{
    DatabaseConfig, FeatureFlags, GrpcServerConfig, LoggingConfig, NatsConfig, ServerConfigDto,
    WorkerProvisioningConfig,
};
use super::error::{ConfigError, Result};
use super::validator::validate_server_config;

/// Configuration loader
///
/// This loader handles loading configuration from:
/// 1. .env file (optional, highest priority)
/// 2. Environment variables
///
/// # Priority
///
/// Values from .env file take precedence over environment variables.
/// This allows for easy local development overrides without modifying
/// the system environment.
///
/// # Example
///
/// ```ignore
/// use hodei_shared::config::ConfigLoader;
/// use std::path::PathBuf;
///
/// // Load from .env file in current directory
/// let loader = ConfigLoader::new(Some(PathBuf::from(".env")));
/// let config = loader.load_server_config()?;
/// ```
#[derive(Debug, Clone)]
pub struct ConfigLoader {
    /// Optional path to .env file
    env_file_path: Option<std::path::PathBuf>,
}

impl ConfigLoader {
    /// Create a new ConfigLoader
    ///
    /// # Arguments
    ///
    /// * `env_file_path` - Optional path to .env file. If provided, the file
    ///                     will be loaded before reading environment variables.
    ///
    /// # Example
    ///
    /// ```
    /// use hodei_shared::config::ConfigLoader;
    ///
    /// // Without .env file
    /// let loader = ConfigLoader::new(None);
    ///
    /// // With .env file
    /// let loader = ConfigLoader::new(Some(".env".into()));
    /// ```
    pub fn new(env_file_path: Option<std::path::PathBuf>) -> Self {
        Self { env_file_path }
    }

    /// Load server configuration
    ///
    /// This method loads configuration from:
    /// 1. .env file (if provided)
    /// 2. Environment variables
    ///
    /// # Returns
    ///
    /// `Ok(ServerConfigDto)` if configuration is valid and complete
    /// `Err(ConfigError)` if required configuration is missing or invalid
    ///
    /// # Example
    ///
    /// ```ignore
    /// let loader = ConfigLoader::new(Some(PathBuf::from(".env")));
    /// match loader.load_server_config() {
    ///     Ok(config) => println!("Loaded config: {:?}", config),
    ///     Err(e) => eprintln!("Configuration error: {}", e),
    /// }
    /// ```
    pub fn load_server_config(&self) -> Result<ServerConfigDto> {
        // 1. Load .env file if provided
        if let Some(path) = &self.env_file_path {
            self.load_env_file(path)?;
        }

        // 2. Build config from environment
        let config = ServerConfigDto::from_env()?;

        // 3. Validate configuration
        validate_server_config(&config)?;

        Ok(config)
    }

    /// Load .env file
    ///
    /// This method loads environment variables from the specified file.
    /// Variables defined in the file will be available via `std::env::var`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the .env file
    fn load_env_file(&self, path: &Path) -> Result<()> {
        // Check if file exists
        if !path.exists() {
            return Err(ConfigError::EnvFileLoad {
                path: path.to_path_buf(),
                source: dotenv::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("File not found: {}", path.display()),
                )),
            });
        }

        // Load .env file
        dotenv::from_path(path).map_err(|e| ConfigError::EnvFileLoad {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(())
    }
}

impl Default for ConfigLoader {
    /// Create a ConfigLoader without .env file support
    ///
    /// This is equivalent to `ConfigLoader::new(None)`
    fn default() -> Self {
        Self::new(None)
    }
}

// ============================================================================
// Implementation: ServerConfigDto::from_env
// ============================================================================

impl ServerConfigDto {
    /// Build server configuration from environment variables
    ///
    /// This method reads all required configuration from environment variables.
    /// It will fail with `ConfigError::MissingRequired` if any required
    /// variable is not set.
    ///
    /// # Required Environment Variables
    ///
    /// - `HODEI_DATABASE_URL`: PostgreSQL connection string
    /// - `HODEI_SERVER_BIND`: gRPC server bind address (e.g., "0.0.0.0:9090")
    /// - `HODEI_SERVER_ADDRESS`: Server hostname for workers to connect
    /// - `HODEI_NATS_URL`: NATS connection URL
    /// - `HODEI_WORKER_IMAGE`: Default Docker image for workers
    ///
    /// # Optional Environment Variables
    ///
    /// - `HODEI_SERVER_PROTOCOL`: Protocol for worker connections (default: "http")
    /// - `HODEI_DB_POOL_SIZE`: Database pool size (default: 50)
    /// - `HODEI_DB_MIN_IDLE`: Minimum idle DB connections (default: 10)
    /// - `HODEI_DEV_MODE`: Development mode "1"=on (default: 0)
    /// - `RUST_LOG`: Log level (default: "info")
    ///
    /// # Returns
    ///
    /// `Ok(ServerConfigDto)` if all required variables are present and valid
    /// `Err(ConfigError)` if required variables are missing or invalid
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Set required environment variables
    /// std::env::set_var("HODEI_DATABASE_URL", "postgresql://localhost/test");
    /// std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090");
    /// std::env::set_var("HODEI_SERVER_ADDRESS", "server.local");
    /// std::env::set_var("HODEI_NATS_URL", "nats://localhost:4222");
    /// std::env::set_var("HODEI_WORKER_IMAGE", "worker:latest");
    ///
    /// let config = ServerConfigDto::from_env()?;
    /// ```
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database: DatabaseConfig::from_env()?,
            grpc: GrpcServerConfig::from_env()?,
            workers: WorkerProvisioningConfig::from_env()?,
            nats: NatsConfig::from_env()?,
            logging: LoggingConfig::from_env()?,
            features: FeatureFlags::from_env()?,
        })
    }
}

// ============================================================================
// Implementation: DatabaseConfig::from_env
// ============================================================================

impl DatabaseConfig {
    /// Build database configuration from environment variables
    ///
    /// # Required Variables
    ///
    /// - `HODEI_DATABASE_URL`: PostgreSQL connection string
    ///
    /// # Optional Variables
    ///
    /// - `HODEI_DB_POOL_SIZE`: Default 50
    /// - `HODEI_DB_MIN_IDLE`: Default 10
    /// - `HODEI_DB_CONNECT_TIMEOUT_SECS`: Default 30
    /// - `HODEI_DB_IDLE_TIMEOUT_SECS`: Default 600
    /// - `HODEI_DB_MAX_LIFETIME_SECS`: Default 1800
    pub fn from_env() -> Result<Self> {
        let url =
            std::env::var("HODEI_DATABASE_URL").map_err(|_| ConfigError::MissingRequired {
                var: "HODEI_DATABASE_URL".to_string(),
            })?;

        let pool_size = parse_optional_var("HODEI_DB_POOL_SIZE", 50)?;
        let min_idle = parse_optional_var("HODEI_DB_MIN_IDLE", 10)?;
        let connect_timeout_secs = parse_optional_var("HODEI_DB_CONNECT_TIMEOUT_SECS", 30)?;
        let idle_timeout_secs = parse_optional_var("HODEI_DB_IDLE_TIMEOUT_SECS", 600)?;
        let max_lifetime_secs = parse_optional_var("HODEI_DB_MAX_LIFETIME_SECS", 1800)?;

        Ok(Self {
            url,
            pool_size,
            min_idle,
            connect_timeout_secs,
            idle_timeout_secs,
            max_lifetime_secs,
        })
    }
}

// ============================================================================
// Implementation: GrpcServerConfig::from_env
// ============================================================================

impl GrpcServerConfig {
    /// Build gRPC server configuration from environment variables
    ///
    /// # Required Variables
    ///
    /// - `HODEI_SERVER_BIND`: Bind address for gRPC server (e.g., "0.0.0.0:9090")
    /// - `HODEI_SERVER_ADDRESS`: Server address for clients/workers (e.g., "hodei-server.hodei-jobs.svc.cluster.local")
    ///
    /// # Optional Variables
    ///
    /// - `HODEI_SERVER_PROTOCOL`: Protocol for worker connections (default: "http")
    pub fn from_env() -> Result<Self> {
        // HODEI_SERVER_BIND: Bind address for gRPC server (e.g., "0.0.0.0:9090")
        let bind_address: std::net::SocketAddr = std::env::var("HODEI_SERVER_BIND")
            .map_err(|_| ConfigError::MissingRequired {
                var: "HODEI_SERVER_BIND".to_string(),
            })?
            .parse()
            .map_err(|_| {
                ConfigError::InvalidSocketAddr(format!(
                    "Invalid HODEI_SERVER_BIND: {}",
                    std::env::var("HODEI_SERVER_BIND").unwrap()
                ))
            })?;

        // Get server address for clients/workers (REQUIRED)
        // This is the hostname that workers use to connect
        let server_address = Self::load_server_address()?;

        // Get protocol for worker connections (default: http)
        let protocol =
            std::env::var("HODEI_SERVER_PROTOCOL").unwrap_or_else(|_| "http".to_string());

        // Extract port from bind_address for worker connections
        let port = bind_address.port();

        // Build worker_connect_url: protocol://host:port
        let worker_connect_url = format!("{}://{}:{}", protocol, server_address, port);

        Ok(Self {
            bind_address,
            worker_connect_url,
            enable_cors: true,
            enable_grpc_web: true,
        })
    }

    /// Load server address (hostname) from environment
    ///
    /// Returns the hostname that clients/workers use to connect to the server.
    /// Format: "host" or "host.domain" (no port, no protocol).
    ///
    /// # Returns
    ///
    /// Ok("hostname") if HODEI_SERVER_ADDRESS is set, Err if missing
    fn load_server_address() -> Result<String> {
        let addr =
            std::env::var("HODEI_SERVER_ADDRESS").map_err(|_| ConfigError::MissingRequired {
                var: "HODEI_SERVER_ADDRESS".to_string(),
            })?;

        // Validate: no protocol prefix, no port
        if addr.starts_with("http://") || addr.starts_with("https://") {
            return Err(ConfigError::InvalidUrl(format!(
                "HODEI_SERVER_ADDRESS should not include protocol (http://), got: {}",
                addr
            )));
        }

        // Extract just the hostname (before any port)
        let hostname = if let Some(pos) = addr.find(':') {
            &addr[..pos]
        } else {
            &addr
        };

        if hostname.is_empty() {
            return Err(ConfigError::InvalidUrl(format!(
                "HODEI_SERVER_ADDRESS hostname cannot be empty, got: {}",
                addr
            )));
        }

        Ok(hostname.to_string())
    }
}

// ============================================================================
// Implementation: WorkerProvisioningConfig::from_env
// ============================================================================

impl WorkerProvisioningConfig {
    /// Build worker provisioning configuration from environment variables
    ///
    /// # Required Variables
    ///
    /// - `HODEI_WORKER_IMAGE`: Default Docker image for workers
    ///
    /// # Optional Variables
    ///
    /// - `HODEI_WORKER_OTP_TTL_SECS`: OTP token TTL in seconds (default: 300)
    /// - `HODEI_MAX_WORKERS_PER_PROVIDER`: Max workers per provider (default: 10)
    /// - `HODEI_PROVISIONING_ENABLED`: "1"=enabled, "0"=disabled (default: 1)
    pub fn from_env() -> Result<Self> {
        let default_image =
            std::env::var("HODEI_WORKER_IMAGE").map_err(|_| ConfigError::MissingRequired {
                var: "HODEI_WORKER_IMAGE".to_string(),
            })?;

        let otp_ttl_secs = parse_optional_var("HODEI_WORKER_OTP_TTL_SECS", 300)?;
        let max_workers_per_provider = parse_optional_var("HODEI_MAX_WORKERS_PER_PROVIDER", 10)?;

        let enabled = std::env::var("HODEI_PROVISIONING_ENABLED")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<u8>()
            .map_err(|_| ConfigError::InvalidValue {
                var: "HODEI_PROVISIONING_ENABLED".to_string(),
                value: "must be 0 or 1".to_string(),
            })?
            == 1;

        Ok(Self {
            default_image,
            otp_ttl_secs,
            max_workers_per_provider,
            enabled,
        })
    }
}

// ============================================================================
// Implementation: NatsConfig::from_env
// ============================================================================

impl NatsConfig {
    /// Build NATS configuration from environment variables
    ///
    /// # Required Variables
    ///
    /// - `HODEI_NATS_URL`: NATS connection URL (comma-separated for clustering)
    ///
    /// # Optional Variables
    ///
    /// - `HODEI_NATS_TIMEOUT_SECS`: Connection timeout (default: 10)
    /// - `HODEI_NATS_MAX_RECONNECTS`: Max reconnect attempts (default: infinite)
    pub fn from_env() -> Result<Self> {
        let urls_str =
            std::env::var("HODEI_NATS_URL").map_err(|_| ConfigError::MissingRequired {
                var: "HODEI_NATS_URL".to_string(),
            })?;

        let urls: Vec<String> = urls_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if urls.is_empty() {
            return Err(ConfigError::InvalidValue {
                var: "HODEI_NATS_URL".to_string(),
                value: urls_str,
            });
        }

        let timeout_secs = parse_optional_var("HODEI_NATS_TIMEOUT_SECS", 10)?;

        let max_reconnects = std::env::var("HODEI_NATS_MAX_RECONNECTS")
            .ok()
            .and_then(|s| s.parse().ok());

        Ok(Self {
            urls,
            timeout_secs,
            max_reconnects,
        })
    }
}

// ============================================================================
// Implementation: LoggingConfig::from_env
// ============================================================================

impl LoggingConfig {
    /// Build logging configuration from environment variables
    ///
    /// # Optional Variables
    ///
    /// - `RUST_LOG`: Log level (default: "info")
    /// - `HODEI_LOG_PERSISTENCE_ENABLED`: "1"=enabled (default: 1)
    /// - `HODEI_LOG_STORAGE_BACKEND`: Storage backend (default: "local")
    /// - `HODEI_LOG_PERSISTENCE_PATH`: Path for log storage (default: "/tmp/hodei-logs")
    /// - `HODEI_LOG_TTL_HOURS`: Log TTL in hours (default: 168 = 7 days)
    pub fn from_env() -> Result<Self> {
        let level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let persistence_enabled = std::env::var("HODEI_LOG_PERSISTENCE_ENABLED")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<u8>()
            .map_err(|_| ConfigError::InvalidValue {
                var: "HODEI_LOG_PERSISTENCE_ENABLED".to_string(),
                value: "must be 0 or 1".to_string(),
            })?
            == 1;

        let storage_backend =
            std::env::var("HODEI_LOG_STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string());

        let persistence_path = std::env::var("HODEI_LOG_PERSISTENCE_PATH")
            .unwrap_or_else(|_| "/tmp/hodei-logs".to_string());

        let ttl_hours = parse_optional_var("HODEI_LOG_TTL_HOURS", 168)?;

        Ok(Self {
            level,
            persistence_enabled,
            storage_backend,
            persistence_path: std::path::PathBuf::from(persistence_path),
            ttl_hours,
        })
    }
}

// ============================================================================
// Implementation: FeatureFlags::from_env
// ============================================================================

impl FeatureFlags {
    /// Build feature flags from environment variables
    ///
    /// # Optional Variables
    ///
    /// - `HODEI_DEV_MODE`: "1"=development mode (default: 0)
    pub fn from_env() -> Result<Self> {
        let dev_mode = std::env::var("HODEI_DEV_MODE")
            .unwrap_or_else(|_| "0".to_string())
            .parse::<u8>()
            .map_err(|_| ConfigError::InvalidValue {
                var: "HODEI_DEV_MODE".to_string(),
                value: "must be 0 or 1".to_string(),
            })?
            == 1;

        Ok(Self { dev_mode })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse optional environment variable with default value
fn parse_optional_var<T>(var: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
{
    std::env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(ConfigError::InvalidValue {
            var: var.to_string(),
            value: "invalid format".to_string(),
        })
        .or(Ok(default))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_config_loader_new() {
        let loader = ConfigLoader::new(None);
        assert!(loader.env_file_path.is_none());

        let loader = ConfigLoader::new(Some(PathBuf::from(".env")));
        assert!(loader.env_file_path.is_some());
    }

    #[test]
    fn test_config_loader_default() {
        let loader = ConfigLoader::default();
        assert!(loader.env_file_path.is_none());
    }

    #[test]
    fn test_parse_optional_var() {
        // Test with valid value
        unsafe { std::env::set_var("TEST_VAR", "42") };
        let result: Result<u32> = parse_optional_var("TEST_VAR", 10);
        assert_eq!(result.unwrap(), 42);

        // Test with missing value (uses default)
        unsafe { std::env::remove_var("TEST_VAR") };
        let result: Result<u32> = parse_optional_var("TEST_VAR", 10);
        assert_eq!(result.unwrap(), 10);

        // Test with invalid value
        unsafe { std::env::set_var("TEST_VAR", "invalid") };
        let result: Result<u32> = parse_optional_var("TEST_VAR", 10);
        assert_eq!(result.unwrap(), 10); // Falls back to default

        unsafe { std::env::remove_var("TEST_VAR") };
    }

    #[test]
    fn test_grpc_server_config_fails_without_server_address() {
        // Clean up first to ensure isolation
        unsafe {
            std::env::remove_var("HODEI_SERVER_ADDRESS");
            std::env::remove_var("HODEI_SERVER_BIND");
        }
        unsafe { std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090") };

        let result = GrpcServerConfig::from_env();
        assert!(result.is_err());

        unsafe { std::env::remove_var("HODEI_SERVER_BIND") };
    }

    #[test]
    fn test_grpc_server_config_from_env() {
        // Clean up first to ensure isolation
        unsafe {
            std::env::remove_var("HODEI_SERVER_ADDRESS");
            std::env::remove_var("HODEI_SERVER_BIND");
        }
        unsafe { std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090") };
        unsafe { std::env::set_var("HODEI_SERVER_ADDRESS", "localhost") };

        let config = GrpcServerConfig::from_env().unwrap();
        assert_eq!(config.bind_address.to_string(), "0.0.0.0:9090");
        assert_eq!(config.worker_connect_url, "http://localhost:9090");

        unsafe { std::env::remove_var("HODEI_SERVER_BIND") };
        unsafe { std::env::remove_var("HODEI_SERVER_ADDRESS") };
    }

    #[test]
    fn test_grpc_server_config_with_server_address_override() {
        // Clean up first to ensure isolation
        unsafe {
            std::env::remove_var("HODEI_SERVER_ADDRESS");
            std::env::remove_var("HODEI_SERVER_BIND");
        }
        unsafe { std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090") };
        unsafe { std::env::set_var("HODEI_SERVER_ADDRESS", "lb.example.com") };

        let config = GrpcServerConfig::from_env().unwrap();
        assert_eq!(config.bind_address.to_string(), "0.0.0.0:9090");
        assert_eq!(config.worker_connect_url, "http://lb.example.com:9090");

        unsafe { std::env::remove_var("HODEI_SERVER_BIND") };
        unsafe { std::env::remove_var("HODEI_SERVER_ADDRESS") };
    }

    #[test]
    fn test_grpc_server_config_server_address_rejects_http_prefix() {
        // Clean up first to ensure isolation
        unsafe {
            std::env::remove_var("HODEI_SERVER_ADDRESS");
            std::env::remove_var("HODEI_SERVER_BIND");
        }
        unsafe { std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090") };
        unsafe { std::env::set_var("HODEI_SERVER_ADDRESS", "http://localhost:9090") };

        let result = GrpcServerConfig::from_env();
        assert!(result.is_err());

        unsafe { std::env::remove_var("HODEI_SERVER_BIND") };
        unsafe { std::env::remove_var("HODEI_SERVER_ADDRESS") };
    }

    #[test]
    fn test_grpc_server_config_with_protocol() {
        // Clean up first to ensure isolation
        unsafe {
            std::env::remove_var("HODEI_SERVER_ADDRESS");
            std::env::remove_var("HODEI_SERVER_BIND");
            std::env::remove_var("HODEI_SERVER_PROTOCOL");
        }
        unsafe { std::env::set_var("HODEI_SERVER_BIND", "0.0.0.0:9090") };
        unsafe { std::env::set_var("HODEI_SERVER_ADDRESS", "server.local") };
        unsafe { std::env::set_var("HODEI_SERVER_PROTOCOL", "https") };

        let config = GrpcServerConfig::from_env().unwrap();
        assert_eq!(config.bind_address.to_string(), "0.0.0.0:9090");
        assert_eq!(config.worker_connect_url, "https://server.local:9090");

        unsafe { std::env::remove_var("HODEI_SERVER_BIND") };
        unsafe { std::env::remove_var("HODEI_SERVER_ADDRESS") };
        unsafe { std::env::remove_var("HODEI_SERVER_PROTOCOL") };
    }

    #[test]
    fn test_worker_provisioning_config_from_env() {
        unsafe { std::env::set_var("HODEI_WORKER_IMAGE", "worker:latest") };

        let config = WorkerProvisioningConfig::from_env().unwrap();
        assert_eq!(config.default_image, "worker:latest");
        assert_eq!(config.otp_ttl_secs, 300); // Default
        assert_eq!(config.max_workers_per_provider, 10); // Default
        assert!(config.enabled); // Default

        unsafe { std::env::remove_var("HODEI_WORKER_IMAGE") };
    }

    #[test]
    fn test_nats_config_from_env() {
        unsafe { std::env::set_var("HODEI_NATS_URL", "nats://localhost:4222") };

        let config = NatsConfig::from_env().unwrap();
        assert_eq!(config.urls, vec!["nats://localhost:4222"]);
        assert_eq!(config.timeout_secs, 10); // Default

        unsafe { std::env::remove_var("HODEI_NATS_URL") };
    }

    #[test]
    fn test_nats_config_multiple_urls() {
        unsafe {
            std::env::set_var(
                "HODEI_NATS_URL",
                "nats://nats1:4222,nats://nats2:4222,nats://nats3:4222",
            )
        };

        let config = NatsConfig::from_env().unwrap();
        assert_eq!(config.urls.len(), 3);

        unsafe { std::env::remove_var("HODEI_NATS_URL") };
    }

    #[test]
    fn test_logging_config_from_env() {
        // All defaults
        let config = LoggingConfig::from_env().unwrap();
        assert_eq!(config.level, "info"); // Default
        assert!(config.persistence_enabled); // Default
        assert_eq!(config.storage_backend, "local"); // Default
    }

    #[test]
    fn test_feature_flags_from_env() {
        // All defaults
        let config = FeatureFlags::from_env().unwrap();
        assert!(!config.dev_mode); // Default
    }

    #[test]
    fn test_feature_flags_dev_mode() {
        unsafe { std::env::set_var("HODEI_DEV_MODE", "1") };

        let config = FeatureFlags::from_env().unwrap();
        assert!(config.dev_mode);

        unsafe { std::env::remove_var("HODEI_DEV_MODE") };
    }
}

//! Configuration module for Hodei Jobs Platform
//!
//! This module provides centralized configuration loading, validation, and
//! Data Transfer Objects (DTOs) for all components of the Hodei Jobs Platform.
//!
//! # Architecture
//!
//! The configuration system follows these principles:
//!
//! 1. **Single Source of Truth**: All configuration is loaded once at startup
//! 2. **Fail Fast**: Errors are reported immediately, no silent fallbacks
//! 3. **DTO Pattern**: Configuration is immutable and passed via dependency injection
//! 4. **Env File Priority**: `.env` file > environment variables > error
//!
//! # Usage
//!
//! ```ignore
//! use hodei_shared::config::{ConfigLoader, ServerConfigDto};
//! use std::path::PathBuf;
//!
//! // Load configuration (with optional .env file)
//! let loader = ConfigLoader::new(Some(PathBuf::from(".env")));
//! let config = loader.load_server_config()?;
//!
//! // Access configuration
//! println!("Server binds to: {}", config.grpc.bind_address);
//! println!("Workers connect to: {}", config.grpc.worker_connect_url);
//! ```
//!
//! # Environment Variables
//!
//! ## Required Variables
//!
//! - `HODEI_DATABASE_URL`: PostgreSQL connection string
//! - `HODEI_SERVER_BIND`: gRPC server bind address (e.g., "0.0.0.0:9090")
//! - `HODEI_SERVER_ADDRESS`: Server hostname for clients/workers
//! - `HODEI_NATS_URL`: NATS connection URL
//! - `HODEI_WORKER_IMAGE`: Default Docker image for workers
//!
//! ## Optional Variables
//!
//! - `HODEI_SERVER_PROTOCOL`: Protocol for worker connections (default: "http")
//! - `HODEI_DB_POOL_SIZE`: Database pool size (default: 50)
//! - `HODEI_DEV_MODE`: Development mode (default: 0)
//! - `RUST_LOG`: Log level (default: "info")

pub mod dto;
pub mod error;
pub mod loader;
pub mod validator;

// Re-export commonly used types
pub use dto::{
    DatabaseConfig, FeatureFlags, GrpcServerConfig, LoggingConfig, NatsConfig, ServerConfigDto,
    TlsConfig, WorkerConfigDto, WorkerProvisioningConfig,
};
pub use error::{ConfigError, Result};
pub use loader::ConfigLoader;
pub use validator::{
    validate_bind_address, validate_database_url, validate_nats_urls, validate_pool_config,
    validate_server_config, validate_worker_provisioning_config,
};

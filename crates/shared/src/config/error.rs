//! Configuration error types
//!
//! This module defines all error types that can occur during configuration loading and validation.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during configuration loading or validation
#[derive(Error, Debug)]
pub enum ConfigError {
    /// A required configuration variable is missing
    #[error("Missing required configuration: {var}")]
    MissingRequired { var: String },

    /// A configuration variable has an invalid value
    #[error("Invalid value for {var}: {value}")]
    InvalidValue { var: String, value: String },

    /// Failed to load .env file
    #[error("Failed to load .env file from {path}: {source}")]
    EnvFileLoad {
        path: PathBuf,
        #[source]
        source: dotenv::Error,
    },

    /// Configuration validation failed
    #[error("Configuration validation failed: {0}")]
    Validation(String),

    /// Invalid URL format
    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),

    /// Invalid socket address format
    #[error("Invalid socket address: {0}")]
    InvalidSocketAddr(String),

    /// Invalid database URL format
    #[error("Invalid database URL format: {0}")]
    InvalidDatabaseUrl(String),
}

// Implement From<std::env::VarError> for convenience
impl From<std::env::VarError> for ConfigError {
    fn from(err: std::env::VarError) -> Self {
        ConfigError::MissingRequired {
            var: err.to_string(),
        }
    }
}

/// Result type for configuration operations
pub type Result<T> = std::result::Result<T, ConfigError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_required_display() {
        let err = ConfigError::MissingRequired {
            var: "HODEI_DATABASE_URL".to_string(),
        };
        assert!(err.to_string().contains("HODEI_DATABASE_URL"));
        assert!(err.to_string().contains("Missing required"));
    }

    #[test]
    fn test_invalid_value_display() {
        let err = ConfigError::InvalidValue {
            var: "GRPC_PORT".to_string(),
            value: "abc".to_string(),
        };
        assert!(err.to_string().contains("GRPC_PORT"));
        assert!(err.to_string().contains("abc"));
    }

    #[test]
    fn test_validation_display() {
        let err = ConfigError::Validation("port must be > 0".to_string());
        assert!(err.to_string().contains("port must be > 0"));
    }
}

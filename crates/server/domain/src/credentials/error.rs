//! Credential errors for the secrets management system
//!
//! Provides specific error types for credential operations with
//! helper methods and retryability information.

use std::fmt;

/// Errors that can occur during credential operations
#[derive(Debug, Clone)]
pub enum CredentialError {
    /// Secret not found in the provider
    NotFound {
        /// The key that was not found
        key: String,
    },

    /// Access to the secret was denied
    AccessDenied {
        /// The key access was denied for
        key: String,
    },

    /// The secret has expired
    Expired {
        /// The key that has expired
        key: String,
    },

    /// The credential provider is unavailable
    ProviderUnavailable {
        /// Name of the unavailable provider
        provider: String,
    },

    /// Connection error to the credential backend
    ConnectionError {
        /// Description of the connection error
        message: String,
    },

    /// Encryption or decryption error
    EncryptionError {
        /// Description of the encryption error
        message: String,
    },

    /// Invalid configuration
    ConfigurationError {
        /// Description of the configuration error
        message: String,
    },

    /// Rate limit exceeded
    RateLimitExceeded {
        /// Time to wait before retrying (in seconds)
        retry_after_secs: Option<u64>,
    },
}

impl CredentialError {
    /// Creates a NotFound error
    pub fn not_found(key: impl Into<String>) -> Self {
        Self::NotFound { key: key.into() }
    }

    /// Creates an AccessDenied error
    pub fn access_denied(key: impl Into<String>) -> Self {
        Self::AccessDenied { key: key.into() }
    }

    /// Creates an Expired error
    pub fn expired(key: impl Into<String>) -> Self {
        Self::Expired { key: key.into() }
    }

    /// Creates a ProviderUnavailable error
    pub fn provider_unavailable(provider: impl Into<String>) -> Self {
        Self::ProviderUnavailable {
            provider: provider.into(),
        }
    }

    /// Creates a ConnectionError
    pub fn connection(message: impl Into<String>) -> Self {
        Self::ConnectionError {
            message: message.into(),
        }
    }

    /// Creates an EncryptionError
    pub fn encryption(message: impl Into<String>) -> Self {
        Self::EncryptionError {
            message: message.into(),
        }
    }

    /// Creates a ConfigurationError
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
        }
    }

    /// Creates a RateLimitExceeded error
    pub fn rate_limited(retry_after_secs: Option<u64>) -> Self {
        Self::RateLimitExceeded { retry_after_secs }
    }

    /// Returns true if this error is retryable
    ///
    /// Retryable errors are transient failures that might succeed
    /// if the operation is retried after some time.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ProviderUnavailable { .. }
                | Self::ConnectionError { .. }
                | Self::RateLimitExceeded { .. }
        )
    }

    /// Returns the key associated with this error, if any
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::NotFound { key } | Self::AccessDenied { key } | Self::Expired { key } => {
                Some(key)
            }
            _ => None,
        }
    }
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { key } => write!(f, "Secret not found: {}", key),
            Self::AccessDenied { key } => write!(f, "Access denied to secret: {}", key),
            Self::Expired { key } => write!(f, "Secret expired: {}", key),
            Self::ProviderUnavailable { provider } => {
                write!(f, "Credential provider unavailable: {}", provider)
            }
            Self::ConnectionError { message } => write!(f, "Connection error: {}", message),
            Self::EncryptionError { message } => write!(f, "Encryption error: {}", message),
            Self::ConfigurationError { message } => write!(f, "Configuration error: {}", message),
            Self::RateLimitExceeded { retry_after_secs } => {
                if let Some(secs) = retry_after_secs {
                    write!(f, "Rate limit exceeded, retry after {} seconds", secs)
                } else {
                    write!(f, "Rate limit exceeded")
                }
            }
        }
    }
}

impl std::error::Error for CredentialError {}

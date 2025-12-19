//! Credential Provider Trait
//!
//! Defines the abstraction for pluggable secret backends.
//! Implementations can include PostgreSQL, HashiCorp Vault,
//! Kubernetes Secrets, AWS Secrets Manager, etc.

use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;

use super::error::CredentialError;
use super::secret::{Secret, SecretValue};

/// Actions that can be audited for secret access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditAction {
    /// Secret was read
    Read,
    /// Secret was created or updated
    Write,
    /// Secret was deleted
    Delete,
    /// Secret was rotated to a new version
    Rotate,
    /// Secrets were listed
    List,
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "read"),
            Self::Write => write!(f, "write"),
            Self::Delete => write!(f, "delete"),
            Self::Rotate => write!(f, "rotate"),
            Self::List => write!(f, "list"),
        }
    }
}

/// Context for audit logging
#[derive(Debug, Clone)]
pub struct AuditContext {
    /// Who is accessing the secret
    pub accessor: String,
    /// Correlation ID for request tracing
    pub correlation_id: Option<String>,
    /// IP address of the requester
    pub ip_address: Option<String>,
    /// User agent or application name
    pub user_agent: Option<String>,
}

impl AuditContext {
    /// Creates a new audit context
    pub fn new(accessor: impl Into<String>) -> Self {
        Self {
            accessor: accessor.into(),
            correlation_id: None,
            ip_address: None,
            user_agent: None,
        }
    }

    /// Sets the correlation ID
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Sets the IP address
    pub fn with_ip_address(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// Sets the user agent
    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }
}

/// Trait for credential providers
///
/// This trait defines the interface for secret storage backends.
/// Implementations must be thread-safe (Send + Sync) and support
/// async operations.
///
/// # Implementations
///
/// - `PostgresCredentialProvider`: Encrypted secrets in PostgreSQL
/// - `VaultCredentialProvider`: HashiCorp Vault integration
/// - `KubernetesCredentialProvider`: Kubernetes Secrets
///
/// # Example
///
/// ```ignore
/// #[async_trait]
/// impl CredentialProvider for MyProvider {
///     fn name(&self) -> &str { "my-provider" }
///
///     async fn get_secret(&self, key: &str) -> Result<Secret, CredentialError> {
///         // Implementation
///     }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Returns the name of this provider
    fn name(&self) -> &str;

    /// Retrieves a secret by key
    ///
    /// # Errors
    ///
    /// - `CredentialError::NotFound` if the secret doesn't exist
    /// - `CredentialError::Expired` if the secret has expired
    /// - `CredentialError::AccessDenied` if access is not allowed
    /// - `CredentialError::ConnectionError` for backend connectivity issues
    async fn get_secret(&self, key: &str) -> Result<Secret, CredentialError>;

    /// Retrieves a specific version of a secret
    ///
    /// # Errors
    ///
    /// Same as `get_secret`, plus version-specific errors
    async fn get_secret_version(&self, key: &str, version: u64) -> Result<Secret, CredentialError> {
        // Default implementation: just get the latest and check version
        let secret = self.get_secret(key).await?;
        if secret.version() == version {
            Ok(secret)
        } else {
            Err(CredentialError::not_found(format!("{}@v{}", key, version)))
        }
    }

    /// Retrieves multiple secrets at once
    ///
    /// This method allows batching for efficiency. The default implementation
    /// fetches secrets one by one, but providers can override for better performance.
    ///
    /// # Returns
    ///
    /// A map of key -> Secret for found secrets. Missing keys are not included.
    async fn get_secrets(&self, keys: &[&str]) -> Result<HashMap<String, Secret>, CredentialError> {
        let mut result = HashMap::new();
        for key in keys {
            match self.get_secret(key).await {
                Ok(secret) => {
                    result.insert(key.to_string(), secret);
                }
                Err(CredentialError::NotFound { .. }) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }

    /// Creates or updates a secret
    ///
    /// # Errors
    ///
    /// - `CredentialError::AccessDenied` if write access is not allowed
    /// - `CredentialError::EncryptionError` if encryption fails
    /// - `CredentialError::ConnectionError` for backend connectivity issues
    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<Secret, CredentialError>;

    /// Deletes a secret
    ///
    /// # Errors
    ///
    /// - `CredentialError::NotFound` if the secret doesn't exist
    /// - `CredentialError::AccessDenied` if delete access is not allowed
    async fn delete_secret(&self, key: &str) -> Result<(), CredentialError>;

    /// Rotates a secret to a new version
    ///
    /// The old version may be kept for a grace period depending on the provider.
    ///
    /// # Errors
    ///
    /// - `CredentialError::NotFound` if the secret doesn't exist
    /// - `CredentialError::AccessDenied` if rotation is not allowed
    async fn rotate_secret(&self, _key: &str) -> Result<Secret, CredentialError> {
        // Default implementation: not supported
        Err(CredentialError::ConfigurationError {
            message: format!(
                "Secret rotation not supported by provider '{}'",
                self.name()
            ),
        })
    }

    /// Lists secret keys matching an optional prefix
    ///
    /// # Security Note
    ///
    /// This method only returns keys, not values.
    async fn list_secrets(&self, prefix: Option<&str>) -> Result<Vec<String>, CredentialError>;

    /// Checks if a secret exists
    async fn exists(&self, key: &str) -> Result<bool, CredentialError> {
        match self.get_secret(key).await {
            Ok(_) => Ok(true),
            Err(CredentialError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Performs a health check on the provider
    async fn health_check(&self) -> Result<(), CredentialError> {
        // Default: try to list with empty prefix
        self.list_secrets(None).await.map(|_| ())
    }

    /// Records an audit event for secret access
    ///
    /// This is called automatically by the credential service wrapper.
    /// Providers can implement this to log to their own audit system.
    ///
    /// The default implementation does nothing. Override in infrastructure
    /// layer implementations to provide actual audit logging.
    async fn audit_access(&self, _key: &str, _action: AuditAction, _context: &AuditContext) {
        // Default implementation: no-op
        // Infrastructure implementations should override this
    }
}

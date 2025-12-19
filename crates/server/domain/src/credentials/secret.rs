//! Secret types with memory protection
//!
//! This module provides secure secret handling with:
//! - Zero-on-drop memory protection
//! - Safe Debug implementation (never exposes values)
//! - Builder pattern for Secret construction
//! - Expiration and versioning support

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A protected secret value that zeros memory on drop
///
/// # Security Features
///
/// - Memory is zeroed when the value is dropped
/// - Debug implementation shows [REDACTED] instead of the value
/// - Clone creates a new copy of the protected data
///
/// # Example
///
/// ```ignore
/// let secret = SecretValue::new("my-api-key");
/// assert_eq!(secret.expose(), b"my-api-key");
/// // When `secret` goes out of scope, memory is zeroed
/// ```
#[derive(Clone)]
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    /// Creates a new SecretValue from any type that can be converted to bytes
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(value.into())
    }

    /// Creates a SecretValue from a string
    pub fn from_str(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }

    /// Exposes the raw bytes of the secret
    ///
    /// # Security Note
    ///
    /// Use this method carefully. The returned slice can be copied or logged.
    /// Prefer using the secret directly without intermediate storage.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Attempts to expose the secret as a UTF-8 string
    ///
    /// Returns `None` if the secret is not valid UTF-8
    pub fn expose_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    /// Returns true if the secret is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the length of the secret in bytes
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretValue([REDACTED, {} bytes])", self.0.len())
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        // Zero out memory to prevent leaking secrets
        self.0.iter_mut().for_each(|b| *b = 0);
    }
}

impl From<&str> for SecretValue {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl From<String> for SecretValue {
    fn from(s: String) -> Self {
        Self::new(s.into_bytes())
    }
}

impl From<Vec<u8>> for SecretValue {
    fn from(v: Vec<u8>) -> Self {
        Self::new(v)
    }
}

/// A complete secret with metadata
///
/// # Example
///
/// ```ignore
/// let secret = Secret::builder("api_key")
///     .value(SecretValue::new("secret123"))
///     .version(1)
///     .metadata("environment", "production")
///     .build();
/// ```
#[derive(Clone)]
pub struct Secret {
    key: String,
    value: SecretValue,
    version: u64,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    metadata: HashMap<String, String>,
}

impl Secret {
    /// Creates a new SecretBuilder
    pub fn builder(key: impl Into<String>) -> SecretBuilder {
        SecretBuilder::new(key)
    }

    /// Returns the secret key
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the secret value
    pub fn value(&self) -> &SecretValue {
        &self.value
    }

    /// Consumes the secret and returns the value
    pub fn into_value(self) -> SecretValue {
        self.value
    }

    /// Returns the version number
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Returns when the secret was created
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns when the secret expires, if set
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    /// Returns the metadata
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Returns true if the secret has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
    }

    /// Returns time until expiration, or None if no expiration or already expired
    pub fn time_to_expiry(&self) -> Option<chrono::Duration> {
        self.expires_at.and_then(|exp| {
            let now = Utc::now();
            if exp > now { Some(exp - now) } else { None }
        })
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret")
            .field("key", &self.key)
            .field("value", &self.value) // SecretValue's Debug is safe
            .field("version", &self.version)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Builder for creating Secret instances
///
/// # Example
///
/// ```ignore
/// let secret = Secret::builder("my_key")
///     .value(SecretValue::new("my_value"))
///     .version(2)
///     .expires_at(Utc::now() + Duration::hours(24))
///     .metadata("owner", "team-a")
///     .build();
/// ```
pub struct SecretBuilder {
    key: String,
    value: Option<SecretValue>,
    version: u64,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    metadata: HashMap<String, String>,
}

impl SecretBuilder {
    /// Creates a new builder with the given key
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
            version: 1,
            created_at: Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Sets the secret value
    pub fn value(mut self, value: SecretValue) -> Self {
        self.value = Some(value);
        self
    }

    /// Sets the version number
    pub fn version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }

    /// Sets the creation timestamp
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    /// Sets the expiration timestamp
    pub fn expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Adds a metadata entry
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Builds the Secret
    ///
    /// # Panics
    ///
    /// Panics if no value was set
    pub fn build(self) -> Secret {
        Secret {
            key: self.key,
            value: self.value.expect("SecretBuilder: value is required"),
            version: self.version,
            created_at: self.created_at,
            expires_at: self.expires_at,
            metadata: self.metadata,
        }
    }

    /// Attempts to build the Secret, returning None if value is not set
    pub fn try_build(self) -> Option<Secret> {
        self.value.map(|value| Secret {
            key: self.key,
            value,
            version: self.version,
            created_at: self.created_at,
            expires_at: self.expires_at,
            metadata: self.metadata,
        })
    }
}

/// A reference to a secret without the actual value
///
/// Used for passing secret identifiers around without exposing values.
///
/// # Example
///
/// ```ignore
/// let ref = SecretRef::new("database_password", "vault")
///     .with_version(3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SecretRef {
    key: String,
    provider: String,
    version: Option<u64>,
}

impl SecretRef {
    /// Creates a new secret reference
    pub fn new(key: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            provider: provider.into(),
            version: None,
        }
    }

    /// Sets a specific version for this reference
    pub fn with_version(mut self, version: u64) -> Self {
        self.version = Some(version);
        self
    }

    /// Returns the secret key
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the provider name
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the version, if specified
    pub fn version(&self) -> Option<u64> {
        self.version
    }
}

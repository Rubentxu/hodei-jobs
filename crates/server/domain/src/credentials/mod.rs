//! Credentials Module - Secure Secrets Management
//!
//! This module provides a secure abstraction for managing secrets and credentials
//! across different backends (Postgres, Vault, Kubernetes Secrets, etc.).
//!
//! # Design Principles
//!
//! - **SecretValue**: Wraps secret bytes with memory protection (zero-on-drop)
//! - **SecretRef**: Reference to a secret without exposing the value
//! - **CredentialProvider**: Trait for pluggable secret backends
//!
//! # Security Features
//!
//! - Secrets are never logged (Debug impl shows [REDACTED])
//! - Memory is zeroed on Drop to prevent leaks
//! - Expiration and versioning support
//! - Full audit trail of secret access

mod error;
mod provider;
mod secret;

pub use error::CredentialError;
pub use provider::{AuditAction, CredentialProvider};
pub use secret::{Secret, SecretRef, SecretValue};

#[cfg(test)]
mod tests;

//! Credentials Infrastructure Module
//!
//! Provides implementations of the CredentialProvider trait for
//! different secret storage backends.
//!
//! # Available Providers
//!
//! - `PostgresCredentialProvider`: Encrypted secrets stored in PostgreSQL
//!
//! # Security
//!
//! All secrets are encrypted at rest using AES-256-GCM with unique nonces.
//! The encryption key must be provided securely (e.g., from environment variable).

mod encryption;
mod postgres;

pub use encryption::{EncryptionError, EncryptionKey};
pub use postgres::{CredentialDatabaseConfig, PostgresCredentialProvider};

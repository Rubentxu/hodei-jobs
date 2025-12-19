//! Credentials Application Services
//!
//! Provides application-level services for credential management:
//! - `CredentialProviderRegistry`: Manages multiple credential providers
//!
//! # Example
//!
//! ```ignore
//! use hodei_server_application::credentials::{
//!     CredentialProviderRegistry, CredentialProviderConfig
//! };
//!
//! let registry = CredentialProviderRegistry::new();
//! registry.register(postgres_provider, CredentialProviderConfig::new("postgres")).await;
//! let secret = registry.get_secret("api_key").await?;
//! ```

mod registry;

pub use registry::{CredentialProviderConfig, CredentialProviderRegistry, CredentialRegistryStats};

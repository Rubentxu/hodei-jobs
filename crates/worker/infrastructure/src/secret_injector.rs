//! Secret Injection Module
//!
//! Provides secure injection of secrets into job execution contexts
//! using various strategies to avoid exposure in environment variables.
//!
//! # Strategies
//!
//! - `Stdin`: Inject as JSON via stdin (secure, not visible in process info)
//! - `TmpfsFile`: Inject as files in tmpfs (secure, not persisted to disk)
//!
//! # Security Considerations
//!
//! - `Stdin`: Secure, stdin is consumed once and not persisted
//! - `TmpfsFile`: Secure, files in memory-only filesystem, deleted after use
//!
//! # Memory Safety
//!
//! This module uses the `secrecy` crate to ensure secrets are zeroized on drop,
//! preventing exposure in core dumps or memory forensics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::info;

// Re-export SecretString for use in other modules
pub use secrecy::SecretString;
// Import ExposeSecret trait for accessing secret values
use secrecy::ExposeSecret;

/// Errors that can occur during secret injection
#[derive(Debug, Error)]
pub enum SecretInjectionError {
    /// Failed to serialize secrets to JSON
    #[error("Failed to serialize secrets: {0}")]
    SerializationError(String),

    /// Failed to create tmpfs directory
    #[error("Failed to create secrets directory: {0}")]
    DirectoryCreationError(String),

    /// Failed to write secret file
    #[error("Failed to write secret file '{path}': {reason}")]
    FileWriteError { path: String, reason: String },

    /// Failed to set file permissions
    #[error("Failed to set permissions on '{path}': {reason}")]
    PermissionError { path: String, reason: String },

    /// Failed to cleanup secrets
    #[error("Failed to cleanup secrets: {0}")]
    CleanupError(String),

    /// Invalid secret key format
    #[error("Invalid secret key '{key}': {reason}")]
    InvalidKeyError { key: String, reason: String },
}

/// Strategy for injecting secrets into job execution
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionStrategy {
    /// Inject secrets as JSON via stdin
    ///
    /// Script reads: `SECRETS=$(cat); API_KEY=$(echo "$SECRETS" | jq -r '.api_key')`
    #[default]
    Stdin,

    /// Inject secrets as files in a tmpfs directory
    ///
    /// Files created at: /run/secrets/<job_id>/<key>
    /// Script reads: `API_KEY=$(cat /run/secrets/$HODEI_JOB_ID/api_key)`
    TmpfsFile,
}

impl std::fmt::Display for InjectionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdin => write!(f, "stdin"),
            Self::TmpfsFile => write!(f, "tmpfs_file"),
        }
    }
}

/// Configuration for secret injection
#[derive(Debug, Clone)]
pub struct InjectionConfig {
    /// Strategy to use for injecting secrets
    pub strategy: InjectionStrategy,
    /// Base directory for tmpfs files (default: /run/secrets)
    pub tmpfs_base_dir: PathBuf,
    /// Whether to scrub secrets from logs (default: true)
    pub scrub_logs: bool,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            strategy: InjectionStrategy::default(),
            tmpfs_base_dir: PathBuf::from("/run/secrets"),
            scrub_logs: true,
        }
    }
}

impl InjectionConfig {
    /// Creates a new config with the specified strategy
    pub fn with_strategy(strategy: InjectionStrategy) -> Self {
        Self {
            strategy,
            ..Default::default()
        }
    }

    /// Sets the tmpfs base directory
    pub fn with_tmpfs_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.tmpfs_base_dir = dir.into();
        self
    }

    /// Sets whether to scrub logs
    pub fn with_log_scrubbing(mut self, scrub: bool) -> Self {
        self.scrub_logs = scrub;
        self
    }
}

/// Prepared execution context with injected secrets
///
/// # Security Note
///
/// All secret values are stored using `SecretString` which implements
/// zero-on-drop to prevent exposure in core dumps or memory forensics.
#[derive(Debug, Clone)]
pub struct PreparedExecution {
    /// Environment variables to set (non-secret only)
    pub env_vars: HashMap<String, String>,
    /// Content to write to stdin (for Stdin strategy) - zeroized on drop
    pub stdin_content: Option<SecretString>,
    /// Directory containing secret files (for TmpfsFile strategy)
    pub secrets_dir: Option<PathBuf>,
    /// The strategy used
    pub strategy: InjectionStrategy,
}

impl PreparedExecution {
    /// Merges non-secret env vars with secrets for command execution
    ///
    /// Note: For production use, prefer strategies that don't expose
    /// secrets via environment variables (stdin or tmpfs files instead).
    pub fn merged_env(&self) -> HashMap<String, String> {
        let merged = self.env_vars.clone();
        // Note: stdin_content contains JSON with all secrets, not individual key-value pairs
        merged
    }
}

/// Secret injector for preparing job execution contexts
///
/// # Example
///
/// ```ignore
/// let injector = SecretInjector::new(InjectionConfig::with_strategy(InjectionStrategy::Stdin));
/// let secrets = vec![("api_key".to_string(), "secret123".to_string())];
///
/// let prepared = injector.prepare_execution("job-123", &secrets, &env_vars)?;
/// // prepared.stdin_content contains JSON: {"api_key": "secret123"}
/// ```
pub struct SecretInjector {
    config: InjectionConfig,
}

impl SecretInjector {
    /// Creates a new secret injector with the given configuration
    pub fn new(config: InjectionConfig) -> Self {
        Self { config }
    }

    /// Creates an injector with default configuration (Stdin strategy)
    pub fn with_defaults() -> Self {
        Self::new(InjectionConfig::default())
    }

    /// Creates an injector with Stdin strategy
    pub fn stdin() -> Self {
        Self::new(InjectionConfig::with_strategy(InjectionStrategy::Stdin))
    }

    /// Creates an injector with TmpfsFile strategy
    pub fn tmpfs() -> Self {
        Self::new(InjectionConfig::with_strategy(InjectionStrategy::TmpfsFile))
    }

    /// Returns the configured strategy
    pub fn strategy(&self) -> InjectionStrategy {
        self.config.strategy
    }

    /// Returns whether log scrubbing is enabled
    pub fn should_scrub_logs(&self) -> bool {
        self.config.scrub_logs
    }

    /// Prepares execution context with injected secrets
    ///
    /// # Arguments
    ///
    /// * `job_id` - Unique job identifier (used for tmpfs directory)
    /// * `secrets` - Map of secret key -> value
    /// * `base_env` - Base environment variables (secrets added on top)
    ///
    /// # Returns
    ///
    /// A `PreparedExecution` containing the prepared environment, stdin, etc.
    pub fn prepare_execution(
        &self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<PreparedExecution, SecretInjectionError> {
        match self.config.strategy {
            InjectionStrategy::Stdin => self.prepare_stdin(job_id, secrets, base_env),
            InjectionStrategy::TmpfsFile => self.prepare_tmpfs(job_id, secrets, base_env),
        }
    }

    /// Prepares execution with secrets as JSON via stdin
    fn prepare_stdin(
        &self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<PreparedExecution, SecretInjectionError> {
        // Validate all keys
        for key in secrets.keys() {
            Self::validate_key(key)?;
        }

        let secrets_json = serde_json::to_string(secrets)
            .map_err(|e| SecretInjectionError::SerializationError(e.to_string()))?;

        // Add indicator env var that secrets are available via stdin
        let mut env_vars = base_env.clone();
        env_vars.insert("HODEI_SECRETS_VIA_STDIN".to_string(), "1".to_string());
        env_vars.insert("HODEI_JOB_ID".to_string(), job_id.to_string());

        info!(
            strategy = "stdin",
            secret_count = secrets.len(),
            job_id = %job_id,
            "Prepared secrets for stdin injection"
        );

        // Use SecretString to zeroize secrets on drop
        Ok(PreparedExecution {
            env_vars,
            stdin_content: Some(SecretString::from(secrets_json)),
            secrets_dir: None,
            strategy: InjectionStrategy::Stdin,
        })
    }

    /// Prepares execution with secrets as files in tmpfs
    fn prepare_tmpfs(
        &self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<PreparedExecution, SecretInjectionError> {
        // Validate all keys before creating any files
        for key in secrets.keys() {
            Self::validate_key(key)?;
        }

        let secrets_dir = self.config.tmpfs_base_dir.join(job_id);

        // Add env vars pointing to secrets directory
        let mut env_vars = base_env.clone();
        env_vars.insert(
            "HODEI_SECRETS_DIR".to_string(),
            secrets_dir.to_string_lossy().to_string(),
        );
        env_vars.insert("HODEI_JOB_ID".to_string(), job_id.to_string());

        info!(
            strategy = "tmpfs",
            secret_count = secrets.len(),
            secrets_dir = %secrets_dir.display(),
            job_id = %job_id,
            "Prepared secrets for tmpfs injection (files will be created at execution)"
        );

        // Note: Actual file creation happens in write_tmpfs_secrets()
        // This is deferred to allow async file operations

        Ok(PreparedExecution {
            env_vars,
            stdin_content: None,
            secrets_dir: Some(secrets_dir),
            strategy: InjectionStrategy::TmpfsFile,
        })
    }

    /// Writes secret files to tmpfs (async operation, call before execution)
    ///
    /// # Arguments
    ///
    /// * `secrets_dir` - Directory to create secrets in
    /// * `secrets` - Map of secret key -> value
    pub async fn write_tmpfs_secrets(
        secrets_dir: &Path,
        secrets: &HashMap<String, String>,
    ) -> Result<(), SecretInjectionError> {
        // Create directory with restrictive permissions
        tokio::fs::create_dir_all(secrets_dir)
            .await
            .map_err(|e| SecretInjectionError::DirectoryCreationError(e.to_string()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            tokio::fs::set_permissions(secrets_dir, perms)
                .await
                .map_err(|e| SecretInjectionError::PermissionError {
                    path: secrets_dir.to_string_lossy().to_string(),
                    reason: e.to_string(),
                })?;
        }

        for (key, value) in secrets {
            let file_path = secrets_dir.join(key);

            tokio::fs::write(&file_path, value).await.map_err(|e| {
                SecretInjectionError::FileWriteError {
                    path: file_path.to_string_lossy().to_string(),
                    reason: e.to_string(),
                }
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o400);
                tokio::fs::set_permissions(&file_path, perms)
                    .await
                    .map_err(|e| SecretInjectionError::PermissionError {
                        path: file_path.to_string_lossy().to_string(),
                        reason: e.to_string(),
                    })?;
            }
        }

        info!(
            secret_count = secrets.len(),
            secrets_dir = %secrets_dir.display(),
            "Wrote secrets to tmpfs directory"
        );

        Ok(())
    }

    /// Cleans up tmpfs secrets after job completion
    pub async fn cleanup_tmpfs_secrets(secrets_dir: &Path) -> Result<(), SecretInjectionError> {
        if secrets_dir.exists() {
            tokio::fs::remove_dir_all(secrets_dir)
                .await
                .map_err(|e| SecretInjectionError::CleanupError(e.to_string()))?;

            info!(
                secrets_dir = %secrets_dir.display(),
                "Cleaned up tmpfs secrets directory"
            );
        }

        Ok(())
    }

    /// Validates a secret key
    fn validate_key(key: &str) -> Result<(), SecretInjectionError> {
        if key.is_empty() {
            return Err(SecretInjectionError::InvalidKeyError {
                key: key.to_string(),
                reason: "Key cannot be empty".to_string(),
            });
        }

        // Allow alphanumeric, underscore, hyphen, and dot
        let is_valid = key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');

        if !is_valid {
            return Err(SecretInjectionError::InvalidKeyError {
                key: key.to_string(),
                reason: "Key contains invalid characters (only alphanumeric, _, -, . allowed)"
                    .to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_strategy_display() {
        assert_eq!(InjectionStrategy::Stdin.to_string(), "stdin");
        assert_eq!(InjectionStrategy::TmpfsFile.to_string(), "tmpfs_file");
    }

    #[test]
    fn test_injection_strategy_default() {
        assert_eq!(InjectionStrategy::default(), InjectionStrategy::Stdin);
    }

    #[test]
    fn test_injection_config_default() {
        let config = InjectionConfig::default();
        assert_eq!(config.strategy, InjectionStrategy::Stdin);
        assert_eq!(config.tmpfs_base_dir, PathBuf::from("/run/secrets"));
        assert!(config.scrub_logs);
    }

    #[test]
    fn test_injection_config_builder() {
        let config = InjectionConfig::with_strategy(InjectionStrategy::Stdin)
            .with_tmpfs_dir("/tmp/secrets")
            .with_log_scrubbing(false);

        assert_eq!(config.strategy, InjectionStrategy::Stdin);
        assert_eq!(config.tmpfs_base_dir, PathBuf::from("/tmp/secrets"));
        assert!(!config.scrub_logs);
    }

    #[test]
    fn test_prepare_stdin() {
        let injector = SecretInjector::stdin();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());

        let base_env = HashMap::new();

        let result = injector
            .prepare_execution("job-1", &secrets, &base_env)
            .unwrap();

        assert_eq!(result.strategy, InjectionStrategy::Stdin);
        assert!(result.stdin_content.is_some());
        assert!(result.secrets_dir.is_none());

        // Check stdin contains JSON - use expose_secret()
        let binding = result.stdin_content.unwrap();
        let stdin = binding.expose_secret();
        let parsed: HashMap<String, String> = serde_json::from_str(stdin).unwrap();
        assert_eq!(parsed.get("api_key"), Some(&"secret123".to_string()));

        // Check env var indicator
        assert_eq!(
            result.env_vars.get("HODEI_SECRETS_VIA_STDIN"),
            Some(&"1".to_string())
        );
        assert_eq!(
            result.env_vars.get("HODEI_JOB_ID"),
            Some(&"job-1".to_string())
        );
    }

    #[test]
    fn test_prepare_tmpfs() {
        let injector = SecretInjector::new(
            InjectionConfig::with_strategy(InjectionStrategy::TmpfsFile)
                .with_tmpfs_dir("/tmp/test-secrets"),
        );

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());

        let base_env = HashMap::new();

        let result = injector
            .prepare_execution("job-1", &secrets, &base_env)
            .unwrap();

        assert_eq!(result.strategy, InjectionStrategy::TmpfsFile);
        assert!(result.stdin_content.is_none());
        assert_eq!(
            result.secrets_dir,
            Some(PathBuf::from("/tmp/test-secrets/job-1"))
        );

        // Check env vars
        assert_eq!(
            result.env_vars.get("HODEI_SECRETS_DIR"),
            Some(&"/tmp/test-secrets/job-1".to_string())
        );
        assert_eq!(
            result.env_vars.get("HODEI_JOB_ID"),
            Some(&"job-1".to_string())
        );
    }

    #[test]
    fn test_validate_key_valid() {
        assert!(SecretInjector::validate_key("api_key").is_ok());
        assert!(SecretInjector::validate_key("API_KEY").is_ok());
        assert!(SecretInjector::validate_key("my-secret").is_ok());
        assert!(SecretInjector::validate_key("config.password").is_ok());
        assert!(SecretInjector::validate_key("key123").is_ok());
    }

    #[test]
    fn test_validate_key_invalid() {
        assert!(SecretInjector::validate_key("").is_err());
        assert!(SecretInjector::validate_key("key with space").is_err());
        assert!(SecretInjector::validate_key("key/slash").is_err());
        assert!(SecretInjector::validate_key("key$special").is_err());
    }

    #[test]
    fn test_empty_secrets() {
        let injector = SecretInjector::stdin();
        let secrets = HashMap::new();
        let base_env = HashMap::new();

        let result = injector.prepare_execution("job-1", &secrets, &base_env);
        assert!(result.is_ok());

        let prepared = result.unwrap();
        // Verify stdin_content is Some with "{}" - use expose_secret and compare strings
        assert!(
            prepared.stdin_content.is_some(),
            "stdin_content should be Some"
        );
        let binding = prepared.stdin_content.unwrap();
        let content = binding.expose_secret();
        assert_eq!(content, "{}");
    }

    #[tokio::test]
    async fn test_write_and_cleanup_tmpfs_secrets() {
        let tmp_dir = std::env::temp_dir().join("test-secrets-injector");
        let secrets_dir = tmp_dir.join("job-test-123");

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());
        secrets.insert("db_password".to_string(), "pw456".to_string());

        // Write secrets
        SecretInjector::write_tmpfs_secrets(&secrets_dir, &secrets)
            .await
            .unwrap();

        // Verify files exist
        assert!(secrets_dir.join("api_key").exists());
        assert!(secrets_dir.join("db_password").exists());

        // Verify content
        let api_key = tokio::fs::read_to_string(secrets_dir.join("api_key"))
            .await
            .unwrap();
        assert_eq!(api_key, "secret123");

        // Cleanup
        SecretInjector::cleanup_tmpfs_secrets(&secrets_dir)
            .await
            .unwrap();

        // Verify cleaned up
        assert!(!secrets_dir.exists());

        // Cleanup parent dir
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    }

    #[test]
    fn test_strategy_serialization() {
        let strategy = InjectionStrategy::Stdin;
        let json = serde_json::to_string(&strategy).unwrap();
        assert_eq!(json, "\"stdin\"");

        let deserialized: InjectionStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, InjectionStrategy::Stdin);
    }
}

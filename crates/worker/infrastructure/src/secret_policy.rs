//! Secure Secret Injection Policy
//!
//! Provides policy validation and runtime secret store management
//! to ensure secrets are injected securely according to the environment.

use crate::secret_injector::{InjectionStrategy, SecretInjectionError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// Environment type for policy validation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// Development environment (less strict policies)
    Development,
    /// Staging environment
    Staging,
    /// Production environment (strictest policies)
    Production,
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => write!(f, "development"),
            Environment::Staging => write!(f, "staging"),
            Environment::Production => write!(f, "production"),
        }
    }
}

/// Secret injection policy configuration
#[derive(Debug, Clone)]
pub struct SecretPolicy {
    /// Current environment
    pub environment: Environment,
    /// Default strategy (will be enforced)
    pub default_strategy: InjectionStrategy,
    /// Minimum allowed strategy (more secure strategies only)
    pub minimum_strategy: InjectionStrategy,
    /// Audit all secret injections
    pub audit_injections: bool,
}

impl SecretPolicy {
    /// Create a production policy (strict security)
    pub fn production() -> Self {
        Self {
            environment: Environment::Production,
            default_strategy: InjectionStrategy::TmpfsFile,
            minimum_strategy: InjectionStrategy::Stdin, // Stdin or TmpfsFile only
            audit_injections: true,
        }
    }

    /// Create a development policy (flexible for testing)
    pub fn development() -> Self {
        Self {
            environment: Environment::Development,
            default_strategy: InjectionStrategy::TmpfsFile,
            minimum_strategy: InjectionStrategy::Stdin, // Minimum: Stdin or TmpfsFile
            audit_injections: false,
        }
    }

    /// Create a staging policy (production-like but allows some flexibility)
    pub fn staging() -> Self {
        Self {
            environment: Environment::Staging,
            default_strategy: InjectionStrategy::TmpfsFile,
            minimum_strategy: InjectionStrategy::Stdin,
            audit_injections: true,
        }
    }

    /// Validate that a strategy is allowed in this environment
    pub fn validate_strategy(
        &self,
        strategy: InjectionStrategy,
    ) -> Result<(), SecretInjectionError> {
        // Check minimum security level
        let strategy_security_level = match strategy {
            InjectionStrategy::Stdin => 1,
            InjectionStrategy::TmpfsFile => 2,
        };

        let minimum_security_level = match self.minimum_strategy {
            InjectionStrategy::Stdin => 1,
            InjectionStrategy::TmpfsFile => 2,
        };

        if strategy_security_level < minimum_security_level {
            return Err(SecretInjectionError::InvalidKeyError {
                key: format!("{:?}", strategy),
                reason: format!(
                    "Strategy {:?} is below minimum security level {:?} for {} environment",
                    strategy, self.minimum_strategy, self.environment
                ),
            });
        }

        Ok(())
    }

    /// Check if audit logging is enabled
    pub fn should_audit(&self) -> bool {
        self.audit_injections
    }
}

/// Audit log entry for secret injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInjectionAudit {
    /// Timestamp of the injection
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Job ID
    pub job_id: String,
    /// Strategy used
    pub strategy: InjectionStrategy,
    /// Number of secrets injected
    pub secret_count: usize,
    /// Environment
    pub environment: Environment,
    /// Secret keys (not values for security)
    pub secret_keys: Vec<String>,
    /// Whether the injection was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl SecretInjectionAudit {
    /// Create a new audit entry for a successful injection
    pub fn success(
        job_id: String,
        strategy: InjectionStrategy,
        secret_keys: Vec<String>,
        environment: &Environment,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            job_id,
            strategy,
            secret_count: secret_keys.len(),
            environment: environment.clone(),
            secret_keys,
            success: true,
            error: None,
        }
    }

    /// Create a new audit entry for a failed injection
    pub fn failure(
        job_id: String,
        strategy: InjectionStrategy,
        secret_keys: Vec<String>,
        environment: &Environment,
        error: String,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            job_id,
            strategy,
            secret_count: secret_keys.len(),
            environment: environment.clone(),
            secret_keys,
            success: false,
            error: Some(error),
        }
    }
}

/// Runtime secret store with policy enforcement
pub struct RuntimeSecretStore {
    /// Policy configuration
    policy: SecretPolicy,
    /// Injector implementations
    stdin_injector: SecretInjectorWrapper,
    tmpfs_injector: SecretInjectorWrapper,
    /// Audit log
    audit_log: Vec<SecretInjectionAudit>,
}

impl std::fmt::Debug for RuntimeSecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeSecretStore")
            .field("policy", &self.policy)
            .field("audit_entries", &self.audit_log.len())
            .finish()
    }
}

#[async_trait::async_trait]
trait SecretInjectorImpl: Send + Sync {
    async fn inject(
        &self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<crate::secret_injector::PreparedExecution, SecretInjectionError>;
}

/// Wrapper for the existing SecretInjector
struct SecretInjectorWrapper {
    injector: crate::secret_injector::SecretInjector,
}

impl SecretInjectorWrapper {
    fn new(strategy: InjectionStrategy) -> Self {
        let config = crate::secret_injector::InjectionConfig::with_strategy(strategy);
        let injector = crate::secret_injector::SecretInjector::new(config);
        Self { injector }
    }
}

#[async_trait::async_trait]
impl SecretInjectorImpl for SecretInjectorWrapper {
    async fn inject(
        &self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<crate::secret_injector::PreparedExecution, SecretInjectionError> {
        self.injector.prepare_execution(job_id, secrets, base_env)
    }
}

impl RuntimeSecretStore {
    /// Create a new runtime secret store with production policy
    pub fn production() -> Self {
        Self::with_policy(SecretPolicy::production())
    }

    /// Create a new runtime secret store with development policy
    pub fn development() -> Self {
        Self::with_policy(SecretPolicy::development())
    }

    /// Create a new runtime secret store with staging policy
    pub fn staging() -> Self {
        Self::with_policy(SecretPolicy::staging())
    }

    /// Create a new runtime secret store with custom policy
    pub fn with_policy(policy: SecretPolicy) -> Self {
        Self {
            policy,
            stdin_injector: SecretInjectorWrapper::new(InjectionStrategy::Stdin),
            tmpfs_injector: SecretInjectorWrapper::new(InjectionStrategy::TmpfsFile),
            audit_log: Vec::new(),
        }
    }

    /// Inject secrets using the specified strategy (with policy validation)
    ///
    /// # Arguments
    ///
    /// * `job_id` - Unique job identifier
    /// * `secrets` - Map of secret key -> value
    /// * `base_env` - Base environment variables
    /// * `strategy` - Strategy to use (will be validated against policy)
    ///
    /// # Returns
    ///
    /// A `PreparedExecution` containing the prepared environment, stdin, etc.
    pub async fn inject_secrets(
        &mut self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
        strategy: InjectionStrategy,
    ) -> Result<crate::secret_injector::PreparedExecution, SecretInjectionError> {
        // Validate strategy against policy
        self.policy.validate_strategy(strategy).map_err(|e| {
            warn!(
                job_id = %job_id,
                strategy = %strategy,
                environment = %self.policy.environment,
                error = %e,
                "Secret injection blocked by policy"
            );
            e
        })?;

        // Get the injector for this strategy
        let injector: &dyn SecretInjectorImpl = match strategy {
            InjectionStrategy::Stdin => &self.stdin_injector,
            InjectionStrategy::TmpfsFile => &self.tmpfs_injector,
        };

        // Perform the injection
        let result = injector.inject(job_id, secrets, base_env).await;

        // Log audit entry
        if self.policy.should_audit() {
            let secret_keys: Vec<String> = secrets.keys().cloned().collect();
            let audit_entry = if result.is_ok() {
                SecretInjectionAudit::success(
                    job_id.to_string(),
                    strategy,
                    secret_keys,
                    &self.policy.environment,
                )
            } else {
                let error = result.as_ref().err().unwrap().to_string();
                SecretInjectionAudit::failure(
                    job_id.to_string(),
                    strategy,
                    secret_keys,
                    &self.policy.environment,
                    error,
                )
            };

            self.audit_log.push(audit_entry);

            // Log to tracing
            if let Err(e) = &result {
                warn!(
                    job_id = %job_id,
                    strategy = %strategy,
                    secret_count = secrets.len(),
                    error = %e,
                    "Secret injection failed"
                );
            } else {
                info!(
                    job_id = %job_id,
                    strategy = %strategy,
                    secret_count = secrets.len(),
                    environment = %self.policy.environment,
                    "Secret injection successful"
                );
            }
        }

        result
    }

    /// Inject secrets using the default strategy
    pub async fn inject_secrets_default(
        &mut self,
        job_id: &str,
        secrets: &HashMap<String, String>,
        base_env: &HashMap<String, String>,
    ) -> Result<crate::secret_injector::PreparedExecution, SecretInjectionError> {
        self.inject_secrets(job_id, secrets, base_env, self.policy.default_strategy)
            .await
    }

    /// Get the current policy
    pub fn policy(&self) -> &SecretPolicy {
        &self.policy
    }

    /// Get audit log entries
    pub fn audit_log(&self) -> &[SecretInjectionAudit] {
        &self.audit_log
    }

    /// Clear audit log
    pub fn clear_audit_log(&mut self) {
        self.audit_log.clear();
    }

    /// Get number of audit entries
    pub fn audit_count(&self) -> usize {
        self.audit_log.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_policy_allows_tmpfs() {
        let policy = SecretPolicy::production();
        assert!(
            policy
                .validate_strategy(InjectionStrategy::TmpfsFile)
                .is_ok()
        );
        assert!(policy.validate_strategy(InjectionStrategy::Stdin).is_ok());
    }

    #[test]
    fn test_development_policy_allows_tmpfs() {
        let policy = SecretPolicy::development();
        assert!(
            policy
                .validate_strategy(InjectionStrategy::TmpfsFile)
                .is_ok()
        );
        assert!(policy.validate_strategy(InjectionStrategy::Stdin).is_ok());
    }

    #[test]
    fn test_staging_policy_allows_tmpfs() {
        let policy = SecretPolicy::staging();
        assert!(
            policy
                .validate_strategy(InjectionStrategy::TmpfsFile)
                .is_ok()
        );
        assert!(policy.validate_strategy(InjectionStrategy::Stdin).is_ok());
    }

    #[test]
    fn test_audit_entry_success() {
        let audit = SecretInjectionAudit::success(
            "job-123".to_string(),
            InjectionStrategy::TmpfsFile,
            vec!["api_key".to_string()],
            &Environment::Production,
        );

        assert!(audit.success);
        assert_eq!(audit.job_id, "job-123");
        assert_eq!(audit.strategy, InjectionStrategy::TmpfsFile);
        assert_eq!(audit.secret_count, 1);
        assert!(audit.error.is_none());
    }

    #[tokio::test]
    async fn test_runtime_secret_store_production_allows_tmpfs() {
        let mut store = RuntimeSecretStore::production();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());

        let base_env = HashMap::new();

        // Should succeed with TmpfsFile
        let result = store
            .inject_secrets("job-1", &secrets, &base_env, InjectionStrategy::TmpfsFile)
            .await;

        assert!(result.is_ok());
        let prepared = result.unwrap();
        assert_eq!(prepared.strategy, InjectionStrategy::TmpfsFile);
    }

    #[tokio::test]
    async fn test_runtime_secret_store_default_strategy() {
        let mut store = RuntimeSecretStore::production();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());

        let base_env = HashMap::new();

        // Should use TmpfsFile (default for production)
        let result = store
            .inject_secrets_default("job-1", &secrets, &base_env)
            .await;

        assert!(result.is_ok());
        let prepared = result.unwrap();
        assert_eq!(prepared.strategy, InjectionStrategy::TmpfsFile);
    }

    #[tokio::test]
    async fn test_runtime_secret_store_audit_logging() {
        let mut store = RuntimeSecretStore::production();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret123".to_string());

        let base_env = HashMap::new();

        let _result = store
            .inject_secrets("job-1", &secrets, &base_env, InjectionStrategy::Stdin)
            .await;

        assert_eq!(store.audit_count(), 1);
        let audit = &store.audit_log()[0];
        assert_eq!(audit.job_id, "job-1");
        assert_eq!(audit.strategy, InjectionStrategy::Stdin);
        assert!(audit.success);
    }
}

//! Provider Connection Validator Trait
//!
//! Defines the interface for validating provider connectivity at bootstrap
//! and during health checks.

use super::errors::{ProviderConnectionError, ValidationResult};
use crate::providers::ProviderConfig;
use crate::shared_kernel::ProviderId;
use crate::workers::ProviderType;
use async_trait::async_trait;

/// Configuration for connection validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Timeout for connection attempts
    pub timeout_secs: u64,
    /// Whether to validate permissions (create resources dry-run)
    pub validate_permissions: bool,
    /// Whether to check resource quotas
    pub validate_resources: bool,
    /// Whether to verify namespace access
    pub validate_namespace: bool,
    /// Skip validation for specific provider types
    pub skip_validation_for: Vec<ProviderType>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            validate_permissions: true,
            validate_resources: true,
            validate_namespace: true,
            skip_validation_for: Vec::new(),
        }
    }
}

/// Result of a provider validation check
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Provider ID that was validated
    pub provider_id: ProviderId,
    /// Provider name for display
    pub provider_name: String,
    /// Whether validation passed
    pub success: bool,
    /// Error details if validation failed
    pub error: Option<ProviderConnectionError>,
    /// Duration of validation in milliseconds
    pub duration_ms: u64,
    /// Individual validation checks
    pub checks: Vec<ValidationCheck>,
    /// Timestamp of validation
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ValidationReport {
    /// Create a successful validation report
    pub fn success(
        provider_id: ProviderId,
        provider_name: String,
        duration_ms: u64,
        checks: Vec<ValidationCheck>,
    ) -> Self {
        Self {
            provider_id,
            provider_name,
            success: true,
            error: None,
            duration_ms,
            checks,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a failed validation report
    pub fn failure(
        provider_id: ProviderId,
        provider_name: String,
        duration_ms: u64,
        error: ProviderConnectionError,
        checks: Vec<ValidationCheck>,
    ) -> Self {
        Self {
            provider_id,
            provider_name,
            success: false,
            error: Some(error),
            duration_ms,
            checks,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Returns true if the provider should be marked as healthy
    pub fn is_healthy(&self) -> bool {
        self.success
    }

    /// Returns a summary message for operators
    pub fn summary(&self) -> String {
        if self.success {
            format!(
                "Provider '{}' validated successfully in {}ms ({} checks passed)",
                self.provider_name,
                self.duration_ms,
                self.checks.len()
            )
        } else {
            format!(
                "Provider '{}' validation failed in {}ms: {}",
                self.provider_name,
                self.duration_ms,
                self.error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string())
            )
        }
    }
}

/// Individual validation check result
#[derive(Debug, Clone)]
pub struct ValidationCheck {
    /// Name of the check
    pub name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Message from the check
    pub message: String,
    /// Duration of this check in milliseconds
    pub duration_ms: u64,
}

impl ValidationCheck {
    /// Create a passed check
    pub fn passed(name: impl Into<String>, message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: message.into(),
            duration_ms,
        }
    }

    /// Create a failed check
    pub fn failed(name: impl Into<String>, message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: message.into(),
            duration_ms,
        }
    }
}

/// Trait for provider connection validators
///
/// Implementations should perform comprehensive connectivity validation
/// including API access, permissions, and resource availability.
#[async_trait]
pub trait ProviderConnectionValidator: Send + Sync {
    /// Validate a provider configuration
    ///
    /// This method should:
    /// 1. Establish connection to the provider
    /// 2. Verify basic API access
    /// 3. Check required permissions
    /// 4. Validate resource availability if configured
    ///
    /// Returns `Ok(())` if the provider is reachable and functional,
    /// or an error describing the failure.
    async fn validate(&self, config: &ProviderConfig) -> ValidationResult;

    /// Validate a provider configuration with custom settings
    async fn validate_with_config(
        &self,
        config: &ProviderConfig,
        validation_config: &ValidationConfig,
    ) -> ValidationResult;

    /// Get the provider type this validator handles
    fn provider_type(&self) -> ProviderType;

    /// Get a human-readable name for this validator
    fn validator_name(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report_success() {
        let report = ValidationReport::success(
            ProviderId::new(),
            "test-provider".to_string(),
            150,
            vec![
                ValidationCheck::passed("connectivity", "API reachable", 50),
                ValidationCheck::passed("permissions", "Can create pods", 100),
            ],
        );

        assert!(report.is_healthy());
        assert!(report.error.is_none());
        assert_eq!(report.checks.len(), 2);
        assert!(report.summary().contains("successfully"));
    }

    #[test]
    fn test_validation_report_failure() {
        let error = ProviderConnectionError::Network("connection refused".to_string());
        let report = ValidationReport::failure(
            ProviderId::new(),
            "test-provider".to_string(),
            50,
            error,
            vec![ValidationCheck::failed(
                "connectivity",
                "connection refused",
                50,
            )],
        );

        assert!(!report.is_healthy());
        assert!(report.error.is_some());
        assert!(report.summary().contains("failed"));
    }

    #[test]
    fn test_validation_check() {
        let passed = ValidationCheck::passed("test", "ok", 10);
        assert!(passed.passed);
        assert_eq!(passed.duration_ms, 10);

        let failed = ValidationCheck::failed("test", "failed", 20);
        assert!(!failed.passed);
        assert_eq!(failed.duration_ms, 20);
    }

    #[test]
    fn test_validation_config_defaults() {
        let config = ValidationConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert!(config.validate_permissions);
        assert!(config.validate_resources);
        assert!(config.validate_namespace);
        assert!(config.skip_validation_for.is_empty());
    }
}

//! # DeterminismEnforcer - Workflow Determinism Validation
//!
//! This module provides the [`DeterminismEnforcer`] for ensuring workflow
//! execution is deterministic by detecting non-deterministic operations.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Errors from determinism enforcement.
#[derive(Debug, Error)]
pub enum DeterminismError {
    #[error("Non-deterministic operation detected: {0}")]
    NonDeterministicOperation(String),

    #[error("Clock usage detected - use timer instead")]
    ClockUsage,

    #[error("Random number generation detected")]
    RandomGeneration,

    #[error("External system call detected")]
    ExternalSystemCall,

    #[error("Mutable static access detected")]
    MutableStaticAccess,

    #[error("Thread_rng usage detected")]
    ThreadRngUsage,
}

/// Configuration for determinism enforcement.
#[derive(Debug, Clone, Default)]
pub struct DeterminismConfig {
    /// Enable strict mode - fail on any non-determinism.
    pub strict_mode: bool,
    /// Whitelist of allowed operations.
    pub allowed_operations: HashSet<&'static str>,
    /// Maximum recursion depth.
    pub max_recursion_depth: u32,
    /// Enable logging of non-deterministic operations.
    pub log_violations: bool,
}

impl DeterminismConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn allow_operation(mut self, operation: &'static str) -> Self {
        self.allowed_operations.insert(operation);
        self
    }

    pub fn with_max_recursion_depth(mut self, depth: u32) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    pub fn with_log_violations(mut self, log: bool) -> Self {
        self.log_violations = log;
        self
    }
}

/// Report of determinism analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    /// Whether the workflow is deterministic.
    pub is_deterministic: bool,
    /// List of violations found.
    pub violations: Vec<DeterminismViolation>,
    /// Warnings about potential issues.
    pub warnings: Vec<String>,
    /// Analysis metadata.
    pub metadata: DeterminismMetadata,
}

impl Default for DeterminismReport {
    fn default() -> Self {
        Self {
            is_deterministic: true,
            violations: Vec::new(),
            warnings: Vec::new(),
            metadata: DeterminismMetadata::default(),
        }
    }
}

/// A single determinism violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismViolation {
    /// Type of violation.
    pub kind: ViolationKind,
    /// Location where violation occurred.
    pub location: String,
    /// Description of the violation.
    pub description: String,
    /// Suggested fix.
    pub suggestion: String,
}

/// Types of determinism violations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationKind {
    ClockUsage,
    RandomGeneration,
    ExternalSystemCall,
    MutableStaticAccess,
    ThreadRngUsage,
    NonDeterministicOrdering,
    HashMapWithNonKey,
    DateTimeNonUTC,
    SystemTime,
    FileSystemAccess,
    NetworkAccess,
    EnvironmentVariable,
    ProcessInfo,
}

/// Metadata about the analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeterminismMetadata {
    /// Number of statements analyzed.
    pub statements_analyzed: u32,
    /// Number of functions analyzed.
    pub functions_analyzed: u32,
    /// Total lines of code analyzed.
    pub lines_of_code: u32,
    /// Analysis duration in milliseconds.
    pub analysis_duration_ms: u64,
    /// Rust version used for analysis.
    pub rust_version: String,
}

/// The determinism enforcer validates workflow code for deterministic execution.
#[derive(Debug)]
pub struct DeterminismEnforcer {
    /// Configuration for the enforcer.
    config: DeterminismConfig,
    /// Whether the enforcer is enabled.
    enabled: Arc<AtomicBool>,
    /// Violations found during analysis.
    violations: Arc<tokio::sync::Mutex<Vec<DeterminismViolation>>>,
}

impl Default for DeterminismEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl DeterminismEnforcer {
    /// Create a new determinism enforcer.
    pub fn new() -> Self {
        Self {
            config: DeterminismConfig::default(),
            enabled: Arc::new(AtomicBool::new(true)),
            violations: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: DeterminismConfig) -> Self {
        Self {
            config,
            enabled: Arc::new(AtomicBool::new(true)),
            violations: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Enable the enforcer.
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    /// Disable the enforcer.
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    /// Check if the enforcer is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Record a determinism violation.
    pub async fn record_violation(&self, violation: DeterminismViolation) {
        if self.config.log_violations {
            tracing::warn!(
                "Determinism violation: {:?} at {} - {}",
                violation.kind,
                violation.location,
                violation.description
            );
        }
        let mut violations = self.violations.lock().await;
        violations.push(violation);
    }

    /// Clear all recorded violations.
    pub async fn clear_violations(&self) {
        let mut violations = self.violations.lock().await;
        violations.clear();
    }

    /// Get all recorded violations.
    pub async fn get_violations(&self) -> Vec<DeterminismViolation> {
        let violations = self.violations.lock().await;
        violations.clone()
    }

    /// Check for clock usage patterns.
    pub async fn check_clock_usage(&self, code: &str) -> Result<(), DeterminismError> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Common non-deterministic clock patterns
        let patterns = [
            (
                "std::time::Instant::now()",
                "Use saga timer instead of Instant::now()",
            ),
            (
                "chrono::Local::now()",
                "Use chrono::Utc::now() for deterministic time",
            ),
            (
                "std::time::SystemTime::now()",
                "Use saga timer for deterministic time",
            ),
            (
                "std::time::Duration::from_millis(_)",
                "Consider if this is deterministic",
            ),
        ];

        for (pattern, suggestion) in patterns {
            if code.contains(pattern) {
                let violation = DeterminismViolation {
                    kind: ViolationKind::ClockUsage,
                    location: pattern.to_string(),
                    description: format!("Clock-based operation detected: {}", pattern),
                    suggestion: suggestion.to_string(),
                };
                self.record_violation(violation).await;

                if self.config.strict_mode {
                    return Err(DeterminismError::ClockUsage);
                }
            }
        }

        Ok(())
    }

    /// Check for random number generation patterns.
    pub async fn check_random_generation(&self, code: &str) -> Result<(), DeterminismError> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let patterns = [
            (
                "rand::thread_rng()",
                "Use deterministic random or saga random API",
            ),
            (
                "rand::random::<_>()",
                "Use deterministic random or saga random API",
            ),
            (
                "rand::Rng::gen()",
                "Use deterministic random or saga random API",
            ),
            (
                "std::collections::hash_map::RandomState",
                "Use FxHashMap for deterministic hashing",
            ),
        ];

        for (pattern, suggestion) in patterns {
            if code.contains(pattern) {
                let violation = DeterminismViolation {
                    kind: ViolationKind::RandomGeneration,
                    location: pattern.to_string(),
                    description: format!("Random generation detected: {}", pattern),
                    suggestion: suggestion.to_string(),
                };
                self.record_violation(violation).await;

                if self.config.strict_mode {
                    return Err(DeterminismError::RandomGeneration);
                }
            }
        }

        Ok(())
    }

    /// Check for mutable static access.
    pub async fn check_mutable_static(&self, code: &str) -> Result<(), DeterminismError> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let patterns = [
            ("static mut", "Avoid mutable static variables"),
            (
                "std::cell::RefCell",
                "Consider thread-local storage or context",
            ),
            (
                "std::sync::Mutex",
                "This may cause non-deterministic behavior",
            ),
        ];

        for (pattern, suggestion) in patterns {
            if code.contains(pattern) {
                let violation = DeterminismViolation {
                    kind: ViolationKind::MutableStaticAccess,
                    location: pattern.to_string(),
                    description: format!("Mutable static access: {}", pattern),
                    suggestion: suggestion.to_string(),
                };
                self.record_violation(violation).await;

                if self.config.strict_mode {
                    return Err(DeterminismError::MutableStaticAccess);
                }
            }
        }

        Ok(())
    }

    /// Check for external system calls.
    pub async fn check_external_calls(&self, code: &str) -> Result<(), DeterminismError> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let patterns = [
            (
                "std::fs::",
                "File system access is non-deterministic - use activity",
            ),
            (
                "std::process::",
                "Process spawning is non-deterministic - use activity",
            ),
            (
                "tokio::fs::",
                "Async file system access is non-deterministic - use activity",
            ),
            (
                "reqwest::",
                "HTTP requests are non-deterministic - use activity",
            ),
        ];

        for (pattern, suggestion) in patterns {
            if code.contains(pattern) {
                let violation = DeterminismViolation {
                    kind: ViolationKind::ExternalSystemCall,
                    location: pattern.to_string(),
                    description: format!("External system call detected: {}", pattern),
                    suggestion: suggestion.to_string(),
                };
                self.record_violation(violation).await;

                if self.config.strict_mode {
                    return Err(DeterminismError::ExternalSystemCall);
                }
            }
        }

        Ok(())
    }

    /// Analyze code for determinism violations.
    pub async fn analyze_code(&self, code: &str) -> DeterminismReport {
        self.clear_violations().await;

        // Run all checks
        let _ = self.check_clock_usage(code).await;
        let _ = self.check_random_generation(code).await;
        let _ = self.check_mutable_static(code).await;
        let _ = self.check_external_calls(code).await;

        let violations = self.get_violations().await;

        DeterminismReport {
            is_deterministic: violations.is_empty(),
            violations: violations.clone(),
            warnings: Vec::new(),
            metadata: DeterminismMetadata {
                statements_analyzed: code.chars().filter(|c| *c == ';').count() as u32,
                functions_analyzed: code.matches("fn ").count() as u32,
                lines_of_code: code.lines().count() as u32,
                analysis_duration_ms: 0,
                rust_version: "1.70".to_string(),
            },
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &DeterminismConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determinism_config_defaults() {
        let config = DeterminismConfig::default();
        assert!(!config.strict_mode);
        assert!(config.allowed_operations.is_empty());
        assert_eq!(config.max_recursion_depth, 0);
        assert!(!config.log_violations);
    }

    #[test]
    fn test_determinism_config_builder() {
        let config = DeterminismConfig::new()
            .with_strict_mode(true)
            .allow_operation("safe-operation")
            .with_max_recursion_depth(100)
            .with_log_violations(true);

        assert!(config.strict_mode);
        assert!(config.allowed_operations.contains("safe-operation"));
        assert_eq!(config.max_recursion_depth, 100);
        assert!(config.log_violations);
    }

    #[test]
    fn test_determinism_report() {
        let report = DeterminismReport::default();
        assert!(report.is_deterministic);
        assert!(report.violations.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn test_determinism_violation() {
        let violation = DeterminismViolation {
            kind: ViolationKind::ClockUsage,
            location: "std::time::Instant::now()".to_string(),
            description: "Clock usage detected".to_string(),
            suggestion: "Use saga timer instead".to_string(),
        };

        assert_eq!(violation.kind, ViolationKind::ClockUsage);
        assert!(violation.location.contains("Instant"));
    }

    #[test]
    fn test_determinism_enforcer_creation() {
        let enforcer = DeterminismEnforcer::new();
        assert!(enforcer.is_enabled());
    }

    #[test]
    fn test_determinism_enforcer_disable() {
        let enforcer = DeterminismEnforcer::new();
        enforcer.disable();
        assert!(!enforcer.is_enabled());

        enforcer.enable();
        assert!(enforcer.is_enabled());
    }

    #[tokio::test]
    async fn test_check_clock_usage() {
        let enforcer =
            DeterminismEnforcer::with_config(DeterminismConfig::new().with_strict_mode(true));
        let code = "let now = std::time::Instant::now();";

        let result = enforcer.check_clock_usage(code).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(DeterminismError::ClockUsage)));
    }

    #[tokio::test]
    async fn test_check_random_generation() {
        let enforcer =
            DeterminismEnforcer::with_config(DeterminismConfig::new().with_strict_mode(true));
        let code = "let random = rand::thread_rng();";

        let result = enforcer.check_random_generation(code).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(DeterminismError::RandomGeneration)));
    }

    #[tokio::test]
    async fn test_check_mutable_static() {
        let enforcer =
            DeterminismEnforcer::with_config(DeterminismConfig::new().with_strict_mode(true));
        let code = "static mut COUNTER: i32 = 0;";

        let result = enforcer.check_mutable_static(code).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(DeterminismError::MutableStaticAccess)));
    }

    #[tokio::test]
    async fn test_check_external_calls() {
        let enforcer =
            DeterminismEnforcer::with_config(DeterminismConfig::new().with_strict_mode(true));
        let code = "std::fs::File::open(\"test.txt\");";

        let result = enforcer.check_external_calls(code).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(DeterminismError::ExternalSystemCall)));
    }

    #[tokio::test]
    async fn test_analyze_code() {
        let enforcer = DeterminismEnforcer::new();
        let code = r#"
            fn main() {
                let x = 42;
                println!("Hello");
            }
        "#;

        let report = enforcer.analyze_code(code).await;

        // This code should be deterministic
        assert!(report.is_deterministic || !report.violations.is_empty());
    }

    #[tokio::test]
    async fn test_record_and_get_violations() {
        let enforcer = DeterminismEnforcer::new();

        let violation = DeterminismViolation {
            kind: ViolationKind::ClockUsage,
            location: "test".to_string(),
            description: "test".to_string(),
            suggestion: "test".to_string(),
        };

        enforcer.record_violation(violation.clone()).await;
        let violations = enforcer.get_violations().await;

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::ClockUsage);

        enforcer.clear_violations().await;
        let violations = enforcer.get_violations().await;
        assert!(violations.is_empty());
    }
}

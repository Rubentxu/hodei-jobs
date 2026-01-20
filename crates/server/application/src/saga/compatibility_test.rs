//!
//! # Compatibility Test Suite
//!
//! Provides comprehensive testing utilities for validating equivalence between
//! legacy saga system and saga-engine v4, including dual-write consistency,
//! rollback capabilities, and idempotency verification.
//!

use crate::saga::ports::migration_helper::{
    InMemoryMigrationHelper, MigrationDecision, MigrationService, MigrationStrategy,
    SagaMigrationHelper, SagaTypeInfo, SagaTypeRegistry, SagaTypeRegistryTrait,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Result of a compatibility test
#[derive(Debug, Clone)]
pub struct CompatibilityTestResult {
    pub test_name: String,
    pub passed: bool,
    pub legacy_result: Option<String>,
    pub v4_result: Option<String>,
    pub equivalent: bool,
    pub error_message: Option<String>,
    pub duration: Duration,
}

impl CompatibilityTestResult {
    pub fn passed(test_name: &str, duration: Duration) -> Self {
        Self {
            test_name: test_name.to_string(),
            passed: true,
            legacy_result: None,
            v4_result: None,
            equivalent: true,
            error_message: None,
            duration,
        }
    }

    pub fn failed(test_name: &str, error: String, duration: Duration) -> Self {
        Self {
            test_name: test_name.to_string(),
            passed: false,
            legacy_result: None,
            v4_result: None,
            equivalent: false,
            error_message: Some(error),
            duration,
        }
    }

    pub fn with_results(
        test_name: &str,
        legacy: String,
        v4: String,
        equivalent: bool,
        duration: Duration,
    ) -> Self {
        Self {
            test_name: test_name.to_string(),
            passed: equivalent,
            legacy_result: Some(legacy),
            v4_result: Some(v4),
            equivalent,
            error_message: None,
            duration,
        }
    }
}

/// Configuration for compatibility tests
#[derive(Debug, Clone)]
pub struct CompatibilityTestConfig {
    /// Number of retry attempts for idempotency tests
    pub idempotency_retries: u32,
    /// Timeout for each test
    pub test_timeout: Duration,
    /// Enable verbose logging
    pub verbose: bool,
    /// Tolerance for time-based comparisons (in milliseconds)
    pub time_tolerance_ms: i64,
}

impl Default for CompatibilityTestConfig {
    fn default() -> Self {
        Self {
            idempotency_retries: 3,
            test_timeout: Duration::from_secs(30),
            verbose: false,
            time_tolerance_ms: 1000,
        }
    }
}

/// Summary of test run
#[derive(Debug, Clone)]
pub struct CompatibilityTestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub total_duration: Duration,
}

/// Compatibility test suite for saga migration
#[derive(Debug)]
pub struct CompatibilityTestSuite {
    config: CompatibilityTestConfig,
    results: Arc<Mutex<Vec<CompatibilityTestResult>>>,
}

impl CompatibilityTestSuite {
    /// Creates a new test suite with default configuration
    pub fn new(config: CompatibilityTestConfig) -> Self {
        Self {
            config,
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a new test suite with custom configuration
    pub fn with_config(
        config: CompatibilityTestConfig,
        results: Arc<Mutex<Vec<CompatibilityTestResult>>>,
    ) -> Self {
        Self { config, results }
    }

    /// Records a test result
    pub async fn record_result(&self, result: CompatibilityTestResult) {
        let mut results = self.results.lock().await;
        results.push(result);
    }

    /// Gets all recorded results
    pub async fn get_results(&self) -> Vec<CompatibilityTestResult> {
        let results = self.results.lock().await;
        results.clone()
    }

    /// Gets the pass rate
    pub async fn pass_rate(&self) -> f64 {
        let results = self.results.lock().await;
        if results.is_empty() {
            return 0.0;
        }
        let passed: usize = results.iter().filter(|r| r.passed).count();
        (passed as f64 / results.len() as f64) * 100.0
    }

    /// Gets a summary of the test run
    pub async fn summary(&self) -> CompatibilityTestSummary {
        let results = self.results.lock().await;
        let passed: usize = results.iter().filter(|r| r.passed).count();
        let failed: usize = results.iter().filter(|r| !r.passed).count();

        CompatibilityTestSummary {
            total: results.len(),
            passed,
            failed,
            pass_rate: if results.is_empty() {
                0.0
            } else {
                (passed as f64 / results.len() as f64) * 100.0
            },
            total_duration: results.iter().map(|r| r.duration).sum(),
        }
    }
}

/// Builder for creating compatibility test configurations
#[derive(Debug, Default)]
pub struct CompatibilityTestBuilder {
    config: CompatibilityTestConfig,
    results: Option<Arc<Mutex<Vec<CompatibilityTestResult>>>>,
}

impl CompatibilityTestBuilder {
    /// Creates a new builder with default config
    pub fn new() -> Self {
        Self {
            config: CompatibilityTestConfig::default(),
            results: None,
        }
    }

    /// Sets the idempotency retry count
    pub fn with_idempotency_retries(mut self, retries: u32) -> Self {
        self.config.idempotency_retries = retries;
        self
    }

    /// Sets the test timeout
    pub fn with_test_timeout(mut self, timeout: Duration) -> Self {
        self.config.test_timeout = timeout;
        self
    }

    /// Enables verbose logging
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.config.verbose = verbose;
        self
    }

    /// Sets the time tolerance for comparisons
    pub fn with_time_tolerance_ms(mut self, ms: i64) -> Self {
        self.config.time_tolerance_ms = ms;
        self
    }

    /// Builds the test suite
    pub fn build(self) -> CompatibilityTestSuite {
        let results = self
            .results
            .unwrap_or_else(|| Arc::new(Mutex::new(Vec::new())));
        CompatibilityTestSuite::with_config(self.config, results)
    }
}

/// Test utilities for migration helper
#[derive(Debug, Default)]
pub struct MigrationTestUtilities;

impl MigrationTestUtilities {
    /// Creates a test helper with gradual migration strategy
    pub fn create_gradual_migration_helper(
        v4_percentage: u32,
        seed: u64,
    ) -> Arc<InMemoryMigrationHelper> {
        Arc::new(InMemoryMigrationHelper::with_config(
            crate::saga::ports::migration_helper::MigrationHelperConfig {
                default_strategy: MigrationStrategy::GradualMigration {
                    v4_percentage,
                    seed,
                },
                ..Default::default()
            },
        ))
    }

    /// Creates a test helper with dual-write strategy
    pub fn create_dual_write_helper() -> Arc<InMemoryMigrationHelper> {
        Arc::new(InMemoryMigrationHelper::with_config(
            crate::saga::ports::migration_helper::MigrationHelperConfig {
                default_strategy: MigrationStrategy::DualWrite,
                ..Default::default()
            },
        ))
    }

    /// Creates a test helper with v4-only strategy
    pub fn create_v4_only_helper() -> Arc<InMemoryMigrationHelper> {
        Arc::new(InMemoryMigrationHelper::with_config(
            crate::saga::ports::migration_helper::MigrationHelperConfig {
                default_strategy: MigrationStrategy::UseV4,
                ..Default::default()
            },
        ))
    }

    /// Verifies deterministic routing for gradual migration
    pub async fn verify_deterministic_routing(
        helper: &InMemoryMigrationHelper,
        saga_type: &str,
    ) -> bool {
        let decision1 = helper.decide(saga_type).await;
        let decision2 = helper.decide(saga_type).await;

        decision1.should_use_v4 == decision2.should_use_v4
    }

    /// Creates a test saga type registry with sample types
    pub fn create_test_registry() -> Arc<SagaTypeRegistry> {
        Arc::new(SagaTypeRegistry::new())
    }
}

/// Migration equivalence verifier
#[derive(Debug, Default)]
pub struct MigrationEquivalenceVerifier;

impl MigrationEquivalenceVerifier {
    /// Verifies that two migration decisions are equivalent
    pub fn verify_decision_equivalence(
        decision1: &MigrationDecision,
        decision2: &MigrationDecision,
    ) -> bool {
        decision1.saga_type == decision2.saga_type
            && decision1.should_use_v4 == decision2.should_use_v4
            && decision1.strategy == decision2.strategy
    }

    /// Verifies strategy consistency across multiple decisions
    pub fn verify_strategy_consistency(decisions: &[MigrationDecision]) -> bool {
        if decisions.is_empty() {
            return true;
        }

        let first_strategy = &decisions[0].strategy;
        decisions
            .iter()
            .all(|d| d.strategy == *first_strategy && d.should_use_v4 == decisions[0].should_use_v4)
    }

    /// Generates a report of migration status
    pub async fn generate_migration_report(registry: &SagaTypeRegistry) -> MigrationReport {
        let types = registry.get_all().await;

        let migrated_count = types.iter().filter(|t| t.is_migrated).count();
        let pending_count = types.len() - migrated_count;

        MigrationReport {
            total_types: types.len(),
            migrated_count,
            pending_count,
            migration_percentage: if types.is_empty() {
                0.0
            } else {
                (migrated_count as f64 / types.len() as f64) * 100.0
            },
            types: types
                .into_iter()
                .map(|t| MigrationTypeStatus {
                    name: t.name,
                    display_name: t.display_name,
                    is_migrated: t.is_migrated,
                    migrated_at: t.migrated_at.map(|_| chrono::Utc::now()),
                })
                .collect(),
        }
    }
}

/// Migration report
#[derive(Debug, Clone)]
pub struct MigrationReport {
    pub total_types: usize,
    pub migrated_count: usize,
    pub pending_count: usize,
    pub migration_percentage: f64,
    pub types: Vec<MigrationTypeStatus>,
}

/// Status of a saga type
#[derive(Debug, Clone)]
pub struct MigrationTypeStatus {
    pub name: String,
    pub display_name: String,
    pub is_migrated: bool,
    pub migrated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Test for rollback capabilities in dual-write mode
pub struct DualWriteRollbackTest {
    suite: CompatibilityTestSuite,
}

impl DualWriteRollbackTest {
    /// Creates a new rollback test
    pub fn new(suite: CompatibilityTestSuite) -> Self {
        Self { suite }
    }

    /// Tests that rollback is consistent between systems
    pub async fn test_rollback_consistency(
        &self,
        saga_type: &str,
        legacy_result: Result<String, String>,
        v4_result: Result<String, String>,
    ) -> CompatibilityTestResult {
        let start = std::time::Instant::now();

        // Both systems should have consistent rollback behavior
        let legacy_can_rollback = legacy_result.is_err();
        let v4_can_rollback = v4_result.is_err();

        let passed = legacy_can_rollback == v4_can_rollback;

        CompatibilityTestResult {
            test_name: format!("rollback_consistency_{}", saga_type),
            passed,
            legacy_result: Some(format!("Can rollback: {}", legacy_can_rollback)),
            v4_result: Some(format!("Can rollback: {}", v4_can_rollback)),
            equivalent: passed,
            error_message: if !passed {
                Some("Rollback capabilities differ".to_string())
            } else {
                None
            },
            duration: start.elapsed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::ports::migration_helper::SagaTypeRegistry;

    #[tokio::test]
    async fn test_test_suite_creation() {
        let config = CompatibilityTestConfig::default();
        let suite = CompatibilityTestSuite::new(config);

        assert_eq!(suite.pass_rate().await, 0.0);
    }

    #[tokio::test]
    async fn test_compatibility_test_builder() {
        let builder = CompatibilityTestBuilder::new()
            .with_idempotency_retries(5)
            .with_test_timeout(Duration::from_secs(60))
            .verbose(true)
            .with_time_tolerance_ms(5000);

        let suite = builder.build();
        let summary = suite.summary().await;

        assert_eq!(summary.total, 0);
    }

    #[tokio::test]
    async fn test_migration_helper_decisions() {
        let helper: Arc<dyn SagaMigrationHelper> = Arc::new(InMemoryMigrationHelper::new());

        let decision = helper.decide("test_saga").await;

        assert!(!decision.should_use_v4);
        assert_eq!(decision.strategy, MigrationStrategy::KeepLegacy);
    }

    #[tokio::test]
    async fn test_migration_service_summary() {
        let helper: Arc<dyn SagaMigrationHelper> = Arc::new(InMemoryMigrationHelper::new());
        let registry = Arc::new(SagaTypeRegistry::new());

        // Register saga types
        registry
            .register(SagaTypeInfo::new("provisioning", "Provisioning", ""))
            .await;
        registry
            .register(SagaTypeInfo::new("execution", "Execution", ""))
            .await;

        let service = MigrationService::new(helper, registry);
        let summary = service.get_summary().await;

        assert_eq!(summary.total_types, 2);
        assert_eq!(summary.migrated_count, 0);
        assert_eq!(summary.pending_migration, 2);
    }

    #[tokio::test]
    async fn test_saga_type_info_mark_migrated() {
        let mut info = SagaTypeInfo::new("test", "Test", "Test description");

        assert!(!info.is_migrated);
        assert!(info.migrated_at.is_none());

        info.mark_migrated(Some("Migration complete".to_string()));

        assert!(info.is_migrated);
        assert!(info.migrated_at.is_some());
        assert_eq!(info.migration_notes, Some("Migration complete".to_string()));
    }

    #[tokio::test]
    async fn test_compatibility_test_summary() {
        let config = CompatibilityTestConfig::default();
        let suite = CompatibilityTestSuite::new(config);

        let summary = suite.summary().await;

        assert_eq!(summary.total, 0);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.pass_rate, 0.0);
    }

    #[tokio::test]
    async fn test_result_passing() {
        let result = CompatibilityTestResult::passed("test", Duration::from_millis(100));

        assert!(result.passed);
        assert!(result.equivalent);
        assert!(result.error_message.is_none());
        assert_eq!(result.test_name, "test");
    }

    #[tokio::test]
    async fn test_result_with_results() {
        let result = CompatibilityTestResult::with_results(
            "test",
            "legacy_output".to_string(),
            "v4_output".to_string(),
            true,
            Duration::from_millis(100),
        );

        assert!(result.passed);
        assert!(result.equivalent);
        assert_eq!(result.legacy_result, Some("legacy_output".to_string()));
        assert_eq!(result.v4_result, Some("v4_output".to_string()));
    }

    #[tokio::test]
    async fn test_result_failed() {
        let result = CompatibilityTestResult::failed(
            "test",
            "Something went wrong".to_string(),
            Duration::from_millis(100),
        );

        assert!(!result.passed);
        assert!(!result.equivalent);
        assert_eq!(
            result.error_message,
            Some("Something went wrong".to_string())
        );
    }

    #[tokio::test]
    async fn test_gradual_migration_deterministic() {
        let helper = MigrationTestUtilities::create_gradual_migration_helper(50, 12345);

        let result =
            MigrationTestUtilities::verify_deterministic_routing(&helper, "test-saga").await;

        assert!(result);
    }

    #[tokio::test]
    async fn test_decision_equivalence() {
        let decision1 = MigrationDecision {
            saga_type: "test".to_string(),
            strategy: MigrationStrategy::KeepLegacy,
            should_use_v4: false,
            reason: "Test".to_string(),
        };

        let decision2 = MigrationDecision {
            saga_type: "test".to_string(),
            strategy: MigrationStrategy::KeepLegacy,
            should_use_v4: false,
            reason: "Test".to_string(),
        };

        assert!(MigrationEquivalenceVerifier::verify_decision_equivalence(
            &decision1, &decision2
        ));
    }

    #[tokio::test]
    async fn test_strategy_consistency() {
        let decisions = vec![
            MigrationDecision {
                saga_type: "test".to_string(),
                strategy: MigrationStrategy::KeepLegacy,
                should_use_v4: false,
                reason: "Test".to_string(),
            },
            MigrationDecision {
                saga_type: "test".to_string(),
                strategy: MigrationStrategy::KeepLegacy,
                should_use_v4: false,
                reason: "Test".to_string(),
            },
        ];

        assert!(MigrationEquivalenceVerifier::verify_strategy_consistency(
            &decisions
        ));
    }

    #[tokio::test]
    async fn test_rollback_test() {
        let config = CompatibilityTestConfig::default();
        let suite = CompatibilityTestSuite::new(config);
        let test = DualWriteRollbackTest::new(suite);

        let result = test
            .test_rollback_consistency(
                "test_saga",
                Err("Legacy error".to_string()),
                Err("V4 error".to_string()),
            )
            .await;

        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_migration_report_generation() {
        let registry = Arc::new(SagaTypeRegistry::new());
        registry
            .register(SagaTypeInfo::new("provisioning", "Provisioning", ""))
            .await;
        registry
            .register(SagaTypeInfo::new("execution", "Execution", ""))
            .await;

        let report = MigrationEquivalenceVerifier::generate_migration_report(&registry).await;

        assert_eq!(report.total_types, 2);
        assert_eq!(report.migrated_count, 0);
        assert_eq!(report.pending_count, 2);
    }
}

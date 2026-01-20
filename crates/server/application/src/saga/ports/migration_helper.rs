//!
//! # Saga Migration Helper
//!
//! Provides utilities for managing saga migration between legacy system
//! and saga-engine v4, including strategy selection and coordination.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Migration strategy for saga execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MigrationStrategy {
    /// Use legacy system only
    KeepLegacy,
    /// Use saga-engine v4 only
    UseV4,
    /// Write to both systems, read from legacy
    DualWrite,
    /// Write to both systems, read from v4
    DualWriteReadV4,
    /// Gradually migrate workflows
    GradualMigration {
        /// Percentage of new workflows to route to v4 (0-100)
        v4_percentage: u32,
        /// Seed for deterministic routing
        seed: u64,
    },
}

impl Default for MigrationStrategy {
    fn default() -> Self {
        MigrationStrategy::KeepLegacy
    }
}

/// Configuration for migration helper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationHelperConfig {
    /// Default strategy for new workflows
    pub default_strategy: MigrationStrategy,
    /// Strategy overrides by saga type
    pub strategy_overrides: HashMap<String, MigrationStrategy>,
    /// Whether to auto-migrate on strategy change
    pub auto_migrate: bool,
    /// Feature flag for v4
    pub v4_enabled: bool,
}

impl Default for MigrationHelperConfig {
    fn default() -> Self {
        Self {
            default_strategy: MigrationStrategy::KeepLegacy,
            strategy_overrides: HashMap::new(),
            auto_migrate: false,
            v4_enabled: false,
        }
    }
}

/// Result of migration decision
#[derive(Debug, Clone)]
pub struct MigrationDecision {
    pub saga_type: String,
    pub strategy: MigrationStrategy,
    pub should_use_v4: bool,
    pub reason: String,
}

/// Helper for making migration decisions
#[async_trait]
pub trait SagaMigrationHelper: Send + Sync {
    /// Decide which system to use for a new workflow
    async fn decide(&self, saga_type: &str) -> MigrationDecision;

    /// Get current strategy for a saga type
    async fn get_strategy(&self, saga_type: &str) -> MigrationStrategy;

    /// Set strategy for a saga type
    async fn set_strategy(&self, saga_type: &str, strategy: MigrationStrategy);

    /// Check if v4 is enabled globally
    fn is_v4_enabled(&self) -> bool;

    /// Enable/disable v4 globally
    fn set_v4_enabled(&self, enabled: bool);
}

/// In-memory implementation of migration helper
#[derive(Debug)]
pub struct InMemoryMigrationHelper {
    config: Arc<RwLock<MigrationHelperConfig>>,
    decision_history: Arc<RwLock<Vec<MigrationDecision>>>,
}

impl InMemoryMigrationHelper {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(MigrationHelperConfig::default())),
            decision_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_config(config: MigrationHelperConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            decision_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl SagaMigrationHelper for InMemoryMigrationHelper {
    async fn decide(&self, saga_type: &str) -> MigrationDecision {
        let config = self.config.read().await;
        let strategy = config
            .strategy_overrides
            .get(saga_type)
            .cloned()
            .unwrap_or_else(|| config.default_strategy.clone());

        let (should_use_v4, reason) = match &strategy {
            MigrationStrategy::KeepLegacy => (false, "Strategy: KeepLegacy".to_string()),
            MigrationStrategy::UseV4 => (true, "Strategy: UseV4".to_string()),
            MigrationStrategy::DualWrite => {
                (false, "Strategy: DualWrite (read from legacy)".to_string())
            }
            MigrationStrategy::DualWriteReadV4 => (true, "Strategy: DualWriteReadV4".to_string()),
            MigrationStrategy::GradualMigration {
                v4_percentage,
                seed,
            } => {
                let use_v4 = self.should_route_to_v4(saga_type, *v4_percentage, *seed);
                if use_v4 {
                    (
                        true,
                        format!("GradualMigration: {}% routing to v4", v4_percentage),
                    )
                } else {
                    (
                        false,
                        format!(
                            "GradualMigration: {}% routing to legacy",
                            100 - v4_percentage
                        ),
                    )
                }
            }
        };

        let decision = MigrationDecision {
            saga_type: saga_type.to_string(),
            strategy: strategy.clone(),
            should_use_v4,
            reason: reason.clone(),
        };

        // Record decision
        let mut history = self.decision_history.write().await;
        history.push(decision.clone());

        decision
    }

    async fn get_strategy(&self, saga_type: &str) -> MigrationStrategy {
        let config = self.config.read().await;
        config
            .strategy_overrides
            .get(saga_type)
            .cloned()
            .unwrap_or_else(|| config.default_strategy.clone())
    }

    async fn set_strategy(&self, saga_type: &str, strategy: MigrationStrategy) {
        let mut config = self.config.write().await;
        config
            .strategy_overrides
            .insert(saga_type.to_string(), strategy);
    }

    fn is_v4_enabled(&self) -> bool {
        // Check both the config and the runtime flag
        // If v4 is not enabled in config, we always return false
        // This provides a global kill switch
        self.config
            .try_read()
            .map(|config| config.v4_enabled)
            .unwrap_or(false)
    }

    fn set_v4_enabled(&self, enabled: bool) {
        if let Ok(mut config) = self.config.try_write() {
            config.v4_enabled = enabled;
        }
    }
}

impl InMemoryMigrationHelper {
    /// Determine if a workflow should route to v4 based on gradual migration strategy
    fn should_route_to_v4(&self, saga_type: &str, v4_percentage: u32, seed: u64) -> bool {
        if v4_percentage == 0 {
            return false;
        }
        if v4_percentage >= 100 {
            return true;
        }

        // Create a deterministic hash from saga_type and seed
        let hash = self.create_hash(saga_type, seed);
        let bucket = hash % 100;

        bucket < v4_percentage as u64
    }

    /// Simple hash function for deterministic routing
    fn create_hash(&self, input: &str, seed: u64) -> u64 {
        let mut hash = seed;
        for byte in input.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }
}

/// Registry for saga type information
#[derive(Debug, Default)]
pub struct SagaTypeRegistry {
    types: Arc<RwLock<HashMap<String, SagaTypeInfo>>>,
}

impl SagaTypeRegistry {
    pub fn new() -> Self {
        Self {
            types: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a saga type
    pub async fn register(&self, saga_type: SagaTypeInfo) {
        let mut types = self.types.write().await;
        types.insert(saga_type.name.clone(), saga_type);
    }

    /// Get saga type info
    pub async fn get(&self, name: &str) -> Option<SagaTypeInfo> {
        let types = self.types.read().await;
        types.get(name).cloned()
    }

    /// Get all registered types
    pub async fn get_all(&self) -> Vec<SagaTypeInfo> {
        let types = self.types.read().await;
        types.values().cloned().collect()
    }

    /// Check if a saga type is registered
    pub async fn is_registered(&self, name: &str) -> bool {
        let types = self.types.read().await;
        types.contains_key(name)
    }
}

/// Information about a saga type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaTypeInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub is_migrated: bool,
    pub migrated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub migration_notes: Option<String>,
}

impl SagaTypeInfo {
    pub fn new(name: &str, display_name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            is_migrated: false,
            migrated_at: None,
            migration_notes: None,
        }
    }

    /// Mark the saga type as migrated
    pub fn mark_migrated(&mut self, notes: Option<String>) {
        self.is_migrated = true;
        self.migrated_at = Some(chrono::Utc::now());
        self.migration_notes = notes;
    }
}

/// Combined migration service that integrates helper and registry
pub struct MigrationService {
    helper: Arc<dyn SagaMigrationHelper>,
    registry: Arc<SagaTypeRegistry>,
}

impl MigrationService {
    pub fn new(helper: Arc<dyn SagaMigrationHelper>, registry: Arc<SagaTypeRegistry>) -> Self {
        Self { helper, registry }
    }

    /// Decide and record migration decision with saga type info
    pub async fn decide_with_info(
        &self,
        saga_type: &str,
    ) -> (MigrationDecision, Option<SagaTypeInfo>) {
        let decision = self.helper.decide(saga_type).await;
        let type_info = self.registry.get(saga_type).await;

        (decision, type_info)
    }

    /// Get summary of all registered saga types
    pub async fn get_summary(&self) -> MigrationSummary {
        let types = self.registry.get_all().await;
        let migrated_count = types.iter().filter(|t| t.is_migrated).count();

        MigrationSummary {
            total_types: types.len(),
            migrated_count,
            pending_migration: types.len() - migrated_count,
            migration_percentage: if types.is_empty() {
                0.0
            } else {
                (migrated_count as f64 / types.len() as f64) * 100.0
            },
            types,
        }
    }
}

/// Summary of migration status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSummary {
    pub total_types: usize,
    pub migrated_count: usize,
    pub pending_migration: usize,
    pub migration_percentage: f64,
    pub types: Vec<SagaTypeInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keep_legacy_strategy() {
        let helper = Arc::new(InMemoryMigrationHelper::new());

        let decision = helper.decide("provisioning").await;

        assert!(!decision.should_use_v4);
        assert_eq!(decision.strategy, MigrationStrategy::KeepLegacy);
    }

    #[tokio::test]
    async fn test_use_v4_strategy() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::UseV4,
                ..Default::default()
            },
        ));

        let decision = helper.decide("execution").await;

        assert!(decision.should_use_v4);
        assert_eq!(decision.strategy, MigrationStrategy::UseV4);
    }

    #[tokio::test]
    async fn test_strategy_override() {
        let helper = Arc::new(InMemoryMigrationHelper::new());

        // Override strategy for specific saga type
        helper
            .set_strategy("provisioning", MigrationStrategy::UseV4)
            .await;

        let decision = helper.decide("provisioning").await;
        assert!(decision.should_use_v4);

        // Other types should still use default
        let decision = helper.decide("execution").await;
        assert!(!decision.should_use_v4);
    }

    #[tokio::test]
    async fn test_dual_write_strategy() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::DualWrite,
                ..Default::default()
            },
        ));

        let decision = helper.decide("recovery").await;

        assert!(!decision.should_use_v4); // Read from legacy in dual-write
        assert_eq!(decision.strategy, MigrationStrategy::DualWrite);
    }

    #[tokio::test]
    async fn test_dual_write_read_v4_strategy() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::DualWriteReadV4,
                ..Default::default()
            },
        ));

        let decision = helper.decide("cancellation").await;

        assert!(decision.should_use_v4); // Read from v4 in dual-write
        assert_eq!(decision.strategy, MigrationStrategy::DualWriteReadV4);
    }

    #[tokio::test]
    async fn test_gradual_migration_0_percent() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::GradualMigration {
                    v4_percentage: 0,
                    seed: 12345,
                },
                ..Default::default()
            },
        ));

        // All should go to legacy with 0%
        for _ in 0..10 {
            let decision = helper.decide("test").await;
            assert!(!decision.should_use_v4);
        }
    }

    #[tokio::test]
    async fn test_gradual_migration_100_percent() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::GradualMigration {
                    v4_percentage: 100,
                    seed: 12345,
                },
                ..Default::default()
            },
        ));

        // All should go to v4 with 100%
        for _ in 0..10 {
            let decision = helper.decide("test").await;
            assert!(decision.should_use_v4);
        }
    }

    #[tokio::test]
    async fn test_gradual_migration_deterministic() {
        let helper = Arc::new(InMemoryMigrationHelper::with_config(
            MigrationHelperConfig {
                default_strategy: MigrationStrategy::GradualMigration {
                    v4_percentage: 50,
                    seed: 12345,
                },
                ..Default::default()
            },
        ));

        // Same saga type should always route to same system
        let first_decision = helper.decide("deterministic-saga").await;
        let second_decision = helper.decide("deterministic-saga").await;

        assert_eq!(first_decision.should_use_v4, second_decision.should_use_v4);
    }

    #[tokio::test]
    async fn test_saga_type_registry() {
        let registry = Arc::new(SagaTypeRegistry::new());

        let info = SagaTypeInfo::new(
            "provisioning",
            "Provisioning Saga",
            "Handles worker provisioning",
        );
        registry.register(info).await;

        let retrieved = registry.get("provisioning").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "provisioning");
    }

    #[tokio::test]
    async fn test_migration_service_summary() {
        let helper: Arc<dyn SagaMigrationHelper> = Arc::new(InMemoryMigrationHelper::new());
        let registry = Arc::new(SagaTypeRegistry::new());

        // Register some saga types
        registry
            .register(SagaTypeInfo::new("provisioning", "Provisioning", ""))
            .await;
        registry
            .register(SagaTypeInfo::new("execution", "Execution", ""))
            .await;

        let service = MigrationService::new(helper, registry);
        let summary = service.get_summary().await;

        assert_eq!(summary.total_types, 2);
        assert_eq!(summary.pending_migration, 2);
        assert_eq!(summary.migrated_count, 0);
    }

    #[tokio::test]
    async fn test_v4_enabled_flag() {
        let helper = Arc::new(InMemoryMigrationHelper::new());

        // Initially disabled
        assert!(!helper.is_v4_enabled());

        // Enable v4
        helper.set_v4_enabled(true);
        assert!(helper.is_v4_enabled());

        // Disable v4
        helper.set_v4_enabled(false);
        assert!(!helper.is_v4_enabled());
    }
}

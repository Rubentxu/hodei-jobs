//!
//! # Saga Migration Flags (US-94.9)
//!
//! This module provides feature flags for controlling saga migration
//! from legacy to saga-engine v4.0.
//!
//! # Flags
//!
//! - `use_v4_for_new_workflows`: Use v4 for new workflow instances
//! - `migrate_existing_workflows`: Strategy for migrating existing workflows
//! - `enable_dual_write`: Write to both legacy and v4
//! - `read_from_v4_only`: Only read from v4 (no legacy reads)
//!
//! # Migration Strategies
//!
//! - `KeepLegacy`: Don't migrate, use legacy only
//! - `UseV4`: Migrate all workflows to v4
//! - `DualWrite`: Write to both legacy and v4
//! - `DualWriteReadV4`: Write to both, read from v4
//! - `GradualMigration`: Migrate percentage of workflows

use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::Debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaMigrationFlags {
    /// Use saga-engine v4.0 for new workflow instances
    pub use_v4_for_new_workflows: bool,

    /// Strategy for migrating existing workflows
    pub use_v4_for_existing_workflows: MigrationStrategy,

    /// Enable dual-write mode (write to both legacy and v4)
    pub enable_dual_write: bool,

    /// Only read from v4 (no legacy reads after migration)
    pub read_from_v4_only: bool,

    /// Percentage of workflows to migrate in gradual migration
    pub gradual_migration_percentage: Option<u8>,
}

impl SagaMigrationFlags {
    /// Create migration flags from environment variables
    ///
    /// Environment Variables:
    /// - `SAGA_USE_V4_NEW`: "true" to use v4 for new workflows
    /// - `SAGA_MIGRATION_STRATEGY`: "keep-legacy", "use-v4", "dual-write", "dual-write-read-v4", "gradual:<percentage>"
    /// - `SAGA_DUAL_WRITE`: "true" to enable dual-write
    /// - `SAGA_V4_ONLY`: "true" to only read from v4
    /// - `SAGA_MIGRATION_PERCENTAGE`: "0-100" for gradual migration
    pub fn from_env() -> Self {
        Self {
            use_v4_for_new_workflows: env::var("SAGA_USE_V4_NEW")
                .map(|v| v == "true")
                .unwrap_or(false),

            use_v4_for_existing_workflows: env::var("SAGA_MIGRATION_STRATEGY")
                .ok()
                .and_then(|s| match s.as_str() {
                    "keep-legacy" => Some(MigrationStrategy::KeepLegacy),
                    "use-v4" => Some(MigrationStrategy::UseV4),
                    "dual-write" => Some(MigrationStrategy::DualWrite),
                    "dual-write-read-v4" => Some(MigrationStrategy::DualWriteReadV4),
                    _ if s.starts_with("gradual:") => s
                        .strip_prefix("gradual:")
                        .and_then(|percent| percent.parse::<u8>().ok())
                        .map(|p| MigrationStrategy::GradualMigration { percentage: p }),
                    _ => None,
                })
                .unwrap_or(MigrationStrategy::KeepLegacy),

            enable_dual_write: env::var("SAGA_DUAL_WRITE")
                .map(|v| v == "true")
                .unwrap_or(false),

            read_from_v4_only: env::var("SAGA_V4_ONLY")
                .map(|v| v == "true")
                .unwrap_or(false),

            gradual_migration_percentage: env::var("SAGA_MIGRATION_PERCENTAGE")
                .ok()
                .and_then(|s| s.parse::<u8>().ok()),
        }
    }

    /// Determine if v4 should be used for a given saga type
    ///
    /// Returns true if v4 should be used based on migration strategy
    /// and gradual migration percentage.
    pub fn should_use_v4(&self, saga_type: &str) -> bool {
        match self.use_v4_for_existing_workflows {
            MigrationStrategy::KeepLegacy => false,
            MigrationStrategy::UseV4 => true,
            MigrationStrategy::DualWrite => true,
            MigrationStrategy::DualWriteReadV4 => true,
            MigrationStrategy::GradualMigration { percentage } => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                saga_type.hash(&mut hasher);
                let hash = hasher.finish();
                // Avoid overflow by dividing first
                let threshold = u64::MAX / 100 * percentage as u64;
                hash < threshold
            }
        }
    }

    /// Check if v4-only mode is enabled
    pub fn is_v4_only(&self) -> bool {
        self.read_from_v4_only
    }

    /// Check if dual-write mode is enabled
    pub fn is_dual_write_enabled(&self) -> bool {
        self.enable_dual_write
    }

    /// Safe defaults for development
    pub fn dev_defaults() -> Self {
        Self {
            use_v4_for_new_workflows: false,
            use_v4_for_existing_workflows: MigrationStrategy::KeepLegacy,
            enable_dual_write: false,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        }
    }

    /// Safe defaults for production (conservative)
    pub fn prod_defaults() -> Self {
        Self {
            use_v4_for_new_workflows: false,
            use_v4_for_existing_workflows: MigrationStrategy::KeepLegacy,
            enable_dual_write: true,
            read_from_v4_only: false,
            gradual_migration_percentage: Some(0), // Start with 0% migrated
        }
    }
}

impl Default for SagaMigrationFlags {
    fn default() -> Self {
        Self::dev_defaults()
    }
}

/// Migration strategy for existing workflows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MigrationStrategy {
    /// Keep using legacy implementation
    KeepLegacy,

    /// Migrate all workflows to v4
    UseV4,

    /// Write to both legacy and v4 (dual-write)
    DualWrite,

    /// Write to both legacy and v4, but only read from v4
    DualWriteReadV4,

    /// Gradually migrate workflows based on percentage
    GradualMigration { percentage: u8 },
}

impl MigrationStrategy {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "keep-legacy" => Some(Self::KeepLegacy),
            "use-v4" => Some(Self::UseV4),
            "dual-write" => Some(Self::DualWrite),
            "dual-write-read-v4" => Some(Self::DualWriteReadV4),
            _ if s.starts_with("gradual:") => s
                .strip_prefix("gradual:")
                .and_then(|percent| percent.parse::<u8>().ok())
                .map(|p| Self::GradualMigration { percentage: p }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_flags_from_env() {
        // Test with all environment variables unset
        let flags = SagaMigrationFlags::from_env();
        assert!(!flags.use_v4_for_new_workflows);
        assert_eq!(
            flags.use_v4_for_existing_workflows,
            MigrationStrategy::KeepLegacy
        );
        assert!(!flags.enable_dual_write);
        assert!(!flags.read_from_v4_only);
        assert!(flags.gradual_migration_percentage.is_none());
    }

    #[test]
    fn test_should_use_v4_keep_legacy() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::KeepLegacy,
            enable_dual_write: false,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        };

        assert!(!flags.should_use_v4("provisioning"));
        assert!(!flags.should_use_v4("execution"));
        assert!(!flags.should_use_v4("recovery"));
        assert!(!flags.should_use_v4("cancellation"));
        assert!(!flags.should_use_v4("timeout"));
        assert!(!flags.should_use_v4("cleanup"));
    }

    #[test]
    fn test_should_use_v4_use_v4() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::UseV4,
            enable_dual_write: false,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        };

        assert!(flags.should_use_v4("provisioning"));
        assert!(flags.should_use_v4("execution"));
        assert!(flags.should_use_v4("recovery"));
        assert!(flags.should_use_v4("cancellation"));
        assert!(flags.should_use_v4("timeout"));
        assert!(flags.should_use_v4("cleanup"));
    }

    #[test]
    fn test_should_use_v4_dual_write() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::DualWrite,
            enable_dual_write: true,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        };

        assert!(flags.should_use_v4("provisioning"));
        assert!(flags.should_use_v4("execution"));
        assert!(flags.should_use_v4("recovery"));
        assert!(flags.should_use_v4("cancellation"));
        assert!(flags.should_use_v4("timeout"));
        assert!(flags.should_use_v4("cleanup"));
        assert!(flags.is_dual_write_enabled());
    }

    #[test]
    fn test_should_use_v4_gradual_migration() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::GradualMigration { percentage: 50 },
            enable_dual_write: false,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        };

        // Test that different saga types hash to different values
        let use_v4_count = [
            flags.should_use_v4("provisioning"),
            flags.should_use_v4("execution"),
            flags.should_use_v4("recovery"),
            flags.should_use_v4("cancellation"),
            flags.should_use_v4("timeout"),
            flags.should_use_v4("cleanup"),
        ]
        .iter()
        .filter(|b| **b)
        .count();

        // With 50% threshold, approximately half should use v4
        assert!(use_v4_count > 0 && use_v4_count < 6);
    }

    #[test]
    fn test_migration_strategy_from_str() {
        assert_eq!(
            MigrationStrategy::from_str("keep-legacy"),
            Some(MigrationStrategy::KeepLegacy)
        );
        assert_eq!(
            MigrationStrategy::from_str("use-v4"),
            Some(MigrationStrategy::UseV4)
        );
        assert_eq!(
            MigrationStrategy::from_str("dual-write"),
            Some(MigrationStrategy::DualWrite)
        );
        assert_eq!(
            MigrationStrategy::from_str("dual-write-read-v4"),
            Some(MigrationStrategy::DualWriteReadV4)
        );
        assert_eq!(
            MigrationStrategy::from_str("gradual:25"),
            Some(MigrationStrategy::GradualMigration { percentage: 25 })
        );
        assert_eq!(MigrationStrategy::from_str("invalid"), None);
    }

    #[test]
    fn test_dev_defaults() {
        let flags = SagaMigrationFlags::dev_defaults();
        assert!(!flags.use_v4_for_new_workflows);
        assert_eq!(
            flags.use_v4_for_existing_workflows,
            MigrationStrategy::KeepLegacy
        );
        assert!(!flags.enable_dual_write);
        assert!(!flags.read_from_v4_only);
    }

    #[test]
    fn test_prod_defaults() {
        let flags = SagaMigrationFlags::prod_defaults();
        assert!(!flags.use_v4_for_new_workflows);
        assert_eq!(
            flags.use_v4_for_existing_workflows,
            MigrationStrategy::KeepLegacy
        );
        assert!(flags.enable_dual_write);
        assert!(!flags.read_from_v4_only);
        assert_eq!(flags.gradual_migration_percentage, Some(0));
    }

    #[test]
    fn test_is_v4_only() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::UseV4,
            enable_dual_write: false,
            read_from_v4_only: true,
            gradual_migration_percentage: None,
        };

        assert!(flags.is_v4_only());
    }

    #[test]
    fn test_is_dual_write_enabled() {
        let flags = SagaMigrationFlags {
            use_v4_for_new_workflows: true,
            use_v4_for_existing_workflows: MigrationStrategy::DualWrite,
            enable_dual_write: true,
            read_from_v4_only: false,
            gradual_migration_percentage: None,
        };

        assert!(flags.is_dual_write_enabled());
    }
}

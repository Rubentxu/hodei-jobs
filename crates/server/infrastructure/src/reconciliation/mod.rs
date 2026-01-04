//! Reconciliation module - Database and infrastructure cleanup
//!
//! This module provides components for automated cleanup and reconciliation:
//! - DatabaseReaper: Marks stuck jobs and workers as failed/terminated
//! - InfrastructureReconciler: Destroys orphaned infrastructure and recovers jobs
//!
//! EPIC-43: Sprint 4 - Reconciliación (Red de Seguridad)

pub mod database_reaper;
pub mod infrastructure_reconciler;
pub mod monitoring;

pub use database_reaper::{DatabaseReaper, DatabaseReaperConfig, DatabaseReaperResult};
pub use infrastructure_reconciler::{
    InfrastructureReconciler, InfrastructureReconcilerConfig, ReconciliationResult,
};
pub use monitoring::{Alert, AlertConfig, AlertEvaluator, AlertSeverity, ReconcilerMonitoring};

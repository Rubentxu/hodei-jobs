//! Job Scheduler - Estrategias de selección de workers y providers
//!
//! ## EPIC-022: Strategy Composition
//! This module contains the selector traits and implementations for worker
//! and provider selection.
//!
//! ## Selector Traits
//! - `WorkerSelector`: Trait for selecting workers
//! - `ProviderSelector`: Trait for selecting providers
//!
//! ## Provider Selectors
//! - `LowestCostProviderSelector`: Cost-optimized selection (uses CostScoring)
//! - `FastestStartupProviderSelector`: Performance-optimized selection (uses StartupTimeScoring)
//! - `MostCapacityProviderSelector`: Capacity-optimized selection (uses CapacityScoring)
//! - `HealthiestProviderSelector`: Health-optimized selection (uses CompositeProviderScoring)
//!
//! ## Worker Selectors
//! - `FirstAvailableWorkerSelector`: First available worker
//! - `LeastLoadedWorkerSelector`: Worker with fewest jobs

use crate::jobs::Job;
use crate::scheduling::scoring::{
    CapacityScoring, CostScoring, HealthScoring, ProviderScoreInput, ProviderScoring,
    StartupTimeScoring,
};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::Worker;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// =============================================================================
// Scheduling Types (re-exported from here to avoid duplication)
// =============================================================================

/// Resultado de una decisión de scheduling
///
/// ## Ephemeral Workers Model (EPIC-21)
/// In the ephemeral model, `ProvisionWorker` is the primary decision.
/// Workers are NOT reused between jobs.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedulingDecision {
    /// Assign job to an already-provisioned worker (for retry/recovery scenarios only)
    /// NOTE: In ephemeral model, this is ONLY used when the worker was provisioned
    /// specifically for this job but assignment failed and needs retry.
    AssignToWorker { job_id: JobId, worker_id: WorkerId },
    /// Provision a NEW ephemeral worker for the job (primary decision path)
    ProvisionWorker {
        job_id: JobId,
        provider_id: ProviderId,
    },
    /// Encolar job para procesamiento posterior
    Enqueue { job_id: JobId, reason: String },
    /// Rechazar job (no hay recursos disponibles)
    Reject { job_id: JobId, reason: String },
}

/// Contexto para tomar decisiones de scheduling
#[derive(Debug, Clone)]
pub struct SchedulingContext {
    /// Job a programar
    pub job: Job,
    /// Preferencias del job (ciudadano de primera clase)
    pub job_preferences: crate::jobs::JobPreferences,
    /// Workers disponibles
    pub available_workers: Vec<Worker>,
    /// Providers disponibles con capacidad
    pub available_providers: Vec<ProviderInfo>,
    /// Jobs en cola esperando
    pub pending_jobs_count: usize,
    /// Carga actual del sistema (0.0 - 1.0)
    pub system_load: f64,
}

/// Información de un provider para scheduling
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderInfo {
    pub provider_id: ProviderId,
    pub provider_type: crate::workers::ProviderType,
    pub active_workers: usize,
    pub max_workers: usize,
    pub estimated_startup_time: Duration,
    pub health_score: f64,
    pub cost_per_hour: f64,
}

impl ProviderInfo {
    /// Capacidad disponible (0.0 - 1.0)
    pub fn available_capacity(&self) -> f64 {
        if self.max_workers == 0 {
            return 0.0;
        }
        1.0 - (self.active_workers as f64 / self.max_workers as f64)
    }

    /// Puede aceptar más workers
    pub fn can_accept_workers(&self) -> bool {
        self.active_workers < self.max_workers
    }
}

/// Trait for schedulers de jobs
#[async_trait]
pub trait JobScheduler: Send + Sync {
    /// Decide cómo programar un job
    async fn schedule(&self, context: SchedulingContext) -> anyhow::Result<SchedulingDecision>;

    /// Nombre de la estrategia
    fn strategy_name(&self) -> &str;
}

/// Trait for selection of workers
pub trait WorkerSelector: Send + Sync {
    /// Selecciona el mejor worker para un job
    fn select_worker(&self, job: &Job, workers: &[Worker]) -> Option<WorkerId>;

    /// Nombre de la estrategia
    fn strategy_name(&self) -> &str;
}

/// Trait for selection of providers
pub trait ProviderSelector: Send + Sync {
    /// Selecciona el mejor provider para provisionar un worker
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId>;

    /// Nombre de la estrategia
    fn strategy_name(&self) -> &str;
}

/// Estrategia de selección de workers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerSelectionStrategy {
    /// Primer worker disponible
    FirstAvailable,
    /// Worker con menos jobs ejecutados (balanceo)
    LeastLoaded,
    /// Worker con más capacidad de recursos
    MostCapacity,
    /// Round-robin entre workers
    RoundRobin,
    /// Afinidad por tipo de job
    Affinity,
}

/// Estrategia de selección de providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderSelectionStrategy {
    /// Primer provider con capacidad
    FirstAvailable,
    /// Provider con menor costo
    LowestCost,
    /// Provider con menor tiempo de startup
    FastestStartup,
    /// Provider con más capacidad disponible
    MostCapacity,
    /// Round-robin entre providers
    RoundRobin,
    /// Basado en health score
    Healthiest,
}

// =============================================================================
// Worker Selectors
// =============================================================================

/// Selector de workers: First Available
#[derive(Debug, Clone, Default)]
pub struct FirstAvailableWorkerSelector;

impl WorkerSelector for FirstAvailableWorkerSelector {
    fn select_worker(&self, _job: &Job, workers: &[Worker]) -> Option<WorkerId> {
        workers
            .iter()
            .find(|w| w.state().can_accept_jobs())
            .map(|w| w.id().clone())
    }

    fn strategy_name(&self) -> &str {
        "first_available"
    }
}

/// Selector de workers: Least Loaded
#[derive(Debug, Clone, Default)]
pub struct LeastLoadedWorkerSelector;

impl WorkerSelector for LeastLoadedWorkerSelector {
    fn select_worker(&self, _job: &Job, workers: &[Worker]) -> Option<WorkerId> {
        workers
            .iter()
            .filter(|w| w.state().can_accept_jobs())
            .min_by_key(|w| w.jobs_executed())
            .map(|w| w.id().clone())
    }

    fn strategy_name(&self) -> &str {
        "least_loaded"
    }
}

// =============================================================================
// Provider Selectors
// =============================================================================

/// Selector de providers: Lowest Cost
///
/// Selecciona el provider con menor costo efectivo.
/// Usa internamente `CostScoring` del módulo de scoring para cálculos de score.
///
/// ## Connascence Reduction
/// Previously: Connascence of Position - duplicated health/capacity logic
/// Now: Connascence of Type - uses ProviderScoring trait
#[derive(Debug, Clone)]
pub struct LowestCostProviderSelector {
    /// Scoring implementation for cost-based selection
    scoring: CostScoring,
}

impl Default for LowestCostProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl LowestCostProviderSelector {
    /// Create a new lowest cost selector
    pub fn new() -> Self {
        Self {
            scoring: CostScoring,
        }
    }
}

impl ProviderSelector for LowestCostProviderSelector {
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        // Convert to score inputs and score each provider
        let mut scored: Vec<(ProviderId, f64)> = providers
            .iter()
            .map(|p| {
                let input = ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                );
                let score = self.scoring.score(job, &input);
                (p.provider_id.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score ascending (lower cost = higher score, but we want lowest cost)
        // CostScoring returns higher score for lower cost, so we want highest score
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored.first().map(|(provider_id, _)| provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "lowest_cost"
    }
}

/// Selector de providers: Fastest Startup
///
/// Selecciona el provider con menor tiempo de startup.
/// Usa internamente `StartupTimeScoring` del módulo de scoring.
///
/// ## Connascence Reduction
/// Previously: Connascence of Position - duplicated health/capacity logic
/// Now: Connascence of Type - uses ProviderScoring trait
#[derive(Debug, Clone)]
pub struct FastestStartupProviderSelector {
    /// Scoring implementation for startup-time-based selection
    scoring: StartupTimeScoring,
}

impl Default for FastestStartupProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl FastestStartupProviderSelector {
    /// Create a new fastest startup selector
    pub fn new() -> Self {
        Self {
            scoring: StartupTimeScoring,
        }
    }
}

impl ProviderSelector for FastestStartupProviderSelector {
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        // Convert to score inputs and score each provider
        let mut scored: Vec<(ProviderId, f64)> = providers
            .iter()
            .map(|p| {
                let input = ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                );
                let score = self.scoring.score(job, &input);
                (p.provider_id.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending (higher score = faster startup)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored.first().map(|(provider_id, _)| provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "fastest_startup"
    }
}

/// Selector de providers: Most Capacity
///
/// Selecciona el provider con mayor capacidad disponible.
/// Usa internamente `CapacityScoring` del módulo de scoring.
///
/// ## Connascence Reduction
/// Previously: Connascence of Position - direct capacity calculation
/// Now: Connascence of Type - uses ProviderScoring trait
#[derive(Debug, Clone)]
pub struct MostCapacityProviderSelector {
    /// Scoring implementation for capacity-based selection
    scoring: CapacityScoring,
}

impl Default for MostCapacityProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl MostCapacityProviderSelector {
    /// Create a new most capacity selector
    pub fn new() -> Self {
        Self {
            scoring: CapacityScoring,
        }
    }
}

impl ProviderSelector for MostCapacityProviderSelector {
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        // Convert to score inputs and score each provider
        let mut scored: Vec<(ProviderId, f64)> = providers
            .iter()
            .map(|p| {
                let input = ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                );
                let score = self.scoring.score(job, &input);
                (p.provider_id.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending (higher score = more capacity)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored.first().map(|(provider_id, _)| provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "most_capacity"
    }
}

/// Selector de providers: Healthiest
///
/// Selecciona el provider con mejor salud compuesta.
/// Usa internamente `HealthScoring` y `CompositeProviderScoring` para cálculos.
///
/// ## Connascence Reduction
/// Previously: Connascence of Position - duplicated multi-factor scoring logic
/// Now: Connascence of Type - uses ProviderScoring trait composition
#[derive(Debug, Clone)]
pub struct HealthiestProviderSelector {
    /// Composite scoring for health-optimized selection
    scoring: HealthScoring,
}

impl Default for HealthiestProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthiestProviderSelector {
    /// Create a new healthiest selector
    pub fn new() -> Self {
        Self {
            scoring: HealthScoring,
        }
    }
}

impl ProviderSelector for HealthiestProviderSelector {
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        // Convert to score inputs and score each provider
        let mut scored: Vec<(ProviderId, f64)> = providers
            .iter()
            .map(|p| {
                let input = ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                );
                let score = self.scoring.score(job, &input);
                (p.provider_id.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending (higher score = healthier)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored.first().map(|(provider_id, _)| provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "healthiest"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::ProviderId;
    use crate::workers::ProviderType;

    fn create_test_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Docker,
                active_workers: 5,
                max_workers: 10,
                estimated_startup_time: Duration::from_secs(5),
                health_score: 0.9,
                cost_per_hour: 0.0,
            },
            ProviderInfo {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Kubernetes,
                active_workers: 2,
                max_workers: 20,
                estimated_startup_time: Duration::from_secs(30),
                health_score: 0.95,
                cost_per_hour: 0.5,
            },
        ]
    }

    #[test]
    fn test_provider_info_available_capacity() {
        let info = ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            active_workers: 5,
            max_workers: 10,
            estimated_startup_time: Duration::from_secs(5),
            health_score: 0.9,
            cost_per_hour: 0.0,
        };

        assert!((info.available_capacity() - 0.5).abs() < 0.01);
        assert!(info.can_accept_workers());
    }

    #[test]
    fn test_lowest_cost_selector() {
        let providers = create_test_providers();
        let selector = LowestCostProviderSelector::new();
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let selected = selector.select_provider(&job, &providers);
        assert!(selected.is_some());
    }

    #[test]
    fn test_fastest_startup_selector() {
        let providers = create_test_providers();
        let selector = FastestStartupProviderSelector::new();
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let selected = selector.select_provider(&job, &providers);
        assert!(selected.is_some());
        // Docker should be selected (5s vs 30s)
        assert_eq!(selected.unwrap(), providers[0].provider_id);
    }

    #[test]
    fn test_most_capacity_selector() {
        let providers = create_test_providers();
        let selector = MostCapacityProviderSelector::new();
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let selected = selector.select_provider(&job, &providers);
        assert!(selected.is_some());
        // Kubernetes has more capacity (90% available vs 50%)
        assert_eq!(selected.unwrap(), providers[1].provider_id);
    }

    #[test]
    fn test_scheduling_context_includes_job_preferences() {
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let providers = create_test_providers();
        let workers = Vec::new();

        let context = SchedulingContext {
            job: job.clone(),
            job_preferences: job.spec.preferences.clone(),
            available_workers: workers.clone(),
            available_providers: providers.clone(),
            pending_jobs_count: 0,
            system_load: 0.5,
        };

        assert_eq!(context.job.id, job.id);
        assert_eq!(
            context.job_preferences.preferred_provider,
            job.spec.preferences.preferred_provider
        );
        assert_eq!(context.available_providers.len(), 2);
        assert_eq!(context.available_workers.len(), 0);
    }
}

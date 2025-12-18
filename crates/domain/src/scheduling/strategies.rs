//! Job Scheduler - Estrategias de scheduling y selección de workers
//!
//! Define traits y tipos para la programación inteligente de jobs.

use crate::jobs::Job;
use crate::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use crate::workers::Worker;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Resultado de una decisión de scheduling
#[derive(Debug, Clone)]
pub enum SchedulingDecision {
    /// Asignar job a un worker existente
    AssignToWorker { job_id: JobId, worker_id: WorkerId },
    /// Provisionar nuevo worker para el job
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
#[derive(Debug, Clone)]
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

/// Trait para schedulers de jobs
#[async_trait]
pub trait JobScheduler: Send + Sync {
    /// Decide cómo programar un job
    async fn schedule(&self, context: SchedulingContext) -> Result<SchedulingDecision>;

    /// Nombre de la estrategia
    fn strategy_name(&self) -> &str;
}

/// Trait para selección de workers
pub trait WorkerSelector: Send + Sync {
    /// Selecciona el mejor worker para un job
    fn select_worker(&self, job: &Job, workers: &[Worker]) -> Option<WorkerId>;

    /// Nombre de la estrategia
    fn strategy_name(&self) -> &str;
}

/// Trait para selección de providers
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

/// Selector de workers: First Available
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

/// Selector de providers: Lowest Cost
pub struct LowestCostProviderSelector;

impl ProviderSelector for LowestCostProviderSelector {
    fn select_provider(&self, _job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        providers
            .iter()
            .filter(|p| p.can_accept_workers())
            .min_by(|a, b| a.cost_per_hour.partial_cmp(&b.cost_per_hour).unwrap())
            .map(|p| p.provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "lowest_cost"
    }
}

/// Selector de providers: Fastest Startup
pub struct FastestStartupProviderSelector;

impl ProviderSelector for FastestStartupProviderSelector {
    fn select_provider(&self, _job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        providers
            .iter()
            .filter(|p| p.can_accept_workers())
            .min_by_key(|p| p.estimated_startup_time)
            .map(|p| p.provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "fastest_startup"
    }
}

/// Selector de providers: Most Capacity
pub struct MostCapacityProviderSelector;

impl ProviderSelector for MostCapacityProviderSelector {
    fn select_provider(&self, _job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        providers
            .iter()
            .filter(|p| p.can_accept_workers())
            .max_by(|a, b| {
                a.available_capacity()
                    .partial_cmp(&b.available_capacity())
                    .unwrap()
            })
            .map(|p| p.provider_id.clone())
    }

    fn strategy_name(&self) -> &str {
        "most_capacity"
    }
}

/// Selector de providers: Healthiest
pub struct HealthiestProviderSelector;

impl ProviderSelector for HealthiestProviderSelector {
    fn select_provider(&self, _job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        providers
            .iter()
            .filter(|p| p.can_accept_workers() && p.health_score > 0.5)
            .max_by(|a, b| a.health_score.partial_cmp(&b.health_score).unwrap())
            .map(|p| p.provider_id.clone())
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
        let selector = LowestCostProviderSelector;
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
        let selector = FastestStartupProviderSelector;
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let selected = selector.select_provider(&job, &providers);
        assert!(selected.is_some());
        // Docker debería ser seleccionado (5s vs 30s)
        assert_eq!(selected.unwrap(), providers[0].provider_id);
    }

    #[test]
    fn test_most_capacity_selector() {
        let providers = create_test_providers();
        let selector = MostCapacityProviderSelector;
        let job = Job::new(
            crate::shared_kernel::JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        let selected = selector.select_provider(&job, &providers);
        assert!(selected.is_some());
        // Kubernetes tiene más capacidad (90% disponible vs 50%)
        assert_eq!(selected.unwrap(), providers[1].provider_id);
    }
}

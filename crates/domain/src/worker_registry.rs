//! Worker Registry - Registro centralizado de workers
//!
//! Define el trait para gestionar el registro y ciclo de vida de workers efímeros.

use crate::shared_kernel::{JobId, ProviderId, Result, WorkerId, WorkerState};
use crate::worker::{ProviderType, Worker, WorkerHandle, WorkerSpec};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Estadísticas del registro de workers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerRegistryStats {
    /// Total de workers registrados
    pub total_workers: usize,
    /// Workers en estado Ready
    pub ready_workers: usize,
    /// Workers en estado Busy
    pub busy_workers: usize,
    /// Workers en estado Idle
    pub idle_workers: usize,
    /// Workers terminando
    pub terminating_workers: usize,
    /// Workers por provider
    pub workers_by_provider: std::collections::HashMap<ProviderId, usize>,
    /// Workers por tipo de provider
    pub workers_by_type: std::collections::HashMap<ProviderType, usize>,
}

/// Filtros para búsqueda de workers
#[derive(Debug, Clone, Default)]
pub struct WorkerFilter {
    /// Filtrar por estado
    pub states: Option<Vec<WorkerState>>,
    /// Filtrar por provider
    pub provider_id: Option<ProviderId>,
    /// Filtrar por tipo de provider
    pub provider_type: Option<ProviderType>,
    /// Filtrar por capacidad de aceptar jobs
    pub can_accept_jobs: Option<bool>,
    /// Filtrar workers idle por más de X tiempo
    pub idle_for: Option<Duration>,
    /// Filtrar workers sin heartbeat por más de X tiempo
    pub unhealthy_timeout: Option<Duration>,
}

impl WorkerFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(mut self, state: WorkerState) -> Self {
        self.states = Some(vec![state]);
        self
    }

    pub fn with_states(mut self, states: Vec<WorkerState>) -> Self {
        self.states = Some(states);
        self
    }

    pub fn with_provider_id(mut self, id: ProviderId) -> Self {
        self.provider_id = Some(id);
        self
    }

    pub fn with_provider_type(mut self, pt: ProviderType) -> Self {
        self.provider_type = Some(pt);
        self
    }

    pub fn accepting_jobs(mut self) -> Self {
        self.can_accept_jobs = Some(true);
        self
    }

    pub fn idle_longer_than(mut self, duration: Duration) -> Self {
        self.idle_for = Some(duration);
        self
    }

    pub fn unhealthy_for(mut self, timeout: Duration) -> Self {
        self.unhealthy_timeout = Some(timeout);
        self
    }
}

/// Trait para el registro centralizado de workers
///
/// Responsabilidades:
/// - Registrar y dar de baja workers
/// - Consultar workers disponibles
/// - Gestionar heartbeats
/// - Asignar y liberar workers para jobs
#[async_trait]
pub trait WorkerRegistry: Send + Sync {
    /// Registrar un nuevo worker
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec) -> Result<Worker>;

    /// Dar de baja un worker
    async fn unregister(&self, worker_id: &WorkerId) -> Result<()>;

    /// Obtener worker por ID
    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>>;

    /// Obtener todos los workers que coinciden con el filtro
    async fn find(&self, filter: &WorkerFilter) -> Result<Vec<Worker>>;

    /// Obtener workers disponibles para jobs
    async fn find_available(&self) -> Result<Vec<Worker>>;

    /// Obtener workers de un provider específico
    async fn find_by_provider(&self, provider_id: &ProviderId) -> Result<Vec<Worker>>;

    /// Actualizar estado del worker
    async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()>;

    /// Registrar heartbeat del worker
    async fn heartbeat(&self, worker_id: &WorkerId) -> Result<()>;

    /// Asignar worker a un job
    async fn assign_to_job(&self, worker_id: &WorkerId, job_id: JobId) -> Result<()>;

    /// Liberar worker de un job
    async fn release_from_job(&self, worker_id: &WorkerId) -> Result<()>;

    /// Obtener workers sin heartbeat reciente
    async fn find_unhealthy(&self, timeout: Duration) -> Result<Vec<Worker>>;

    /// Obtener workers que deberían terminarse (idle timeout o lifetime exceeded)
    async fn find_for_termination(&self) -> Result<Vec<Worker>>;

    /// Obtener estadísticas del registro
    async fn stats(&self) -> Result<WorkerRegistryStats>;

    /// Número total de workers registrados
    async fn count(&self) -> Result<usize>;
}

/// Eventos del ciclo de vida de workers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerLifecycleEvent {
    /// Worker registrado
    Registered {
        worker_id: WorkerId,
        provider_id: ProviderId,
        timestamp: DateTime<Utc>,
    },
    /// Worker cambió de estado
    StateChanged {
        worker_id: WorkerId,
        from: WorkerState,
        to: WorkerState,
        timestamp: DateTime<Utc>,
    },
    /// Worker asignado a job
    AssignedToJob {
        worker_id: WorkerId,
        job_id: JobId,
        timestamp: DateTime<Utc>,
    },
    /// Worker liberado de job
    ReleasedFromJob {
        worker_id: WorkerId,
        job_id: JobId,
        timestamp: DateTime<Utc>,
    },
    /// Heartbeat recibido
    HeartbeatReceived {
        worker_id: WorkerId,
        timestamp: DateTime<Utc>,
    },
    /// Worker marcado como no saludable
    UnhealthyDetected {
        worker_id: WorkerId,
        last_heartbeat: DateTime<Utc>,
        timestamp: DateTime<Utc>,
    },
    /// Worker dado de baja
    Unregistered {
        worker_id: WorkerId,
        reason: String,
        timestamp: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_filter_builder() {
        let filter = WorkerFilter::new()
            .with_state(WorkerState::Ready)
            .accepting_jobs()
            .idle_longer_than(Duration::from_secs(300));

        assert!(filter.states.is_some());
        assert!(filter.can_accept_jobs.unwrap());
        assert_eq!(filter.idle_for.unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_worker_registry_stats_default() {
        let stats = WorkerRegistryStats::default();
        assert_eq!(stats.total_workers, 0);
        assert_eq!(stats.ready_workers, 0);
    }
}

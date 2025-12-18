use crate::jobs::JobSpec;
use crate::shared_kernel::{JobId, JobState, ProviderId, ProviderStatus, WorkerId, WorkerState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Representa un evento de dominio que ha ocurrido en el sistema.
/// Los eventos son hechos inmutables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainEvent {
    /// Se ha solicitado la creación de un nuevo job
    JobCreated {
        job_id: JobId,
        spec: JobSpec,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El estado de un job ha cambiado
    JobStatusChanged {
        job_id: JobId,
        old_state: JobState,
        new_state: JobState,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un nuevo worker se ha registrado en el sistema
    WorkerRegistered {
        worker_id: WorkerId,
        provider_id: ProviderId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El estado de un worker ha cambiado
    WorkerStatusChanged {
        worker_id: WorkerId,
        old_status: WorkerState,
        new_status: WorkerState,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un nuevo provider se ha registrado
    ProviderRegistered {
        provider_id: ProviderId,
        provider_type: String,
        config_summary: String, // Resumen de configuración (safe logging)
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un provider ha sido actualizado
    ProviderUpdated {
        provider_id: ProviderId,
        changes: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido cancelado explícitamente
    JobCancelled {
        job_id: JobId,
        reason: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker ha sido terminado (desregistrado o destruido)
    WorkerTerminated {
        worker_id: WorkerId,
        provider_id: ProviderId,
        reason: TerminationReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker se ha desconectado inesperadamente
    WorkerDisconnected {
        worker_id: WorkerId,
        last_heartbeat: Option<DateTime<Utc>>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker ha sido provisionado por el lifecycle manager
    WorkerProvisioned {
        worker_id: WorkerId,
        provider_id: ProviderId,
        spec_summary: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido reintentado
    JobRetried {
        job_id: JobId,
        attempt: u32,
        max_attempts: u32,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido asignado a un worker específico
    JobAssigned {
        job_id: JobId,
        worker_id: WorkerId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El estado de salud de un provider ha cambiado
    ProviderHealthChanged {
        provider_id: ProviderId,
        old_status: ProviderStatus,
        new_status: ProviderStatus,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
}

/// Razón de terminación de un worker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Desregistro voluntario por el worker
    Unregistered,
    /// Terminación por idle timeout
    IdleTimeout,
    /// Terminación por lifetime exceeded
    LifetimeExceeded,
    /// Terminación por health check fallido
    HealthCheckFailed,
    /// Terminación manual por administrador
    ManualTermination,
    /// Terminación por error del provider
    ProviderError { message: String },
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::Unregistered => write!(f, "UNREGISTERED"),
            TerminationReason::IdleTimeout => write!(f, "IDLE_TIMEOUT"),
            TerminationReason::LifetimeExceeded => write!(f, "LIFETIME_EXCEEDED"),
            TerminationReason::HealthCheckFailed => write!(f, "HEALTH_CHECK_FAILED"),
            TerminationReason::ManualTermination => write!(f, "MANUAL_TERMINATION"),
            TerminationReason::ProviderError { message } => {
                write!(f, "PROVIDER_ERROR: {}", message)
            }
        }
    }
}

impl DomainEvent {
    /// Obtiene el ID de correlación asociado al evento (si existe)
    pub fn correlation_id(&self) -> Option<String> {
        match self {
            DomainEvent::JobCreated { correlation_id, .. }
            | DomainEvent::JobStatusChanged { correlation_id, .. }
            | DomainEvent::WorkerRegistered { correlation_id, .. }
            | DomainEvent::WorkerStatusChanged { correlation_id, .. }
            | DomainEvent::ProviderRegistered { correlation_id, .. }
            | DomainEvent::ProviderUpdated { correlation_id, .. }
            | DomainEvent::JobCancelled { correlation_id, .. }
            | DomainEvent::WorkerTerminated { correlation_id, .. }
            | DomainEvent::WorkerDisconnected { correlation_id, .. }
            | DomainEvent::WorkerProvisioned { correlation_id, .. }
            | DomainEvent::JobRetried { correlation_id, .. }
            | DomainEvent::JobAssigned { correlation_id, .. }
            | DomainEvent::ProviderHealthChanged { correlation_id, .. } => correlation_id.clone(),
        }
    }

    /// Obtiene el actor que inició el evento (si existe)
    pub fn actor(&self) -> Option<String> {
        match self {
            DomainEvent::JobCreated { actor, .. }
            | DomainEvent::JobStatusChanged { actor, .. }
            | DomainEvent::WorkerRegistered { actor, .. }
            | DomainEvent::WorkerStatusChanged { actor, .. }
            | DomainEvent::ProviderRegistered { actor, .. }
            | DomainEvent::ProviderUpdated { actor, .. }
            | DomainEvent::JobCancelled { actor, .. }
            | DomainEvent::WorkerTerminated { actor, .. }
            | DomainEvent::WorkerDisconnected { actor, .. }
            | DomainEvent::WorkerProvisioned { actor, .. }
            | DomainEvent::JobRetried { actor, .. }
            | DomainEvent::JobAssigned { actor, .. }
            | DomainEvent::ProviderHealthChanged { actor, .. } => actor.clone(),
        }
    }

    /// Obtiene la fecha en que ocurrió el evento
    pub fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            DomainEvent::JobCreated { occurred_at, .. }
            | DomainEvent::JobStatusChanged { occurred_at, .. }
            | DomainEvent::WorkerRegistered { occurred_at, .. }
            | DomainEvent::WorkerStatusChanged { occurred_at, .. }
            | DomainEvent::ProviderRegistered { occurred_at, .. }
            | DomainEvent::ProviderUpdated { occurred_at, .. }
            | DomainEvent::JobCancelled { occurred_at, .. }
            | DomainEvent::WorkerTerminated { occurred_at, .. }
            | DomainEvent::WorkerDisconnected { occurred_at, .. }
            | DomainEvent::WorkerProvisioned { occurred_at, .. }
            | DomainEvent::JobRetried { occurred_at, .. }
            | DomainEvent::JobAssigned { occurred_at, .. }
            | DomainEvent::ProviderHealthChanged { occurred_at, .. } => *occurred_at,
        }
    }

    /// Obtiene el tipo de evento como string
    pub fn event_type(&self) -> &'static str {
        match self {
            DomainEvent::JobCreated { .. } => "JobCreated",
            DomainEvent::JobStatusChanged { .. } => "JobStatusChanged",
            DomainEvent::WorkerRegistered { .. } => "WorkerRegistered",
            DomainEvent::WorkerStatusChanged { .. } => "WorkerStatusChanged",
            DomainEvent::ProviderRegistered { .. } => "ProviderRegistered",
            DomainEvent::ProviderUpdated { .. } => "ProviderUpdated",
            DomainEvent::JobCancelled { .. } => "JobCancelled",
            DomainEvent::WorkerTerminated { .. } => "WorkerTerminated",
            DomainEvent::WorkerDisconnected { .. } => "WorkerDisconnected",
            DomainEvent::WorkerProvisioned { .. } => "WorkerProvisioned",
            DomainEvent::JobRetried { .. } => "JobRetried",
            DomainEvent::JobAssigned { .. } => "JobAssigned",
            DomainEvent::ProviderHealthChanged { .. } => "ProviderHealthChanged",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);
        let event = DomainEvent::JobCreated {
            job_id: job_id.clone(),
            spec: spec.clone(),
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation-id".to_string()),
            actor: Some("test-actor".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_worker_terminated_event_serialization() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let event = DomainEvent::WorkerTerminated {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            reason: TerminationReason::IdleTimeout,
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("system".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerTerminated");
    }

    #[test]
    fn test_worker_disconnected_event_serialization() {
        let worker_id = WorkerId::new();
        let event = DomainEvent::WorkerDisconnected {
            worker_id: worker_id.clone(),
            last_heartbeat: Some(Utc::now()),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerDisconnected");
    }

    #[test]
    fn test_worker_provisioned_event_serialization() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let event = DomainEvent::WorkerProvisioned {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            spec_summary: "image=hodei-worker:latest, cpu=2, memory=4Gi".to_string(),
            occurred_at: Utc::now(),
            correlation_id: Some("provision-123".to_string()),
            actor: Some("scheduler".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerProvisioned");
    }

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(TerminationReason::Unregistered.to_string(), "UNREGISTERED");
        assert_eq!(TerminationReason::IdleTimeout.to_string(), "IDLE_TIMEOUT");
        assert_eq!(
            TerminationReason::LifetimeExceeded.to_string(),
            "LIFETIME_EXCEEDED"
        );
        assert_eq!(
            TerminationReason::HealthCheckFailed.to_string(),
            "HEALTH_CHECK_FAILED"
        );
        assert_eq!(
            TerminationReason::ManualTermination.to_string(),
            "MANUAL_TERMINATION"
        );
        assert_eq!(
            TerminationReason::ProviderError {
                message: "connection lost".to_string()
            }
            .to_string(),
            "PROVIDER_ERROR: connection lost"
        );
    }

    #[test]
    fn test_event_type_method() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]);

        let events = vec![
            (
                DomainEvent::JobCreated {
                    job_id: job_id.clone(),
                    spec,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "JobCreated",
            ),
            (
                DomainEvent::WorkerTerminated {
                    worker_id: WorkerId::new(),
                    provider_id: ProviderId::new(),
                    reason: TerminationReason::Unregistered,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerTerminated",
            ),
            (
                DomainEvent::WorkerDisconnected {
                    worker_id: WorkerId::new(),
                    last_heartbeat: None,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerDisconnected",
            ),
            (
                DomainEvent::WorkerProvisioned {
                    worker_id: WorkerId::new(),
                    provider_id: ProviderId::new(),
                    spec_summary: "test".to_string(),
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerProvisioned",
            ),
            (
                DomainEvent::JobRetried {
                    job_id: JobId::new(),
                    attempt: 2,
                    max_attempts: 3,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "JobRetried",
            ),
            (
                DomainEvent::JobAssigned {
                    job_id: JobId::new(),
                    worker_id: WorkerId::new(),
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "JobAssigned",
            ),
        ];

        for (event, expected_type) in events {
            assert_eq!(event.event_type(), expected_type);
        }
    }

    #[test]
    fn test_job_retried_event_serialization() {
        let job_id = JobId::new();
        let event = DomainEvent::JobRetried {
            job_id: job_id.clone(),
            attempt: 2,
            max_attempts: 3,
            occurred_at: Utc::now(),
            correlation_id: Some("retry-123".to_string()),
            actor: Some("scheduler".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "JobRetried");
    }

    #[test]
    fn test_job_assigned_event_serialization() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let event = DomainEvent::JobAssigned {
            job_id: job_id.clone(),
            worker_id: worker_id.clone(),
            occurred_at: Utc::now(),
            correlation_id: Some("assign-456".to_string()),
            actor: Some("job-controller".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "JobAssigned");
    }

    #[test]
    fn test_provider_health_changed_event_serialization() {
        let provider_id = ProviderId::new();
        let event = DomainEvent::ProviderHealthChanged {
            provider_id: provider_id.clone(),
            old_status: ProviderStatus::Active,
            new_status: ProviderStatus::Unhealthy,
            occurred_at: Utc::now(),
            correlation_id: Some("health-check-789".to_string()),
            actor: Some("health-monitor".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "ProviderHealthChanged");
    }
}

//! Heartbeat Processor
//!
//! Componente dedicado para el procesamiento de heartbeats de workers.
//! Extraído de WorkerAgentServiceImpl para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Procesar heartbeats entrantes de workers
//! - Actualizar estado de workers en el registry
//! - Publicar eventos WorkerHeartbeat
//! - Transicionar workers de Creating a Ready

use hodei_jobs::AckMessage;
use hodei_server_application::workers::actor::WorkerSupervisorHandle;
use hodei_server_domain::{
    event_bus::EventBus,
    events::DomainEvent,
    shared_kernel::{JobId, WorkerId, WorkerState},
    workers::WorkerRegistry,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Resultado del procesamiento de un heartbeat
#[derive(Debug, Clone)]
pub struct HeartbeatResult {
    pub worker_id: WorkerId,
    pub processed: bool,
    pub state_changed: bool,
    pub new_state: Option<WorkerState>,
}

/// Configuración del HeartbeatProcessor
#[derive(Debug, Clone)]
pub struct HeartbeatProcessorConfig {
    /// Timeout para considerar un worker como no respondedor
    pub heartbeat_timeout_secs: u64,
}

impl Default for HeartbeatProcessorConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_secs: 60,
        }
    }
}

/// Interfaz pública del HeartbeatProcessor
#[derive(Clone)]
pub struct HeartbeatProcessor {
    /// Worker registry para persistencia
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    /// Event bus para publicación de eventos
    event_bus: Option<Arc<dyn EventBus>>,
    /// Supervisor handle para routing through Actor (EPIC-42)
    supervisor_handle: Option<WorkerSupervisorHandle>,
    /// Configuración
    config: HeartbeatProcessorConfig,
}

impl HeartbeatProcessor {
    /// Crear nuevo HeartbeatProcessor
    pub fn new(
        worker_registry: Option<Arc<dyn WorkerRegistry>>,
        event_bus: Option<Arc<dyn EventBus>>,
        config: Option<HeartbeatProcessorConfig>,
    ) -> Self {
        Self {
            worker_registry,
            event_bus,
            supervisor_handle: None,
            config: config.unwrap_or_default(),
        }
    }

    /// Configurar supervisor handle (EPIC-42)
    pub fn with_supervisor_handle(mut self, handle: WorkerSupervisorHandle) -> Self {
        self.supervisor_handle = Some(handle);
        self
    }

    /// Procesar un heartbeat de worker
    ///
    /// Returns: Result<HeartbeatResult, String>
    pub async fn process_heartbeat(
        &self,
        worker_id: &str,
        running_job_ids: Vec<String>,
    ) -> Result<HeartbeatResult, String> {
        let worker_id = Self::parse_worker_uuid(worker_id)?;

        // EPIC-42: Route through Actor if enabled for lock-free heartbeat processing
        if let Some(ref supervisor) = self.supervisor_handle {
            debug!("EPIC-42: Routing heartbeat through Actor for {}", worker_id);

            match supervisor.heartbeat(&worker_id).await {
                Ok(()) => {
                    debug!("EPIC-42: Heartbeat routed through Actor successfully");
                    return Ok(HeartbeatResult {
                        worker_id,
                        processed: true,
                        state_changed: false,
                        new_state: None,
                    });
                }
                Err(e) => {
                    warn!(
                        "EPIC-42: Failed to route heartbeat through Actor, falling back to legacy: {:?}",
                        e
                    );
                    // Continue with legacy path
                }
            }
        }

        // Legacy path: direct registry update
        let Some(registry) = &self.worker_registry else {
            return Ok(HeartbeatResult {
                worker_id,
                processed: false,
                state_changed: false,
                new_state: None,
            });
        };

        // Update heartbeat in registry
        registry
            .heartbeat(&worker_id)
            .await
            .map_err(|e| format!("Failed to update heartbeat: {}", e))?;

        // Get worker to check state transition
        let worker = registry
            .get(&worker_id)
            .await
            .map_err(|e| format!("Failed to get worker: {}", e))?
            .ok_or_else(|| "Worker not found in registry".to_string())?;

        let mut state_changed = false;
        let mut new_state = None;

        // Transition from Creating to Ready on first heartbeat
        if matches!(worker.state(), WorkerState::Creating) {
            registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| format!("Failed to update state: {}", e))?;

            state_changed = true;
            new_state = Some(WorkerState::Ready);

            // Publish WorkerStatusChanged event
            if let Some(event_bus) = &self.event_bus {
                let event = DomainEvent::WorkerStatusChanged {
                    worker_id: worker.id().clone(),
                    old_status: WorkerState::Creating,
                    new_status: WorkerState::Ready,
                    occurred_at: chrono::Utc::now(),
                    correlation_id: None,
                    actor: None,
                };
                if let Err(e) = event_bus.publish(&event).await {
                    warn!("Failed to publish WorkerStatusChanged event: {}", e);
                }
            }
        }

        // Publish WorkerHeartbeat event (EPIC-29)
        if let Some(event_bus) = &self.event_bus {
            let heartbeat_event = DomainEvent::WorkerHeartbeat {
                worker_id: worker_id.clone(),
                state: worker.state().clone(),
                load_average: None,
                memory_usage_mb: None,
                current_job_id: running_job_ids
                    .first()
                    .and_then(|id| id.parse::<uuid::Uuid>().ok().map(JobId)),
                occurred_at: chrono::Utc::now(),
                correlation_id: None,
                actor: Some("heartbeat_processor".to_string()),
            };

            if let Err(e) = event_bus.publish(&heartbeat_event).await {
                error!("Failed to publish WorkerHeartbeat event: {}", e);
            }
        }

        Ok(HeartbeatResult {
            worker_id,
            processed: true,
            state_changed,
            new_state,
        })
    }

    /// Generar ACK message para heartbeat
    pub fn create_heartbeat_ack(&self, worker_id: &str) -> AckMessage {
        AckMessage {
            message_id: Uuid::new_v4().to_string(),
            success: true,
            worker_id: worker_id.to_string(),
        }
    }

    /// Parse worker UUID
    fn parse_worker_uuid(worker_id: &str) -> Result<WorkerId, String> {
        let id =
            uuid::Uuid::parse_str(worker_id).map_err(|_| "worker_id must be a UUID".to_string())?;
        Ok(WorkerId(id))
    }
}

/// Datos serializables para métricas
#[derive(Debug, Serialize)]
pub struct HeartbeatMetrics {
    pub total_heartbeats: u64,
    pub failed_heartbeats: u64,
    pub state_transitions: u64,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for HeartbeatMetrics {
    fn default() -> Self {
        Self {
            total_heartbeats: 0,
            failed_heartbeats: 0,
            state_transitions: 0,
            last_heartbeat_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_processor_creation() {
        let processor = HeartbeatProcessor::new(None, None, None);
        assert!(processor.worker_registry.is_none());
        assert!(processor.event_bus.is_none());
        assert!(processor.supervisor_handle.is_none());
    }

    #[tokio::test]
    async fn test_heartbeat_ack_creation() {
        let processor = HeartbeatProcessor::new(None, None, None);
        let ack = processor.create_heartbeat_ack("test-worker-id");
        assert!(!ack.message_id.is_empty());
        assert!(ack.success);
        assert_eq!(ack.worker_id, "test-worker-id");
    }

    #[tokio::test]
    async fn test_heartbeat_metrics_default() {
        let metrics = HeartbeatMetrics::default();
        assert_eq!(metrics.total_heartbeats, 0);
        assert_eq!(metrics.failed_heartbeats, 0);
        assert!(metrics.last_heartbeat_at.is_none());
    }

    #[tokio::test]
    async fn test_parse_valid_uuid() {
        let result = HeartbeatProcessor::parse_worker_uuid("550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_parse_invalid_uuid() {
        let result = HeartbeatProcessor::parse_worker_uuid("not-a-uuid");
        assert!(result.is_err());
    }
}

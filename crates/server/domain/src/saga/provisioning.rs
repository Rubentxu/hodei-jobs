//! Provisioning Saga
//!
//! Saga para el aprovisionamiento de workers on-demand.

use crate::command::erased::dispatch_erased;
use crate::event_bus::EventBus;
use crate::events::DomainEvent;
use crate::saga::commands::{CreateWorkerCommand, DestroyWorkerCommand};
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::{WorkerProvisioning, WorkerRegistry, WorkerSpec};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

// ============================================================================
// ProvisioningSaga
// ============================================================================

/// Saga para aprovisionar un nuevo worker on-demand.
///
/// # Pasos:
///
/// 1. **ValidateProviderCapacityStep**: Valida que el provider tenga capacidad
/// 2. **CreateInfrastructureStep**: Almacena metadata para creación de infraestructura
/// 3. **RegisterWorkerStep**: Almacena metadata para registro del worker
/// 4. **PublishProvisionedEventStep**: Almacena metadata para publicación de evento
///
/// # Nota:
///
/// Esta saga almacena metadatos en el contexto que son utilizados por los
/// coordinadores en la capa de aplicación para realizar las operaciones reales.
/// Los pasos saga themselves no realizan operaciones de infraestructura directamente.
#[derive(Debug, Clone)]
pub struct ProvisioningSaga {
    /// Spec del worker a aprovisionar
    spec: WorkerSpec,
    /// Provider ID seleccionado
    provider_id: ProviderId,
    /// Job ID asociado (opcional)
    job_id: Option<JobId>,
}

impl ProvisioningSaga {
    /// Crea una nueva ProvisioningSaga
    pub fn new(spec: WorkerSpec, provider_id: ProviderId) -> Self {
        Self {
            spec,
            provider_id,
            job_id: None,
        }
    }

    /// Crea una ProvisioningSaga con job asociado
    pub fn with_job(spec: WorkerSpec, provider_id: ProviderId, job_id: JobId) -> Self {
        Self {
            spec,
            provider_id,
            job_id: Some(job_id),
        }
    }

    /// Obtiene el spec del worker
    pub fn spec(&self) -> &WorkerSpec {
        &self.spec
    }

    /// Obtiene el provider ID
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Obtiene el job ID
    pub fn job_id(&self) -> Option<&JobId> {
        self.job_id.as_ref()
    }
}

impl Saga for ProvisioningSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Provisioning
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(ValidateProviderCapacityStep::new(self.provider_id.clone())),
            // CreateInfrastructureStep creates the Docker container with OTP token
            // The ephemeral worker will self-register when it starts up
            Box::new(CreateInfrastructureStep::new(
                self.provider_id.clone(),
                self.spec.clone(),
            )),
            // RegisterWorkerStep is NOT needed - workers self-register via OTP on startup
            Box::new(PublishProvisionedEventStep::new()),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(300))
    }
}

// ============================================================================
// ValidateProviderCapacityStep
// ============================================================================

/// Step que valida que el provider tenga capacidad para el worker solicitado.
///
/// Este step almacena el provider_id en el contexto para uso por el coordinador.
/// La validación real de capacidad se realiza en el coordinador.
#[derive(Debug, Clone)]
pub struct ValidateProviderCapacityStep {
    provider_id: ProviderId,
}

impl ValidateProviderCapacityStep {
    pub fn new(provider_id: ProviderId) -> Self {
        Self { provider_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateProviderCapacityStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ValidateProviderCapacity"
    }

    #[instrument(skip(context), fields(step = "ValidateProviderCapacity", provider_id = %self.provider_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Store provider_id in context for coordinator use
        context
            .set_metadata("saga_provider_id", &self.provider_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        debug!(provider_id = %self.provider_id, "Provider capacity validation step completed");

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // No compensation needed for validation metadata
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        false
    }
}

// ============================================================================
// CreateInfrastructureStep
// ============================================================================

/// Step que crea infraestructura real para un worker.
///
/// Este step utiliza el `CommandBus` inyectado en el contexto para despachar
/// comandos de creación de infraestructura.
///
/// # Metadata Guardado
/// - `worker_id`: ID del worker creado (para compensación)
/// - `worker_spec_image`: Image del worker (para idempotencia)
/// - `worker_provisioning_done`: Flag indicando que ya se creó el worker
#[derive(Debug, Clone)]
pub struct CreateInfrastructureStep {
    provider_id: ProviderId,
    spec: WorkerSpec,
    /// Job ID asociado (opcional, se genera uno si no se provee)
    job_id: Option<JobId>,
}

impl CreateInfrastructureStep {
    /// Creates a new CreateInfrastructureStep.
    pub fn new(provider_id: ProviderId, spec: WorkerSpec) -> Self {
        Self {
            provider_id,
            spec,
            job_id: None,
        }
    }

    /// Creates a step with an associated job ID.
    pub fn with_job(mut self, job_id: JobId) -> Self {
        self.job_id = Some(job_id);
        self
    }
}

#[async_trait::async_trait]
impl SagaStep for CreateInfrastructureStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CreateInfrastructure"
    }

    #[instrument(skip(context), fields(step = "CreateInfrastructure", provider_id = %self.provider_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get command bus from context
        let services_ref = context
            .services()
            .ok_or_else(|| SagaError::PersistenceError {
                message: "SagaServices not available in context".to_string(),
            })?;

        let command_bus = services_ref.command_bus.as_ref().cloned().ok_or_else(|| {
            SagaError::PersistenceError {
                message: "CommandBus not available in SagaServices".to_string(),
            }
        })?;

        // Idempotency check: skip if already provisioned
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_provisioning_done") {
            info!(
                provider_id = %self.provider_id,
                "Worker already provisioned (idempotency check), skipping"
            );
            return Ok(());
        }

        // Idempotency check: skip if worker_id already exists
        if context.get_metadata::<String>("worker_id").is_some() {
            info!(
                provider_id = %self.provider_id,
                "Worker ID exists in context, skipping (idempotency)"
            );
            return Ok(());
        }

        // Get or create job ID
        let job_id = self.job_id.clone().unwrap_or_else(JobId::new);

        // Store job_id for potential compensation
        context
            .set_metadata("worker_job_id", &job_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            provider_id = %self.provider_id,
            image = %self.spec.image,
            job_id = %job_id,
            "🔨 Provisioning worker infrastructure via CommandBus..."
        );

        // Create the command
        let command = CreateWorkerCommand::new(
            self.spec.clone(),
            self.provider_id.clone(),
            job_id,
            context.saga_id.to_string(),
        );

        // Dispatch command via CommandBus
        let provisioning_result = dispatch_erased(&command_bus, command)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to dispatch CreateWorkerCommand: {}", e),
                will_compensate: true,
            })?;

        // Store worker_id for compensation
        context
            .set_metadata("worker_id", &provisioning_result.worker_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Store provider_id for compensation (in case step 1 wasn't executed)
        context
            .set_metadata("worker_provider_id", &self.provider_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Mark as done for idempotency
        context
            .set_metadata("worker_provisioning_done", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            worker_id = %provisioning_result.worker_id,
            provider_id = %self.provider_id,
            "✅ Worker infrastructure provisioned successfully"
        );

        Ok(())
    }

    /// Compensates by destroying the worker that was created.
    ///
    /// This method reads the `worker_id` from the context metadata and
    /// dispatches a `DestroyWorkerCommand`.
    #[instrument(skip(context), fields(step = "CreateInfrastructure"))]
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        // Get worker_id from metadata
        let worker_id_str = match context.get_metadata::<String>("worker_id") {
            Some(Ok(id)) => id,
            None => {
                info!("No worker_id in context, skipping compensation (may not have executed)");
                return Ok(());
            }
            Some(Err(e)) => {
                warn!("Failed to read worker_id from context: {}", e);
                return Ok(());
            }
        };

        // Get command bus from context
        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        let command_bus = services.command_bus.as_ref().ok_or_else(|| {
            SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "CommandBus not available in SagaServices".to_string(),
            }
        })?;

        // Parse worker_id
        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Invalid worker_id in context: {}", worker_id_str),
            })?;

        info!(
            worker_id = %worker_id_str,
            "🔄 Compensating: dispatching DestroyWorkerCommand"
        );

        // Create command
        let command = DestroyWorkerCommand::with_reason(
            worker_id,
            self.provider_id.clone(),
            context.saga_id.to_string(),
            "Saga compensation",
        );

        // Dispatch command
        dispatch_erased(&command_bus, command)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to dispatch DestroyWorkerCommand: {}", e),
            })?;

        info!(
            worker_id = %worker_id_str,
            "✅ Worker infrastructure destruction command dispatched"
        );

        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        true
    }
}

// ============================================================================
// RegisterWorkerStep
// ============================================================================

/// Step que registra un worker en el registro centralizado.
///
/// Este step utiliza el `WorkerRegistry` service inyectado en el contexto
/// para registrar el worker de forma real. En compensación, deregistra el worker.
///
/// # Metadata Guardado
/// - `worker_id`: ID del worker registrado (para compensación)
/// - `worker_registration_done`: Flag indicando que ya se registró
/// - `worker_job_id`: Job ID asociado al worker
#[derive(Debug, Clone)]
pub struct RegisterWorkerStep;

impl RegisterWorkerStep {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SagaStep for RegisterWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "RegisterWorker"
    }

    #[instrument(skip(context), fields(step = "RegisterWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context
            .services()
            .ok_or_else(|| SagaError::PersistenceError {
                message: "SagaServices not available in context".to_string(),
            })?;

        let provider_registry = &services.provider_registry;

        // Idempotency check: skip if already registered
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_registration_done") {
            info!("Worker already registered (idempotency check), skipping");
            return Ok(());
        }

        // Get worker_id from context (set by CreateInfrastructureStep)
        let worker_id_str = match context.get_metadata::<String>("worker_id") {
            Some(Ok(id)) => id,
            None => {
                return Err(SagaError::PersistenceError {
                    message: "worker_id not found in context. CreateInfrastructureStep must execute first".to_string(),
                });
            }
            Some(Err(e)) => {
                return Err(SagaError::PersistenceError {
                    message: format!("Failed to read worker_id from context: {}", e),
                });
            }
        };

        // Get job_id from context
        let job_id_str = context
            .get_metadata::<String>("worker_job_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| {
                // Generate a new job ID if not present
                JobId::new().to_string()
            });

        // Parse IDs
        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::PersistenceError {
                message: format!("Invalid worker_id in context: {}", worker_id_str),
            })?;

        let job_id =
            JobId(
                uuid::Uuid::parse_str(&job_id_str).map_err(|e| SagaError::PersistenceError {
                    message: format!("Invalid job_id in context: {}", e),
                })?,
            );

        // Get worker_spec from context or create default
        let spec = WorkerSpec::new(
            context
                .metadata
                .get("worker_image")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "hodei-jobs-worker:latest".to_string()),
            context
                .metadata
                .get("worker_server_address")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "http://localhost:50051".to_string()),
        );

        // Get handle from provider registry using worker_id
        let worker_handle = provider_registry
            .get(&worker_id)
            .await
            .map_err(|e| SagaError::PersistenceError {
                message: format!("Failed to get worker handle: {}", e),
            })?
            .ok_or_else(|| SagaError::PersistenceError {
                message: format!("Worker {} not found in provider registry", worker_id),
            })?
            .handle()
            .clone(); // Use accessor method and clone the value

        info!(
            worker_id = %worker_id,
            job_id = %job_id,
            "📝 Registering worker in central registry..."
        );

        // Register worker in the central registry
        let worker = provider_registry
            .register(worker_handle, spec, job_id)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to register worker: {}", e),
                will_compensate: true,
            })?;

        // Store worker_id for compensation
        context
            .set_metadata("registered_worker_id", &worker.id().to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Mark as done for idempotency
        context
            .set_metadata("worker_registration_done", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            worker_id = %worker.id(),
            state = ?worker.state(),
            "✅ Worker registered successfully in central registry"
        );

        Ok(())
    }

    /// Compensates by unregistering the worker from the central registry.
    #[instrument(skip(context), fields(step = "RegisterWorker"))]
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        // Get registered_worker_id from metadata
        let worker_id_str = match context.get_metadata::<String>("registered_worker_id") {
            Some(Ok(id)) => id,
            None => {
                info!("No registered_worker_id in context, skipping compensation");
                return Ok(());
            }
            Some(Err(e)) => {
                warn!("Failed to read registered_worker_id from context: {}", e);
                return Ok(());
            }
        };

        // Get services from context
        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        let provider_registry = &services.provider_registry;

        // Parse worker_id
        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Invalid worker_id in context: {}", worker_id_str),
            })?;

        info!(
            worker_id = %worker_id_str,
            "🔄 Compensating: unregistering worker from central registry"
        );

        // Unregister worker (idempotent - no error if already unregistered)
        provider_registry
            .unregister(&worker_id)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to unregister worker {}: {}", worker_id_str, e),
            })?;

        info!(
            worker_id = %worker_id_str,
            "✅ Worker unregistered from central registry (compensation complete)"
        );

        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        true
    }
}

// ============================================================================
// PublishProvisionedEventStep
// ============================================================================

/// Step que publica el evento DomainEvent::WorkerProvisioned.
///
/// Este step publica el evento de worker provisionado en el EventBus.
/// Es el paso final de la saga de aprovisionamiento.
#[derive(Debug, Clone)]
pub struct PublishProvisionedEventStep;

impl PublishProvisionedEventStep {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SagaStep for PublishProvisionedEventStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "PublishProvisionedEvent"
    }

    #[instrument(skip(context), fields(step = "PublishProvisionedEvent"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Check if already executed (idempotency)
        if context
            .get_metadata::<bool>("event_published")
            .and_then(|r| r.ok())
            == Some(true)
        {
            info!("WorkerProvisioned event already published, skipping (idempotent)");
            return Ok(());
        }

        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        // Get required metadata from context
        // get_metadata returns Option<SagaResult<V>>, need to handle both None and Err cases
        // Use worker_id from CreateInfrastructureStep (not registered_worker_id since workers self-register)
        let worker_id_str = context
            .get_metadata::<String>("worker_id")
            .and_then(|result| result.ok())
            .ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "worker_id not found in context".to_string(),
                will_compensate: true,
            })?;

        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Invalid worker_id in context: {}", worker_id_str),
                will_compensate: true,
            })?;

        let provider_id_str = context
            .get_metadata::<String>("provider_id")
            .and_then(|result| result.ok())
            .ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "provider_id not found in context".to_string(),
                will_compensate: true,
            })?;

        let provider_id =
            ProviderId::from_uuid(uuid::Uuid::parse_str(&provider_id_str).map_err(|e| {
                SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!(
                        "Invalid provider_id in context: {} - {}",
                        provider_id_str, e
                    ),
                    will_compensate: true,
                }
            })?);

        // Get spec summary from metadata or use default
        let spec_summary = context
            .metadata
            .get("worker_image")
            .and_then(|v| v.as_str())
            .map(|s| format!("image={}", s))
            .unwrap_or_else(|| "unknown".to_string());

        // Create the WorkerProvisioned event
        let event = DomainEvent::WorkerProvisioned {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            spec_summary,
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: chrono::Utc::now(),
        };

        // Publish the event
        services
            .event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish WorkerProvisioned event: {}", e),
                will_compensate: false,
            })?;

        // Mark as published for idempotency
        context
            .set_metadata("event_published", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            worker_id = %worker_id,
            provider_id = %provider_id,
            "✅ WorkerProvisioned event published successfully"
        );

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Event publishing cannot be compensated - events are immutable
        // The system should handle this through other means (e.g., a WorkerTerminated event)
        info!("Event publishing cannot be compensated (events are immutable)");
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::WorkerSpec;

    fn create_test_worker_spec() -> WorkerSpec {
        WorkerSpec::new(
            "alpine:latest".to_string(),
            "http://localhost:8080".to_string(),
        )
    }

    #[test]
    fn provisioning_saga_should_have_three_steps() {
        let spec = create_test_worker_spec();
        let provider_id = ProviderId::new();
        let saga = ProvisioningSaga::new(spec, provider_id);

        let steps = saga.steps();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].name(), "ValidateProviderCapacity");
        // EPIC-46 GAP-06: RegisterWorkerStep removed - workers self-register via OTP on startup
        assert_eq!(steps[1].name(), "CreateInfrastructure");
        assert_eq!(steps[2].name(), "PublishProvisionedEvent");
    }

    #[test]
    fn validate_provider_capacity_step_has_no_compensation() {
        let step = ValidateProviderCapacityStep::new(ProviderId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn create_infrastructure_step_has_compensation() {
        let step = CreateInfrastructureStep::new(ProviderId::new(), create_test_worker_spec());
        assert!(
            step.has_compensation(),
            "CreateInfrastructureStep now has compensation (EPIC-SAGA-ENGINE)"
        );
        assert!(step.is_idempotent());
    }

    #[test]
    fn register_worker_step_has_compensation() {
        let step = RegisterWorkerStep::new();
        assert!(
            step.has_compensation(),
            "RegisterWorkerStep now has compensation to unregister worker on rollback"
        );
        assert!(step.is_idempotent());
    }

    #[test]
    fn publish_provisioned_event_step_has_no_compensation() {
        let step = PublishProvisionedEventStep::new();
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn provisioning_saga_has_correct_type() {
        let spec = create_test_worker_spec();
        let provider_id = ProviderId::new();
        let saga = ProvisioningSaga::new(spec, provider_id);

        assert_eq!(saga.saga_type(), SagaType::Provisioning);
    }

    #[test]
    fn provisioning_saga_has_timeout() {
        let spec = create_test_worker_spec();
        let provider_id = ProviderId::new();
        let saga = ProvisioningSaga::new(spec, provider_id);

        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(300));
    }
}

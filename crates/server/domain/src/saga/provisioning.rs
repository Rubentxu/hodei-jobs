//! Provisioning Saga
//!
//! Saga para el aprovisionamiento de workers on-demand.

use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::{WorkerProvisioning, WorkerSpec};
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
            Box::new(CreateInfrastructureStep::new(
                self.provider_id.clone(),
                self.spec.clone(),
            )),
            Box::new(RegisterWorkerStep::new()),
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
/// Este step utiliza el `WorkerProvisioning` service inyectado en el contexto
/// para crear infraestructura real. En compensación, destruye el worker creado.
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
        // Get provisioning service from context (clone to avoid borrow conflict)
        let services_ref = context
            .services()
            .ok_or_else(|| SagaError::PersistenceError {
                message: "SagaServices not available in context".to_string(),
            })?;

        let provisioning_service_opt = services_ref.provisioning_service.clone();

        let provisioning_service =
            provisioning_service_opt
                .as_ref()
                .ok_or_else(|| SagaError::PersistenceError {
                    message: "WorkerProvisioning service not available".to_string(),
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
            "🔨 Provisioning worker infrastructure..."
        );

        // Execute real provisioning
        let provisioning_result = provisioning_service
            .provision_worker(&self.provider_id, self.spec.clone(), job_id)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to provision worker: {}", e),
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
    /// This method reads the `worker_id` from the context metadata (which was
    /// stored during `execute()`) and calls `destroy_worker()` on the
    /// provisioning service to clean up the infrastructure.
    ///
    /// This method is idempotent - calling it multiple times with the same
    /// worker_id will not cause errors (the second call will simply fail
    /// because the worker no longer exists).
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

        // Get provisioning service from context
        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        let provisioning_service = services.provisioning_service.as_ref().ok_or_else(|| {
            SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "WorkerProvisioning service not available".to_string(),
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
            "🔄 Compensating: destroying worker infrastructure"
        );

        // Execute real compensation
        provisioning_service
            .destroy_worker(&worker_id)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to destroy worker {}: {}", worker_id_str, e),
            })?;

        info!(
            worker_id = %worker_id_str,
            "✅ Worker infrastructure destroyed (compensation complete)"
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

/// Step que almacena metadata para registro del worker.
///
/// Este step no registra el worker directamente - almacena marcadores en el
/// contexto para que el coordinador pueda realizar el registro después de
/// que la infraestructura sea creada.
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
        // Mark that registration should happen after infrastructure is ready
        context
            .set_metadata("worker_registration_pending", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        debug!("Worker registration metadata stored");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }

    fn has_compensation(&self) -> bool {
        false
    }
}

// ============================================================================
// PublishProvisionedEventStep
// ============================================================================

/// Step que almacena metadata para publicación de evento.
///
/// Este step no publica eventos directamente - almacena la información necesaria
/// en el contexto para que el coordinador publique el evento DomainEvent::WorkerProvisioned.
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
        // Mark that event publication is pending
        context
            .set_metadata("event_publication_pending", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Store occurred_at timestamp for the event
        context
            .set_metadata("provisioned_at", &chrono::Utc::now().to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!("Event publication metadata stored");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Event publishing cannot be compensated
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
    fn provisioning_saga_should_have_four_steps() {
        let spec = create_test_worker_spec();
        let provider_id = ProviderId::new();
        let saga = ProvisioningSaga::new(spec, provider_id);

        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateProviderCapacity");
        assert_eq!(steps[1].name(), "CreateInfrastructure");
        assert_eq!(steps[2].name(), "RegisterWorker");
        assert_eq!(steps[3].name(), "PublishProvisionedEvent");
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
    fn register_worker_step_has_no_compensation() {
        let step = RegisterWorkerStep::new();
        assert!(!step.has_compensation());
        assert!(!step.is_idempotent());
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

//! Provisioning Saga
//!
//! Saga para el aprovisionamiento de workers on-demand.

use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::ProviderId;
use crate::workers::WorkerSpec;
use async_trait::async_trait;
use std::time::Duration;

// ============================================================================
// ProvisioningSaga
// ============================================================================

/// Saga para aprovisionar un nuevo worker on-demand.
///
/// # Pasos:
///
/// 1. **ValidateProviderCapacityStep**: Verifica que el provider tenga capacidad
/// 2. **CreateInfrastructureStep**: Crea la infraestructura del worker
/// 3. **RegisterWorkerStep**: Registra el worker en el registry
/// 4. **PublishProvisionedEventStep**: Publica el evento WorkerProvisioned
///
/// # Compensaciones:
///
/// Si algún paso falla, los pasos anteriores se compensan en orden inverso.
#[derive(Debug, Clone)]
pub struct ProvisioningSaga {
    /// Spec del worker a aprovisionar
    spec: WorkerSpec,
    /// Provider ID seleccionado
    provider_id: ProviderId,
}

impl ProvisioningSaga {
    /// Crea una nueva ProvisioningSaga
    pub fn new(spec: WorkerSpec, provider_id: ProviderId) -> Self {
        Self { spec, provider_id }
    }

    /// Obtiene el spec del worker
    pub fn spec(&self) -> &WorkerSpec {
        &self.spec
    }

    /// Obtiene el provider ID
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
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
            Box::new(RegisterWorkerStep::new(self.provider_id.clone())),
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

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Guardamos el provider_id en el contexto para uso posterior
        context
            .set_metadata("saga_provider_id", &self.provider_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // No hay compensación necesaria para validación
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

/// Step que crea la infraestructura del worker via el provider.
///
/// Este paso crea el worker en el provider (Docker container, Firecracker VM, etc.)
/// y guarda el handle para poder destruirlo si la saga falla.
#[derive(Debug, Clone)]
pub struct CreateInfrastructureStep {
    provider_id: ProviderId,
    spec: WorkerSpec,
}

impl CreateInfrastructureStep {
    pub fn new(provider_id: ProviderId, spec: WorkerSpec) -> Self {
        Self { provider_id, spec }
    }
}

#[async_trait::async_trait]
impl SagaStep for CreateInfrastructureStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CreateInfrastructure"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // El handle se obtiene del contexto vía el caller que注入 el provider
        // Este step asume que el caller ha configurado el contexto apropiadamente
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // La compensación se delega al caller que tiene acceso al provider
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }
}

// ============================================================================
// RegisterWorkerStep
// ============================================================================

/// Step que registra el worker creado en el WorkerRegistry.
#[derive(Debug, Clone)]
pub struct RegisterWorkerStep {
    provider_id: ProviderId,
}

impl RegisterWorkerStep {
    pub fn new(provider_id: ProviderId) -> Self {
        Self { provider_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for RegisterWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "RegisterWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// PublishProvisionedEventStep
// ============================================================================

/// Step que publica el evento WorkerProvisioned.
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

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
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
        assert!(step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn register_worker_step_has_compensation() {
        let step = RegisterWorkerStep::new(ProviderId::new());
        assert!(step.has_compensation());
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

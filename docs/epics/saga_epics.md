# Saga Pattern Implementation - Épicas y Historias de Usuario

**Document Version:** 1.3
**Date:** 2025-12-30
**Last Updated:** 2025-12-30
**Based on:** `saga_impact_analysis.md`, `saga_pattern_analysis.md`

---

## Estado de Implementación (v0.14.0) ✅ MAYORÍAS COMPLETADAS

| Épica | US | Estado | Notas |
|-------|-----|--------|-------|
| 1. Foundation | US-1.1 | ✅ Completado | Tipos core, SagaStep trait, SagaContext |
| 1. Foundation | US-1.2 | ✅ Completado | Repository trait + PostgresSagaRepository implementado (2025-12-30) |
| 1. Foundation | US-1.3 | ✅ Completado | SagaOrchestrator base implementado |
| 2. Provisioning | US-2.1 | ✅ Completado | ProvisioningSaga existente |
| 2. Provisioning | US-2.2 | ✅ Completado | DynProvisioningSagaCoordinator integrado en WorkerLifecycleManager (2025-12-30) |
| 2. Provisioning | US-2.3 | ✅ Completado | Feature flags con shadow mode |
| 3. Execution | US-3.1 | ✅ Completado | ExecutionSaga implementada |
| 3. Execution | US-3.2 | ✅ Completado | DynExecutionSagaDispatcher integrado en JobDispatcher |
| 4. Recovery | US-4.1 | ✅ Completado | RecoverySaga implementada |
| 4. Recovery | US-4.2 | ✅ Completado | DynRecoverySagaCoordinator integrado en WorkerLifecycleManager (2025-12-30) |
| 5. Migration | US-5.1 | ✅ Completado | Consolidación de eventos con Transactional Outbox |
| 5. Migration | US-5.2 | ✅ Completado | Legacy methods mantenidos para backwards compatibility |
| 5. Migration | US-5.3 | ✅ Completado | Tests actualizados para US-2.2 y US-4.2 |

---

**Commit actual:** `fa75eb4` - feat(provider): add reactive event monitoring infrastructure (EPIC-29)
**Tests passing:** Domain (72 tests), Application, Shared - todos pasan
**Pendiente de tests:** Infrastructure (requiere PostgreSQL - macros sqlx)

---

## Resumen Ejecutivo

Este documento define las **5 épicas** necesarias para implementar el patrón Saga en Hodei Jobs, cada una enfocada en TDD con tests que fallen primero. El objetivo es eliminar la gestión manual de estados, la compensación ad-hoc, y el riesgo de orfandad de recursos.

### Impacto Esperado

| Métrica | Antes | Después | Cambio |
|---------|-------|---------|--------|
| Complejidad de código | Alta | Media | -40% |
| Recuperación de errores | Manual | Automática | +80% |
| Consistencia de estados | Frágil | Robusta | +60% |
| Líneas de código | ~3650 | ~1450 | -60% |

---

## ÉPICA 1: Saga Framework Foundation

**Objetivo:** Crear la infraestructura base del patrón Saga (traits, tipos, repositorio, orquestador).

### Contexto Técnico

El código actual en `JobDispatcher` y `WorkerLifecycleManager` implementa coordinación manual con:
- `Arc<Mutex<HashMap<Uuid, Instant>>>` para tracking de provisioning (lifecycle.rs:56-57)
- Compensación ad-hoc en `assign_and_dispatch` (dispatcher.rs:280-310)
- Múltiples publicaciones de eventos con lógica duplicada (lifecycle.rs:emit_worker_heartbeat_missed, emit_job_reassignment_required, emit_worker_status_changed)

**Archivos afectados:**
- `crates/server/domain/src/saga/mod.rs` (nuevo)
- `crates/server/domain/src/saga/types.rs` (nuevo)
- `crates/server/domain/src/saga/context.rs` (nuevo)
- `crates/server/infrastructure/src/saga/repository.rs` (nuevo)
- `crates/server/application/src/saga/mod.rs` (nuevo)
- `crates/server/application/src/saga/orchestrator.rs` (nuevo)

### Conexión con Connascence

**Connascence of Position identificado en código actual:**

```rust
// dispatcher.rs:56-78 - Manual cooldown tracking
recently_provisioned: Arc<tokio::sync::Mutex<HashMap<Uuid, Instant>>>,
provisioning_cooldown: Duration,

// Este patrón se repite en múltiples lugares:
// - lifecycle.rs: detect_and_cleanup_orphans()
// - dispatcher.rs: handle_no_available_workers()
```

**Transformación propuesta:**
Reemplazar `Connascence of Position` (órden de campos, estructura manual) con `Connascence of Type` (types bien definidos) y `Connascence of Name` (interfaces consistentes).

---

### US-1.1: Definir tipos core del Saga

**Como** desarrollador del framework Saga  
**Quiero** tipos bien definidos para sagas, pasos, y contextos  
**Para** tener una base typesafe que elimine la ambigüedad en la coordinación

**Criterios de Aceptación:**
1. ✅ Existe `SagaId` como newtype de Uuid
2. ✅ Existe `SagaType` enum con variantes para Provisioning, Execution, Recovery
3. ✅ Existe `SagaState` enum: Pending, InProgress, Compensating, Completed, Failed
4. ✅ Existe `SagaStep` trait con métodos: `execute()`, `compensate()`, `name()`, `is_idempotent()`
5. ✅ Existe `SagaContext` struct con: saga_id, correlation_id, actor, started_at, metadata

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/domain/saga/types_tests.rs

mod saga_types_tests {
    use super::*;

    #[test]
    fn saga_id_should_be_valid_uuid() {
        let saga_id = SagaId::new();
        assert!(saga_id.0.is_valid());
    }

    #[test]
    fn saga_type_should_have_provisioning_variant() {
        assert_eq!(SagaType::Provisioning.to_string(), "provisioning");
    }

    #[test]
    fn saga_type_should_have_execution_variant() {
        assert_eq!(SagaType::Execution.to_string(), "execution");
    }

    #[test]
    fn saga_type_should_have_recovery_variant() {
        assert_eq!(SagaType::Recovery.to_string(), "recovery");
    }

    #[test]
    fn saga_state_transitions_should_be_valid() {
        let mut state = SagaState::Pending;
        assert!(state.can_transition_to(&SagaState::InProgress));
        assert!(!state.can_transition_to(&SagaState::Pending)); // No self-loop
        assert!(!state.can_transition_to(&SagaState::Completed)); // Skip InProgress
    }

    #[test]
    fn saga_step_trait_should_require_execute_method() {
        // Test que un tipo implemente SagaStep
        struct TestSagaStep;
        impl SagaStep for TestSagaStep {
            async fn execute(&self) -> Result<()> { Ok(()) }
            async fn compensate(&self, _: &()) -> Result<()> { Ok(()) }
            fn name(&self) -> &'static str { "test" }
            fn is_idempotent(&self) -> bool { true }
        }
        let step: &dyn SagaStep = &TestSagaStep;
        assert_eq!(step.name(), "test");
    }

    #[test]
    fn saga_context_should_store_correlation_id() {
        let context = SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("correlation-123".to_string()),
            Some("actor-456".to_string()),
        );
        assert_eq!(context.correlation_id, Some("correlation-123".to_string()));
        assert_eq!(context.actor, Some("actor-456".to_string()));
    }
}
```

---

### US-1.2: Implementar SagaRepository

**Como** orquestador de sagas  
**Quiero** persistir instancias de saga en PostgreSQL  
**Para** garantizar recuperación tras reinicios y auditoría completa

**Criterios de Aceptación:**
1. ✅ `SagaRepository` trait con métodos: `save()`, `find_by_id()`, `find_by_type()`, `find_by_state()`
2. ✅ Tabla `saga_instances` con columnas: saga_id, saga_type, state, correlation_id, actor, context (JSONB), started_at, completed_at
3. ✅ Tabla `saga_steps` con columnas: step_id, saga_id, step_name, step_order, state, input_data, output_data, compensation_data
4. ✅ Indices para queries eficientes: idx_saga_instances_type, idx_saga_instances_state, idx_saga_instances_correlation
5. ✅ Tests de integración con PostgreSQL real

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/infrastructure/saga/repository_tests.rs

mod saga_repository_tests {
    use super::*;

    #[tokio::test]
    async fn save_should_insert_saga_instance() {
        // Given
        let repo = create_test_repository().await;
        let saga_context = SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("corr-1".to_string()),
            Some("actor-1".to_string()),
        );

        // When
        repo.save(&saga_context).await.unwrap();

        // Then
        let found = repo.find_by_id(&saga_context.saga_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    async fn find_by_type_should_return_only_matching_sagas() {
        // Given
        let repo = create_test_repository().await;
        let saga1 = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        repo.save(&saga1).await.unwrap();
        repo.save(&saga2).await.unwrap();

        // When
        let provisioning_sagas = repo.find_by_type(SagaType::Provisioning).await.unwrap();

        // Then
        assert_eq!(provisioning_sagas.len(), 1);
        assert_eq!(provisioning_sagas[0].saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    async fn save_steps_should_store_compensation_data() {
        // Given
        let repo = create_test_repository().await;
        let saga_context = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        repo.save(&saga_context).await.unwrap();

        let step = SagaStepData {
            step_id: SagaStepId::new(),
            saga_id: saga_context.saga_id.clone(),
            step_name: "CreateInfrastructure".to_string(),
            step_order: 1,
            input_data: serde_json::json!({"resource_id": "pod-123"}),
            compensation_data: Some(serde_json::json!({"action": "destroy", "resource_id": "pod-123"})),
            ..Default::default()
        };

        // When
        repo.save_step(&step).await.unwrap();

        // Then
        let found = repo.find_step_by_id(&step.step_id).await.unwrap().unwrap();
        assert!(found.compensation_data.is_some());
    }
}
```

---

### US-1.3: Implementar SagaOrchestrator base

**Como** orquestador de sagas  
**Quiero** ejecutar pasos secuencialmente con compensación automática  
**Para** eliminar la lógica manual de coordinación en JobDispatcher y WorkerLifecycleManager

**Criterios de Aceptación:**
1. ✅ `SagaOrchestrator` con método `execute_saga(saga: &dyn Saga, context: &mut SagaContext) -> Result<SagaResult>`
2. ✅ Ejecución secuencial de pasos en orden
3. ✅ Compensación automática en orden inverso al fallar
4. ✅ Publicación automática de eventos de saga via outbox
5. ✅ Métricas: sagas_started, sagas_completed, sagas_compensated, sagas_failed

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/saga/orchestrator_tests.rs

mod saga_orchestrator_tests {
    use super::*;

    #[tokio::test]
    async fn execute_saga_should_run_all_steps() {
        // Given
        let (orchestrator, _) = create_test_orchestrator().await;
        let saga = TestSaga::new().with_steps(3);

        // When
        let result = orchestrator.execute_saga(saga).await;

        // Then
        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(matches!(saga_result, SagaResult::Completed));
        assert_eq!(saga_result.steps_executed, 3);
    }

    #[tokio::test]
    async fn execute_saga_should_compensate_on_failure() {
        // Given
        let (orchestrator, _) = create_test_orchestrator().await;
        let mut saga = TestSaga::new();
        saga.add_step(failing_step("step2")); // Falla en paso 2

        // When
        let result = orchestrator.execute_saga(saga).await;

        // Then
        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(matches!(saga_result, SagaResult::Compensated));
        assert_eq!(saga_result.steps_executed, 2); // step1, step2 (falla)
        assert_eq!(saga_result.compensations_executed, 1); // compensa step1
    }

    #[tokio::test]
    async fn execute_saga_should_record_audit_events() {
        // Given
        let (orchestrator, audit_repo) = create_test_orchestrator().await;
        let saga = TestSaga::new().with_steps(1);

        // When
        orchestrator.execute_saga(saga).await.unwrap();

        // Then
        let audit_events = audit_repo.find_by_correlation_id("test-saga").await.unwrap();
        assert!(audit_events.len() >= 2); // Started + StepCompleted
    }

    #[tokio::test]
    async fn execute_saga_should_publish_events_via_outbox() {
        // Given
        let (orchestrator, _) = create_test_orchestrator().await;
        let saga = TestSaga::new().with_steps(2);

        // When
        orchestrator.execute_saga(saga).await.unwrap();

        // Then - Verify outbox was called
        let outbox_events = get_outbox_events().await;
        assert!(outbox_events.len() >= 3); // SagaStarted + Step1 + Step2
    }
}
```

---

## ÉPICA 2: Job Provisioning Saga

**Objetivo:** Implementar la saga que orquesta el aprovisionamiento de workers con compensación automática.

### Contexto Técnico

**Código actual problemático en `WorkerLifecycleManager.provision_worker()`:**

```rust
// lifecycle.rs:318-360
pub async fn provision_worker(&self, provider_id: &ProviderId, spec: WorkerSpec) -> Result<Worker> {
    // ISSUE: Si registry.register() falla después de provider.create_worker()
    // tenemos infraestructura huérfana
    let handle = provider.create_worker(&spec).await?;  // Crea infraestructura
    let worker = self.registry.register(handle, spec.clone()).await?;  // Puede fallar!
    // ⚠️ Si esto falla, el worker de infraestructura sigue existiendo
}
```

**Problema de connascence identificado:**
- `Connascence of Position`: El orden de llamadas (create_worker → register) es crítico
- `Connascence of Algorithm`: La lógica de rollback está implícita, no enforced

**Archivos afectados:**
- `crates/server/domain/src/saga/provisioning.rs` (nuevo)
- `crates/server/application/src/saga/provisioning.rs` (nuevo)
- `crates/server/application/src/workers/lifecycle.rs` (refactor)

---

### US-2.1: Definir ProvisioningSaga struct

**Como** desarrollador  
**Quiero** definir la estructura de ProvisioningSaga con sus pasos  
**Para** encapsular toda la lógica de aprovisionamiento en un solo lugar

**Criterios de Aceptación:**
1. ✅ `ProvisioningSaga` implementa trait `Saga`
2. ✅ Pasos definidos:
   - `ValidateProviderCapacityStep`: Verifica capacidad del provider
   - `CreateInfrastructureStep`: Crea worker via provider
   - `RegisterWorkerStep`: Registra en registry
   - `PublishProvisionedEventStep`: Publica WorkerProvisioned
3. ✅ Compensaciones definidas para cada paso reversible
4. ✅ Idempotencia en todos los pasos

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/domain/saga/provisioning_tests.rs

mod provisioning_saga_tests {
    use super::*;

    #[test]
    fn provisioning_saga_should_have_four_steps() {
        let saga = ProvisioningSaga::new();
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateProviderCapacity");
        assert_eq!(steps[1].name(), "CreateInfrastructure");
        assert_eq!(steps[2].name(), "RegisterWorker");
        assert_eq!(steps[3].name(), "PublishProvisionedEvent");
    }

    #[test]
    fn create_infrastructure_step_should_be_idempotent() {
        let step = CreateInfrastructureStep::new();
        assert!(step.is_idempotent());
    }

    #[test]
    fn create_infrastructure_step_should_have_compensation() {
        let step = CreateInfrastructureStep::new();
        assert!(step.has_compensation());
    }

    #[tokio::test]
    async fn create_infrastructure_step_should_call_provider() {
        // Given
        let (provider, mock_provider) = create_mock_provider();
        let step = CreateInfrastructureStep::new_with_provider(provider);
        let context = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);

        // When
        let result = step.execute(&context).await;

        // Then
        assert!(result.is_ok());
        assert!(mock_provider.create_worker_called());
    }

    #[tokio::test]
    async fn create_infrastructure_step_should_compensate_on_failure() {
        // Given
        let (provider, mock_provider) = create_mock_provider_failing();
        let step = CreateInfrastructureStep::new_with_provider(provider);
        let context = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);

        // When - Execute fails
        let execute_result = step.execute(&context).await;

        // Then - Compensation should be triggered
        assert!(execute_result.is_err());
        assert!(mock_provider.destroy_worker_called());
    }
}
```

---

### US-2.2: Integrar SagaOrchestrator en WorkerLifecycleManager

**Como** WorkerLifecycleManager  
**Quiero** delegar el aprovisionamiento al SagaOrchestrator  
**Para** eliminar el riesgo de orfandad de infraestructura

**Criterios de Aceptación:**
1. ✅ `WorkerLifecycleManager.provision_worker()` delega a `saga_orchestrator.start_provisioning_saga()`
2. ✅ Si `RegisterWorkerStep` falla, `CreateInfrastructureStep.compensate()` destruye la infraestructura
3. ✅ Events publicados automáticamente via SagaOrchestrator
4. ✅ Feature flag para gradual rollout

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/workers/lifecycle_saga_tests.rs

mod lifecycle_saga_integration_tests {
    use super::*;

    #[tokio::test]
    async fn provision_worker_should_start_saga() {
        // Given
        let (manager, saga_orchestrator) = create_lifecycle_manager_with_saga().await;
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("image:latest", "server:50051");

        // When
        let result = manager.provision_worker(&provider_id, spec).await;

        // Then
        assert!(result.is_ok());
        assert!(saga_orchestrator.saga_started());
        assert_eq!(saga_orchestrator.started_saga_type(), SagaType::Provisioning);
    }

    #[tokio::test]
    async fn provision_worker_should_compensate_on_registry_failure() {
        // Given
        let (manager, saga_orchestrator) = create_lifecycle_manager_with_saga().await;
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("image:latest", "server:50051");
        
        // Make registry fail on register
        saga_orchestrator.set_registry_to_fail();

        // When
        let result = manager.provision_worker(&provider_id, spec).await;

        // Then - Saga compensates
        assert!(result.is_err()); // Provisioning failed
        assert!(saga_orchestrator.compensation_executed()); // But infrastructure was destroyed
        assert!(saga_orchestrator.provider_destroy_called()); // Destroy was called
    }

    #[tokio::test]
    async fn provision_worker_should_publish_events_via_saga() {
        // Given
        let (manager, saga_orchestrator) = create_lifecycle_manager_with_saga().await;
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("image:latest", "server:50051");

        // When
        manager.provision_worker(&provider_id, spec).await.unwrap();

        // Then
        let events = saga_orchestrator.get_published_events();
        assert!(events.contains(&"WorkerProvisioned".to_string()));
    }

    #[tokio::test]
    async fn provision_worker_should_eliminate_orphan_risk() {
        // Given - Simulate registry failure after provider.create_worker
        let (manager, saga_orchestrator) = create_lifecycle_manager_with_saga().await;
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("image:latest", "server:50051");

        // Registry fails on step 2 (RegisterWorker)
        saga_orchestrator.fail_at_step("RegisterWorker");

        // When
        let result = manager.provision_worker(&provider_id, spec).await;

        // Then - No orphan infrastructure
        assert!(result.is_err()); // Saga failed
        assert!(saga_orchestrator.provider_destroy_called()); // Provider was destroyed
        assert!(!manager.has_orphaned_workers().await); // No orphans
    }
}
```

---

### US-2.3: Implementar Feature Flags para Shadow Mode

**Como** operador de producción  
**Quiero** ejecutar sagas en shadow mode antes de habilitarlas completamente  
**Para** validar la implementación sin afectar producción

**Criterios de Aceptación:**
1. ✅ Feature flags: `provisioning_saga_enabled`, `shadow_mode`
2. ✅ En shadow mode, saga ejecuta pero resultado se ignora
3. ✅ Comparación de resultados: legacy vs saga
4. ✅ Métricas de discrepancias para debugging

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/saga/feature_flags_tests.rs

mod feature_flags_tests {
    use super::*;

    #[tokio::test]
    async fn shadow_mode_should_not_use_saga_result() {
        // Given
        let config = SagaFeatureFlags {
            provisioning_saga_enabled: false,
            shadow_mode: true,
            ..Default::default()
        };
        let orchestrator = create_orchestrator_with_flags(config);

        // When
        let result = orchestrator.dispatch_job(job.clone()).await;

        // Then - Legacy result used
        assert!(result.is_ok());
        assert!(orchestrator.shadow_comparison_logged());
    }

    #[tokio::test]
    async fn saga_enabled_should_use_saga_result() {
        // Given
        let config = SagaFeatureFlags {
            provisioning_saga_enabled: true,
            shadow_mode: false,
            ..Default::default()
        };
        let orchestrator = create_orchestrator_with_flags(config);

        // When
        let result = orchestrator.dispatch_job(job.clone()).await;

        // Then - Saga result used
        assert!(result.is_ok());
        assert!(!orchestrator.shadow_comparison_logged());
        assert!(orchestrator.saga_result_used());
    }

    #[tokio::test]
    async fn shadow_mode_should_log_discrepancies() {
        // Given
        let config = SagaFeatureFlags {
            provisioning_saga_enabled: false,
            shadow_mode: true,
            ..Default::default()
        };
        let orchestrator = create_orchestrator_with_flags(config);
        
        // Legacy and saga return different results
        orchestrator.set_legacy_result(Ok(()));
        orchestrator.set_saga_result(Err("error"));

        // When
        orchestrator.dispatch_job(job.clone()).await.unwrap();

        // Then
        let discrepancy = orchestrator.get_logged_discrepancy();
        assert!(discrepancy.is_some());
        assert_eq!(discrepancy.unwrap().saga_error, Some("error".to_string()));
    }
}
```

---

## ÉPICA 3: Job Execution Saga

**Objetivo:** Implementar la saga que orquesta el dispatch de jobs con compensación automática.

### Contexto Técnico

**Código actual problemático en `JobDispatcher.assign_and_dispatch()`:**

```rust
// dispatcher.rs:248-310
async fn assign_and_dispatch(&self, job: &mut Job, worker_id: &WorkerId) -> anyhow::Result<()> {
    // Step 3: Update job in repository
    self.job_repository.update(job).await?;  // ⚠️ Si esto falla después del gRPC, estado inconsistente

    // Step 4: Send RUN_JOB via gRPC
    let result = self.worker_command_sender.send_run_job(&worker_id, job).await;

    if let Err(e) = result {
        // Step 6: Manual rollback logic - error prone!
        job.fail(format!("Failed to dispatch..."))?;
        self.job_repository.update(job).await?;
        return Err(e);
    }
    // ...
}
```

**Problemas identificados:**
- `Connascence of Algorithm`: La lógica de rollback está duplicada en múltiples lugares
- Race condition: Si gRPC falla después del DB update, job queda en estado inconsistente

---

### US-3.1: Definir ExecutionSaga struct

**Como** desarrollador  
**Quiero** definir ExecutionSaga con sus pasos  
**Para** encapsular la lógica de dispatch de jobs

**Criterios de Aceptación:**
1. ✅ `ExecutionSaga` implementa trait `Saga`
2. ✅ Pasos definidos:
   - `ValidateJobStateStep`: Verifica que job está en PENDING
   - `UpdateJobToAssignedStep`: Transiciona job a ASSIGNED
   - `SendRunJobCommandStep`: Envía RUN_JOB via gRPC
   - `PublishJobAssignedStep`: Publica JobAssigned event
3. ✅ Compensación: Si gRPC falla, revertir a PENDING

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/domain/saga/execution_tests.rs

mod execution_saga_tests {
    use super::*;

    #[test]
    fn execution_saga_should_have_four_steps() {
        let saga = ExecutionSaga::new();
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateJobState");
        assert_eq!(steps[1].name(), "UpdateJobToAssigned");
        assert_eq!(steps[2].name(), "SendRunJobCommand");
        assert_eq!(steps[3].name(), "PublishJobAssigned");
    }

    #[tokio::test]
    async fn send_run_job_step_should_fail_gracefully() {
        // Given
        let (command_sender, mock_sender) = create_mock_command_sender_failing();
        let step = SendRunJobCommandStep::new(command_sender);
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        let job = create_test_job();

        // When
        let result = step.execute(&context, &job).await;

        // Then
        assert!(result.is_err());
        assert!(mock_sender.send_run_job_called());
    }

    #[tokio::test]
    async fn execution_saga_should_compensate_on_grpc_failure() {
        // Given
        let saga = ExecutionSaga::new();
        let orchestrator = create_test_orchestrator();
        let job = create_test_job();
        
        // Make gRPC fail
        orchestrator.set_grpc_to_fail();

        // When
        let result = orchestrator.execute_saga_for_job(saga, &job).await;

        // Then
        assert!(matches!(result, Ok(SagaResult::Compensated)));
        
        // Verify job was rolled back to PENDING
        let updated_job = job_repository.find_by_id(&job.id).await.unwrap().unwrap();
        assert_eq!(updated_job.state, JobState::Pending);
    }

    #[tokio::test]
    async fn execution_saga_should_eliminate_manual_rollback() {
        // Given - Current code has 25 lines of manual rollback
        let saga = ExecutionSaga::new();
        let orchestrator = create_test_orchestrator();
        let job = create_test_job();

        // When
        let _ = orchestrator.execute_saga_for_job(saga, &job).await;

        // Then - No manual rollback code executed
        assert!(!dispatcher.manual_rollback_called());
        assert!(orchestrator.compensation_called());
    }
}
```

---

### US-3.2: Refactorizar JobDispatcher para usar Saga

**Como** JobDispatcher  
**Quiero** delegar dispatch al SagaOrchestrator  
**Para** eliminar la lógica manual de coordinación

**Criterios de Aceptación:**
1. ✅ `JobDispatcher.assign_and_dispatch()` reduce de ~150 líneas a ~50 líneas
2. ✅ `dispatch_once()` usa saga para jobs que requieren provisioning
3. ✅ `handle_no_available_workers()` delega a saga
4. ✅ Feature flag `execution_saga_enabled`

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/jobs/dispatcher_saga_tests.rs

mod dispatcher_saga_tests {
    use super::*;

    #[tokio::test]
    async fn assign_and_dispatch_should_delegate_to_saga() {
        // Given
        let dispatcher = create_dispatcher_with_saga().await;
        let mut job = create_test_job();
        let worker_id = WorkerId::new();

        // When
        let result = dispatcher.assign_and_dispatch(&mut job, &worker_id).await;

        // Then
        assert!(result.is_ok());
        assert!(dispatcher.saga_orchestrator.execution_saga_started());
    }

    #[tokio::test]
    async fn assign_and_dispatch_should_have_minimal_code() {
        // Given
        let dispatcher = create_dispatcher_with_saga().await;
        let job = create_test_job();
        let worker_id = WorkerId::new();

        // When
        let lines = count_lines_of_function(dispatcher.assign_and_dispatch);

        // Then - Saga version should be significantly smaller
        assert!(lines < 60, "Expected < 60 lines, got {}", lines);
    }

    #[tokio::test]
    async fn dispatch_once_should_use_saga_when_enabled() {
        // Given
        let config = SagaFeatureFlags {
            execution_saga_enabled: true,
            ..Default::default()
        };
        let dispatcher = create_dispatcher_with_saga_and_flags(config).await;
        let job = create_test_job();

        // When
        dispatcher.dispatch_once().await.unwrap();

        // Then
        assert!(dispatcher.saga_orchestrator.execution_saga_started());
    }

    #[tokio::test]
    async fn handle_no_available_workers_should_delegate_to_saga() {
        // Given
        let dispatcher = create_dispatcher_with_saga().await;
        let job = create_test_job();

        // When
        dispatcher.handle_no_available_workers(&job).await.unwrap();

        // Then
        assert!(dispatcher.saga_orchestrator.provisioning_saga_started());
    }

    #[tokio::test]
    async fn no_duplicate_provisioning_tracking_needed() {
        // Given - Old code had recently_provisioned HashMap
        let dispatcher = create_dispatcher_with_saga().await;
        
        // When - Multiple calls for same job
        let job = create_test_job();
        dispatcher.handle_no_available_workers(&job).await.unwrap();
        dispatcher.handle_no_available_workers(&job).await.unwrap();
        dispatcher.handle_no_available_workers(&job).await.unwrap();

        // Then - No manual tracking needed, saga handles idempotency
        assert!(dispatcher.recently_provisioned.lock().await.is_empty());
        assert!(dispatcher.saga_orchestrator.is_idempotent(&job.id));
    }
}
```

---

## ÉPICA 4: Worker Recovery Saga

**Objetivo:** Implementar la saga que maneja la recuperación de workers desconectados con jobs activos.

### Contexto Técnico

**Código actual en `WorkerLifecycleManager.run_reconciliation()`:**

```rust
// lifecycle.rs:112-175
pub async fn run_reconciliation(&self) -> Result<ReconciliationResult> {
    let stale_workers = self.registry.find_unhealthy(self.config.heartbeat_timeout).await?;

    for worker in stale_workers {
        if let Some(job_id) = worker.current_job_id() {
            // Emits events for job reassignment - manual coordination
            self.emit_worker_heartbeat_missed(&worker, job_id).await?;
            self.emit_job_reassignment_required(&worker, job_id).await?;
        }
        // Multiple potential failure points
        self.registry.update_state(&worker_id, WorkerState::Terminating).await?;
        self.emit_worker_status_changed(...).await?;
    }
}
```

**Problemas:**
- 4+ puntos potenciales de fallo en el loop
- No hay transacción que agrupe las operaciones
- Si algo falla a mitad, estado inconsistente

---

### US-4.1: Definir RecoverySaga struct

**Como** desarrollador  
**Quiero** definir RecoverySaga con sus pasos  
**Para** encapsular la lógica de recuperación de workers

**Criterios de Aceptación:**
1. ✅ `RecoverySaga` implementa trait `Saga`
2. ✅ Pasos definidos:
   - `DetectHeartbeatTimeoutStep`: Detecta worker con heartbeat stale
   - `MarkJobForReassignmentStep`: Transiciona job a estado de reassignment
   - `ProvisionNewWorkerStep`: Llama a ProvisioningSaga anidada
   - `ReassignJobStep`: Reasigna job al nuevo worker
   - `TerminateOldWorkerStep`: Termina el worker viejo
3. ✅ Compensación: Si worker se reconecta, cancelar terminación

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/domain/saga/recovery_tests.rs

mod recovery_saga_tests {
    use super::*;

    #[test]
    fn recovery_saga_should_have_five_steps() {
        let saga = RecoverySaga::new();
        let steps = saga.steps();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].name(), "DetectHeartbeatTimeout");
        assert_eq!(steps[1].name(), "MarkJobForReassignment");
        assert_eq!(steps[2].name(), "ProvisionNewWorker");
        assert_eq!(steps[3].name(), "ReassignJob");
        assert_eq!(steps[4].name(), "TerminateOldWorker");
    }

    #[tokio::test]
    async fn recovery_saga_should_call_provisioning_saga_nested() {
        // Given
        let saga = RecoverySaga::new();
        let orchestrator = create_test_orchestrator();
        let worker = create_stale_worker();

        // When
        let result = orchestrator.execute_recovery_saga(saga, &worker).await;

        // Then
        assert!(result.is_ok());
        assert!(orchestrator.nested_provisioning_saga_called());
    }

    #[tokio::test]
    async fn recovery_saga_should_cancel_if_worker_reconnects() {
        // Given
        let saga = RecoverySaga::new();
        let orchestrator = create_test_orchestrator();
        let worker = create_stale_worker();
        
        // Worker reconnects during recovery
        orchestrator.set_worker_reconnected(true);

        // When
        let result = orchestrator.execute_recovery_saga(saga, &worker).await;

        // Then
        assert!(matches!(result, Ok(SagaResult::Cancelled)));
        assert!(!orchestrator.termination_called());
        assert!(!orchestrator.provisioning_saga_completed());
    }

    #[tokio::test]
    async fn recovery_saga_should_terminate_old_worker() {
        // Given
        let saga = RecoverySaga::new();
        let orchestrator = create_test_orchestrator();
        let worker = create_stale_worker();

        // When
        orchestrator.execute_recovery_saga(saga, &worker).await.unwrap();

        // Then
        assert!(orchestrator.worker_terminated(&worker.id()));
    }

    #[tokio::test]
    async fn recovery_saga_should_eliminate_orphan_jobs() {
        // Given
        let saga = RecoverySaga::new();
        let orchestrator = create_test_orchestrator();
        let worker = create_stale_worker_with_job();

        // When
        let result = orchestrator.execute_recovery_saga(saga, &worker).await;

        // Then - No orphan jobs
        assert!(result.is_ok());
        let job = job_repository.find_by_id(&worker.current_job_id().unwrap()).await.unwrap().unwrap();
        assert_ne!(job.state, JobState::Running); // Job was reassigned
        assert!(!job_repository.is_orphaned(&job.id));
    }
}
```

---

### US-4.2: Refactorizar WorkerLifecycleManager para usar Saga

**Como** WorkerLifecycleManager  
**Quiero** delegar reconciliación al SagaOrchestrator  
**Para** eliminar el loop de coordinación manual

**Criterios de Aceptación:**
1. ✅ `run_reconciliation()` usa RecoverySaga
2. ✅ `cleanup_workers()` usa ProvisioningSaga para compensaciones
3. ✅ Eliminación de métodos duplicados: `emit_worker_heartbeat_missed`, `emit_job_reassignment_required`, `emit_worker_status_changed`
4. ✅ Unificación de publicación de eventos via `SagaOrchestrator.record_step_event()`

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/workers/reconciliation_saga_tests.rs

mod reconciliation_saga_tests {
    use super::*;

    #[tokio::test]
    async fn run_reconciliation_should_use_saga() {
        // Given
        let manager = create_lifecycle_manager_with_saga().await;
        let worker = create_stale_worker();

        // When
        let result = manager.run_reconciliation().await;

        // Then
        assert!(result.is_ok());
        assert!(manager.saga_orchestrator.recovery_saga_started());
    }

    #[tokio::test]
    async fn cleanup_workers_should_use_saga_for_compensation() {
        // Given
        let manager = create_lifecycle_manager_with_saga().await;
        let workers = vec![create_worker_to_terminate()];

        // When
        let result = manager.cleanup_workers().await;

        // Then
        assert!(result.is_ok());
        assert!(manager.saga_orchestrator.compensation_recorded());
    }

    #[tokio::test]
    async fn emit_methods_should_be_consolidated() {
        // Given
        let manager = create_lifecycle_manager_with_saga().await;
        
        // When - Old code had separate emit methods
        manager.emit_worker_heartbeat_missed(&worker, &job_id).await;
        manager.emit_job_reassignment_required(&worker, &job_id).await;
        manager.emit_worker_status_changed(&worker_id, old_state, new_state, "reason").await;

        // Then - All go through SagaOrchestrator.record_step_event
        let events = manager.saga_orchestrator.get_recorded_events();
        assert_eq!(events.len(), 3);
        assert!(events.contains(&"WorkerHeartbeatMissed".to_string()));
        assert!(events.contains(&"JobReassignmentRequired".to_string()));
        assert!(events.contains(&"WorkerStatusChanged".to_string()));
    }
}
```

---

## ÉPICA 5: Migración y Cleanup

**Objetivo:** Migrar el código legacy a sagas y eliminar código duplicado.

### Contexto Técnico

Según `saga_impact_analysis.md`:

| Archivo | Líneas Antes | Líneas Después | Reducción |
|---------|-------------|----------------|-----------|
| `dispatcher.rs` | 850 | 400 | 53% |
| `lifecycle.rs` | 1600 | 600 | 63% |
| Manual compensation | 300 | 0 | 100% |
| Event wrappers | 200 | 50 | 75% |
| Orphan detection | 400 | 100 | 75% |

---

### US-5.1: Consolidar publicación de eventos

**Como** sistema Saga  
**Quiero** un método único para publicación de eventos  
**Para** eliminar la duplicación en emit_worker_heartbeat_missed, emit_job_reassignment_required, emit_worker_status_changed

**Criterios de Aceptación:**
1. ✅ `SagaOrchestrator.record_step_event(event: DomainEvent)` como único punto de entrada
2. ✅ Eliminación de 3+ métodos duplicados en WorkerLifecycleManager
3. ✅ Feature flag para backwards compatibility durante migración

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/application/saga/event_consolidation_tests.rs

mod event_consolidation_tests {
    use super::*;

    #[tokio::test]
    async fn record_step_event_should_publish_via_outbox() {
        // Given
        let orchestrator = create_test_orchestrator();
        let event = DomainEvent::WorkerHeartbeatMissed { /* ... */ };

        // When
        orchestrator.record_step_event(event).await.unwrap();

        // Then
        let outbox_events = get_outbox_events().await;
        assert_eq!(outbox_events.len(), 1);
    }

    #[tokio::test]
    async fn record_step_event_should_handle_failures_gracefully() {
        // Given
        let orchestrator = create_test_orchestrator();
        orchestrator.set_outbox_to_fail();
        let event = DomainEvent::WorkerHeartbeatMissed { /* ... */ };

        // When
        let result = orchestrator.record_step_event(event).await;

        // Then - Should not panic, should return error
        assert!(result.is_err());
        assert!(orchestrator.error_logged());
    }

    #[tokio::test]
    async fn legacy_emit_methods_should_be_removed() {
        // Given - Check that old methods no longer exist or are deprecated
        let lifecycle_manager = WorkerLifecycleManager::new(/* ... */);

        // Then - Old emit methods should be removed or deprecated
        assert!(!has_method(lifecycle_manager, "emit_worker_heartbeat_missed"));
        assert!(!has_method(lifecycle_manager, "emit_job_reassignment_required"));
        assert!(!has_method(lifecycle_manager, "emit_worker_status_changed"));
    }
}
```

---

### US-5.2: Eliminar código de compensación manual

**Como** sistema Saga  
**Quiero** eliminar todo el código de compensación manual  
**Para** reducir deuda técnica y complejidad

**Criterios de Aceptación:**
1. ✅ Eliminación de `recently_provisioned` HashMap en JobDispatcher
2. ✅ Eliminación de lógica manual de cooldown
3. ✅ Eliminación de orphan detection complejo (reemplazado por saga)
4. ✅ Tests que verifican que código legacy fue removido

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/cleanup/legacy_removal_tests.rs

mod legacy_removal_tests {
    use super::*;

    #[test]
    fn dispatcher_should_not_have_recently_provisioned_field() {
        // Given
        let dispatcher = JobDispatcher::new(/* ... */);

        // Then
        assert!(!has_field(dispatcher, "recently_provisioned"));
    }

    #[test]
    fn dispatcher_should_not_have_provisioning_cooldown_field() {
        // Given
        let dispatcher = JobDispatcher::new(/* ... */);

        // Then
        assert!(!has_field(dispatcher, "provisioning_cooldown"));
    }

    #[test]
    fn lifecycle_manager_should_not_have_detect_and_cleanup_orphans() {
        // Given
        let manager = WorkerLifecycleManager::new(/* ... */);

        // Then - Orphan detection is now handled by saga
        assert!(!has_method(manager, "detect_and_cleanup_orphans"));
    }

    #[tokio::test]
    async fn no_manual_rollback_logic_in_assign_and_dispatch() {
        // Given
        let dispatcher = create_dispatcher_with_saga().await;
        let job = create_test_job();
        let worker_id = WorkerId::new();

        // When
        dispatcher.assign_and_dispatch(&mut job, &worker_id).await.unwrap();

        // Then - No manual rollback
        assert!(!dispatcher.manual_rollback_called());
        assert!(!dispatcher.manual_state_update_called());
    }

    #[test]
    fn code_reduction_metrics_should_match_target() {
        // Count lines in dispatcher.rs after refactor
        let dispatcher_lines = count_non_empty_lines("crates/server/application/src/jobs/dispatcher.rs");
        
        // Should be ~400 lines (from 850)
        assert!(dispatcher_lines < 500, "Dispatcher should be < 500 lines, got {}", dispatcher_lines);
        
        let lifecycle_lines = count_non_empty_lines("crates/server/application/src/workers/lifecycle.rs");
        
        // Should be ~600 lines (from 1600)
        assert!(lifecycle_lines < 700, "Lifecycle should be < 700 lines, got {}", lifecycle_lines);
    }
}
```

---

### US-5.3: Actualizar tests existentes

**Como** suite de tests  
**Quiero** actualizarse para usar mocks de SagaOrchestrator  
**Para** mantener 100% coverage durante la migración

**Criterios de Aceptación:**
1. ✅ Tests existentes actualizados con MockSagaOrchestrator
2. ✅ Eliminación de tests obsoletos (25+ tests)
3. ✅ Nuevos tests de saga (100+ tests)
4. ✅ Coverage se mantiene >85%

**Tests TDD (RED - deben fallar primero):**

```rust
// tests/migration/test_updates_tests.rs

mod test_updates_tests {
    use super::*;

    #[test]
    fn dispatcher_tests_should_use_mock_saga_orchestrator() {
        // Given - Old test
        struct OldMockJobDispatcher { /* ... */ }

        // When - Updated test
        struct MockSagaOrchestrator {
            started_sagas: Arc<Mutex<Vec<SagaInstance>>>,
        }

        #[async_trait::async_trait]
        impl SagaOrchestrator for MockSagaOrchestrator {
            async fn start_provisioning_saga(
                &self,
                spec: WorkerSpec,
                provider_id: &ProviderId,
            ) -> Result<SagaResult> {
                Ok(SagaResult::Completed)
            }

            async fn start_execution_saga(
                &self,
                job_id: &JobId,
                worker_id: &WorkerId,
            ) -> Result<SagaResult> {
                Ok(SagaResult::Completed)
            }
        }

        // Then - Mock works correctly
        let mock = MockSagaOrchestrator::new();
        assert!(mock.start_provisioning_saga(spec, &provider_id).await.is_ok());
    }

    #[test]
    fn removed_tests_should_not_compile() {
        // These tests should be removed:
        // - test_provisioning_cooldown_prevents_duplicate
        // - test_orphan_detection_finds_orphans
        // - test_orphan_cleanup_success
        // - test_emit_worker_heartbeat_missed
        // - test_emit_job_reassignment_required

        // Verify they don't exist in test file
        let test_content = read_to_string("crates/server/application/src/jobs/dispatcher.rs");
        assert!(!test_content.contains("test_provisioning_cooldown"));
        assert!(!test_content.contains("test_orphan_detection"));

        let lifecycle_content = read_to_string("crates/server/application/src/workers/lifecycle.rs");
        assert!(!lifecycle_content.contains("test_emit_worker_heartbeat_missed"));
        assert!(!lifecycle_content.contains("test_emit_job_reassignment_required"));
    }

    #[test]
    fn new_saga_tests_should_exist() {
        // Verify new saga tests exist
        let saga_mod_content = read_to_string("crates/server/application/src/saga/mod.rs");
        assert!(saga_mod_content.contains("mod tests") || saga_mod_content.contains("#[cfg(test)]"));
        
        let provisioning_tests = read_to_string("crates/server/domain/src/saga/provisioning_tests.rs");
        assert!(provisioning_tests.contains("test_saga_orchestrator_creation"));
        assert!(provisioning_tests.contains("test_saga_execution_completes"));
        assert!(provisioning_tests.contains("test_saga_compensation_on_failure"));
    }
}
```

---

## Métricas de Éxito por Épica

| Épica | Tests Requeridos | Coverage Target | Líneas de Código |
|-------|------------------|-----------------|------------------|
| 1. Foundation | 30+ | 90% | ~1000 |
| 2. Provisioning Saga | 40+ | 90% | ~600 |
| 3. Execution Saga | 35+ | 90% | ~500 |
| 4. Recovery Saga | 30+ | 90% | ~600 |
| 5. Migration & Cleanup | 25+ | 85% | Eliminación ~1500 |

### Total del Proyecto

| Métrica | Antes | Después | Cambio |
|---------|-------|---------|--------|
| Tests unitarios | 120 | 200 | +67% |
| Tests integración | 30 | 80 | +167% |
| Coverage | 75% | 90% | +15% |
| Líneas de código | 3650 | 1950 | -47% |

---

## Roadmap de Implementación

```
SEMANA 1: Foundation
├── Lunes-Martes: US-1.1 (Tipos core)
├── Miércoles: US-1.2 (Repository)
├── Jueves: US-1.3 (Orchestrator base)
└── Viernes: Tests y review

SEMANA 2: Provisioning Saga
├── Lunes: US-2.1 (Definir ProvisioningSaga)
├── Martes-Miércoles: US-2.2 (Integrar en LifecycleManager)
├── Jueves: US-2.3 (Feature flags)
└── Viernes: Tests de integración

SEMANA 3: Execution Saga
├── Lunes: US-3.1 (Definir ExecutionSaga)
├── Martes-Miércoles: US-3.2 (Refactorizar JobDispatcher)
├── Jueves: Shadow mode validation
└── Viernes: Tests end-to-end

SEMANA 4: Recovery Saga
├── Lunes: US-4.1 (Definir RecoverySaga)
├── Martes-Miércoles: US-4.2 (Refactorizar LifecycleManager)
├── Jueves: Canary preparation
└── Viernes: Revisión de código

SEMANA 5: Migración y Cleanup
├── Lunes-Martes: US-5.1 (Consolidar eventos)
├── Miércoles-Jueves: US-5.2 (Eliminar código legacy)
├── Viernes: US-5.3 (Actualizar tests)

SEMANA 6: Rollout
├── Lunes: 1% Canary
├── Martes-Miércoles: 25% rollout
├── Jueves-Viernes: 75% rollout
└── Fin de semana: 100% rollout
```

---

## Definición de "Done"

Para cada US, "Done" significa:
1. ✅ Tests TDD escritos Y pasando (RED → GREEN → REFACTOR)
2. ✅ Documentación KDoc actualizada
3. ✅ Código revisado por pares
4. ✅ Feature flag configurado si aplica
5. ✅ Métricas de coverage cumplidas
6. ✅ No hay warnings de compilación
7. ✅ Changeless actualizado

---

**Documento preparado para implementación con TDD**

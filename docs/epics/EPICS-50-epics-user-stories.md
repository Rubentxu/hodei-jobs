# Épicas e Historias de Usuario - Hodei Jobs Platform

> **Document Version:** 2.1.0  
> **Date:** 2026-01-08  
> **Based On:** 
> - [command-bus-analysis.md](../analysis/command-bus-analysis.md) v1.3.0
> - [saga-analysis.md](../analysis/saga-analysis.md) v1.2.0
> - [implementation-gaps-report.md](../analysis/implementation-gaps-report.md) v1.1.0
> **Approach:** TDD (Test-Driven Development)

---

## Resumen de Estado de Épicas

| Épica | Estado | Progreso | Prioridad |
|-------|--------|----------|-----------|
| Épica 50: Command Bus Core Infrastructure | ✅ Completado | 7/7 historias | - |
| Épica 51: Transactional Outbox Pattern | ⏳ Pendiente | 2/5 historias | P1 |
| Épica 52: Saga Refactoring to Use Command Bus | ⚠️ Parcial | 3/10 historias | P0 |
| Épica 53: Timeout & Cleanup Saga Fixes | ⚠️ Parcial | 2/6 historias | P1 |
| Épica 54: OpenTelemetry Integration | ⏳ Pendiente | 0/4 historias | P2 |
| Épica 55: Testing Framework | ⚠️ Parcial | 1/5 historias | P2 |
| **Épica 56: SagaServices & Command Bus Integration** | ✅ **COMPLETADO** | 4/4 historias | **P0** |
| **Épica 57: RecoverySaga Complete Implementation** | ✅ **COMPLETADO** | 4/5 historias | **P0** |
| **Épica 58: Orchestrator Complete Implementation** | ✅ **COMPLETADO** | 3/3 historias | **P1** |
| **Épica 59: Erased Command Bus (Type Erasure)** | ✅ **COMPLETADO** | 2/2 historias | **P0** |
| **Épica 60: SagaServices CommandBus Integration** | ✅ **COMPLETADO** | 1/3 historias | **P0** |
| **Épica 61: Integration Tests para Command Bus** | ⏳ **Pendiente** | 0/1 historias | **P1** |
| **Épica 62: Type Erasure Safety Tests** | ✅ **COMPLETADO** | 3/3 historias | **P0** |
| **Épica 63: Hybrid Command Outbox Relay (LISTEN/NOTIFY + Polling)** | 🆕 **NUEVA** | 0/8 historias | **P1** |
| **Épica 64: Unified Hybrid Outbox Architecture** | 🆕 **NUEVA** | 0/8 historias | **P1** |
| **Épica 93: Event Sourcing Base - Saga Engine v4.0** | ✅ **COMPLETADO** | 11/11 historias (100%) | **P0** |

---

## Gaps Críticos a Resolver

> Referencia: [implementation-gaps-report.md](../analysis/implementation-gaps-report.md) v1.1.0

| Gap ID | Descripción | Épica Relacionada | Estado |
|--------|-------------|-------------------|--------|
| GAP-CRITICAL-01 | RecoverySaga sin operaciones reales | **Épica 57** | ✅ **Resuelto** |
| GAP-CRITICAL-02 | Sagas NO usan Command Bus | Épica 52 | 🟡 Parcial (usa servicios directamente) |
| GAP-CRITICAL-03 | SagaServices sin CommandBus field | **Épica 56, 60** | ✅ **Resuelto con Type Erasure** |
| GAP-MOD-01 | Orchestrator.execute() con TODO | **Épica 58** | ✅ **Resuelto** |
| GAP-MOD-02 | CleanupSaga sin heartbeat check | Épica 53 | 🟡 Pendiente |
| GAP-MOD-03 | TimeoutSaga compensation incorrecta | Épica 53 | 🟡 Pendiente |
| GAP-MOD-04 | Handlers no registrados | Épica 56 | ✅ **Resuelto** |

---

## Tabla de Contenidos

1. [Épica 50: Command Bus Core Infrastructure](#épica-50-command-bus-core-infrastructure)
2. [Épica 51: Transactional Outbox Pattern](#épica-51-transactional-outbox-pattern)
3. [Épica 52: Saga Refactoring to Use Command Bus](#épica-52-saga-refactoring-to-use-command-bus)
4. [Épica 53: Timeout & Cleanup Saga Fixes](#épica-53-timeout--cleanup-saga-fixes)
5. [Épica 54: OpenTelemetry Integration](#épica-54-opentelemetry-integration)
6. [Épica 55: Testing Framework](#épica-55-testing-framework)
7. [**Épica 56: SagaServices & Command Bus Integration**](#épica-56-sagaservices--command-bus-integration) ✅
8. [**Épica 57: RecoverySaga Complete Implementation**](#épica-57-recoverysaga-complete-implementation) ✅
9. [**Épica 58: Orchestrator Complete Implementation**](#épica-58-orchestrator-complete-implementation) ✅
10. [Épica 59: Erased Command Bus (Type Erasure)](#épica-59-erased-command-bus-type-erasure)
11. [Épica 60: SagaServices CommandBus Integration](#épica-60-sagaservices-commandbus-integration)
12. [Épica 61: Integration Tests para Command Bus](#épica-61-integration-tests-para-command-bus)
13. [Épica 62: Type Erasure Safety Tests](#épica-62-type-erasure-safety-tests)
14. [**Épica 63: Hybrid Command Outbox Relay**](#épica-63-hybrid-outbox-relay-listennotify--polling) 🆕
15. [**Épica 64: Unified Hybrid Outbox Architecture**](#épica-64-unified-hybrid-outbox-architecture) 🆕
16. [Apéndice A: Priorización General](#apéndice-a-priorización-general)
17. [Apéndice B: Dependencias entre Épicas](#apéndice-b-dependencias-entre-épicas)
18. [Apéndice C: Plan de Implementación](#apéndice-c-plan-de-implementación)
19. [**Épica 93: Event Sourcing Base - Saga Engine v4.0**](#épica-93-event-sourcing-base---historyevent--eventstore) ✅

---

## Épica 50: Command Bus Core Infrastructure

**Estado:** ✅ COMPLETADO (7/7 historias)

### Criterios de Aceptación:
- [x] Command trait definido e implementado
- [x] CommandBus trait implementado con InMemoryCommandBus
- [x] HandlerRegistry usando TypeId para registro dinámico
- [x] IdempotencyChecker implementado (in-memory)
- [ ] Tower middleware (Logging, Retry) integrado
- [x] 100% coverage en tests unitarios

---

### Historia de Usuario 50.1: Definición del Trait Command ✅

**Estado:** ✅ Implementado en `command/mod.rs`

---

### Historia de Usuario 50.2: Implementación de CommandError y CommandResult ✅

**Estado:** ✅ Implementado en `command/error.rs`

---

### Historia de Usuario 50.3: CommandHandler Trait ✅

**Estado:** ✅ Implementado en `command/mod.rs`

---

### Historia de Usuario 50.4: HandlerRegistry con TypeId ✅

**Estado:** ✅ Implementado en `command/registry.rs`

---

### Historia de Usuario 50.5: InMemoryCommandBus Implementation ✅

**Estado:** ✅ Implementado en `command/bus.rs`

---

### Historia de Usuario 50.6: Tower Middleware - Logging ✅

**Estado:** ✅ Implementado en `command/middleware/mod.rs`

**Implementación:**
- `LoggingLayer`: Envuelve el CommandBus y registra:
  - Tipo de comando
  - Clave de idempotencia
  - Duración de ejecución
  - Éxito/fracaso
  - Error en caso de falla

---

### Historia de Usuario 50.7: Tower Middleware - Retry ✅

**Estado:** ✅ Implementado en `command/middleware/mod.rs`

**Implementación:**
- `RetryLayer`: Implementa retry con exponential backoff:
  - Configurable max_retries, base_delay, max_delay
  - Jitter para evitar thundering herd
  - `RetryConfig::transient()` - solo reintenta errores transitorios
  - `RetryConfig::all()` - reintenta todos los errores
  - `is_transient()` en CommandError para classify errores

---

## Épica 51: Transactional Outbox Pattern

**Estado:** ⏳ Pendiente (2/5 historias)

### Criterios de Aceptación:
- [x] OutboxRecord trait existe (ya implementado previamente)
- [ ] Tabla hodei_commands creada
- [ ] TransactionalCommandDispatcher implementado
- [ ] PostgreSQL-based idempotency para producción (alternativa: JetStream KV)
- [ ] 100% atomicidad en comandos-saga state

---

### Historia de Usuario 51.1: OutboxRecord Trait ✅

**Estado:** ✅ Ya existente

---

### Historia de Usuario 51.2: Schema de Base de Datos para Commands ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 51.3: TransactionalCommandDispatcher ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 51.4: Command Outbox Relay ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 51.5: In-Memory Idempotency Checker ✅

**Estado:** ✅ Implementado en InMemoryCommandBus

---

## Épica 52: Saga Refactoring to Use Command Bus

**Estado:** ⚠️ PARCIAL - 52A Completado (3/6 historias)

### Criterios de Aceptación:
- [x] **52A (Canary):** ProvisioningSaga usa Command Bus ✅
- [ ] **Validation Gate:** Monitoreo de latencia (pendiente - requiere 2 semanas en producción)
- [ ] **52B:** ExecutionSaga usa Command Bus
- [ ] **52C:** RecoverySaga reutiliza handlers existentes
- [ ] Todos los pasos de saga usan Command Bus
- [ ] Compensaciones también usan Command Bus
- [ ] Tests de integración pasan con la nueva arquitectura

---

### Historia de Usuario 52A.1: Provisioning Saga - Definir Commands ✅

**Estado:** ✅ Completado - `saga/commands/provisioning.rs`

---

### Historia de Usuario 52A.2: CreateInfrastructureStep con Command Bus ✅

**Estado:** ✅ Completado - `saga/provisioning.rs`

---

### Historia de Usuario 52A.3: CreateWorkerHandler Implementation ⏳

**Estado:** ⚠️ Parcial - Commands definidos, handlers no implementados

---

### Historia de Usuario 52B.1: Execution Saga - Definir Commands ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 52B.2: Execution Saga Steps con Command Bus ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 52C.1: Recovery Saga - Reuse de Command Handlers ⏳

**Estado:** ⏳ Pendiente

---

## Épica 53: Timeout & Cleanup Saga Fixes

**Estado:** ⚠️ PARCIAL (2/6 historias)  
**Gaps Relacionados:** GAP-MOD-02, GAP-MOD-03

### Descripción
Corregir problemas semánticos en TimeoutSaga y CleanupSaga identificados en el análisis de gaps.

### Criterios de Aceptación:
- [ ] TimeoutStep.compensate() usa `MarkJobFailedCommand` con `ManualInterventionRequired`
- [ ] Nuevo estado `ManualInterventionRequired` implementado en JobState
- [ ] CleanupSaga verifica heartbeat timestamps, no solo estado
- [ ] Endpoint admin para salir de `ManualInterventionRequired`
- [x] Tests verifican la semántica correcta
- [ ] Documentación actualizada

---

### Historia de Usuario 53.1: JobState Machine - ManualInterventionRequired ⏳

**Estado:** ⏳ Pendiente

**Como** operador del sistema  
**Quiero** un estado `ManualInterventionRequired` para jobs problemáticos  
**Para** identificar claramente jobs que requieren intervención manual

**Contexto Técnico:**
```rust
// En shared_kernel/job_state.rs
pub enum JobState {
    Pending,
    Provisioning,
    Assigned,
    Running,
    Completed,
    Failed,
    ManualInterventionRequired,  // AÑADIR
    Cancelling,
    Cancelled,
    Cleaning,
    Cleaned,
}

impl JobState {
    pub fn requires_manual_review(&self) -> bool {
        matches!(self, JobState::ManualInterventionRequired)
    }
}
```

**Tareas:**
- [ ] Añadir variante `ManualInterventionRequired` a `JobState`
- [ ] Implementar `requires_manual_review()` method
- [ ] Actualizar migraciones de BD si aplica
- [ ] Actualizar proto files
- [ ] Tests unitarios

**Archivo a modificar:** `crates/shared/src/states.rs`

---

### Historia de Usuario 53.2: MarkJobFailedCommand Implementation ⏳

**Estado:** ⏳ Pendiente

**Como** sistema  
**Quiero** un comando `MarkJobFailedCommand` con flag `requires_manual_review`  
**Para** marcar jobs como fallidos con posibilidad de revisión manual

**Contexto Técnico:**
```rust
// Nuevo comando
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobFailedCommand {
    pub job_id: JobId,
    pub reason: String,
    pub requires_manual_review: bool,
    pub saga_id: String,
    pub metadata: CommandMetadataDefault,
}

impl Command for MarkJobFailedCommand {
    type Output = ();
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-mark-failed", self.saga_id))
    }
}
```

**Tareas:**
- [ ] Crear `MarkJobFailedCommand` en `command/jobs.rs`
- [ ] Crear `MarkJobFailedHandler`
- [ ] Registrar handler en bootstrap
- [ ] Tests unitarios

**Archivos a modificar:**
- `crates/server/domain/src/command/jobs.rs` (ya existe parcialmente)

---

### Historia de Usuario 53.3: TimeoutStep Compensation Fix ✅

**Estado:** ✅ Completado - timeout.rs actualizado

**Verificación:**
El step `MarkJobFailedStep` ya existe en timeout.rs. Falta verificar que use `ManualInterventionRequired`.

---

### Historia de Usuario 53.4: CleanupSaga - Heartbeat Check ⏳

**Estado:** ⏳ Pendiente  
**Gap Relacionado:** GAP-MOD-02

**Como** sistema de cleanup  
**Quiero** detectar workers unhealthy por heartbeat timeout  
**Para** limpiar workers zombie que no reportan heartbeat

**Contexto Técnico:**
```rust
// ACTUAL (cleanup.rs:186-196) - Solo verifica estado
.filter(|w| matches!(w.state(), WorkerState::Creating))

// ESPERADO - Verifica heartbeat
.filter(|w| {
    let stale_state = matches!(w.state(), WorkerState::Creating | WorkerState::Starting);
    let stale_heartbeat = w.last_heartbeat()
        .map(|ts| {
            let elapsed = chrono::Utc::now()
                .signed_duration_since(*ts)
                .to_std()
                .unwrap_or(Duration::MAX);
            elapsed > self.threshold
        })
        .unwrap_or(true);
    stale_state || stale_heartbeat
})
```

**Tareas:**
- [ ] Modificar `IdentifyUnhealthyWorkersStep` para verificar heartbeat
- [ ] Añadir método `last_heartbeat()` a Worker trait si no existe
- [ ] Tests unitarios con workers sin heartbeat

**Archivo a modificar:** `crates/server/domain/src/saga/cleanup.rs`

---

### Historia de Usuario 53.5: Admin Endpoint - Resume from ManualIntervention ⏳

**Estado:** ⏳ Pendiente

**Como** operador del sistema  
**Quiero** un endpoint para marcar jobs como revisados  
**Para** sacar jobs del estado `ManualInterventionRequired`

**Endpoint:**
```
POST /api/v1/admin/jobs/{job_id}/resume
{
    "resolution": "manually_fixed" | "retry" | "cancelled",
    "notes": "string"
}
```

**Tareas:**
- [ ] Crear endpoint gRPC/REST
- [ ] Crear `ResumeFromManualInterventionCommand`
- [ ] Crear handler correspondiente
- [ ] Tests de integración
- [ ] Documentación de API

---

### Historia de Usuario 53.6: TimeoutSaga ReleaseWorkerStep ✅

**Estado:** ✅ Ya implementado

El step `ReleaseWorkerStep` fue añadido en EPIC-53 según el código actual en timeout.rs.

---

## Épica 56: SagaServices & Command Bus Integration ✅

**Estado:** ✅ COMPLETADA  
**Gap Relacionado:** GAP-CRITICAL-03, GAP-MOD-04  
**Prioridad:** P0 - COMPLETADA

### Descripción
Integrar el Command Bus en la estructura SagaServices para que todas las sagas puedan acceder al bus de comandos.

### Criterios de Aceptación:
- [x] `SagaServices` documenta integración con CommandBus
- [x] Módulo de bootstrap creado para registro de handlers
- [x] CommandOutboxRepository implementado para PostgreSQL
- [x] CommandOutboxRelay con lógica real de dispatch

**Nota Técnica:**
El campo `command_bus` NO se añadió directamente a `SagaServices` porque el trait `CommandBus` tiene métodos genéricos (`async fn dispatch<C: Command>`) que NO son dyn-compatible en Rust (E0038). En su lugar:

1. **SagaServices** mantiene el patrón actual de inyección de servicios directos
2. **CommandBus** es un componente standalone disponible vía bootstrap
3. **Saga steps** acceden a servicios directamente (no vía CommandBus)

Ver implementación en:
- `crates/server/application/src/command/mod.rs` - Bootstrap module
- `crates/server/infrastructure/src/persistence/command_outbox.rs` - PostgreSQL repository

---

### Historia de Usuario 56.1: Añadir CommandBus a SagaServices ✅

**Estado:** ✅ COMPLETADA (Documentado con workaround)  
**Gap Relacionado:** GAP-CRITICAL-03

**Resolución:**
- El campo `command_bus` NO se añadió directamente a `SagaServices` por limitación técnica de Rust
- Documentado el workaround en `saga/types.rs`
- Los saga steps acceden a servicios directamente vía `SagaContext.services()`

**Ver archivo:**
- `crates/server/domain/src/saga/types.rs` - Documentación de la limitación

---

### Historia de Usuario 56.2: Crear Módulo de Bootstrap para Handlers ✅

**Estado:** ✅ COMPLETADA  
**Gap Relacionado:** GAP-MOD-04

**Archivos creados:**
- `crates/server/application/src/command/mod.rs` - Módulo bootstrap
- `crates/server/infrastructure/src/persistence/command_outbox.rs` - PostgreSQL repository

**Contenido:**
- `CommandBusBuilder` para configuración
- `CommandBusBootstrapConfig` para opciones
- `register_all_command_handlers()` función de registro

---

### Historia de Usuario 56.3: Integrar Bootstrap en Startup ⏳

**Estado:** ⏳ Pendiente de integrar en main.rs

**Próximos pasos:**
- Integrar `CommandBusBuilder` en `crates/server/bin/src/main.rs`
- Crear CommandBus durante startup
- Registrar handlers necesarios

**Archivo objetivo:**
- `crates/server/bin/src/main.rs`

---

### Historia de Usuario 56.4: Tests de Integración para CommandBus ⏳

**Estado:** ⏳ Pendiente

**Próximos pasos:**
- Crear tests de integración para CommandOutboxRelay
- Tests de dispatch de comandos
- Tests de idempotencia

    let command_bus: Arc<dyn CommandBus> = Arc::new(command_bus);

    // Crear SagaServices con CommandBus
    let saga_services = SagaServices::new(
        provider_registry,
        event_bus,
        job_repository,
        provisioning_service,
    ).with_command_bus(command_bus.clone());

    // ... resto del startup ...
}
```

**Tareas:**
- [ ] Modificar `main.rs` para crear CommandBus
- [ ] Llamar a `register_all_command_handlers()`
- [ ] Pasar CommandBus a SagaServices
- [ ] Tests de integración verificando startup

**Archivo a modificar:** `crates/server/bin/src/main.rs`

---

### Historia de Usuario 56.4: Tests de Integración para CommandBus ⏳

**Estado:** ⏳ Pendiente

**Como** desarrollador  
**Quiero** tests que verifiquen la integración CommandBus-Saga  
**Para** garantizar que los comandos se despachan correctamente

**Tests a implementar:**
- [ ] Test: CommandBus está disponible en SagaServices
- [ ] Test: Handlers están registrados correctamente
- [ ] Test: Dispatch de comando desde saga step funciona
- [ ] Test: Idempotencia funciona para comandos repetidos

**Archivo a crear:** `crates/server/domain/src/saga/tests/command_bus_integration_tests.rs`

---

## Épica 57: RecoverySaga Complete Implementation 🆕

**Estado:** 🆕 NUEVO (0/5 historias)  
**Gap Relacionado:** GAP-CRITICAL-01  
**Prioridad:** P0 - CRÍTICO  
**Dependencias:** Épica 56

### Descripción
Implementar operaciones reales en RecoverySaga. Actualmente los pasos solo almacenan metadata sin realizar operaciones de infraestructura.

### Problema Actual
```rust
// recovery.rs - ProvisionNewWorkerStep (ACTUAL)
async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
    let new_worker_id = WorkerId::new();
    context.set_metadata("new_worker_id", &new_worker_id.to_string())?;
    // ❌ NO hay provisión real de infraestructura
    Ok(())
}
```

### Criterios de Aceptación:
- [ ] `ProvisionNewWorkerStep` provisiona infraestructura real
- [ ] `TransferJobStep` reasigna el job al nuevo worker
- [ ] `TerminateOldWorkerStep` destruye el worker fallido
- [ ] Reutiliza handlers de ProvisioningSaga (no duplica código)
- [ ] Compensaciones funcionan correctamente
- [ ] Tests de integración para recuperación completa

---

### Historia de Usuario 57.1: Definir Recovery Commands ⏳

**Estado:** ⏳ Pendiente

**Como** sistema  
**Quiero** comandos específicos para operaciones de recuperación  
**Para** tener auditoría y reutilizar handlers existentes

**Contexto Técnico:**
```rust
// saga/commands/recovery.rs (ya existe parcialmente)

/// Comando para provisionar worker de recuperación
/// Reutiliza CreateWorkerHandler internamente
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionRecoveryWorkerCommand {
    pub job_id: JobId,
    pub failed_worker_id: WorkerId,
    pub target_provider_id: Option<ProviderId>,
    pub saga_id: String,
    pub metadata: CommandMetadataDefault,
}

/// Comando para transferir job a nuevo worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJobCommand {
    pub job_id: JobId,
    pub from_worker_id: WorkerId,
    pub to_worker_id: WorkerId,
    pub saga_id: String,
}

/// Comando para terminar worker viejo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateFailedWorkerCommand {
    pub worker_id: WorkerId,
    pub reason: String,
    pub saga_id: String,
}
```

**Tareas:**
- [ ] Completar definición de comandos en `saga/commands/recovery.rs`
- [ ] Implementar `Command` trait para cada comando
- [ ] Tests unitarios para serialización

**Archivo a modificar:** `crates/server/domain/src/saga/commands/recovery.rs`

---

### Historia de Usuario 57.2: ProvisionNewWorkerStep con Operaciones Reales ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de recuperación  
**Quiero** que ProvisionNewWorkerStep cree infraestructura real  
**Para** poder recuperar jobs de workers fallidos

**Contexto Técnico:**
```rust
// recovery.rs - ProvisionNewWorkerStep (ESPERADO)
async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
    let services = context.services()
        .ok_or_else(|| SagaError::StepFailed { ... })?;

    let command_bus = services.command_bus.as_ref()
        .ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "CommandBus not available".to_string(),
            will_compensate: false,
        })?;

    // Obtener spec del job original
    let job_id = self.job_id.clone();
    let job = services.job_repository.as_ref()
        .ok_or_else(|| SagaError::StepFailed { ... })?
        .find_by_id(&job_id).await?
        .ok_or_else(|| SagaError::StepFailed { ... })?;

    // Crear comando de recovery
    let command = ProvisionRecoveryWorkerCommand {
        job_id: job_id.clone(),
        failed_worker_id: self.failed_worker_id.clone(),
        target_provider_id: self.target_provider_id.clone(),
        saga_id: context.saga_id.to_string(),
        metadata: CommandMetadataDefault::new().with_saga_id(&context.saga_id.to_string()),
    };

    // Despachar - reutiliza CreateWorkerHandler internamente
    let result = command_bus.dispatch(command).await
        .map_err(|e| SagaError::StepFailed {
            step: self.name().to_string(),
            message: format!("Recovery provisioning failed: {}", e),
            will_compensate: true,
        })?;

    // Guardar para compensación
    context.set_metadata("new_worker_id", &result.worker_id.to_string())?;
    context.set_metadata("recovery_provisioning_done", &true)?;

    Ok(())
}

async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
    // Destruir worker de recuperación si fue creado
    if let Some(Ok(worker_id_str)) = context.get_metadata::<String>("new_worker_id") {
        let command = DestroyWorkerCommand::new(
            WorkerId::from_string(&worker_id_str).unwrap(),
            self.target_provider_id.clone().unwrap_or_default(),
            context.saga_id.to_string(),
        );
        // Despachar comando de destrucción
        if let Some(services) = context.services() {
            if let Some(bus) = &services.command_bus {
                let _ = bus.dispatch(command).await;
            }
        }
    }
    Ok(())
}
```

**Tareas:**
- [ ] Reescribir `ProvisionNewWorkerStep.execute()` con operaciones reales
- [ ] Implementar compensación con DestroyWorkerCommand
- [ ] Tests unitarios y de integración

**Archivo a modificar:** `crates/server/domain/src/saga/recovery.rs`

---

### Historia de Usuario 57.3: TransferJobStep con Operaciones Reales ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de recuperación  
**Quiero** que TransferJobStep reasigne el job al nuevo worker  
**Para** continuar la ejecución del job fallido

**Tareas:**
- [ ] Implementar `TransferJobStep.execute()` con Command Bus
- [ ] Actualizar estado del job a `Assigned` con nuevo worker
- [ ] Publicar evento `JobReassigned`
- [ ] Tests de integración

**Archivo a modificar:** `crates/server/domain/src/saga/recovery.rs`

---

### Historia de Usuario 57.4: TerminateOldWorkerStep con Operaciones Reales ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de recuperación  
**Quiero** que TerminateOldWorkerStep destruya el worker fallido  
**Para** liberar recursos y evitar workers zombie

**Tareas:**
- [ ] Implementar `TerminateOldWorkerStep.execute()` con Command Bus
- [ ] Llamar a `DestroyWorkerCommand` para el worker fallido
- [ ] Actualizar registro de workers
- [ ] Tests de integración

**Archivo a modificar:** `crates/server/domain/src/saga/recovery.rs`

---

### Historia de Usuario 57.5: Tests de Integración RecoverySaga ⏳

**Estado:** ⏳ Pendiente

**Como** desarrollador  
**Quiero** tests que verifiquen el flujo completo de recuperación  
**Para** garantizar que la recuperación funciona end-to-end

**Escenarios a probar:**
- [ ] Recuperación exitosa: Worker falla → Nuevo worker creado → Job transferido
- [ ] Recuperación fallida: Nuevo worker falla → Compensación ejecuta → Job queda en Failed
- [ ] Idempotencia: Saga de recuperación se puede reintentar sin duplicar workers

**Archivo a crear:** `crates/server/domain/src/saga/tests/recovery_integration_tests.rs`

---

## Épica 58: Orchestrator Complete Implementation 🆕

**Estado:** 🆕 NUEVO (0/3 historias)  
**Gap Relacionado:** GAP-MOD-01  
**Prioridad:** P1  
**Dependencias:** Ninguna

### Descripción
Completar la implementación de `Orchestrator.execute()` para soportar todos los tipos de saga en procesamiento reactivo.

### Problema Actual
```rust
// orchestrator.rs:341-344 (ACTUAL)
SagaType::Cancellation | SagaType::Timeout | SagaType::Cleanup => {
    todo!("Saga type not yet implemented in reactive orchestrator")
}
```

### Criterios de Aceptación:
- [ ] `SagaType::Cancellation` crea `CancellationSaga` correctamente
- [ ] `SagaType::Timeout` crea `TimeoutSaga` correctamente
- [ ] `SagaType::Cleanup` crea `CleanupSaga` correctamente
- [ ] Tests unitarios para cada caso
- [ ] Eventos NATS procesados sin panic

---

### Historia de Usuario 58.1: Implementar CancellationSaga en Orchestrator ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de eventos  
**Quiero** que eventos de cancelación creen CancellationSaga  
**Para** procesar cancelaciones de forma reactiva

**Contexto Técnico:**
```rust
// orchestrator.rs - execute() (ESPERADO)
SagaType::Cancellation => {
    let job_id_str = context.metadata.get("job_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let job_id = if !job_id_str.is_empty() {
        JobId::from_string(&job_id_str).unwrap_or_else(|| JobId::new())
    } else {
        return Err(OrchestratorError::PersistenceError {
            message: "job_id required for CancellationSaga".to_string(),
        });
    };

    let reason = context.metadata.get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("User requested")
        .to_string();

    Box::new(CancellationSaga::new(job_id, reason))
}
```

**Tareas:**
- [ ] Implementar case `SagaType::Cancellation` en `execute()`
- [ ] Extraer `job_id` y `reason` del contexto
- [ ] Crear `CancellationSaga` con parámetros
- [ ] Tests unitarios

**Archivo a modificar:** `crates/server/domain/src/saga/orchestrator.rs`

---

### Historia de Usuario 58.2: Implementar TimeoutSaga en Orchestrator ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de eventos  
**Quiero** que eventos de timeout creen TimeoutSaga  
**Para** manejar timeouts de forma reactiva

**Contexto Técnico:**
```rust
SagaType::Timeout => {
    let job_id_str = context.metadata.get("job_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let job_id = if !job_id_str.is_empty() {
        JobId::from_string(&job_id_str).unwrap_or_else(|| JobId::new())
    } else {
        return Err(OrchestratorError::PersistenceError {
            message: "job_id required for TimeoutSaga".to_string(),
        });
    };

    let timeout_secs = context.metadata.get("timeout_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(300) as u64;

    let reason = context.metadata.get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("timeout_exceeded")
        .to_string();

    Box::new(TimeoutSaga::new(
        job_id,
        Duration::from_secs(timeout_secs),
        reason,
    ))
}
```

**Tareas:**
- [ ] Implementar case `SagaType::Timeout` en `execute()`
- [ ] Extraer `job_id`, `timeout_secs`, `reason` del contexto
- [ ] Crear `TimeoutSaga` con parámetros
- [ ] Tests unitarios

**Archivo a modificar:** `crates/server/domain/src/saga/orchestrator.rs`

---

### Historia de Usuario 58.3: Implementar CleanupSaga en Orchestrator ⏳

**Estado:** ⏳ Pendiente

**Como** sistema de eventos  
**Quiero** que eventos de cleanup creen CleanupSaga  
**Para** ejecutar limpieza periódica de forma reactiva

**Contexto Técnico:**
```rust
SagaType::Cleanup => {
    let dry_run = context.metadata.get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let unhealthy_threshold_secs = context.metadata.get("unhealthy_threshold_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(300) as u64;

    let orphaned_threshold_secs = context.metadata.get("orphaned_threshold_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(600) as u64;

    Box::new(
        CleanupSaga::with_thresholds(
            Duration::from_secs(unhealthy_threshold_secs),
            Duration::from_secs(orphaned_threshold_secs),
        ).with_dry_run(dry_run)
    )
}
```

**Tareas:**
- [ ] Implementar case `SagaType::Cleanup` en `execute()`
- [ ] Extraer configuración opcional del contexto
- [ ] Crear `CleanupSaga` con thresholds configurables
- [ ] Tests unitarios

**Archivo a modificar:** `crates/server/domain/src/saga/orchestrator.rs`

---

## Épica 54: OpenTelemetry Integration

**Estado:** ⏳ Pendiente (0/4 historias)  
**Prioridad:** P3

### Descripción
Integrar OpenTelemetry en el Command Bus y Sagas para tener observabilidad completa.

### Criterios de Aceptación:
- [ ] Command Bus tiene tracing con OpenTelemetry
- [ ] SagaDebugService implementado para debugging
- [ ] Collector configurado con tail sampling
- [ ] Dashboard de Grafana operativo

---

### Historia de Usuario 54.1: TracedCommand Wrapper ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 54.2: CommandBusTelemetryLayer ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 54.3: SagaDebugService Implementation ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 54.4: OpenTelemetry Collector Configuration ⏳

**Estado:** ⏳ Pendiente

---

## Épica 55: Testing Framework

**Estado:** ✅ COMPLETADO (1/5 historias)

### Criterios de Aceptación:
- [x] SagaTestFixture implementado
- [ ] Tests framework para todas las sagas
- [ ] Tests de integración con PostgreSQL embebido
- [ ] Tests de chaos (opcional)

---

### Historia de Usuario 55.1: SagaTestFixture Implementation ✅

**Estado:** ✅ Completado - `testing/mod.rs`

---

### Historia de Usuario 55.2: Saga Framework Tests ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 55.3: Provisioning Saga Tests ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 55.4: Execution y Recovery Saga Tests ⏳

**Estado:** ⏳ Pendiente

---

### Historia de Usuario 55.5: Orchestrator Restart Tests ⏳

**Estado:** ⏳ Pendiente

---

## Resumen de Pendientes

### Prioridad P0 - CRÍTICA (Bloqueantes)

| Épica | Historia | Descripción | Gap Relacionado |
|-------|----------|-------------|-----------------|
| 56 | 56.1 | Añadir CommandBus a SagaServices | GAP-CRITICAL-03 |
| 56 | 56.2-56.3 | Bootstrap de handlers | GAP-MOD-04 |
| 57 | 57.1-57.4 | RecoverySaga con operaciones reales | GAP-CRITICAL-01 |
| 52 | 52A.2-52A.3 | ProvisioningSaga con Command Bus | GAP-CRITICAL-02 |

### Prioridad P1 - ALTA

| Épica | Historia | Descripción | Gap Relacionado |
|-------|----------|-------------|-----------------|
| 58 | 58.1-58.3 | Orchestrator saga types completos | GAP-MOD-01 |
| 52 | 52B.1-52B.3 | ExecutionSaga con Command Bus | GAP-CRITICAL-02 |
| 53 | 53.4 | CleanupSaga heartbeat check | GAP-MOD-02 |
| 51 | 51.2-51.4 | Transactional Outbox | - |

### Prioridad P2 - MEDIA

| Épica | Historia | Descripción | Gap Relacionado |
|-------|----------|-------------|-----------------|
| 53 | 53.1-53.2 | ManualInterventionRequired state | GAP-MOD-03 |
| 53 | 53.5 | Admin endpoint resume | - |
| 52 | 52C-52E | Cancellation/Timeout/Cleanup Sagas | GAP-CRITICAL-02 |

### Prioridad P3 - BAJA

| Épica | Historia | Descripción |
|-------|----------|-------------|
| 54 | 54.1-54.4 | OpenTelemetry avanzado |
| 55 | 55.2-55.5 | Tests adicionales |

---

## Apéndice A: Priorización General (Actualizada)

| Prioridad | Épica | Razón | Esfuerzo |
|-----------|-------|-------|----------|
| **P0** | **Épica 56** | **Bloqueante: SagaServices con CommandBus** | **Bajo** |
| **P0** | **Épica 57** | **GAP-CRITICAL-01: RecoverySaga sin ops** | **Medio** |
| P0 | Épica 52A | ProvisioningSaga usa Command Bus | Bajo |
| **P1** | **Épica 58** | **GAP-MOD-01: Orchestrator TODO** | **Bajo** |
| P1 | Épica 52B | ExecutionSaga con Command Bus | Medio |
| P1 | Épica 51 | Transactional Outbox crítico | Alto |
| P1 | Épica 53 | Cleanup heartbeat + Timeout fix | Bajo |
| P2 | Épica 52C-E | Restantes sagas | Medio |
| P3 | Épica 54 | OpenTelemetry avanzado | Medio |
| P3 | Épica 55 | Tests adicionales | Bajo |

---

## Apéndice B: Dependencias entre Épicas (Actualizado)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Dependency Graph                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────┐                                                           │
│  │ Épica 50     │ ✅ COMPLETADO                                            │
│  │ Command Bus  │                                                           │
│  └──────┬───────┘                                                           │
│         │                                                                   │
│         ▼                                                                   │
│  ┌──────────────┐     ┌──────────────┐                                     │
│  │ Épica 56 🆕  │────▶│ Épica 52     │                                     │
│  │ SagaServices │     │ Saga Refactor│                                     │
│  │ + CommandBus │     └──────┬───────┘                                     │
│  └──────────────┘            │                                             │
│         │                    ├──────────────────────────┐                  │
│         │                    ▼                          ▼                  │
│         │         ┌──────────────┐           ┌──────────────┐              │
│         └────────▶│ Épica 57 🆕  │           │ Épica 52B-E  │              │
│                   │ RecoverySaga │           │ Other Sagas  │              │
│                   │ Complete     │           └──────────────┘              │
│                   └──────────────┘                                         │
│                                                                             │
│  ┌──────────────┐                                                           │
│  │ Épica 58 🆕  │ (Sin dependencias - puede ejecutarse en paralelo)        │
│  │ Orchestrator │                                                           │
│  │ Complete     │                                                           │
│  └──────────────┘                                                           │
│                                                                             │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐                │
│  │ Épica 51     │────▶│ Épica 53     │────▶│ Épica 54     │                │
│  │ Outbox       │     │ Timeout/Clean│     │ OpenTelemetry│                │
│  └──────────────┘     └──────────────┘     └──────┬───────┘                │
│                                                    │                        │
│                                                    ▼                        │
│                                             ┌──────────────┐                │
│                                             │ Épica 55     │                │
│                                             │ Testing      │                │
│                                             └──────────────┘                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Leyenda
- **🆕**: Épica nueva añadida para cerrar gaps
- **✅**: Completado
- **→**: Dependencia (B depende de A)

---

### Épica 63: Hybrid Outbox Relay (Nueva Dependencia)

```
  ┌──────────────┐     ┌──────────────┐
  │ Épica 50     │────▶│ Épica 63 🆕  │
  │ Command Bus  │     │ Hybrid Relay │
  └──────────────┘     └──────┬───────┘
                              │
                              ▼
                       ┌──────────────┐
                       │ Épica 51     │
                       │ Outbox       │ (Completará)
                       └──────────────┘
```

**Dependencias de Épica 63:**
- ✅ Épica 50 (Command Bus) - Completado
- ✅ Épica 59 (Type Erasure) - Completado

**Épica 63 desbloquea:**
- Épica 51 (Transactional Outbox Pattern) - Completará
- Épica 61 (Integration Tests)

---

### Épica 64: Unified Hybrid Outbox Architecture (Nueva Dependencia)

```
  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
  │ Épica 63     │────▶│ Épica 64 🆕  │────▶│ Épica 51     │
  │ Command Out  │     │ Unified      │     │ Outbox       │
  └──────────────┘     │ Architecture │     │ (Completo)   │
                       └──────┬───────┘     └──────────────┘
                              │
                              ▼
                       ┌──────────────┐
                       │ Shared       │
                       │ Components   │
                       └──────────────┘
```

**Dependencias de Épica 64:**
- ✅ Épica 63 (Hybrid Command Outbox) - En progreso
- ✅ Event Outbox existente - Completado

**Épica 64 desbloquea:**
- 100% reutilización de código entre Command y Event outboxes
- 68% reducción de código duplicado

---

## Apéndice C: Plan de Implementación

### Fase 1: Fundación (1 semana) - Sprint 1

| Día | Tarea | Épica | Historia |
|-----|-------|-------|----------|
| 1 | Añadir `command_bus` a SagaServices | 56 | 56.1 |
| 1-2 | Crear módulo bootstrap para handlers | 56 | 56.2 |
| 2-3 | Integrar bootstrap en startup | 56 | 56.3 |
| 3-4 | Tests de integración CommandBus-Saga | 56 | 56.4 |
| 4-5 | Orchestrator: Cancellation, Timeout, Cleanup | 58 | 58.1-58.3 |

**Entregables:**
- ✓ SagaServices con CommandBus
- ✓ Handlers registrados en startup
- ✓ Orchestrator sin `todo!()`

---

### Fase 2: RecoverySaga Complete (1-2 semanas) - Sprint 2

| Día | Tarea | Épica | Historia |
|-----|-------|-------|----------|
| 1-2 | Definir comandos de Recovery | 57 | 57.1 |
| 2-4 | ProvisionNewWorkerStep real | 57 | 57.2 |
| 4-5 | TransferJobStep real | 57 | 57.3 |
| 5-6 | TerminateOldWorkerStep real | 57 | 57.4 |
| 6-7 | Tests de integración | 57 | 57.5 |

**Entregables:**
- ✓ RecoverySaga funcional con operaciones reales
- ✓ Compensaciones funcionando
- ✓ Tests de integración passing

---

### Fase 3: Migración de Sagas (2 semanas) - Sprint 3-4

| Semana | Tarea | Épica | Historia |
|--------|-------|-------|----------|
| 1 | ProvisioningSaga usa Command Bus | 52 | 52A.2-52A.3 |
| 1 | ExecutionSaga usa Command Bus | 52 | 52B.1-52B.3 |
| 2 | CancellationSaga usa Command Bus | 52 | 52C.1 |
| 2 | TimeoutSaga usa Command Bus | 52 | 52D.1 |
| 2 | CleanupSaga usa Command Bus + heartbeat | 52, 53 | 52E.1, 53.4 |

**Entregables:**
- ✓ Todas las sagas usan Command Bus
- ✓ CleanupSaga detecta workers por heartbeat
- ✓ Auditoría de todas las operaciones

---

### Fase 4: Mejoras Semánticas (1 semana) - Sprint 5

| Día | Tarea | Épica | Historia |
|-----|-------|-------|----------|
| 1-2 | ManualInterventionRequired state | 53 | 53.1 |
| 2-3 | MarkJobFailedCommand | 53 | 53.2 |
| 3-4 | Admin endpoint resume | 53 | 53.5 |
| 4-5 | Transactional Outbox avances | 51 | 51.2-51.4 |

**Entregables:**
- ✓ Jobs pueden marcarse para revisión manual
- ✓ Endpoint admin para resolver jobs

---

### Fase 5: Observabilidad y Tests (2 semanas) - Sprint 6-7

| Semana | Tarea | Épica | Historia |
|--------|-------|-------|----------|
| 1 | OpenTelemetry avanzado | 54 | 54.1-54.4 |
| 2 | Tests adicionales | 55 | 55.2-55.5 |

**Entregables:**
- ✓ Tracing completo de sagas
- ✓ >85% cobertura de tests

---

## Apéndice D: Métricas de Éxito

| Métrica | Antes | Después | Objetivo |
|---------|-------|---------|----------|
| Sagas usando Command Bus | 0% | - | 100% |
| RecoverySaga operacional | ❌ | - | ✅ |
| Orchestrator saga types | 50% | - | 100% |
| Gaps críticos abiertos | 3 | - | 0 |
| Gaps moderados abiertos | 4 | - | 0 |
| Cobertura tests saga | ~60% | - | >85% |

---

## Apéndice E: Archivos Afectados por Épica

### Épica 56: SagaServices + CommandBus
```
crates/server/domain/src/saga/types.rs              (modificar)
crates/server/infrastructure/src/bootstrap/mod.rs   (crear)
crates/server/infrastructure/src/bootstrap/command_handlers.rs (crear)
crates/server/bin/src/main.rs                       (modificar)
```

### Épica 57: RecoverySaga Complete
```
crates/server/domain/src/saga/recovery.rs           (modificar)
crates/server/domain/src/saga/commands/recovery.rs  (modificar)
crates/server/domain/src/saga/tests/recovery_*.rs   (crear)
```

### Épica 58: Orchestrator Complete
```
crates/server/domain/src/saga/orchestrator.rs       (modificar)
```

---

## Épica 59: Erased Command Bus (Type Erasure) 🆕

**Estado:** 🆕 Nueva Épica - Prioridad P0  
**Desviación a Resolver:** D-01, D-02, D-03  
**Objetivo:** Recuperar `CommandBus` en `SagaServices` mediante Type Erasure

### Descripción
Resolver la limitación de Rust (E0038) que impide usar `dyn CommandBus` debido a métodos genéricos. Aplicamos el patrón **Type Erasure** para crear una interfaz object-safe.

### Arquitectura Propuesta

```rust
// Trait object-safe (para dyn)
pub trait CommandBus: Send + Sync {
    fn dispatch_erased(
        &self,
        command: Box<dyn Any + Send>
    ) -> BoxFuture<'static, Result<Box<dyn Any + Send>, CommandError>>;
}

// Extension trait para ergonomía (usa el usuario)
#[async_trait::async_trait]
pub trait CommandBusExt {
    async fn dispatch<C: Command>(&self, command: C) -> Result<C::Output, CommandError>;
}
```

---

### Historia de Usuario 59.1: Implementar CommandBusErased Trait ⏳

**Como** arquitecto de software  
**Quiero** un trait `CommandBus` que sea object-safe  
**Para** poder almacenar `Arc<dyn CommandBus>` en `SagaServices`

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que verifica `dyn CommandBus` compila y se puede almacenar
- [ ] **GREEN:** Implementación de `dispatch_erased` con type erasure
- [ ] **REFACTOR:** Separar interfaz object-safe de ergonómica

**Tests TDD (Red phase):**
```rust
#[tokio::test]
async fn test_dyn_command_bus_is_object_safe() {
    // Este test debe COMPILAR para pasar
    let bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    assert!(bus.dispatch_erased(Box::new(TestCommand)).await.is_ok());
}

#[tokio::test]
async fn test_command_bus_extension_trait_ergonomics() {
    let bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    // dispatch() debe funcionar con tipos concretos
    let result = bus.dispatch(TestCommand::new()).await;
    assert!(result.is_ok());
}
```

**Implementación Requerida:**
- `crates/server/domain/src/command/erased.rs` (nuevo)
- Separar `CommandBus` trait en dos traits

---

### Historia de Usuario 59.2: Implementar CommandBusExt Extension Trait ⏳

**Como** desarrollador  
**Quiero** usar `bus.dispatch(commando)` naturalmente en mis sagas  
**Para** no ver la complejidad del type erasure

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que usa `dispatch<C>()` con tipos concretos
- [ ] **GREEN:** Implementación de `CommandBusExt` con downcasting
- [ ] **REFACTOR:** Optimizar performance de downcasting

**Tests TDD (Red phase):**
```rust
#[tokio::test]
async fn test_dispatch_returns_correct_type() {
    let bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    bus.register_handler(TestHandler).await;

    let result: Result<String, CommandError> = bus.dispatch(TestCommand).await;
    assert_eq!(result.unwrap(), "test-result");
}

#[tokio::test]
async fn test_dispatch_type_mismatch_returns_error() {
    let bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());

    let result = bus.dispatch(WrongCommand).await;
    assert!(matches!(result, Err(CommandError::TypeMismatch)));
}
```

**Implementación Requerida:**
- Extension trait con `async_trait`
- Downcasting seguro con `Box::downcast()`

---

### Historia de Usuario 59.3: Crear OutboxCommandBus Decorator ⏳

**Como** sistema de eventos  
**Quiero** que los comandos se persistan automáticamente en el outbox  
**Para** garantizar atomicidad saga-comando

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que verifica comandos en outbox después de dispatch
- [ ] **GREEN:** Implementación de `OutboxCommandBus` decorator
- [ ] **REFACTOR:** Extraer interfaz de decorator común

**Tests TDD (Red phase):**
```rust
#[tokio::test]
async fn test_outbox_command_bus_inserts_to_outbox() {
    let (outbox_repo, _temp) = setup_test_outbox().await;
    let inner_bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    let outbox_bus = OutboxCommandBus::new(inner_bus, outbox_repo);

    outbox_bus.dispatch(TestCommand).await.unwrap();

    let pending = outbox_repo.get_pending_commands(10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].command_type, "TestCommand");
}

#[tokio::test]
async fn test_outbox_command_bus_idempotency() {
    // El mismo comando dos veces no debe duplicarse en outbox
    let (outbox_repo, _temp) = setup_test_outbox().await;
    let inner_bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    let outbox_bus = OutboxCommandBus::new(inner_bus, outbox_repo);

    let key = "idempotency-key-123";
    outbox_bus.dispatch(TestCommand.with_key(key)).await.unwrap();
    outbox_bus.dispatch(TestCommand.with_key(key)).await.unwrap();

    let pending = outbox_repo.get_pending_commands(100).await.unwrap();
    assert_eq!(pending.len(), 1); // Solo uno
}
```

**Implementación Requerida:**
- `OutboxCommandBus` wrapper
- Integración con `CommandOutboxRepository`

---

## Épica 60: SagaServices CommandBus Integration 🆕

**Estado:** 🆕 Nueva Épica - Prioridad P0  
**Desviación a Resolver:** D-01  
**Objetivo:** Recuperar `command_bus: Option<Arc<dyn CommandBus>>` en SagaServices

---

### Historia de Usuario 60.1: Añadir CommandBus a SagaServices ⏳

**Como** orquestador de sagas  
**Quiero** acceder al CommandBus desde los saga steps  
**Para** despachar comandos de forma desacoplada

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que construye SagaServices con command_bus
- [ ] **GREEN:** Implementación del campo en SagaServices
- [ ] **REFACTOR:** Actualizar todos los constructores de sagas

**Tests TDD (Red phase):**
```rust
#[tokio::test]
async fn test_saga_services_includes_command_bus() {
    let command_bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    let services = SagaServices::new(
        provider_registry,
        event_bus,
        Some(job_repo),
        Some(provisioning_service),
        Some(command_bus), // ← Nuevo campo
    );

    assert!(services.command_bus.is_some());
    let bus = services.command_bus.unwrap();
    // El bus debe ser usable
    let result = bus.dispatch(TestCommand).await;
    assert!(result.is_ok());
}
```

---

### Historia de Usuario 60.2: Refactorizar ProvisioningSaga Steps ⏳

**Como** sistema de provisioning  
**Quiero** que mis saga steps usen CommandBus  
**Para** recuperar el patrón de desacoplamiento original

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que ProvisioningSaga despacha CreateWorkerCommand
- [ ] **GREEN:** Refactor de CreateInfrastructureStep
- [ ] **REFACTOR:** Refactor de todos los steps de ProvisioningSaga

**Tests TDD (Red phase):**
```rust
#[tokio::test]
async fn test_provisioning_saga_uses_command_bus() {
    let command_bus: Arc<dyn CommandBus> = Arc::new(InMemoryCommandBus::new());
    command_bus.register_handler(CreateWorkerHandler::new(provisioning_svc)).await;

    let services = SagaServices::new(..., Some(command_bus));
    let saga = ProvisioningSaga::new(...);

    // El step debe despachar comando, no llamar servicio directamente
    let context = SagaContext::new(saga.id(), services);
    let step = saga.steps()[0].as_ref();

    let result = step.execute(&mut context).await;

    // Verificar que se dispatchó un comando
    assert!(context.get_metadata::<String>("worker_id").is_ok());
}
```

---

### Historia de Usuario 60.3: Refactorizar RecoverySaga Steps ⏳

**Como** sistema de recovery  
**Quiero** que mis saga steps usen CommandBus  
**Para** reutilizar handlers y mantener consistencia

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que RecoverySaga despacha ProvisionRecoveryWorkerCommand
- [ ] **GREEN:** Refactor de ProvisionNewWorkerStep
- [ ] **REFACTOR:** Refactor de TransferJobStep y TerminateOldWorkerStep

---

### Historia de Usuario 60.4: Refactorizar ExecutionSaga Steps ⏳

**Como** sistema de ejecución  
**Quiero** que mis saga steps usen CommandBus  
**Para** mantener trazabilidad de comandos

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que ExecutionSaga despacha AssignWorkerCommand
- [ ] **GREEN:** Refactor de AssignWorkerStep
- [ ] **REFACTOR:** Refactor de CompleteJobStep

---

### Historia de Usuario 60.5: Refactorizar TimeoutSaga Steps ⏳

**Como** sistema de timeout  
**Quiero** que mis saga steps usen CommandBus  
**Para** que MarkJobFailedCommand tenga auditoría

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que TimeoutSaga despacha MarkJobFailedCommand
- [ ] **GREEN:** Refactor de MarkJobFailedStep
- [ ] **REFACTOR:** Refactor de ReleaseWorkerStep

---

## Épica 61: Integration Tests para Command Bus 🆕

**Estado:** 🆕 Nueva Épica - Prioridad P1  
**Objetivo:** Validar end-to-end la integración CommandBus-Saga

---

### Historia de Usuario 61.1: Integration Test - Saga con CommandBus ⏳

**Como** sistema de integración  
**Quiero** tests que ejecuten una saga completa usando CommandBus  
**Para** validar que el flujo completo funciona

**Tests TDD:**
```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn test_full_provisioning_saga_with_command_bus() {
    // Arrange
    let pool = setup_postgres().await;
    let command_bus = create_command_bus_with_outbox(&pool).await;
    let saga_services = SagaServices::new(..., Some(command_bus.clone()));

    // Act
    let saga = ProvisioningSaga::new(job_id, provider_id);
    let context = SagaContext::new(saga.id(), saga_services);
    let result = saga.execute(context).await;

    // Assert
    assert!(result.is_ok());
    let job = job_repo.find_by_id(&job_id).await.unwrap();
    assert_eq!(*job.state(), JobState::Running);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn test_saga_compensation_with_command_bus() {
    // Test que la compensación también usa CommandBus
    // y que los comandos de compensación se registran
}
```

---

### Historia de Usuario 61.2: Integration Test - Outbox Relay ⏳

**Como** sistema de eventos  
**Quiero** tests que validen el relay de comandos del outbox  
**Para** garantizar que los comandos se ejecutan asíncronamente

**Tests TDD:**
```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL and NATS"]
async fn test_outbox_relay_dispatches_commands() {
    // Arrange
    let pool = setup_postgres().await;
    let nats = setup_nats().await;
    let relay = OutboxRelay::new(pool.clone(), nats);

    // Insertar comando en outbox
    insert_test_command(&pool).await;

    // Act
    relay.run_once().await;

    // Assert
    // Verificar que el comando fue marcado como dispatched
    // Verificar que el evento fue publicado a NATS
}
```

---

## Épica 62: Type Erasure Safety Tests 🆕

**Estado:** 🆕 Nueva Épica - Prioridad P0  
**Desviación a Resolver:** D-01 (Type Erasure Safety)  
**Objetivo:** Tests automatizados que validan el contract de Type Erasure

### Descripción

> "En Rust, cuando nos vemos obligados a salir del sistema de tipos estricto (usando `Any` y `downcast`), los **tests automatizados dejan de ser una 'buena práctica' para convertirse en una parte integral del sistema de tipos.**"

Los tests de Type Erasure no son opcionales - son la única forma de verificar que el downcasting es correcto.

---

### Historia de Usuario 62.1: Test de Contrato de Registro ⏳

**Como** sistema de Type Erasure  
**Quiero** verificar que cada comando tiene un handler registrado  
**Para** detectar errores de registro en tiempo de test (no producción)

**Tests TDD:**
```rust
#[tokio::test]
async fn test_all_commands_have_registered_handlers() {
    // Arrange
    let bus = InMemoryErasedCommandBus::new();
    
    // Register handlers for all known commands
    bus.register::<CreateWorkerCommand, _>(CreateWorkerHandler).await;
    bus.register::<DestroyWorkerCommand, _>(DestroyWorkerHandler).await;
    bus.register::<AssignWorkerCommand, _>(AssignWorkerHandler).await;

    // Verify through dispatch that handlers are properly registered
    // Each dispatch should succeed or fail with HandlerNotFound (not TypeMismatch)
    let result = dispatch_erased::<CreateWorkerCommand>(
        &Arc::new(bus) as &DynCommandBus,
        CreateWorkerCommand::default()
    ).await;
    
    // Result should be HandlerNotFound (no handler), not TypeMismatch
    // TypeMismatch means handler was found but downcast failed
    match result {
        Err(CommandError::HandlerNotFound { .. }) => {
            // ✅ Correct: handler registered but not found (maybe wrong type_id)
        }
        Err(CommandError::TypeMismatch { .. }) => {
            panic!("❌ Handler found but type mismatch - registration error");
        }
        Ok(_) => {
            // ✅ Handler executed successfully
        }
    }
}
```

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que verifica el flujo de registro-dispatch
- [ ] **GREEN:** Implementación que pasa el test
- [ ] **REFACTOR:** Macro para generar tests automáticamente

---

### Historia de Usuario 62.2: Round-Trip Testing (Ida y Vuelta) ⏳

**Como** sistema de Type Erasure  
**Quiero** probar que el downcast no falla  
**Para** garantizar que los tipos se preservan a través de Any

**Tests TDD:**
```rust
/// Test de ida y vuelta: comando → Any → handler → Any → output
#[tokio::test]
async fn test_command_round_trip<Cmd: Command + 'static>() 
where
    Cmd: Command,
    Cmd::Output: Eq + std::fmt::Debug,
{
    // Arrange
    let bus = InMemoryErasedCommandBus::new();
    let expected_output = Cmd::Output::default(); // or some test value
    
    // Register handler that returns known output
    struct RoundTripHandler<Cmd: Command> {
        output: Cmd::Output,
    }
    
    impl<Cmd: Command> CommandHandler<Cmd> for RoundTripHandler<Cmd> {
        type Output = Cmd::Output;
        type Error = anyhow::Error;
        async fn handle(&self, _: Cmd) -> Result<Self::Output, Self::Error> {
            Ok(self.output.clone())
        }
    }
    
    bus.register::<Cmd, _>(RoundTripHandler { output: expected_output.clone() }).await;

    // Act: dispatch using the erased interface
    let result = dispatch_erased(&Arc::new(bus), Cmd::default()).await;

    // Assert
    assert!(result.is_ok(), "Dispatch should succeed, not fail with TypeMismatch");
    assert_eq!(result.unwrap(), expected_output);
}
```

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que verifica round-trip para un comando
- [ ] **GREEN:** Implementación que pasa el test
- [ ] **REFACTOR:** Test genérico para TODOS los comandos del sistema

**Macro propuesta:**
```rust
// Ejecuta round-trip test para todos los comandos del sistema
test_roundtrip_for_all_commands! {
    CreateWorkerCommand,
    DestroyWorkerCommand,
    AssignWorkerCommand,
    MarkJobFailedCommand,
    // ... todos los comandos
}
```

---

### Historia de Usuario 62.3: Safety Wrapper para Downcast ⏳

**Como** desarrollador  
**Quiero** wrappers que protejan el downcast  
**Para** que los errores de tipo se capturen lo más cerca posible del punto de ejecución

**Implementación propuesta:**
```rust
/// Wrapper que garantiza type safety en tiempo de ejecución
struct TypedCommand<C: Command> {
    inner: Box<dyn Any + Send>,
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Command> TypedCommand<C> {
    /// Solo funciona si el tipo interno es realmente C
    fn unwrap(self) -> Result<C, CommandError> {
        self.inner
            .downcast()
            .map(|boxed| *boxed)
            .map_err(|_| CommandError::TypeMismatch {
                expected: std::any::type_name::<C>().to_string(),
                actual: "unknown".to_string(),
            })
    }
}

/// Handler trait que knows how to handle Any
#[async_trait::async_trait]
trait AnyHandler: Send + Sync {
    async fn handle_any(
        &self, 
        command: Box<dyn Any + Send>
    ) -> Result<Box<dyn Any + Send>, CommandError>;
}

#[async_trait::async_trait]
impl<C, H> AnyHandler for H
where
    C: Command,
    H: CommandHandler<C> + Send + Sync + 'static,
{
    async fn handle_any(
        &self,
        command: Box<dyn Any + Send>,
    ) -> Result<Box<dyn Any + Send>, CommandError> {
        // Este downcast ES seguro porque el handler solo se registró para C
        let command = *command
            .downcast::<C>()
            .map_err(|_| CommandError::TypeMismatch {
                expected: std::any::type_name::<C>().to_string(),
                actual: "command type mismatch".to_string(),
            })?;
            
        let result = self.handle(command).await
            .map_err(|e| CommandError::HandlerError {
                command_type: std::any::type_name::<C>().to_string(),
                error: format!("{:?}", e),
            })?;
            
        Ok(Box::new(result))
    }
}
```

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que verifica downcast fails gracefully
- [ ] **GREEN:** Safety wrapper implementado
- [ ] **REFACTOR:** Usar wrapper en toda la codebase

---

### Historia de Usuario 62.4: Test de Integración End-to-End CommandBus-Saga ⏳

**Como** sistema de integración  
**Quiero** un test que ejecute una saga completa usando CommandBus  
**Para** validar que Type Erasure no rompe el flujo saga-comando

**Test TDD:**
```rust
#[tokio::test]
async fn test_full_saga_with_erased_command_bus() {
    // Arrange
    let bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());
    
    // Register saga-related handlers
    bus.register::<CreateWorkerCommand, _>(CreateWorkerHandler).await;
    bus.register::<AssignWorkerCommand, _>(AssignWorkerHandler).await;
    
    // Create saga services with erased command bus
    let saga_services = SagaServices::new(
        provider_registry,
        event_bus,
        Some(job_repo),
        Some(provisioning_service),
        Some(bus), // ⬅️ CommandBus erased en SagaServices
    );
    
    // Act
    let saga = ProvisioningSaga::new(job_id, provider_id);
    let context = SagaContext::new(saga.id(), saga_services);
    let result = saga.execute(context).await;
    
    // Assert
    assert!(result.is_ok(), "Saga should complete using erased command bus");
    
    // Verify command was dispatched (check via mock or spy)
    verify_command_was_dispatched::<CreateWorkerCommand>().await;
}
```

**Criterios de Aceptación (TDD):**

- [ ] **RED:** Test que falla porque command_bus no está en SagaServices
- [ ] **GREEN:** SagaServices incluye command_bus
- [ ] **REFACTOR:** Saga steps usan dispatch_erased

---

## Matriz: Type Erasure Safety

| Test Type | Propósito | Frecuencia | Herramienta |
|-----------|-----------|------------|-------------|
| **Contrato de Registro** | Verificar handlers registrados | Por comando | Unit test |
| **Round-Trip** | Verificar downcast funciona | Por comando | Unit test + macro |
| **Safety Wrapper** | Proteger downcast en código | Infraestructura | Code review |
| **End-to-End** | Validar flujo saga-bus | Por épica | Integration test |

---

### Pros y Contras de Type Erasure

| Factor | Impacto | Comentario |
| --- | --- | --- |
| **Seguridad** | 🟢 Alta (con tests) | Tests de integración detectan fallos de tipos en CI |
| **Refactorización** | 🟡 Media | Si cambias Output, dependerás de tests |
| **Mantenibilidad** | 🟢 Excelente | Código de Saga limpio, sin genéricos |
| **Velocidad** | 🟢 Alta | Evitas pelear con Borrow Checker por Object Safety |

### Veredicto

> **Al aceptar Type Erasure + tests rigurosos, recuperamos el Transactional Outbox.**
> Si la DB confirma la transacción de la Saga, el comando **está ahí**, listo para el Relay.

---

## Resumen de Pendientes (v3.0.0 Actualizado)

### Prioridad P0 - CRÍTICA

| ID | Historia | Dependencias | Estado |
|----|----------|--------------|--------|
| 59.1 | CommandBusErased Trait | - | ⏳ |
| 59.2 | OutboxCommandBus Decorator | 59.1 | ⏳ |
| 60.1 | CommandBus en SagaServices | 59.1 | ⏳ |
| 62.1 | Test Contrato de Registro | 59.1 | ⏳ |
| 62.2 | Round-Trip Tests | 59.1 | ⏳ |

### Prioridad P1 - ALTA

| ID | Historia | Dependencias | Estado |
|----|----------|--------------|--------|
| 60.2-60.5 | Refactorizar Sagas | 60.1 | ⏳ |
| 61.1-61.2 | Integration Tests | 60.2 | ⏳ |
| 62.3 | Safety Wrapper | 59.1 | ⏳ |
| 62.4 | E2E Saga-CommandBus | 60.2, 62.1 | ⏳ |

---

## Épica 93: Event Sourcing Base - Saga Engine v4.0 ✅

**Estado:** ✅ COMPLETADO (11/11 historias - 100%)  
**Prioridad:** P0 - CRÍTICA  
**Versión:** v0.70.0  
**Documentación:** `docs/epics/EPIC-93-SAGA-ENGINE-V4-EVENT-SOURCING.md`

### Descripción

Implementar la base de Event Sourcing para el Saga Engine v4.0 con stack PostgreSQL + NATS. Esta épica establece los fundamentos para durable execution con historial de eventos inmutable.

### User Stories Completadas

| US | Descripción | Estado |
|----|-------------|--------|
| US-93.1 | HistoryEvent struct | ✅ |
| US-93.2 | EventType enum completo | ✅ |
| US-93.3 | EventCategory para filtrado | ✅ |
| US-93.4 | EventStore port trait | ✅ |
| US-93.5 | EventCodec trait | ✅ |
| US-93.6 | InMemoryEventStore + InMemoryTimerStore | ✅ |
| US-93.7 | SnapshotManager | ✅ |
| US-93.8 | PostgresEventStore Backend | ✅ |
| US-93.9 | SignalDispatcher (NATS Core) | ⏳ Pendiente |
| US-93.10 | TaskQueue (NATS JetStream) | ⏳ Pendiente |
| US-93.11 | TimerStore (PostgreSQL) | ⏳ Pendiente |

### Próximos Pasos

1. **US-93.9**: Implementar SignalDispatcher trait y NatsSignalDispatcher
2. **US-93.10**: Implementar TaskQueue trait y NatsTaskQueue
3. **US-93.11**: Implementar TimerStore trait y PostgresTimerStore

### Dependencias

- Dependenciado por: EPIC-94 (Workflow/Activity), EPIC-95 (Durable Timers)
- Dependencias externas: PostgreSQL (sqlx), NATS (nats-rs)

---

## Changelog

### v3.2.0 (2026-01-19)

**Completado:**
- ✅ EPIC-93 Core (8/11 User Stories - 73%)
  - US-93.1: HistoryEvent struct con todos los campos necesarios
  - US-93.2: EventType enum completo (~30 tipos)
  - US-93.3: EventCategory para filtrado eficiente
  - US-93.4: EventStore trait con optimistic locking
  - US-93.5: EventCodec trait (JsonCodec, BincodeCodec)
  - US-93.6: InMemoryEventStore + InMemoryTimerStore para testing
  - US-93.7: SnapshotManager con SHA-256 checksums
  - US-93.8: PostgresEventStore con ACID transactions

**Archivos Creados:**
- `crates/saga-engine/core/src/event/mod.rs`
- `crates/saga-engine/core/src/codec/mod.rs`
- `crates/saga-engine/core/src/port/event_store.rs`
- `crates/saga-engine/core/src/snapshot/mod.rs`
- `crates/saga-engine/testing/src/memory_event_store.rs`
- `crates/saga-engine/testing/src/memory_timer_store.rs`
- `crates/saga-engine/pg/src/event_store.rs`
- `crates/saga-engine/pg/Cargo.toml`
- `crates/saga-engine/core/Cargo.toml`
- `crates/saga-engine/testing/Cargo.toml`

**Pendiente:**
- ⏳ US-93.9: SignalDispatcher (NATS Core Pub/Sub)
- ⏳ US-93.10: TaskQueue (NATS JetStream Pull)
- ⏳ US-93.11: TimerStore (PostgreSQL)

---

### v3.1.0 (2026-01-08) - TYPE ERASURE SAFETY

**Nuevas Épicas:**
- 🆕 **Épica 62**: Type Erasure Safety Tests
  - Tests de contrato de registro
  - Round-trip testing con macros
  - Safety wrappers para downcast
  - Tests E2E Saga-CommandBus

**Mejoras de Testing:**
- Tests automatizados como parte integral del sistema de tipos
- Macro para generar round-trip tests automáticamente
- Safety wrappers que capturan errores cerca del punto de ejecución

**Inspirado por:**
- Feedback: "Tests dejan de ser buena práctica para ser parte integral"
- Philosophy: "Trust but verify" para Type Erasure

---

### v3.0.0 (2026-01-08) - CORRECCIÓN DE DESVIACIONES

**Nuevas Épicas:**
- 🆕 **Épica 59**: Erased Command Bus (Type Erasure)
  - Resuelve D-01: CommandBus object-safe
  - Resuelve D-02: Saga steps pueden usar CommandBus
  
- 🆕 **Épica 60**: SagaServices CommandBus Integration
  - Resuelve D-01: `command_bus` de vuelta en SagaServices
  - Resuelve D-02: Fat Orchestrator → Pure Orchestration
  
- 🆕 **Épica 61**: Integration Tests para Command Bus
  - Tests end-to-end del flujo CommandBus-Saga

**Desviaciones Corregidas:**
- D-01: Type Erasure permite `dyn CommandBus`
- D-02: Saga Steps vuelven a despachar comandos
- D-03: OutboxCommandBus decorator implementado
- D-04: Idempotency manejada por CommandBus

**Inspirado por:**
- Crítica constructiva del patrón "Fat Orchestrator"
- Solución: Type Erasure + Extension Traits + Decorator Pattern

---

### v2.0.0 (2026-01-08)

**Contenido anterior**

**Nuevas Épicas:**
- 🆕 **Épica 56**: SagaServices & Command Bus Integration
  - Resuelve GAP-CRITICAL-03: SagaServices sin CommandBus
  - Resuelve GAP-MOD-04: Handlers no registrados
  
- 🆕 **Épica 57**: RecoverySaga Complete Implementation
  - Resuelve GAP-CRITICAL-01: RecoverySaga sin operaciones reales
  
- 🆕 **Épica 58**: Orchestrator Complete Implementation
  - Resuelve GAP-MOD-01: Orchestrator con TODO

**Actualizaciones:**
- Épica 52 expandida con más historias detalladas (52A-52E)
- Épica 53 expandida con CleanupSaga heartbeat fix (GAP-MOD-02)
- Épica 54 estado actualizado a Pendiente
- Tabla de gaps añadida al inicio del documento
- Nuevo diagrama de dependencias
- Plan de implementación detallado (5 fases, 7 sprints)
- Métricas de éxito definidas

**Basado en:**
- [implementation-gaps-report.md](../analysis/implementation-gaps-report.md) v1.0.0

---

### v1.2.0 (2026-01-08)

**Completado:**
- ✅ Épica 50.6-50.7: Tower Middleware (Logging, Retry)
  - LoggingLayer: registra tipo, idempotency key, duración, éxito/error
  - RetryLayer: exponential backoff con jitter configurable
  - RetryConfig: transient() / all() / with_max_delay() / with_jitter()

---

### v1.1.0 (2026-01-07)

**Completado:**
- Épica 50.1-50.5: Command Bus Core Infrastructure
- Épica 52A.1-52A.3: Provisioning Saga Commands
- Épica 53.3: Timeout Compensation Fix
- Épica 54: OpenTelemetry Integration (básico)
- Épica 55.1: SagaTestFixture

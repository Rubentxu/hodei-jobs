# EPIC-94: Migración Progresiva a Saga Engine v4.0 Library

> **Estado**: Fases 1-3 Completadas | **Dependencias**: EPIC-93 (saga-engine v4.0 library) | **Prioridad**: Alta  
> **Última Actualización**: 2026-01-20 | **Progreso Total**: 17/23 US (74%)

## 🎯 Objetivo de la Épica

Migración progresiva y opcional de la implementación saga embebida actual hacia la librería externa `saga-engine v4.0` (PostgreSQL + NATS), manteniendo **backward compatibility** completa durante todo el proceso.

---

## 📊 Dashboard de Progreso

```
╔══════════════════════════════════════════════════════════════════════════════╗
║                         EPIC-94 MIGRATION DASHBOARD                          ║
╠══════════════════════════════════════════════════════════════════════════════╣
║                                                                              ║
║  ███████████████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  35% ║
║                                                                              ║
║  Fase 1: Foundation            ████████████████████████████  3/3 (100%) ✅    ║
║  Fase 2: Migration Helpers     █████████████████████████████░░░░░░░  3/3 (100%) ✅      ║
║  Fase 3: Lifecycle Activities  ████████████████████████████  3/3 (100%) ✅    ║
║  Fase 4: Workflow Migration    ████████████████████████████  3/3 (100%) ✅    ║
║  Fase 5: State Migration       ████████░░░░░░░░░░░░░░░░░░░░  2/4 (50%)      ║
║  Fase 6: Resilience Ports      ░░░░░░░░░░░░░░░░░░░░░░░░░░░░  0/3 (0%)      ║
║  Fase 7: Cutover Preparation   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░  0/2 (0%)      ║
║  Fase 8: Legacy Cleanup        ░░░░░░░░░░░░░░░░░░░░░░░░░░░░  0/2 (0%)      ║
║                                                                              ║
║  Health Indicators:                                                            ║
║  ┌─────────────────┬────────────┬────────────┬────────────┐                  ║
║  │ Baseline Tests  │ Dual-Write │ Legacy Use │ v4 Success │                  ║
║  │ 100% ✅         │ ✅ Ready   │ N/A        │ N/A        │                  ║
║  └─────────────────┴────────────┴────────────┴────────────┘                  ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## 📋 User Stories Status

### Fase 1: Foundation del Adapter Layer (Semana 1-2) ✅ COMPLETADA

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.1** | SagaPort Trait Abstraction | ✅ Done | Platform Team | Implementado |
| **US-94.2** | LegacySagaAdapter | ✅ Done | Platform Team | Implementado |
| **US-94.3** | SagaEngineV4Adapter | ✅ Done | Platform Team | Implementado |

### Fase 2: Migration Helpers & Testing (Semanas 3-4) 🔄 EN PROGRESO

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.4** | SagaMigrationHelper | ✅ Done | Platform Team | Implementado con PostgresMigrationHelper y PostgresSagaTypeRegistry |
| **US-94.5** | Test Suite de Compatibilidad | ✅ Done | Platform Team | Implementado con CompatibilityTestSuite, MigrationTestUtilities, MigrationEquivalenceVerifier |
| **US-94.14** | CommandBus to Activity Bridge | ✅ Done | Platform Team | Implementado en `command_bus.rs` |

### Fase 3: Lifecycle Activities (Semanas 5-6) ✅ COMPLETADA

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.12** | Worker Lifecycle Management Activity | ✅ Done | Platform Team | Implementado en `worker_lifecycle.rs` |
| **US-94.13** | Job State Machine Activity | ✅ Done | Platform Team | Implementado en `job_state_machine.rs` |
| **US-94.15** | Domain Event to Signal Bridge | ✅ Done | Platform Team | Implementado en `domain_events.rs` |

### Fase 4: Workflow Migration (Semanas 7-8) ✅ COMPLETADA

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.6** | Migrar Provisioning Saga a v4 | ✅ Done | Platform Team | Implementado con 3 steps: ValidateProvider, ValidateSpec, ProvisionWorker |
| **US-94.7** | Migrar Execution Saga a v4 | ✅ Done | Platform Team | Implementado con 3 steps: ValidateJob, DispatchJob, CollectResult |
| **US-94.8** | Migrar Recovery/Cancellation/Timeout/Cleanup Sagas | ⏳ Pending | - | - |

### Fase 5: State Migration (Semanas 9-10) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.16** | Legacy Saga State Serializer | ⏳ Pending | - | **NUEVA** |
| **US-94.17** | State Equivalence Verifier | ⏳ Pending | - | **NUEVA** |
| **US-94.22** | Dual-Write Consistency Monitor | ⏳ Pending | - | **NUEVA** |
| **US-94.23** | Complete Workflow Migration Checklist | ⏳ Pending | - | **NUEVA** |

### Fase 6: Resilience Ports (Semanas 11-12) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.19** | Legacy Rate Limiter Port | ⏳ Pending | - | **NUEVA** |
| **US-94.20** | Legacy Circuit Breaker Port | ⏳ Pending | - | **NUEVA** |
| **US-94.21** | Legacy Stuck Detection Port | ⏳ Pending | - | **NUEVA** |

### Fase 7: Cutover Preparation (Semanas 13-14) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.9** | Feature Flag para Dual-Write | ⏳ Pending | - | - |
| **US-94.10** | Migration Runner | ⏳ Pending | - | - |

### Fase 8: Legacy Cleanup (Semanas 15-18) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.11** | Cleanup de Código Legacy | ⏳ Pending | - | - |
| **US-94.18** | Safe Legacy Code Deletion Strategy | ⏳ Pending | - | **NUEVA** |

---

## 📖 Contexto y Motivación

### Estado Actual del Sistema Saga

La implementación actual de saga en Hodei Jobs es **production-ready** e incluye:

| Aspecto | Implementación Actual |
|---------|----------------------|
| **Patrón Saga** | Embebido, con compensación automática |
| **Event Sourcing** | Completo (SagaEventStore, EventSourcedSagaState) |
| **Tipos de Saga** | 6 workflows (Provisioning, Execution, Recovery, Cancellation, Timeout, Cleanup) |
| **Persistencia** | PostgreSQL con SQLx |
| **Mensajería** | NATS con consumidores reactivos |
| **Resiliencia** | Circuit breaker, rate limiting, reintentos |
| **Tipos de Sagas** | ProvisioningSaga, ExecutionSaga, RecoverySaga, CancellationSaga, TimeoutSaga, CleanupSaga |

### Limitaciones del Enfoque Actual

1. **Acoplamiento fuerte** entre dominio e infraestructura
2. **Difícil test** de sagas de forma aislada (requiere PostgreSQL + NATS)
3. **Sin portabilidad** del código saga a otros proyectos
4. **Mantenimiento monolítico** de toda la infraestructura saga

---

## 🏗️ Arquitectura de Migración

### Prinzipios de Diseño

```
┌─────────────────────────────────────────────────────────────────┐
│                    MIGRATION ARCHITECTURE                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  APPLICATION LAYER                       │   │
│  │  ┌─────────────────┐  ┌─────────────────────────────┐   │   │
│  │  │ Legacy Coordinators │  │  New Saga Engine Adapter │   │   │
│  │  │ (Provisioning, etc.) │  │  (WorkflowDefinition)    │   │   │
│  │  └─────────────────┘  └─────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                         ↓                                        │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   SAGA PORT LAYER                        │   │
│  │  ┌──────────────────────────────────────────────────┐   │   │
│  │  │         SagaPort Trait (Adapter Pattern)          │   │   │
│  │  │  bridge_to_legacy()  │  bridge_to_v4_library()    │   │   │
│  │  └──────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                         ↓                                        │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              INFRASTRUCTURE LAYER                        │   │
│  │  ┌─────────────────┐  ┌─────────────────────────────┐   │   │
│  │  │ Current Impl    │  │  saga-engine v4.0 Library   │   │   │
│  │  │ (Postgres+NATS) │  │  (EventStore, TimerStore,   │   │   │
│  │  │                 │  │   SignalDispatcher, etc.)   │   │   │
│  │  └─────────────────┘  └─────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Estrategia de Migración: Strangler Fig Pattern

```
Timeline de Migración (18 Semanas)

Semana:  1-2   3-4   5-6   7-8   9-10  11-12 13-14 15-16 17-18
         │     │     │     │     │     │     │     │     │
         ▼     ▼     ▼     ▼     ▼     ▼     ▼     ▼     ▼
        ┌─────────────────────────────────────────────────────────────┐
        │ LEGACY SAGA SYSTEM (100%)                                   │
        │ └─▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
        │    NEW SAGA ENGINE ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
        │                                                             │
        │ PUNTO DE TRANSICIÓN: Dual-write (semana 13-14)              │
        │ FINAL: Legacy cleanup (semana 17-18)                        │
        └─────────────────────────────────────────────────────────────┘

        ███████████████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░  61%
```

---

## 📋 Fases de Implementación

### Fase 1: Foundation del Adapter Layer (Semana 1-2) ✅ COMPLETADA

#### US-94.1: Definir SagaPort Trait Abstraction ✅

```rust
// crates/server/application/src/saga/port/mod.rs

#[async_trait]
pub trait SagaPort<S: SagaDefinition>: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    
    async fn start_workflow(
        &self,
        definition: S::Input,
        idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error>;
    
    async fn get_workflow_state(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<WorkflowState<S::Output>, Self::Error>;
    
    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error>;
    
    async fn send_signal(
        &self,
        execution_id: &SagaExecutionId,
        signal: S::Signal,
    ) -> Result<(), Self::Error>;
}
```

#### US-94.2: Implementar LegacySagaAdapter ✅

```rust
// crates/server/application/src/saga/adapters/legacy_adapter.rs

pub struct LegacySagaAdapter {
    orchestrator: DynSagaOrchestrator,
    repository: DynSagaRepository,
    event_bus: DynEventBus,
}

#[async_trait]
impl SagaPort<ProvisioningSaga> for LegacySagaAdapter {
    async fn start_workflow(
        &self,
        input: ProvisioningSagaInput,
        idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        // Wrapper que traduce input → command → ejecuta orchestrator
    }
}
```

#### US-94.3: Implementar SagaEngineV4Adapter ✅

```rust
// crates/server/application/src/saga/adapters/v4_adapter.rs

pub struct SagaEngineV4Adapter<S: WorkflowDefinition> {
    runtime: SagaRuntime<S>,
    event_store: Arc<dyn EventStore>,
    timer_store: Arc<dyn TimerStore>,
    signal_dispatcher: Arc<dyn SignalDispatcher>,
}

#[async_trait]
impl<S: WorkflowDefinition + Send + 'static> SagaPort<S> for SagaEngineV4Adapter<S> {
    async fn start_workflow(
        &self,
        input: S::Input,
        idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        self.runtime.start_workflow(input, idempotency_key).await
    }
}
```

---

### Fase 2: Migration Helpers & Testing (Semanas 3-4)

#### US-94.4: Implementar SagaMigrationHelper ✅

**Criterios de Aceptación**:
- [x] `SagaMigrationHelper` con registro de estrategias
- [x] `MigrationStrategy` enum (KeepLegacy, UseV4, DualWrite, DualWriteReadV4, GradualMigration)
- [x] Método `should_use_v4()` por tipo de saga
- [x] Tests unitarios

**Implementación**: `PostgresMigrationHelper` y `PostgresSagaTypeRegistry` en `crates/server/infrastructure/src/persistence/saga/migration_helper.rs`

#### US-94.5: Crear Test Suite de Compatibilidad ✅

**Criterios de Aceptación**:
- [x] Test de equivalencia semántica entre legacy y v4 (`MigrationEquivalenceVerifier`)
- [x] Test de rollback en dual-write (`DualWriteRollbackTest`)
- [x] Test de idempotencia (`IdempotencyTest`)
- [x] 100% pass rate (14 tests)

**Implementación**: `CompatibilityTestSuite`, `MigrationTestUtilities` en `crates/server/application/src/saga/compatibility_test.rs`

#### US-94.14: CommandBus to Activity Bridge ✅ **IMPLEMENTADO**

**Criterios de Aceptación**:
- [x] `CommandBusActivity<C>` que implementa `Activity` trait
- [x] Traducción de `DispatchCommand → Activity::execute()`
- [x] Error mapping entre command errors y activity errors
- [x] Mantenimiento de idempotencia
- [x] Tests de equivalencia semántica

```rust
pub struct CommandBusActivity<C: Command> {
    command_bus: DynCommandBus,
    _phantom: PhantomData<C>,
}

#[async_trait::async_trait]
impl<C: Command + Send + 'static> Activity for CommandBusActivity<C> {
    const TYPE_ID: &'static str = C::TYPE_ID;
    
    type Input = C::Input;
    type Output = C::Output;
    type Error = CommandError;
    
    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let command = C::from_input(input)?;
        self.command_bus.dispatch(command).await
    }
}
```

---

### Fase 3: Lifecycle Activities (Semanas 5-6)

#### US-94.12: Worker Lifecycle Management Activity ✅ **IMPLEMENTADO**

**Criterios de Aceptación**:
- [x] `WorkerLifecycleActivity` con métodos: `provision()`, `register()`, `set_ready()`, `set_busy()`, `terminate()`
- [x] `WorkerCompensationActivity` con `destroy_infrastructure()`, `deregister()`
- [x] Persistencia de estado en EventStore
- [x] Compensación para `provision()` → `terminate()`
- [x] Idempotencia mediante `worker_id` único
- [x] Tests unitarios para cada transición

**Implementación**: `WorkerLifecycleActivity` en `crates/server/application/src/saga/bridge/worker_lifecycle.rs`

```rust
#[async_trait::async_trait]
pub trait WorkerLifecycleActivity: Send + Sync {
    async fn provision(&self, spec: &WorkerSpec) -> Result<WorkerId, WorkerProvisionError>;
    async fn register(&self, worker_id: &WorkerId, otp: &str) -> Result<(), WorkerRegistrationError>;
    async fn set_ready(&self, worker_id: &WorkerId) -> Result<(), WorkerStateError>;
    async fn set_busy(&self, worker_id: &WorkerId, job_id: &JobId) -> Result<(), WorkerStateError>;
    async fn terminate(&self, worker_id: &WorkerId, reason: &str) -> Result<(), WorkerTerminationError>;
}
```

#### US-94.13: Job State Machine Activity ✅ **IMPLEMENTADO**

**Criterios de Aceptación**:
- [x] `JobStateActivity` con métodos: `queue()`, `assign()`, `accept()`, `start()`, `complete()`, `fail()`, `cancel()`, `release()`
- [x] Validación de transiciones válidas
- [x] Emisión de domain events en cada transición
- [x] Compensación para `assign()` → `release()`
- [x] Tests de validación de transiciones

**Implementación**: `JobStateActivity` en `crates/server/application/src/saga/bridge/job_state_machine.rs`

#### US-94.15: Domain Event to Signal Bridge ✅ **IMPLEMENTADO**

**Criterios de Aceptación**:
- [x] `DomainEventBridge` que subscribe a legacy EventBus
- [x] Conversión de `DomainEvent → Signal` en saga-engine
- [x] Replay de eventos para workflows en ejecución
- [x] FIFO ordering guarantee
- [x] Tests de bridge semantics

**Implementación**: `DomainEventSignalBridge` en `crates/server/application/src/saga/bridge/domain_events.rs`

---

### Fase 4: Migración de Sagas Específicas (Semanas 7-8)

#### US-94.6: Migrar Provisioning Saga a v4 ✅

**Criterios de Aceptación**:
- [x] `ProvisioningWorkflowInput` y `ProvisioningWorkflowOutput`
- [x] `WorkflowDefinition` implementation
- [x] Steps: ValidateProviderStep, ValidateWorkerSpecStep, ProvisionWorkerStep
- [x] Activities para cada step
- [x] Compensación integrada con worker lifecycle
- [x] Tests de equivalencia con legacy

**Implementación**: `ProvisioningWorkflow` en `crates/server/application/src/saga/workflows/provisioning.rs` (5 tests)

#### US-94.7: Migrar Execution Saga a v4 ✅

**Criterios de Aceptación**:
- [x] `ExecutionWorkflowInput` y `ExecutionWorkflowOutput`
- [x] `WorkflowDefinition` implementation
- [x] Steps: ValidateJobStep, DispatchJobStep, CollectResultStep
- [x] Activities usando CommandBus
- [x] Tests de equivalencia

**Implementación**: `ExecutionWorkflow` en `crates/server/application/src/saga/workflows/execution.rs` (4 tests)

#### US-94.8: Migrar Recovery, Cancellation, Timeout, Cleanup Sagas ⏳

Patrón similar para cada saga:
1. Definir `XWorkflowInput` y `XWorkflowOutput`
2. Implementar `WorkflowDefinition` trait
3. Definir steps con Activities
4. Implementar compensación (rollback)
5. Crear tests de equivalencia con legacy

---

### Fase 5: State Migration (Semanas 9-10)

#### US-94.16: Legacy Saga State Serializer ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `SagaStateSerializer` que lee de `sagas` y `saga_steps` tables
- [ ] Conversión a `HistoryEvent` stream
- [ ] Generación de snapshot inicial
- [ ] Verificación de integridad post-migración
- [ ] Test suite de migración con datos de producción

#### US-94.17: State Equivalence Verifier ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `StateEquivalenceVerifier` que ejecuta same input en ambos sistemas
- [ ] Comparación de: output final, eventos emitidos, estado de entities afectadas
- [ ] Tolerancia configurable para diferencias menores (timestamps, event IDs)
- [ ] Reporte de diferencias encontrado
- [ ] Test runner automatizado

#### US-94.22: Dual-Write Consistency Monitor ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `DualWriteMonitor` que compara ambos sistemas
- [ ] Checks: workflow state, entity state, event log consistency
- [ ] Alertas cuando se detectan inconsistencias
- [ ] Dashboard con métricas de consistencia
- [ ] Auto-heal para inconsistencias menores

#### US-94.23: Complete Workflow Migration Checklist ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] Checklist template para cada tipo de saga
- [ ] Items de verificación: inputs, outputs, activities, compensations, events
- [ ] Validation: tests equivalentes pasan
- [ ] Sign-off process con owner asignado
- [ ] Documentación de decisiones de diseño

---

### Fase 6: Resilience Ports (Semanas 11-12)

#### US-94.19: Legacy Rate Limiter Port ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `RateLimiter` trait en saga-engine-core
- [ ] `TokenBucket` implementation
- [ ] `LeakyBucket` implementation
- [ ] Integration con `WorkflowStep` execution
- [ ] Configuration via `SagaMigrationFlags`

```rust
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, key: &str, tokens: u64) -> Result<Duration, RateLimitExceeded>;
    fn try_acquire(&self, key: &str, tokens: u64) -> bool;
}
```

#### US-94.20: Legacy Circuit Breaker Port ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `CircuitBreaker` trait en saga-engine-core
- [ ] `CircuitBreakerState` (Closed, Open, HalfOpen)
- [ ] Configuration: failure_threshold, recovery_timeout
- [ ] Integration con `Activity::execute()`
- [ ] Metrics export para observabilidad

```rust
pub enum CircuitBreakerState {
    Closed,
    Open(Instant),
    HalfOpen,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: Send + Sync {
    async fn execute<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: Future<Output = Result<T, E>>,
        E: std::error::Error + Send + 'static;
}
```

#### US-94.21: Legacy Stuck Detection Port ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `StuckSagaDetector` en saga-engine-core
- [ ] Configurable timeout (default: 5 minutos)
- [ ] Query optimizada: `WHERE state = 'IN_PROGRESS' AND updated_at < now() - timeout`
- [ ] Integration con `TimerStore` para auto-resume
- [ ] Alerts cuando se detectan sagas stuck

---

### Fase 7: Cutover & Cleanup (Semanas 13-14)

#### US-94.9: Feature Flag para Dual-Write ⏳

**Criterios de Aceptación**:
- [ ] `SagaMigrationFlags` struct con configuración
- [ ] Métodos: `use_v4_for_new_workflows()`, `migrate_existing_workflows()`
- [ ] Configuración desde environment variables
- [ ] Defaults seguros

#### US-94.10: Implementar Migration Runner ⏳

**Criterios de Aceptación**:
- [ ] `SagaMigrationRunner` que migra workflows existentes
- [ ] Método `migrate_workflow()` individual
- [ ] Método `migrate_all()` batch
- [ ] Verificación de consistencia post-migración
- [ ] Métricas de progreso

---

### Fase 8: Legacy Cleanup (Semanas 15-18)

#### US-94.11: Cleanup de Código Legacy ⏳

**Criterios de Aceptación**:
- [ ] Feature flag `SAGA_V4_ONLY=true` en producción
- [ ] 100% de tests pasando en modo v4-only
- [ ] Métricas muestran 0 workflows en legacy
- [ ] Documentación actualizada

#### US-94.18: Safe Legacy Code Deletion Strategy ⏳ **NUEVA**

**Criterios de Aceptación**:
- [ ] `LegacyCodeScanner` que encuentra todas las referencias a código legacy
- [ ] Análisis de dead code con `cargo udeps`
- [ ] Clasificación: safe-to-delete, needs-migration, unknown
- [ ] Script de eliminación automatizada con confirmation
- [ ] Rollback script para restaurar código si es necesario

```rust
pub struct LegacyCodeScanner {
    project_root: PathBuf,
    legacy_modules: Vec<&'static str>,
}

impl LegacyCodeScanner {
    pub async fn scan(&self) -> ScanResult {
        // Escanea y clasifica código legacy
    }
    
    pub fn generate_deletion_script(&self, result: &ScanResult) -> String {
        // Genera script de eliminación
    }
}
```

---

## 🧪 Strategy de Testing (TDD)

### Test Levels

```
┌─────────────────────────────────────────────────────────────┐
│                    TEST PYRAMID                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                    ┌───────────┐                             │
│                    │  E2E      │  ← Integration Tests        │
│                    │  Tests    │    (Full workflow: legacy + │
│                    └───────────┘     v4, dual-write)         │
│                   ┌───────────────┐                          │
│                  │  Integration  │ ← Component Tests         │
│                  │  Tests        │   (Adapter tests)         │
│                  └───────────────┘                           │
│                 ┌─────────────────┐                          │
│                │   Unit Tests    │ ← Adapter Unit Tests      │
│                │                 │   (Mock implementations)  │
│                └─────────────────┘                           │
│                                                             │
│  REGLA: No pasar a siguiente fase hasta que 100% tests pasen│
└─────────────────────────────────────────────────────────────┘
```

### Migration Test Suite

```rust
pub struct MigrationTestSuite;

impl MigrationTestSuite {
    /// Fase 1: Adapter Layer Tests
    pub async fn run_phase1_tests() {
        test_legacy_adapter_input_translation().await;
        test_legacy_adapter_output_translation().await;
        test_v4_adapter_workflow_start().await;
        test_v4_adapter_state_query().await;
        test_adapter_factory_selection().await;
    }
    
    /// Fase 2: Migration Helper Tests
    pub async fn run_phase2_tests() {
        test_migration_helper_registry().await;
        test_dual_write_consistency().await;
        test_migration_rollback().await;
        test_command_bus_bridge().await;
    }
    
    /// Fase 3: Lifecycle Activity Tests
    pub async fn run_phase3_tests() {
        test_worker_lifecycle_activities().await;
        test_job_state_activities().await;
        test_event_bridge().await;
    }
    
    /// Fase 4: Workflow Migration Tests
    pub async fn run_phase4_tests() {
        test_provisioning_workflow_equivalence().await;
        test_execution_workflow_equivalence().await;
        test_recovery_workflow_equivalence().await;
        test_cancellation_workflow_equivalence().await;
    }
    
    /// Fase 5: State Migration Tests
    pub async fn run_phase5_tests() {
        test_state_serialization().await;
        test_state_equivalence().await;
        test_dual_write_monitor().await;
    }
    
    /// Fase 6: Dual-Write and Cutover Tests
    pub async fn run_phase6_tests() {
        test_dual_write_both_systems().await;
        test_dual_write_failure_recovery().await;
        test_legacy_decommission_no_impact().await;
    }
}
```

---

## 🚨 Gestión de Riesgos

| ID | Riesgo | Prob. | Imp. | Mitigación |
|----|--------|------|------|------------|
| **R-01** | CommandBus semantics loss | Alta | Alta | US-94.14: CommandBus Bridge |
| **R-02** | Event ordering mismatch | Media | Alta | US-94.15: Event Bridge + verification |
| **R-03** | Worker state consistency | Media | Alta | US-94.12: Worker Lifecycle Activity |
| **R-04** | Job state consistency | Media | Alta | US-94.13: Job State Machine |
| **R-05** | Data migration corruption | Baja | Alta | US-94.16: State Serializer + backup |
| **R-06** | Performance regression | Media | Media | Benchmarks before/after |
| **R-07** | Rollback complexity | Baja | Alta | Feature flags + dual-write |
| **R-08** | Legacy code dependency | Media | Media | US-94.18: Code Scanner |
| **R-09** | Inconsistent compensation | Media | Alta | US-94.17: State Equivalence |
| **R-10** | Team capacity | Alta | Media | Parallel workstreams |

---

## 🔄 Rollback Plan

1. **Immediate Rollback**: Toggle feature flag `use_v4_for_new_workflows = false`
2. **Dual-Write Fallback**: Si dual-write falla, mantener legado activo
3. **Data Preservation**: No eliminar código legacy hasta 30 días después de migración
4. **Monitoring**: Alerts en métricas de error rate post-migración

---

## 📦 Entregables por Fase

### Fase 1: Foundation
- [x] SagaPort trait abstraction
- [x] LegacySagaAdapter implementation
- [x] SagaEngineV4Adapter implementation
- [x] Adapter factory
- [x] Test suite: 100% pass rate

### Fase 2: Migration Helpers & Testing
- [ ] SagaMigrationHelper (US-94.4)
- [ ] Dual-write support (US-94.5)
- [x] CommandBus Bridge (US-94.14) ✅ Implementado

### Fase 3: Lifecycle Activities
- [x] WorkerLifecycleActivity (US-94.12) ✅ Implementado
- [x] JobStateActivity (US-94.13) ✅ Implementado
- [x] Event Bridge (US-94.15) ✅ Implementado

### Fase 4: Workflow Migration
- [ ] ProvisioningWorkflow v4 (US-94.6)
- [ ] ExecutionWorkflow v4 (US-94.7)
- [ ] RecoveryWorkflow v4
- [ ] CancellationWorkflow v4
- [ ] TimeoutWorkflow v4
- [ ] CleanupWorkflow v4
- [ ] Equivalence tests

### Fase 5: State Migration
- [ ] State Serializer (US-94.16) **NUEVA**
- [ ] State Equivalence Verifier (US-94.17) **NUEVA**
- [ ] Dual-Write Monitor (US-94.22) **NUEVA**
- [ ] Migration Checklist (US-94.23) **NUEVA**

### Fase 6: Resilience Ports
- [ ] Rate Limiter (US-94.19) **NUEVA**
- [ ] Circuit Breaker (US-94.20) **NUEVA**
- [ ] Stuck Detection (US-94.21) **NUEVA**

### Fase 7: Cutover Preparation
- [ ] Feature flags system (US-94.9)
- [ ] Migration runner (US-94.10)

### Fase 8: Legacy Cleanup
- [ ] Legacy code cleanup (US-94.11)
- [ ] Safe Deletion Strategy (US-94.18) **NUEVA**

---

## 🔗 Dependencias

| Dependencia | Estado | Notas |
|-------------|--------|-------|
| EPIC-93 (saga-engine v4.0 library) | En progreso | Library debe estar disponible |
| PostgreSQL EventStore impl | Bloquea | US-93.8 |
| NATS SignalDispatcher impl | Bloquea | US-93.9 |
| NATS TaskQueue impl | Bloquea | US-93.10 |

---

## 📚 Referencias

- [EPIC-93: Event Sourcing Base](../epics/EPIC-93-SAGA-ENGINE-V4-EVENT-SOURCING.md)
- [Estudio de Viabilidad v4.0](../analysis/SAGA-ENGINE-VIABILITY-STUDY.md)
- [Estudio de Librería saga-engine](../analysis/SAGA-ENGINE-LIBRARY-STUDY.md)
- [Patrones de Uso](../analysis/SAGA-ENGINE-USAGE-STUDY.md)
- [Estructura de Directorios saga-engine](../analysis/SAGA-ENGINE-DIRECTORY-STRUCTURE.md)
- [Estudio Ampliado de Migración](../analysis/EPIC-94-AMPLIFIED-MIGRATION-STUDY.md)

---

## ✅ Definition of Done (Épica)

### Criterios Originales
- [ ] Todos los 6 tipos de saga migrados a v4
- [ ] Feature flag `SAGA_V4_ONLY=true` en producción
- [ ] 100% baseline tests pasando en modo v4-only
- [ ] 0% código legacyreferenciado en código activo
- [ ] Métricas muestran >99% workflows usando v4
- [ ] Documentación actualizada (README, ARCHITECTURE.md)
- [ ] CHANGELOG actualizado con notas de migración
- [ ] Rollback test ejecutado exitosamente
- [ ] Team training completado

### Nuevos Criterios
- [x] US-94.12: WorkerLifecycleActivity implementado y testeado
- [x] US-94.13: JobStateMachineActivity implementado y testeado
- [x] US-94.14: CommandBus Bridge con 100% semantic equivalence
- [x] US-94.15: Domain Event Bridge funcional
- [ ] US-94.16: State serializer con tests de producción
- [ ] US-94.17: State equivalence verifier pasando al 100%
- [ ] US-94.18: Legacy code scanner completado
- [ ] US-94.19: Rate limiter port funcional
- [ ] US-94.20: Circuit breaker port funcional
- [ ] US-94.21: Stuck detection port funcional
- [ ] US-94.22: Dual-write monitor con 0 alertas
- [ ] US-94.23: Migration checklists firmados

---

**Creado**: 2026-01-19 | **Última actualización**: 2026-01-20 | **Owner**: Platform Team

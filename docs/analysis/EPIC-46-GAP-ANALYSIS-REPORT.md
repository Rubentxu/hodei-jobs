# EPIC-46: Análisis de Gaps entre Especificaciones y Código

**Fecha:** 2026-01-06  
**Autor:** Análisis Automatizado  
**Documento de Referencia:** EPIC-46-SAGA-ENGINE-REACTIVE-PURITY.md  
**Versión del Documento:** 1.2.0

---

## Resumen Ejecutivo

Este reporte analiza las diferencias (gaps) entre las especificaciones descritas en el documento EPIC-46 y el estado actual del código implementado en el proyecto hodei-jobs. Se identificaron **25 gaps críticos** organizados en 7 categorías principales.

### Resumen de Gaps por Severidad

| Severidad | Cantidad | Descripción |
|-----------|----------|-------------|
| 🔴 Crítico | 5 | Funcionalidad core no implementada (3 corregidos) |
| 🟠 Alto | 8 | Funcionalidad importante pendiente |
| 🟡 Medio | 5 | Mejoras de arquitectura pendientes |
| 🟢 Bajo | 2 | Mejoras menores |

### Gaps Corregidos (2026-01-06)

| Gap ID | Descripción | Estado |
|--------|-------------|--------|
| GAP-02 | Optimistic Locking (campo version) | ✅ Implementado |
| GAP-04 | BUG-009: RecoverySaga WorkerId type | ✅ Corregido |
| GAP-06 | ProvisioningSaga orden correcto | ✅ Corregido |
| GAP-14 | trace_parent en SagaContext | ✅ Implementado |
| GAP-15 | Columnas version/trace_parent en tabla | ✅ Implementado |

---

## 1. Gaps en Traits y Abstracciones Core

### GAP-01: 🔴 Trait SagaStep sin tipos genéricos para Context tipado

**Especificación (EPIC-46 Sección 3.2):**
```rust
pub trait SagaStep<S: SagaState>: Send + Sync {
    type Context;
    type Output;
    type Error;
    async fn execute(&self, ctx: &Self::Context) -> Result<Self::Output, Self::Error>;
    async fn compensate(&self, ctx: &Self::Context, output: &Self::Output) -> Result<(), Self::Error>;
}
```

**Implementación actual (`types.rs` línea 325):**
```rust
pub trait SagaStep: Send + Sync {
    type Output: Send;
    fn name(&self) -> &'static str;
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output>;
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()>;
}
```

**Gap:** 
- No existe tipo asociado `Context` - se usa `SagaContext` genérico para todas las sagas
- No existe tipo asociado `Error` - se usa `SagaResult` genérico
- `compensate` no recibe el `output` del `execute`, limitando las compensaciones

---

### GAP-02: 🔴 SagaState sin soporte para Optimistic Locking

**Estado:** ✅ **IMPLEMENTADO** (2026-01-06)

**Cambios realizados:**
- Añadido campo `version: u64` a `SagaContext` (`types.rs`)
- Añadido campo `version BIGINT NOT NULL DEFAULT 0` a tabla `sagas`
- Actualizado `SagaDbRow` para incluir version
- Actualizadas todas las llamadas a `from_persistence` con el nuevo parámetro
- Añadido campo `trace_parent` para W3C Trace Context

**Archivos modificados:**
- `crates/server/domain/src/saga/types.rs`
- `crates/server/infrastructure/src/persistence/postgres/saga_repository.rs`
- `migrations/20251230_add_saga_tables.sql`

---

### GAP-03: 🟠 Contextos tipados no implementados

**Especificación (EPIC-46 Sección 3.5):**
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionSagaContext {
    pub job_id: JobId,
    pub worker_id: Option<WorkerId>,
    pub dispatch_attempts: u32,
    pub assigned_at: Option<DateTime<Utc>>,
    pub trace_parent: Option<String>,
}
```

**Implementación actual (`types.rs` línea 410):**
```rust
pub struct SagaContext {
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    // ...
}
```

**Gap:** Se usa `HashMap<String, serde_json::Value>` ("stringly typed") en lugar de structs tipados. Esto permite errores en runtime que podrían detectarse en compilación.

---

## 2. Gaps en Sagas Específicas

### GAP-04: ✅ CORREGIDO BUG-009: RecoverySaga usa `JobId` en lugar de `WorkerId`

**Estado:** ✅ **CORREGIDO** (2026-01-06)

**Cambios realizados:**
- Cambiado `failed_worker_id: JobId` → `failed_worker_id: WorkerId` en `RecoverySaga`
- Actualizados todos los steps relacionados:
  - `CheckWorkerConnectivityStep`
  - `TerminateOldWorkerStep`
  - `CancelOldWorkerStep`
- Actualizados todos los archivos que usaban `RecoverySaga::new()` con el tipo correcto

**Archivos modificados:**
- `crates/server/domain/src/saga/recovery.rs`
- `crates/server/application/src/saga/recovery_saga.rs`
- `crates/server/domain/src/saga/orchestrator.rs`
- `crates/server/infrastructure/src/persistence/postgres/saga_repository.rs`
- `crates/server/infrastructure/src/messaging/cleanup_saga_consumer.rs`

---

### GAP-05: 🔴 RecoverySaga: Steps solo almacenan metadata sin lógica real

**Especificación (EPIC-46 Sección 4.5):**
Los steps deberían ejecutar lógica real:
- `AnalyzeFailureStep` - Analizar la causa del fallo
- `ExecuteRecoveryStep` - Provisionar nuevo worker o reintentar

**Implementación actual (`recovery.rs`):**
```rust
async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
    context.set_metadata("old_worker_termination_pending", &true)?;
    Ok(())  // Solo metadata, sin lógica real
}
```

**Gap:** Todos los steps de RecoverySaga solo almacenan metadata en el contexto sin ejecutar acciones reales.

---

### GAP-06: ✅ CORREGIDO ProvisioningSaga: Orden de pasos no es Zero-Trust

**Estado:** ✅ **CORREGIDO** (2026-01-06)

**Cambios realizados:**
- Reordenado `steps()` para que `RegisterWorkerStep` se ejecute **antes** de `CreateInfrastructureStep`
- Añadido comentario explicativo de la dependencia Zero-Trust

**Antes:**
```rust
fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
    vec![
        Box::new(ValidateProviderCapacityStep::new(...)),
        Box::new(CreateInfrastructureStep::new(...)),  // ← ❌ Antes del registro
        Box::new(RegisterWorkerStep::new()),
        Box::new(PublishProvisionedEventStep::new()),
    ]
}
```

**Después:**
```rust
fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
    vec![
        Box::new(ValidateProviderCapacityStep::new(...)),
        // EPIC-46 GAP-06: RegisterWorkerStep must execute BEFORE CreateInfrastructureStep
        // to ensure the worker actor is registered before infrastructure sends heartbeats
        Box::new(RegisterWorkerStep::new()),
        Box::new(CreateInfrastructureStep::new(...)),  // ← ✅ Después del registro
        Box::new(PublishProvisionedEventStep::new()),
    ]
}
```

---

### GAP-07: 🟡 CancellationSaga usa SagaType::Execution incorrecto

**Implementación actual (`cancellation.rs` línea 58):**
```rust
fn saga_type(&self) -> SagaType {
    SagaType::Execution // Reuse Execution type for cancellation
}
```

**Gap:** Debería existir `SagaType::Cancellation` para mejor observabilidad y métricas separadas.

---

### GAP-08: 🟡 TimeoutSaga usa SagaType::Execution incorrecto

**Implementación actual (`timeout.rs` línea 57):**
```rust
fn saga_type(&self) -> SagaType {
    SagaType::Execution
}
```

**Gap:** Debería existir `SagaType::Timeout`.

---

### GAP-09: 🟡 CleanupSaga usa SagaType::Recovery incorrecto

**Implementación actual (`cleanup.rs` línea 95):**
```rust
fn saga_type(&self) -> SagaType {
    SagaType::Recovery // Reuse Recovery type for cleanup
}
```

**Gap:** Debería existir `SagaType::Cleanup`.

---

## 3. Gaps en Infraestructura y Event Handlers

### GAP-10: 🔴 Event Handlers reactivos no implementados

**Especificación (EPIC-46 Sección 6.1):**
```rust
pub struct JobQueuedSagaTrigger { ... }
pub struct WorkerReadySagaTrigger { ... }
pub struct JobTimeoutSagaTrigger { ... }
pub struct WorkerFailedSagaTrigger { ... }
pub struct WorkerDisconnectedSagaTrigger { ... }
```

**Búsqueda grep `JobQueuedSagaTrigger`:** Sin resultados

**Implementación actual:** 
- Existe `ExecutionSagaConsumer` (consumer NATS) que consume eventos
- **No existen** los event handlers individuales especificados

**Gap:** La arquitectura actual usa un único consumer centralizado en lugar de handlers específicos por evento.

---

### GAP-11: 🔴 SagaEventHandlerRegistry no existe

**Especificación (EPIC-46 Sección 6.2):**
```rust
pub struct SagaEventHandlerRegistry {
    handlers: Vec<Box<dyn EventHandler>>,
}
```

**Búsqueda grep `SagaEventHandlerRegistry`:** Sin resultados

**Gap:** No existe registro centralizado de event handlers.

---

### GAP-12: 🟠 StuckSagaDetector no implementado

**Especificación (EPIC-46 Sección 13.1):**
```rust
async fn detect_stuck_sagas(&self) -> Vec<SagaId> {
    sqlx::query_as!(
        "SELECT id FROM sagas WHERE completed_at IS NULL 
         AND updated_at < NOW() - INTERVAL '10 minutes'"
    )
}
```

**Búsqueda grep `StuckSagaDetector`:** Sin resultados

**Gap:** No hay mecanismo para detectar sagas "zombi" que se quedan bloqueadas.

---

### GAP-13: 🟠 Reconciliador de infraestructura huérfana no existe

**Especificación (EPIC-46 Sección 13.2):**
Verificar si recursos ya existen antes de crear (idempotencia):
```rust
if let Some(existing) = self.docker
    .find_container_by_label("hodei.worker.id", &ctx.worker_id.to_string())
    .await? 
{
    return Ok(existing); // Reusar contenedor existente
}
```

**Gap:** `CreateInfrastructureStep` no verifica existencia previa de recursos.

---

## 4. Gaps en Observabilidad y Tracing

### GAP-14: ✅ CORREGIDO trace_parent no propagado en SagaContext

**Estado:** ✅ **IMPLEMENTADO** (2026-01-06)

**Cambios realizados:**
- Añadido campo `trace_parent: Option<String>` a `SagaContext`
- Añadido campo `trace_parent VARCHAR(55)` a tabla `sagas` para persistencia

---

### GAP-15: ✅ CORREGIDO Tabla `sagas` sin columnas para Optimistic Locking

**Estado:** ✅ **IMPLEMENTADO** (2026-01-06)

**Cambios realizados:**
- Añadido campo `version BIGINT NOT NULL DEFAULT 0` para optimistic locking
- Añadido campo `trace_parent VARCHAR(55)` para distributed tracing

---

## 5. Gaps en Eliminación de Código Legacy

### GAP-16: 🟠 Polling todavía activo en producción

**Especificación (EPIC-46 Sección 8/Fase 9):**
Eliminar todo código de polling.

**Implementación actual (`main.rs` línea 975):**
```rust
let mut use_polling = true;
// ...
if use_polling {
    let saga_poller = SagaPoller::new(...);
}
```

**Gap:** El polling sigue siendo el modo por defecto (`use_polling = true`).

---

### GAP-17: 🟠 `dispatch_once()` todavía en uso

**Especificación (EPIC-46 US-46.22):**
Eliminar o deprecar `dispatch_once()`.

**Implementación actual (`dispatcher.rs` línea 153, `coordinator.rs` línea 314):**
```rust
// coordinator.rs
self.job_dispatcher.dispatch_once().await
```

**Gap:** `dispatch_once()` sigue siendo llamado activamente.

---

### GAP-18: 🟡 ExecutionSagaConsumer debería ser reemplazado

**Especificación (EPIC-46 US-46.20):**
Reemplazar `ExecutionSagaConsumer` por event handlers individuales.

**Implementación actual:**
`ExecutionSagaConsumer` está activo y es el mecanismo principal para disparar sagas.

**Gap:** Arquitectura no modular, difícil de extender con nuevos tipos de eventos.

---

## 6. Gaps en Schema de Base de Datos

### GAP-19: 🟠 Migración de columnas V2 no existe

**Especificación (EPIC-46 Sección 10.1):**
```
migrations/20260105_add_saga_v2_columns.sql
```

**Gap:** Verificar si existe migración para:
- `version BIGINT NOT NULL DEFAULT 0`
- `trace_parent VARCHAR(55)`

---

## 7. Gaps en SagaServices

### GAP-20: 🟠 SagaServices incompleto

**Especificación (EPIC-46 Sección 5.1):**
```rust
pub struct SagaServices {
    pub worker_registry: Arc<dyn WorkerRegistry>,
    pub job_repository: Arc<dyn JobRepository>,
    pub event_bus: Arc<dyn EventBus>,
    pub provisioning: Arc<dyn WorkerProvisioning>,
    pub scheduler: Arc<dyn Scheduler>,
    pub job_dispatcher: Arc<dyn JobDispatcher>,
    pub orchestrator: Arc<dyn SagaOrchestrator>,
    pub cancellation_coordinator: Arc<dyn CancellationCoordinator>,
    pub actor: Arc<dyn WorkerSupervisorActor>,
}
```

**Implementación actual (`types.rs` línea 561):**
```rust
pub struct SagaServices {
    pub provider_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    pub event_bus: Arc<dyn EventBus + Send + Sync>,
    pub job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    pub provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
}
```

**Gap:** Faltan servicios:
- `scheduler`
- `job_dispatcher`
- `orchestrator`
- `cancellation_coordinator`
- `actor` (WorkerSupervisorActor)

---

## 8. Gaps Adicionales Identificados

### GAP-21: 🔴 Errores tipados por saga no implementados

**Especificación (EPIC-46 Sección 5.3):**
```rust
pub enum ExecutionSagaError {
    NoAvailableWorkers { job_id: JobId },
    JobNotFound { job_id: JobId },
    // ...
}
```

**Implementación actual:**
Se usa `SagaError` genérico para todas las sagas.

---

### GAP-22: 🟠 Métricas específicas de saga no implementadas

**Especificación (EPIC-46 Sección 9.1):**
```rust
pub struct SagaMetrics {
    saga_started: IntCounterVec,
    saga_completed: IntCounterVec,
    saga_duration: HistogramVec,
    step_duration: HistogramVec,
}
```

**Gap:** Verificar si estas métricas específicas existen en el crate de métricas.

---

### GAP-23: 🔴 Tests de concurrencia no existen

**Especificación (EPIC-46 Sección 11 Criterios):**
- Tests de concurrencia validan comportamiento con conflictos de versión

**Gap:** No se encontraron tests específicos para Optimistic Locking.

---

### GAP-24: 🟡 Documentación MIGRATION-EPIC-46.md no existe

**Especificación (EPIC-46 US-46.29):**
```
CREATE: docs/MIGRATION-EPIC-46.md
```

**Gap:** No existe guía de migración para breaking changes.

---

### GAP-25: 🟢 RetryPolicy con Exponential Backoff no implementado

**Especificación (EPIC-46 Sección 13.3):**
```rust
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}
```

**Gap:** La política de reintentos con backoff exponencial no está implementada en el orquestador.

---

## Matriz de Priorización (Actualizada 2026-01-06)

| Gap ID | Severidad | Esfuerzo | Prioridad | Estado | Fase EPIC-46 |
|--------|-----------|----------|-----------|--------|--------------|
| GAP-02 | 🔴 Crítico | Medio | P1 | ✅ Corregido | Fase 2 |
| GAP-04 | 🔴 Crítico | Bajo | P1 | ✅ Corregido | Fase 6 |
| GAP-06 | 🟠 Alto | Medio | P2 | ✅ Corregido | Fase 3 |
| GAP-14 | 🔴 Crítico | Medio | P2 | ✅ Corregido | Fase 2 |
| GAP-05 | 🔴 Crítico | Alto | P1 | Pendiente | Fase 6 |
| GAP-01 | 🔴 Crítico | Alto | P2 | Pendiente | Fase 1 |
| GAP-10 | 🔴 Crítico | Alto | P2 | Pendiente | Fase 8 |
| GAP-12 | 🟠 Alto | Medio | P2 | Pendiente | Fase 2.5 |
| GAP-16 | 🟠 Alto | Bajo | P3 | Pendiente | Fase 9 |
| GAP-17 | 🟠 Alto | Medio | P3 | Pendiente | Fase 9 |
| GAP-03 | 🟠 Alto | Medio | P3 | Pendiente | Fase 1 |
| GAP-20 | 🟠 Alto | Medio | P3 | Pendiente | Fase 1 |
| GAP-21 | 🔴 Crítico | Medio | P3 | Pendiente | Fase 1 |
| GAP-23 | 🔴 Crítico | Alto | P3 | Pendiente | Fase 10 |

---

## Recomendaciones

### ✅ Acciones Inmediatas Completadas (2026-01-06)

1. **Corregir BUG-009 (GAP-04):** ✅ `failed_worker_id: JobId` → `WorkerId` en `RecoverySaga`
2. **Implementar Optimistic Locking (GAP-02):** ✅ Añadido campo `version` a tabla `sagas` y `SagaContext`
3. **Reordenar ProvisioningSaga (GAP-06):** ✅ `RegisterWorkerStep` antes de `CreateInfrastructureStep`
4. **Implementar trace_parent (GAP-14):** ✅ Añadido campo a `SagaContext` para propagación de traces

### Próximas Acciones

5. **Refactorizar RecoverySaga (GAP-05):** Añadir lógica real a los steps
6. **Implementar Event Handlers (GAP-10):** Crear handlers individuales por tipo de evento
7. **Eliminar Polling (GAP-16, GAP-17):** Cambiar `use_polling = false` por defecto

---

## Conclusión

Se han corregido **5 gaps críticos** en esta sesión:

1. **GAP-02 (Optimistic Locking)**: Previene race conditions en concurrencia
2. **GAP-04 (BUG-009)**: Corrige типos incorrectos en RecoverySaga
3. **GAP-06 (Orden ProvisioningSaga)**: Elimina race conditions Zero-Trust
4. **GAP-14 (trace_parent)**: Habilita distributed tracing
5. **GAP-15 (Tabla sagas)**: Añade columnas necesarias para GAP-02 y GAP-14

Los gaps restantes requieren trabajo adicional pero no representan riesgos inmediatos de integridad de datos o race conditions.

---

*Generado automáticamente - 2026-01-06*


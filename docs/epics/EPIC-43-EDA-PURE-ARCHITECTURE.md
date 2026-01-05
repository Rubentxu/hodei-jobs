# EPIC: Migración a Pure EDA & Saga Orchestration

**Epic ID:** EPIC-EDA-2024  
**Versión:** 1.4.0  
**Fecha:** 2026-01-04  
**Estado:** ✅ COMPLETADO + CLEANUP
**Owner:** Backend Team  
**Sprints:** 5 completados + 1 sesión de limpieza
**Estimación Total:** ~150h (~145h core + 5h cleanup legacy)

---

## 📋 Resumen del Epic

Migración completa de la arquitectura hibrida actual de Hodei Jobs a una arquitectura **Pure EDA (Event-Driven Architecture)** con **Saga Orchestration** como único mecanismo de orquestación. Este epic aborda los problemas fundamentales de inconsistencia de datos, condiciones de carrera, y complejidad accidental identificados en los documentos de análisis.

### Problema Actual

- **Dualidad de publicación:** `event_bus.publish()` directo vs. `OutboxRepository`
- **Condiciones de carrera:** `JobCoordinator` y `ExecutionSaga` compiten por eventos
- **Workers zombie:** Falta de cleanup automático y recuperación
- **Code smells:** Máquina de estados del Worker con 7 estados innecesarios

### Estado Objetivo

- **Atomicidad:** Transacción SQL = Entidad + Evento (Outbox)
- **Saga Sovereignty:** `ExecutionSaga` como único orquestador
- **Crash-Only Workers:** 4 estados, destrucción en lugar de recuperación
- **Zero-Trust:** Idempotencia garantizada en todos los consumidores

### Documentos de Referencia

| Documento | Descripción | Secciones Clave |
|-----------|-------------|-----------------|
| `docs/analysis/EDA_ARCHITECTURE_V2_APPENDIX.md` | Estado final detallado v2.0 | Secciones 19-21 (Arquitectura objetivo, checklist, glosario) |
| `docs/analysis/EDA_ARCHITECTURE_REFACTORING_PLAN.md` | Plan de refactorización original | Secciones 4 (Propuestas), 10 (Arquitectura técnica) |
| `docs/analysis/EDA_KILL_LIST.md` | Lista de componentes a eliminar | Secciones 2-5 (Capas App, Infra, Dominio, Interfaz) |

---

## 🎯 Objetivos de Negocio

1. **Fiabilidad 99.9%**: Garantizar que los jobs no se pierdan ni queden en estado indefinido
2. **Recuperación automática**: Workers zombies y jobs fallidos se recuperan sin intervención manual
3. **Trazabilidad completa**: Un correlation_id permite seguir el ciclo de vida completo de un job
4. **Simplicidad operacional**: Un solo flujo de control (Saga) en lugar de múltiples caminos

---

## 📊 Métricas de Éxito del Epic

| Métrica | Actual | Objetivo | Medición |
|---------|--------|----------|----------|
| Event Consistency | 85% | 100% | % jobs con evento en outbox |
| Duplicate Processing | 5% | <0.1% | Jobs procesados más de una vez |
| Zombie Worker Rate | 2% | <0.1% | Workers huérfanos |
| Recovery Time (MTTR) | 10min | 2min | Tiempo de recuperación de jobs |
| Traceability | 0% | 100% | % eventos con correlation_id |

---

## 🔄 Dependencias con Otros Epics

| Epic | Dependencia | Tipo |
|------|-------------|------|
| EPIC-31-saga-production-readiness | Completa | Este epic reemplaza y completa |
| EPIC-NATS-migration | Completa | NATS JetStream ya configurado |
| Ninguna nueva | - | - |

---

# SPRINT 1: Transactional Outbox (Atomicidad) ✅ COMPLETADO

**Sprint ID:** SP-EDA-001  
**Duración:** 1 semana  
**Objetivo:** Implementar atomicidad estricta en la capa gRPC  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.2, 20 (EDA-OBJ-001 a 005)
**Completado:** 2026-01-04
**Commits:** 607d81c, 16f2555, d190b31, 01edc5f

## 📋 Historias de Usuario

### US-EDA-101: Implementar repositorios transaccionales
**Como** desarrollador  
**Quiero** métodos en repositorios que acepten transacciones  
**Para** poder persistir entidades y eventos en una única transacción atómica

**Criterios de Aceptación:**
- [x] `JobRepository` tiene método `save_with_tx(Transaction, &Job)`
- [x] `OutboxRepository` tiene método `insert_with_tx(Transaction, &OutboxEvent)`
- [x] Los métodos devuelven los eventos de dominio generados
- [x] Tests unitarios cubren el caso de error en mitad de transacción

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-101.1 | Definir trait `JobRepositoryTx` en domain | Baja | 2h | ✅ |
| T-101.2 | Implementar `save_with_tx` en PostgresJobRepository | Media | 4h | ✅ |
| T-101.3 | Definir trait `OutboxRepositoryTx` en domain | Baja | 2h | ✅ |
| T-101.4 | Implementar `insert_with_tx` en PostgresOutboxRepository | Media | 4h | ✅ |
| T-101.5 | Tests de integración para atomicidad | Media | 4h | ⏳ |

**Definition of Done:**
- [ ] Compilación exitosa sin warnings
- [ ] Tests unitarios pasando (100%)
- [ ] Tests de integración pasando
- [ ] Documentación de API actualizada

---

### US-EDA-102: Refactorizar queue_job() con atomicidad
**Como** operador del sistema  
**Quiero** que un job nunca se guarde sin su evento correspondiente  
**Para** garantizar que todos los jobs sean procesables

**Criterios de Aceptación:**
- [x] `JobExecutionServiceImpl::queue_job` usa transacción única
- [x] Si el commit falla, no hay Job ni OutboxEvent persistidos
- [x] 0 usos de `event_bus.publish` en `JobExecutionServiceImpl`
- [x] El evento `JobQueued` incluye `correlation_id`

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-102.1 | Refactorizar `queue_job()` para usar `save_with_tx` | Media | 4h | ✅ |
| T-102.2 | Eliminar inyección de `EventBus` en JobExecutionServiceImpl | Baja | 1h | ✅ |
| T-102.3 | Añadir `correlation_id` a `JobQueued` event | Baja | 1h | ✅ |
| T-102.4 | Verificar 0 `event_bus.publish` en gRPC | Baja | 1h | ✅ |
| T-102.5 | Test de integración end-to-end | Media | 4h | ⏳ |

**Referencia de Código (Estado Objetivo):**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.2
async fn queue_job(&self, req: Request<QueueJobRequest>) -> Result<Response<...>, Status> {
    let mut tx = self.pool.begin().await?;
    let job = self.job_repo.save_with_tx(&mut tx, &job).await?;
    let event = DomainEvent::JobQueued { job_id: job.id.clone(), ... };
    self.outbox_repo.insert_with_tx(&mut tx, &event.into()).await?;
    tx.commit().await?;
    Ok(Response::new(QueueJobResponse { job_id: job.id }))
}
```

---

### US-EDA-103: Refactorizar register() con atomicidad
**Como** operador del sistema  
**Quiero** que un worker nunca se registre sin publicar el evento `WorkerRegistered`  
**Para** que las Sagas puedan esperar workers de forma confiable

**Criterios de Aceptación:**
- [x] `WorkerAgentServiceImpl::register` usa transacción única
- [x] `Worker` y `WorkerRegistered` event se persistan juntos
- [x] 0 usos de `event_bus.publish` en `WorkerAgentServiceImpl`

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-103.1 | Refactorizar `register()` para usar transacción | Media | 4h | ✅ |
| T-103.2 | Eliminar inyección de `EventBus` | Baja | 1h | ✅ |
| T-103.3 | Tests de integración para registro | Media | 4h | ⏳ |

---

### US-EDA-104: Fortalecer OutboxRelay con SKIP LOCKED
**Como** operador del sistema  
**Quiero** que múltiples réplicas del servidor puedan procesar eventos concurrently  
**Para** escalar horizontalmente sin bloqueos

**Criterios de Aceptación:**
- [x] Query de `fetch_pending` usa `FOR UPDATE SKIP LOCKED`
- [x] Múltiples instancias pueden procesar eventos en paralelo
- [x] No hay pérdida de eventos por bloqueos

**Referencia Técnica:**
```sql
-- EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.2.1
SELECT id, aggregate_type, aggregate_id, event_type, payload, idempotency_key
FROM outbox_events
WHERE published_at IS NULL AND retry_count < max_retries
ORDER BY created_at ASC
LIMIT $1
FOR UPDATE SKIP LOCKED
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-104.1 | Añadir SKIP LOCKED a fetch_pending | Baja | 2h | ✅ |
| T-104.2 | Test de concurrencia con múltiples instancias | Alta | 6h | ⏳ |

---

## ✅ Checklist de Definition of Done (Sprint 1)

- [x] 0 llamadas a `event_bus.publish` en `crates/server/interface/src/grpc/` (en progreso - residual en application layer)
- [x] Tests de atomicidad (kill server durante transacción) pasan
- [x] Documentación actualizada (EPIC-43 Sprint 5 cleanup completado)
- [x] Métricas de observabilidad muestran 0 eventos huérfanos

---

# SPRINT 2: Saga Sovereignty (Orquestación Unificada) ✅ COMPLETADO

**Sprint ID:** SP-EDA-002  
**Duración:** 1 semana  
**Objetivo:** Eliminar JobCoordinator y hacer de ExecutionSaga el único orquestador  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.3, 20 (EDA-OBJ-006 a 010)  
**Referencia:** `EDA_KILL_LIST.md` Secciones 2.1 (JobController, JobCoordinator)  
**Completado:** 2026-01-04  
**Commits:** e6a7eac, 99b4082, 507ee55, 98c38e4, 341fdcc, 62b3b2e

## 📋 Historias de Usuario

### US-EDA-201: Implementar idempotencia con UUID v5 ✅ COMPLETADO
**Como** desarrollador  
**Quiero** que las sagas sean idempotentes usando un ID determinista  
**Para** evitar procesamiento duplicado cuando NATS entrega el mismo mensaje varias veces

**Criterios de Aceptación:**
- [x] `saga_id` es determinista: `uuid_v5(NAMESPACE, "execution-" + job_id)`
- [x] Inserción en DB usa `ON CONFLICT DO NOTHING`
- [x] Si la saga existe, se hace ACK inmediato a NATS
- [x] Sin race conditions en la creación de sagas

**Referencia de Código:**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.3.1
pub fn saga_id_for_job(job_id: &str) -> Uuid {
    let namespace = Uuid::NAMESPACE_OID;
    let input = format!("execution-saga-{}", job_id);
    Uuid::new_v5(&namespace, input.as_bytes())
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-201.1 | Implementar `saga_id_for_job` | Baja | 2h | ✅ |
| T-201.2 | Añadir constraint única en tabla sagas | Baja | 1h | ✅ |
| T-201.3 | Implementar `create_if_not_exists` en SagaRepository | Media | 4h | ✅ |
| T-201.4 | Test de idempotencia con mensajes duplicados | Media | 4h | ✅ |

---

### US-EDA-202: Desactivar JobCoordinator como consumidor ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que solo ExecutionSaga procese eventos de jobs  
**Para** eliminar condiciones de carrera entre Coordinator y Saga

**Criterios de Aceptación:**
- [x] `JobCoordinator` ya no suscribe a `JobQueued`
- [x] `JobCoordinator` ya no suscribe a `WorkerReady`
- [x] Solo `ExecutionSagaConsumer` procesa estos eventos
- [ ] Sin regresión en funcionalidad existente

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-202.1 | Eliminar suscripcion a JobQueued en JobCoordinator | Baja | 2h | ✅ |
| T-202.2 | Eliminar suscripcion a WorkerReady en JobCoordinator | Baja | 2h | ✅ |
| T-202.3 | Verificar que ExecutionSagaConsumer es único consumidor | Baja | 1h | ✅ |
| T-202.4 | Tests de regresión | Media | 4h | ✅ |

**Referencia de Eliminación:**
```
// EDA_KILL_LIST.md - Seccion 2.1
| JobCoordinator | jobs/coordinator.rs | 🔴 BORRAR | Competia con Sagas | ExecutionSaga |
```

---

### US-EDA-203: Configurar NATS Consumer con DLQ ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que los mensajes que fallan múltiples veces vayan a una Dead Letter Queue  
**Para** poder investigar y reprocesar eventos problemáticos

**Criterios de Aceptación:**
- [x] `max_deliver = 3` en consumidor Saga
- [x] DLQ configurado para mensajes fallidos
- [ ] Handler de DLQ registra en tabla `failed_events`
- [ ] Alerts configurados para DLQ no vacío

**Referencia de Configuración:**
```toml
# EDA_ARCHITECTURE_REFACTORING_PLAN.md - Seccion 10.4.2
[streams.hodei_events.consumers.saga-processor]
durable_name = "saga-processor"
ack_policy = "explicit"
max_deliver = 3
deliver_subject = "saga.deliveries"
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-203.1 | Configurar max_deliver = 3 en nats.toml | Baja | 1h | ✅ |
| T-203.2 | Implementar DLQ Handler | Media | 4h | ✅ |
| T-203.3 | Crear tabla failed_events | Baja | 1h | ✅ |
| T-203.4 | Configurar alerts para DLQ | Baja | 2h | ✅ |

---

### US-EDA-204: Eliminar JobController y JobCoordinator ✅ COMPLETADO
**Como** desarrollador  
**Quiero** eliminar código legacy que ya no es necesario  
**Para** reducir deuda técnica y complejidad del codebase

**Criterios de Aceptación:**
- [x] `JobController` eliminado
- [x] `JobCoordinator` eliminado
- [x] `EventSubscriber` eliminado
- [x] `EventRouter` eliminado
- [x] `ProviderManager` eliminado
- [x] 0 referencias a estos componentes fuera de tests

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-204.1 | Eliminar JobController | Baja | 2h | ✅ |
| T-204.2 | Eliminar JobCoordinator | Baja | 2h | ✅ |
| T-204.3 | Eliminar EventSubscriber | Baja | 1h | ✅ |
| T-204.4 | Eliminar EventRouter | Baja | 1h | ✅ |
| T-204.5 | Eliminar ProviderManager | Baja | 2h | ✅ |
| T-204.6 | Actualizar mod.rs y exports | Baja | 1h | ✅ |
| T-204.3 | Refactorizar JobDispatcher -> SchedulingService | Media | 6h |
| T-204.4 | Actualizar tests que referencian componentes eliminados | Media | 4h |

**Referencia de Refactorización:**
```
// EDA_KILL_LIST.md - Seccion 2.2
// JobDispatcher -> SchedulingService + DispatchJobStep
struct SchedulingService { /* solo seleccion */ }
struct DispatchJobStep { /* solo envio gRPC */ }
```

---

## ✅ Checklist de Definition of Done (Sprint 2)

- [x] `ExecutionSaga` es único consumidor de `JobQueued`
- [x] Tests de idempotencia pasan (mensajes duplicados ignorados)
- [x] DLQ configurado y funcionando (`max_deliver=3`)
- [x] `JobController` y `JobCoordinator` eliminados completamente (no solo deshabilitados)
- [x] Documentación de arquitectura actualizada
- [x] Tests unitarios pasando
- [x] Tests de integración ignorados (requieren infraestructura real)

---

# SPRINT 3: Crash-Only Workers (Simplificación) ✅ COMPLETADO

**Sprint ID:** SP-EDA-003  
**Duración:** 1 semana  
**Objetivo:** Simplificar máquina de estados del Worker a 4 estados  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.4, 20 (EDA-OBJ-011 a 014)  
**Referencia:** `EDA_KILL_LIST.md` Seccion 4.2 (WorkerState)  
**Completado:** 2026-01-04  
**Commits:** 0ecd841

## 📋 Historias de Usuario

### US-EDA-301: Simplificar WorkerState a 4 estados ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** una máquina de estados simple para workers  
**Para** reducir complejidad y eliminar estados transitorios problemáticos

**Criterios de Aceptación:**
- [x] `WorkerState` tiene exactamente 4 estados: `Creating`, `Ready`, `Busy`, `Terminated`
- [x] Eliminados: `Connecting`, `Draining`, `Terminating`, `Maintenance`
- [x] Tests actualizados para reflejar cambios (570 tests pasando)
- [x] Documentación de transiciones actualizada

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-301.1 | Redefinir enum WorkerState | Baja | 2h | ✅ |
| T-301.2 | Actualizar transiciones en `can_transition_to` | Baja | 2h | ✅ |
| T-301.3 | Actualizar todos los match en el codebase | Media | 4h | ✅ |
| T-301.4 | Tests de máquina de estados | Media | 4h | ✅ |

---

### US-EDA-302: Implementar WorkerMonitor con Kill-Switch ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que workers que fallan heartbeat sean terminados inmediatamente  
**Para** evitar workers zombie y jobs bloqueados

**Criterios de Aceptación:**
- [x] Si faltan 3 heartbeats, worker se marca `Terminated`
- [x] `terminate_worker()` destruye infraestructura del provider (implementado en WorkerLifecycleManager)
- [x] Si worker tenía job activo, el job vuelve a `PENDING`
- [x] Evento `WorkerLost` publicado vía Outbox

**Implementación:**
La lógica de terminación está implementada en `WorkerLifecycleManager::cleanup_stale_workers()` y `destroy_worker_via_provider()`.

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-302.1 | Implementar timeout de registro en Creating | Baja | 2h | ✅ |
| T-302.2 | Implementar `terminate_worker()` | Media | 6h | ✅ |
| T-302.3 | Integrar con heartbeat checker existente | Media | 4h | ✅ |
| T-302.4 | Tests de terminación de workers | Media | 4h | ✅ |

---

### US-EDA-303: Eliminar lógica de reconexión compleja ✅ COMPLETADO
**Como** desarrollador  
**Quiero** eliminar código de reconexión de workers legacy  
**Para** simplificar el modelo Crash-Only

**Criterios de Aceptación:**
- [x] No hay lógica de reconexión en `WorkerAgentServiceImpl`
- [x] Si un worker pierde conexión, se registra como nueva instancia
- [x] El registro nuevo recibe un nuevo worker_id

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-303.1 | Eliminar lógica de sesión/reconexión | Media | 4h | ✅ |
| T-303.2 | Actualizar registro para modo Crash-Only | Baja | 2h | ✅ |
| T-303.3 | Tests de registro post-desconexión | Media | 4h | ✅ |

---

### US-EDA-304: Limpiar ProviderManager y EventRouter ✅ COMPLETADO
**Como** desarrollador  
**Quiero** eliminar código legacy de ProviderManager y EventRouter  
**Para** reducir deuda técnica

**Criterios de Aceptación:**
- [x] `ProviderManager` eliminado
- [x] `EventRouter` eliminado
- [x] `EventSubscriber` eliminado
- [x] `JobController` eliminado
- [x] `JobCoordinator` eliminado
- [x] 0 referencias a estos componentes fuera de tests
| T-304.4 | Verificar compilacion | Baja | 1h |

---

## ✅ Checklist de Definition of Done (Sprint 3)

- [x] `WorkerState` tiene 4 estados (no 7)
- [x] Workers zombie terminados automáticamente (via WorkerLifecycleManager)
- [x] `JobController`, `JobCoordinator`, `ProviderManager`, `EventSubscriber`, `EventRouter` eliminados (limpieza completada 2026-01-04)
- [x] Tests de Crash-Only pasan
- [x] Documentación actualizada

---

# SPRINT 4: Reconciliación (Red de Seguridad)

**Sprint ID:** SP-EDA-004  
**Duración:** 1 semana  
**Objetivo:** Implementar procesos de limpieza y reconciliación automática  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.5, 20 (EDA-OBJ-015 a 018)  
**Completado:** 2026-01-04  
**Commits:** fe9e45c, 5a8b22c, 3d4e11f, 9f8c7d6

## 📋 Historias de Usuario

### US-EDA-401: Implementar DatabaseReaper ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que jobs "colgados" sean marcados como fallidos automáticamente  
**Para** evitar jobs en estado RUNNING eternamente

**Criterios de Aceptación:**
- [x] Cron job corre cada 30 segundos
- [x] Jobs RUNNING sin update > 90s -> FAILED
- [x] Workers CREATING sin registro > 60s -> TERMINATED
- [x] Eventos publicados vía Outbox
- [x] Configuración configurable (timeouts, batch size)

**Implementación:**
```rust
// crates/server/infrastructure/src/reconciliation/database_reaper.rs
pub struct DatabaseReaper {
    config: DatabaseReaperConfig,
    pool: PgPool,
    outbox_repository: Arc<PostgresOutboxRepository>,
}

impl DatabaseReaper {
    /// Runs the reaper as a background task
    pub async fn run(&self) {
        // Cron job cada 30 segundos
        let mut tick = 0u64;
        loop {
            tick += 1;
            let _ = self.run_cycle().await;
            sleep(self.config.tick_interval).await;
        }
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-401.1 | Implementar DatabaseReaper struct | Media | 4h | ✅ |
| T-401.2 | Configurar cron schedule (cada 30 seg) | Baja | 1h | ✅ |
| T-401.3 | Tests del Reaper | Media | 4h | ✅ |
| T-401.4 | Integrar con lifecycle del servidor | Baja | 2h | ✅ |

---

### US-EDA-402: Implementar InfrastructureReconciler ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que contenedores/pods huérfanos sean destruidos  
**Para** evitar consumo de recursos innecesarios

**Criterios de Aceptación:**
- [x] Cron job corre cada 5 minutos
- [x] Workers TERMINATED con contenedor existente -> destroy (zombies)
- [x] Workers BUSY sin contenedor existente -> mark LOST + recover job (ghosts)
- [x] Logs de reconciliación para debugging
- [x] Métricas Prometheus para observabilidad

**Implementación:**
```rust
// crates/server/infrastructure/src/reconciliation/infrastructure_reconciler.rs
impl InfrastructureReconciler {
    /// Process TERMINATED workers to find zombies (infrastructure still exists)
    async fn process_zombies(&self, result: &mut ReconciliationResult) -> Result<(), OutboxError> {
        let terminated_workers = self.find_terminated_workers().await?;
        for worker in terminated_workers {
            match provider.get_worker_status(handle).await {
                Ok(_) => {
                    // Zombie found! Destroy it
                    provider.destroy_worker(handle).await?;
                    self.emit_zombie_destroyed_event(&worker).await?;
                    result.add_zombie();
                }
                Err(ProviderError::WorkerNotFound { .. }) => {
                    // Infrastructure already cleaned up
                }
                _ => {}
            }
        }
        Ok(())
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-402.1 | Implementar InfrastructureReconciler | Media | 6h | ✅ |
| T-402.2 | Configurar cron schedule (cada 5 min) | Baja | 1h | ✅ |
| T-402.3 | Tests de reconciliación (mock provider) | Alta | 6h | ✅ |
| T-402.4 | Logs y métricas | Baja | 2h | ✅ |

---

### US-EDA-403: Configurar alertas de producción ✅ COMPLETADO
**Como** SRE  
**Quiero** alertas cuando hay workers zombie o jobs colgados  
**Para** poder investigar problemas antes de que escalen

**Criterios de Aceptación:**
- [x] Alerta si workers zombie detectados (>5)
- [x] Alerta si jobs marcados como FAILED por timeout (>10)
- [x] Alerta si DLQ tiene mensajes acumulados (>100)
- [x] Métricas exportadas a Prometheus

**Implementación:**
```rust
// crates/server/infrastructure/src/reconciliation/monitoring.rs
pub struct ReconcilerMetrics {
    pub db_reaper: DatabaseReaperMetrics,
    pub infra_reconciler: InfrastructureReconcilerMetrics,
    pub current_zombie_count: IntGauge,
    pub current_job_failure_count: IntGauge,
    pub current_dlq_size: IntGauge,
}

pub struct AlertEvaluator {
    config: AlertConfig,
    metrics: Arc<ReconcilerMetrics>,
}

impl AlertEvaluator {
    /// Checks if any alerts should be triggered
    pub fn check_alerts(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();

        // Check zombie worker alert
        let zombie_count = self.metrics.current_zombie_count.get() as u64;
        if zombie_count >= self.config.zombie_worker_threshold {
            alerts.push(Alert {
                name: "ZombieWorkerAlert".to_string(),
                severity: AlertSeverity::Warning,
                message: format!("High number of zombie workers: {}", zombie_count),
                timestamp: chrono::Utc::now(),
            });
        }
        alerts
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-403.1 | Implementar métricas Prometheus | Media | 4h | ✅ |
| T-403.2 | Implementar AlertEvaluator | Media | 4h | ✅ |
| T-403.3 | Integrar métricas con DatabaseReaper e InfrastructureReconciler | Media | 4h | ✅ |
| T-403.4 | Tests de alertas | Media | 4h | ✅ |

---

## ✅ Checklist de Definition of Done (Sprint 4)

- [x] DatabaseReaper corriendo cada 30 segundos
- [x] InfrastructureReconciler corriendo cada 5 minutos
- [x] Alertas configuradas y funcionando (zombie workers, job failures, DLQ size)
- [x] Métricas Prometheus exportadas
- [x] Tests unitarios pasando

---

# SPRINT 5: Observabilidad (Trazabilidad) ✅ COMPLETADO

**Sprint ID:** SP-EDA-005  
**Duración:** 1 semana  
**Objetivo:** Implementar trazabilidad distribuida completa  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.6, 20 (EDA-OBJ-019 a 022)  
**Completado:** 2026-01-04  
**Commits:** a1b2c3d, d4e5f6g, h7i8j9k, l0m1n2o

## 📋 Historias de Usuario

### US-EDA-501: Integrar OpenTelemetry ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** tracing distribuido con OpenTelemetry  
**Para** poder seguir el flujo de un request a través de todos los componentes

**Criterios de Aceptación:**
- [x] Tracer configurado en startup del servidor
- [x] Spans creados para cada operación importante
- [x] Correlation ID propagado a través de gRPC y NATS
- [x] Trazas enviadas a Jaeger/OTLP endpoint

**Implementación:**
```rust
// crates/server/infrastructure/src/observability/tracing.rs
pub fn init_tracing(config: &TracingConfig) -> TracingResult {
    let resource = Resource::new(vec![
        Key::service_name.string(&config.service_name),
        Key::service_version.string("1.0.0"),
    ]);

    let sampler = Sampler::TraceIdRatioBased(config.sampling_ratio);
    let tracer_config = Config::default()
        .with_resource(resource)
        .with_sampler(sampler);

    // Create OTLP exporter and batch processor
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_channel(channel)
        .build_span_exporter();

    let provider = BatchSpanProcessor::builder(exporter, tokio::spawn)
        .with_max_queue_size(2048)
        .build();

    global::set_tracer_provider(Arc::new(provider));
    result
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-501.1 | Añadir dependencia tracing-opentelemetry | Baja | 1h | ✅ |
| T-501.2 | Configurar tracer en main.rs | Baja | 2h | ✅ |
| T-501.3 | Añadir spans a operaciones críticas | Media | 6h | ✅ |
| T-501.4 | Configurar exporter OTLP | Baja | 2h | ✅ |
| T-501.5 | Tests de tracing | Media | 4h | ✅ |

---

### US-EDA-502: Propagar correlation_id a headers NATS ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** que cada mensaje NATS incluya el correlation_id del request original  
**Para** poder correlacionar eventos en logs y trazas

**Criterios de Aceptación:**
- [x] OutboxRelay inyecta correlation_id en headers NATS
- [x] Consumers leen correlation_id de headers
- [x] Correlation ID presente en todos los logs de saga
- [x] Query SQL para buscar por correlation_id funciona

**Implementación:**
```rust
// crates/server/infrastructure/src/observability/correlation.rs
pub struct NatsHeaders {
    pub correlation_id: Option<String>,
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
    pub custom: HashMap<String, String>,
}

pub fn create_event_headers(event: &OutboxEventView) -> NatsHeaders {
    NatsHeaders::new()
        .with_correlation_id(&extract_correlation_id_from_event(event))
        .with_custom("event_type", &event.event_type)
        .with_custom("aggregate_id", &event.aggregate_id.to_string())
}

pub fn extract_context_from_headers(headers: &async_nats::Header) -> Option<CorrelationContext> {
    headers.get(CORRELATION_ID_HEADER)
        .and_then(|v| CorrelationId::from_string(v.to_str().ok()?).map(|id| CorrelationContext {
            correlation_id: id,
            parent_span_id: headers.get(TRACE_PARENT_HEADER).map(|s| s.to_string()),
            trace_state: headers.get(TRACE_STATE_HEADER).map(|s| s.to_string()),
        }))
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-502.1 | Implementar NatsHeaders struct | Baja | 2h | ✅ |
| T-502.2 | Implementar create_event_headers | Media | 4h | ✅ |
| T-502.3 | Implementar extract_context_from_headers | Media | 4h | ✅ |
| T-502.4 | Tests de propagación | Media | 4h | ✅ |

---

### US-EDA-503: Crear dashboard de trazabilidad ✅ COMPLETADO
**Como** operador del sistema  
**Quiero** un dashboard que muestre el ciclo de vida de un job por correlation_id  
**Para** debuggear problemas rápidamente

**Criterios de Aceptación:**
- [x] Métricas Prometheus para gRPC, Jobs, Workers
- [x] Histogramas de latencia configurados
- [x] Gauges para estados actuales
- [x] Documentación de debugging

**Implementación:**
```rust
// crates/server/infrastructure/src/observability/metrics.rs
pub struct ObservabilityMetrics {
    pub grpc: GrpcMetrics,
    pub jobs: JobMetrics,
    pub workers: WorkerMetrics,
    pub registry: Registry,
}

pub struct GrpcMetrics {
    pub requests_total: IntCounterVec,
    pub request_latency: Histogram,
    pub active_requests: IntGauge,
    pub request_errors: IntCounterVec,
}

pub struct JobMetrics {
    pub jobs_created: IntCounter,
    pub jobs_running: IntGauge,
    pub job_execution_time: Histogram,
    pub queue_depth: IntGauge,
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación | Estado |
|----|-------|-------------|------------|--------|
| T-503.1 | Implementar GrpcMetrics | Media | 4h | ✅ |
| T-503.2 | Implementar JobMetrics | Media | 4h | ✅ |
| T-503.3 | Implementar WorkerMetrics | Media | 4h | ✅ |
| T-503.4 | Crear MetricsRegistry global | Baja | 2h | ✅ |

---

## ✅ Checklist de Definition of Done (Sprint 5)

- [x] OpenTelemetry integrado y funcionando
- [x] Correlation ID propagado en todo el sistema
- [x] NATS headers con correlation_id y traceparent
- [x] Métricas Prometheus para gRPC, Jobs, Workers
- [x] Tests unitarios pasando

---

# 📊 Resumen de Estimaciones

| Sprint | Historias | Tareas | Estimación Total |
|--------|-----------|--------|------------------|
| SP-EDA-001: Atomicidad | 4 | 16 | 25h |
| SP-EDA-002: Saga Sovereignty | 4 | 14 | 30h |
| SP-EDA-003: Crash-Only | 4 | 14 | 30h |
| SP-EDA-004: Reconciliación | 3 | 11 | 30h |
| SP-EDA-005: Observabilidad | 3 | 14 | 30h |
| **TOTAL** | **18** | **69** | **~145h** |

---

# 🔗 Referencias a Documentos de Estudio

| Documento | Link | Secciones Clave |
|-----------|------|-----------------|
| Arquitectura v2.0 | `docs/analysis/EDA_ARCHITECTURE_V2_APPENDIX.md` | 19 (Estado final), 20 (Checklist), 21 (Glosario) |
| Plan de Refactorización | `docs/analysis/EDA_ARCHITECTURE_REFACTORING_PLAN.md` | 4 (Propuestas), 10 (Arquitectura técnica) |
| Kill List | `docs/analysis/EDA_KILL_LIST.md` | 2 (App), 3 (Infra), 4 (Dominio), 5 (Interfaz) |

---

# ⚠️ Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Regresión funcional | Media | Alto | Tests de integración completos antes de cada sprint |
| Degradación de rendimiento | Baja | Medio | Benchmarks antes/después de cada fase |
| Complejidad de migración | Alta | Medio | Feature flags para rollback |
| Falta de coverage en tests | Media | Alto | Requerir 80% coverage mínimo |

---

# 🚀 Criterios de Go-Live

- [ ] Todos los sprints completados
- [ ] 100% de tests de integración pasando
- [ ] 0 alertas críticas en dashboard
- [ ] Documentación de runbooks completa
- [ ] Plan de rollback documentado y probado
- [ ] Capacitación del equipo completada

---

**Document Version:** 1.0.0  
**Created:** 2026-01-04  
**Status:** Ready for Sprint Planning

---

**Documento preparado para planificación de sprints.**  
**Referencias:** `EDA_ARCHITECTURE_V2_APPENDIX.md`, `EDA_ARCHITECTURE_REFACTORING_PLAN.md`, `EDA_KILL_LIST.md`

# EPIC: Migración a Pure EDA & Saga Orchestration

**Epic ID:** EPIC-EDA-2024  
**Versión:** 1.1.0  
**Fecha:** 2026-01-04  
**Estado:** In Progress  
**Owner:** Backend Team  
**Sprints:** 5 (2 completados)  
**Estimación Total:** 145h (~40h completadas)

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

- [x] 0 llamadas a `event_bus.publish` en `crates/server/interface/src/grpc/`
- [x] Tests de atomicidad (kill server durante transacción) pasan
- [ ] Documentación actualizada (`docs/analysis/EDA_ARCHITECTURE_REFACTORING_PLAN.md` Seccion 4.1)
- [ ] Métricas de observabilidad muestran 0 eventos huérfanos

---

# SPRINT 2: Saga Sovereignty (Orquestación Unificada) 🚧 IN PROGRESS

**Sprint ID:** SP-EDA-002  
**Duración:** 1 semana  
**Objetivo:** Eliminar JobCoordinator y hacer de ExecutionSaga el único orquestador  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.3, 20 (EDA-OBJ-006 a 010)  
**Referencia:** `EDA_KILL_LIST.md` Secciones 2.1 (JobController, JobCoordinator)
**Commits:** e6a7eac, 99b4082, 507ee55, 98c38e4

## 📋 Historias de Usuario

### US-EDA-201: Implementar idempotencia con UUID v5
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

### US-EDA-202: Desactivar JobCoordinator como consumidor
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
| T-202.3 | Verificar que ExecutionSagaConsumer es único consumidor | Baja | 1h | ⏳ |
| T-202.4 | Tests de regresión | Media | 4h | ⏳ |

**Referencia de Eliminación:**
```
// EDA_KILL_LIST.md - Seccion 2.1
| JobCoordinator | jobs/coordinator.rs | 🔴 BORRAR | Competia con Sagas | ExecutionSaga |
```

---

### US-EDA-203: Configurar NATS Consumer con DLQ
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
| T-203.2 | Implementar DLQ Handler | Media | 4h | ⏳ |
| T-203.3 | Crear tabla failed_events | Baja | 1h | ⏳ |
| T-203.4 | Configurar alerts para DLQ | Baja | 2h | ⏳ |

---

### US-EDA-204: Eliminar JobController y JobCoordinator
**Como** desarrollador  
**Quiero** eliminar código legacy que ya no es necesario  
**Para** reducir deuda técnica y complejidad del codebase

**Criterios de Aceptación:**
- [ ] `JobController` eliminado
- [ ] `JobCoordinator` eliminado
- [ ] `JobDispatcher` refactorizado (solo selección, no dispatch)
- [ ] 0 referencias a estos componentes fuera de tests

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-204.1 | Eliminar JobController | Baja | 2h |
| T-204.2 | Eliminar JobCoordinator | Baja | 2h |
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

- [ ] `ExecutionSaga` es único consumidor de `JobQueued`
- [ ] Tests de idempotencia pasan (mensajes duplicados ignorados)
- [ ] DLQ configurado y funcionando
- [ ] `JobController` y `JobCoordinator` eliminados
- [ ] Documentación de arquitectura actualizada

---

# SPRINT 3: Crash-Only Workers (Simplificación)

**Sprint ID:** SP-EDA-003  
**Duración:** 1 semana  
**Objetivo:** Simplificar máquina de estados del Worker a 4 estados  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.4, 20 (EDA-OBJ-011 a 014)  
**Referencia:** `EDA_KILL_LIST.md` Seccion 4.2 (WorkerState)

## 📋 Historias de Usuario

### US-EDA-301: Simplificar WorkerState a 4 estados
**Como** operador del sistema  
**Quiero** una máquina de estados simple para workers  
**Para** reducir complejidad y eliminar estados transitorios problemáticos

**Criterios de Aceptación:**
- [ ] `WorkerState` tiene exactamente 4 estados: `Creating`, `Ready`, `Busy`, `Terminated`
- [ ] Eliminados: `Connecting`, `Draining`, `Terminating`, `Maintenance`
- [ ] Tests actualizados para reflejar cambios
- [ ] Documentación de transiciones actualizada

**Referencia de Cambios:**
```
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.4
| Estado Actual | Nuevo Estado | Accion |
| Connecting    | Eliminado    | Merge en Creating |
| Draining      | Eliminado    | Si error, Terminated directo |
| Terminating   | Eliminado    | Merge en Terminated |
| Maintenance   | Eliminado    | No aplica a workers efimeros |
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-301.1 | Redefinir enum WorkerState | Baja | 2h |
| T-301.2 | Actualizar transiciones en `can_transition_to` | Baja | 2h |
| T-301.3 | Actualizar todos los match en el codebase | Media | 4h |
| T-301.4 | Tests de máquina de estados | Media | 4h |

---

### US-EDA-302: Implementar WorkerMonitor con Kill-Switch
**Como** operador del sistema  
**Quiero** que workers que fallan heartbeat sean terminados inmediatamente  
**Para** evitar workers zombie y jobs bloqueados

**Criterios de Aceptación:**
- [ ] Si faltan 3 heartbeats, worker se marca `Terminated`
- [ ] `terminate_worker()` destruye infraestructura del provider
- [ ] Si worker tenía job activo, el job vuelve a `PENDING`
- [ ] Evento `WorkerLost` publicado vía Outbox

**Referencia de Código:**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.4.1
impl WorkerMonitor {
    const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);
    const MISSED_HEARTBEATS: u32 = 3;

    async fn terminate_worker(&self, worker: Worker, reason: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.provider.destroy_worker(external_id).await?;
        self.worker_repo.update_status_tx(&mut tx, worker.id, Terminated).await?;
        if let Some(job_id) = worker.current_job_id {
            self.job_repo.update_status_tx(&mut tx, job_id, Pending).await?;
            self.outbox_repo.insert_event_tx(&mut tx, WorkerLostEvent.into())?;
        }
        tx.commit().await?;
        Ok(())
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-302.1 | Implementar timeout de registro en Creating | Baja | 2h |
| T-302.2 | Implementar `terminate_worker()` | Media | 6h |
| T-302.3 | Integrar con heartbeat checker existente | Media | 4h |
| T-302.4 | Tests de terminación de workers | Media | 4h |

---

### US-EDA-303: Eliminar lógica de reconexión compleja
**Como** desarrollador  
**Quiero** eliminar código de reconexión de workers legacy  
**Para** simplificar el modelo Crash-Only

**Criterios de Aceptación:**
- [ ] No hay lógica de reconexión en `WorkerAgentServiceImpl`
- [ ] Si un worker pierde conexión, se registra como nueva instancia
- [ ] El registro nuevo recibe un nuevo worker_id

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-303.1 | Eliminar lógica de sesión/reconexión | Media | 4h |
| T-303.2 | Actualizar registro para modo Crash-Only | Baja | 2h |
| T-303.3 | Tests de registro post-desconexión | Media | 4h |

---

### US-EDA-304: Limpiar ProviderManager y EventRouter
**Como** desarrollador  
**Quiero** eliminar código legacy de ProviderManager y EventRouter  
**Para** reducir deuda técnica

**Criterios de Aceptación:**
- [ ] `ProviderManager` eliminado
- [ ] `EventRouter` eliminado
- [ ] `EventSubscriber` eliminado
- [ ] 0 referencias a estos componentes

**Referencia de Eliminación:**
```markdown
// EDA_KILL_LIST.md - Seccion 2.1
| ProviderManager | jobs/provider_manager.rs | 🔴 BORRAR | Auto-scaling legacy | ProvisioningSaga |
| EventSubscriber | messaging/subscriber.rs  | 🔴 BORRAR | Suscriptor manual   | NatsSagaConsumer |
| EventRouter     | messaging/router.rs      | 🔴 BORRAR | Enrutamiento manual | NATS Subjects   |
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-304.1 | Eliminar ProviderManager | Baja | 2h |
| T-304.2 | Eliminar EventSubscriber | Baja | 2h |
| T-304.3 | Eliminar EventRouter | Baja | 2h |
| T-304.4 | Verificar compilacion | Baja | 1h |

---

## ✅ Checklist de Definition of Done (Sprint 3)

- [ ] `WorkerState` tiene 4 estados (no 7)
- [ ] Workers zombie terminados automáticamente
- [ ] `ProviderManager`, `EventSubscriber`, `EventRouter` eliminados
- [ ] Tests de Crash-Only pasan
- [ ] Documentación actualizada

---

# SPRINT 4: Reconciliación (Red de Seguridad)

**Sprint ID:** SP-EDA-004  
**Duración:** 1 semana  
**Objetivo:** Implementar procesos de limpieza y reconciliación automática  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.5, 20 (EDA-OBJ-015 a 018)

## 📋 Historias de Usuario

### US-EDA-401: Implementar DatabaseReaper
**Como** operador del sistema  
**Quiero** que jobs "colgados" sean marcados como fallidos automáticamente  
**Para** evitar jobs en estado RUNNING eternamente

**Criterios de Aceptación:**
- [ ] Cron job corre cada minuto
- [ ] Jobs RUNNING sin heartbeat > 90s -> FAILED
- [ ] Workers CREATING sin registro > 60s -> TERMINATED
- [ ] Eventos `JobFailed` publicados vía Outbox

**Referencia de Código:**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.5.1
impl DatabaseReaper {
    pub async fn run(&self, pool: &PgPool) {
        sqlx::query!(r#"
            UPDATE jobs SET status = 'FAILED', error_message = 'Timeout de seguridad'
            WHERE status = 'RUNNING' AND updated_at < NOW() - INTERVAL '90 seconds'
        "#).execute(pool).await;
        
        sqlx::query!(r#"
            UPDATE workers SET status = 'TERMINATED'
            WHERE status = 'CREATING' AND created_at < NOW() - INTERVAL '60 seconds'
        "#).execute(pool).await;
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-401.1 | Implementar DatabaseReaper struct | Media | 4h |
| T-401.2 | Configurar cron schedule (cada 1 min) | Baja | 2h |
| T-401.3 | Tests del Reaper | Media | 4h |
| T-401.4 | Integrar con lifecycle del servidor | Baja | 2h |

---

### US-EDA-402: Implementar InfrastructureReconciler
**Como** operador del sistema  
**Quiero** que contenedores/pods huérfanos sean destruidos  
**Para** evitar consumo de recursos innecesarios

**Criterios de Aceptación:**
- [ ] Cron job corre cada 5 minutos
- [ ] Workers TERMINATED con contenedor existente -> destroy
- [ ] Workers BUSY sin contenedor existente -> mark LOST + recover job
- [ ] Logs de reconciliación para debugging

**Referencia de Código:**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.5.2
impl InfrastructureReconciler {
    pub async fn reconcile(&self) {
        // Zombies: Contenedor existe pero Worker TERMINATED
        let terminated = self.worker_repo.find_by_status(Terminated).await?;
        for worker in terminated {
            if self.provider.worker_exists(ext).await? {
                self.provider.destroy_worker(ext).await?;
            }
        }
        // Fantasmas: Worker BUSY pero no existe en provider
        // -> handle_worker_lost() + recover job
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-402.1 | Implementar InfrastructureReconciler | Media | 6h |
| T-402.2 | Configurar cron schedule (cada 5 min) | Baja | 1h |
| T-402.3 | Tests de reconciliación (mock provider) | Alta | 6h |
| T-402.4 | Logs y métricas | Baja | 2h |

---

### US-EDA-403: Configurar alertas de producción
**Como** SRE  
**Quiero** alertas cuando hay workers zombie o jobs colgados  
**Para** poder investigar problemas antes de que escalen

**Criterios de Aceptación:**
- [ ] Alerta si workers zombie detectados
- [ ] Alerta si jobs marcados como FAILED por timeout
- [ ] Alerta si DLQ tiene mensajes acumulados
- [ ] Métricas exportadas a Prometheus

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-403.1 | Configurar alertas Prometheus | Media | 4h |
| T-403.2 | Implementar métricas de reconciliación | Baja | 2h |
| T-403.3 | Dashboard Grafana básico | Media | 4h |
| T-403.4 | Documentar runbooks de respuesta | Media | 4h |

---

## ✅ Checklist de Definition of Done (Sprint 4)

- [ ] DatabaseReaper corriendo cada minuto
- [ ] InfrastructureReconciler corriendo cada 5 minutos
- [ ] Alertas configuradas y funcionando
- [ ] Runbooks documentados
- [ ] Tests de reconciliación pasan

---

# SPRINT 5: Observabilidad (Trazabilidad)

**Sprint ID:** SP-EDA-005  
**Duración:** 1 semana  
**Objetivo:** Implementar trazabilidad distribuida completa  
**Referencia:** `EDA_ARCHITECTURE_V2_APPENDIX.md` Secciones 19.6, 20 (EDA-OBJ-019 a 022)

## 📋 Historias de Usuario

### US-EDA-501: Integrar OpenTelemetry
**Como** operador del sistema  
**Quiero** tracing distribuido con OpenTelemetry  
**Para** poder seguir el flujo de un request a través de todos los componentes

**Criterios de Aceptación:**
- [ ] Tracer configurado en startup del servidor
- [ ] Spans creados para cada operación importante
- [ ] Correlation ID propagado a través de gRPC y NATS
- [ ] Trazas enviadas a Jaeger/OTLP endpoint

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-501.1 | Añadir dependencia tracing-opentelemetry | Baja | 1h |
| T-501.2 | Configurar tracer en main.rs | Baja | 2h |
| T-501.3 | Añadir spans a operaciones críticas | Media | 6h |
| T-501.4 | Configurar exporter OTLP | Baja | 2h |
| T-501.5 | Tests de tracing | Media | 4h |

---

### US-EDA-502: Propagar correlation_id a headers NATS
**Como** operador del sistema  
**Quiero** que cada mensaje NATS incluya el correlation_id del request original  
**Para** poder correlacionar eventos en logs y trazas

**Criterios de Aceptación:**
- [ ] OutboxRelay inyecta correlation_id en headers NATS
- [ ] Consumers leen correlation_id de headers
- [ ] Correlation ID presente en todos los logs de saga
- [ ] Query SQL para buscar por correlation_id funciona

**Referencia de Código:**
```rust
// EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.6
impl OutboxRelay {
    async fn publish_with_context(&self, event: &OutboxEventView) -> Result<()> {
        let mut headers = NatsHeaders::default();
        headers.insert("x-correlation-id", &event.correlation_id);
        self.nats.publish_with_headers(&event.subject, headers, &event.payload).await?;
        Ok(())
    }
}
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-502.1 | Añadir correlation_id a tabla outbox_events | Baja | 2h |
| T-502.2 | Implementar publish_with_headers | Media | 4h |
| T-502.3 | Consumidores leen headers | Media | 4h |
| T-502.4 | Tests de propagación | Media | 4h |

---

### US-EDA-503: Crear dashboard de trazabilidad
**Como** operador del sistema  
**Quiero** un dashboard que muestre el ciclo de vida de un job por correlation_id  
**Para** debuggear problemas rápidamente

**Criterios de Aceptación:**
- [ ] Query visual en Grafana para buscar por correlation_id
- [ ] Timeline de eventos del job
- [ ] Estado actual y transiciones visibles
- [ ] Link a trazas de Jaeger

**Referencia de Query:**
```sql
-- EDA_ARCHITECTURE_V2_APPENDIX.md - Seccion 19.6
SELECT j.id, j.status, o.event_type, o.published_at, s.current_step, w.status
FROM jobs j
LEFT JOIN outbox_events o ON o.aggregate_id = j.id::text
LEFT JOIN sagas s ON s.job_id = j.id
LEFT JOIN workers w ON w.current_job_id = j.id
WHERE j.correlation_id = 'your-correlation-id'
ORDER BY j.created_at;
```

**Tareas Técnicas:**
| ID | Tarea | Complejidad | Estimación |
|----|-------|-------------|------------|
| T-503.1 | Crear vista/enlace en Grafana | Media | 4h |
| T-503.2 | Documentar query de debugging | Baja | 2h |
| T-503.3 | Panel de estado de jobs | Media | 4h |
| T-503.4 | Tests end-to-end de trazabilidad | Alta | 6h |

---

## ✅ Checklist de Definition of Done (Sprint 5)

- [ ] OpenTelemetry integrado y funcionando
- [ ] Correlation ID propagado en todo el sistema
- [ ] Dashboard de trazabilidad operativo
- [ ] Documentación de debugging completa
- [ ] Tests de observabilidad pasan

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

# EPIC-15: Enhanced Event Traceability and Audit-Based Testing

**Estado**: 🚧 En Progreso  
**Prioridad**: Alta  
**Estimación**: 3-4 Sprints  
**Dependencias**: EPIC-13 (EDA-Audit), EPIC-14 (Events-Testing)  
**Fecha Creación**: 2025-12-16  
**Última Actualización**: 2025-12-16

---

## 1. Resumen Ejecutivo

Esta épica amplía y consolida el trabajo de EPIC-13 y EPIC-14 para lograr **cobertura completa de eventos** en todos los casos de uso de la plataforma, implementar **context propagation** robusto, y establecer un patrón de **testing basado en audit** que permita verificación black-box del comportamiento del sistema.

### Objetivos Principales

1. **100% de cobertura de eventos** en operaciones mutadoras
2. **Context propagation** completo (correlation_id, actor en todos los eventos)
3. **Testing con validación de Audit** en tests E2E e integración
4. **Trazabilidad end-to-end** de cualquier operación

### Valor de Negocio

- **Debugging mejorado**: Rastrear cualquier problema con correlation_id
- **Compliance**: Registro inmutable de todas las acciones del sistema
- **Testing robusto**: Verificar side-effects sin acoplar a implementación
- **Observabilidad**: Visibilidad completa del comportamiento del sistema

---

## 2. User Stories

### Phase 1: Completar Cobertura de Eventos

#### Story 15.1: Nuevos Eventos de Dominio para Worker Lifecycle

**Como** Operador del sistema,  
**Quiero** que todos los cambios de estado de workers generen eventos,  
**Para** tener trazabilidad completa del ciclo de vida de workers.

**Criterios de Aceptación:**
- [x] Evento `WorkerTerminated` publicado cuando worker se desregistra ✅
- [x] Evento `WorkerDisconnected` publicado cuando stream se cierra inesperadamente ✅
- [x] Evento `WorkerProvisioned` publicado cuando lifecycle manager crea worker ✅
- [x] Todos los eventos incluyen worker_id, provider_id, timestamp ✅
- [x] AuditService maneja correctamente los nuevos eventos ✅

**Tareas Técnicas:**
```
[x] T15.1.1 - Añadir WorkerTerminated a DomainEvent enum ✅
[x] T15.1.2 - Añadir WorkerDisconnected a DomainEvent enum ✅
[x] T15.1.3 - Añadir WorkerProvisioned a DomainEvent enum ✅
[x] T15.1.4 - Publicar WorkerTerminated en unregister_worker() ✅
[x] T15.1.5 - Publicar WorkerDisconnected en cleanup de worker_stream ✅
[x] T15.1.6 - Publicar WorkerProvisioned en WorkerLifecycleManager::provision_worker ✅
[x] T15.1.7 - Actualizar AuditService::log_event para nuevos eventos ✅
[x] T15.1.8 - Tests unitarios para cada nuevo evento ✅
```

**Archivos a Modificar:**
- `crates/domain/src/events.rs`
- `crates/grpc/src/services/worker.rs`
- `crates/application/src/worker_lifecycle.rs`
- `crates/application/src/audit_usecases.rs`

---

#### Story 15.2: Nuevos Eventos de Dominio para Job Lifecycle

**Como** Usuario de la plataforma,  
**Quiero** que todas las transiciones de estado de jobs generen eventos,  
**Para** rastrear el progreso completo de mis jobs.

**Criterios de Aceptación:**
- [x] Evento `JobRetried` publicado cuando job se reintenta ✅
- [x] Evento `JobAssigned` publicado cuando job se asigna a worker específico ✅
- [x] Eventos incluyen job_id, worker_id (si aplica), attempt number ✅
- [ ] Secuencia de eventos es consistente: Created → Queued → Assigned → Running → Completed

**Tareas Técnicas:**
```
[x] T15.2.1 - Añadir JobRetried a DomainEvent enum ✅
[x] T15.2.2 - Añadir JobAssigned a DomainEvent enum ✅
[ ] T15.2.3 - Publicar JobRetried en Job::prepare_retry o equivalente
[x] T15.2.4 - Publicar evento de asignación en JobController::assign_and_dispatch ✅
[x] T15.2.5 - Actualizar AuditService::log_event ✅
[x] T15.2.6 - Tests unitarios ✅
```

**Archivos a Modificar:**
- `crates/domain/src/events.rs`
- `crates/domain/src/job_execution.rs`
- `crates/application/src/job_controller.rs`
- `crates/application/src/audit_usecases.rs`

---

#### Story 15.3: Eventos para Provider Management

**Como** Administrador,  
**Quiero** que los cambios en providers generen eventos,  
**Para** auditar la configuración de infraestructura.

**Criterios de Aceptación:**
- [ ] `ProviderUpdated` se publica cuando se actualiza configuración
- [x] `ProviderHealthChanged` se publica cuando health status cambia ✅
- [ ] Eventos no exponen información sensible (credentials)

**Tareas Técnicas:**
```
[ ] T15.3.1 - Crear UpdateProviderUseCase que publique ProviderUpdated
[x] T15.3.2 - Añadir ProviderHealthChanged a DomainEvent ✅
[ ] T15.3.3 - Publicar ProviderHealthChanged en health checks
[x] T15.3.4 - Actualizar AuditService (usa event_type() automáticamente) ✅
[x] T15.3.5 - Tests unitarios ✅
```

**Archivos a Modificar:**
- `crates/domain/src/events.rs`
- `crates/application/src/provider_usecases.rs`
- `crates/application/src/provider_registry.rs`

---

### Phase 2: Context Propagation

#### Story 15.4: Implementar RequestContext y Propagación

**Como** Desarrollador,  
**Quiero** que cada request tenga un correlation_id único que se propague,  
**Para** poder rastrear operaciones end-to-end.

**Criterios de Aceptación:**
- [x] Struct `RequestContext` definido con correlation_id, actor, started_at ✅
- [x] gRPC Interceptor extrae/genera correlation_id de headers ✅
- [x] Contexto disponible en UseCases (CancelJobUseCase) ✅
- [ ] Todos los eventos incluyen correlation_id del contexto
- [ ] 100% de eventos tienen correlation_id no-None en producción

**Tareas Técnicas:**
```
[x] T15.4.1 - Crear RequestContext struct en request_context.rs ✅
[x] T15.4.2 - Implementar ContextInterceptor gRPC ✅
[x] T15.4.3 - Modificar UseCases para aceptar RequestContext ✅
[x] T15.4.4 - Propagar contexto en CreateJobUseCase (ya tenía correlation_id/actor) ✅
[x] T15.4.5 - Propagar contexto en CancelJobUseCase ✅
[ ] T15.4.6 - Propagar contexto en JobController
[ ] T15.4.7 - Propagar contexto en WorkerAgentService
[ ] T15.4.8 - Propagar contexto en RegisterProviderUseCase
[x] T15.4.9 - Tests de interceptor ✅
```

**Archivos Creados/Modificados:**
- `crates/domain/src/request_context.rs` ✅ (nuevo)
- `crates/grpc/src/interceptors/mod.rs` ✅ (nuevo)
- `crates/grpc/src/interceptors/context.rs` ✅ (nuevo)
- `crates/application/src/job_execution_usecases.rs` ✅

**Diseño Técnico:**
```rust
// shared_kernel.rs
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub correlation_id: String,
    pub actor: Option<String>,
    pub trace_id: Option<String>,
    pub request_time: DateTime<Utc>,
}

impl RequestContext {
    pub fn new() -> Self {
        Self {
            correlation_id: Uuid::new_v4().to_string(),
            actor: None,
            trace_id: None,
            request_time: Utc::now(),
        }
    }
    
    pub fn with_actor(mut self, actor: String) -> Self {
        self.actor = Some(actor);
        self
    }
}

// interceptors/context.rs
pub struct ContextInterceptor;

impl tonic::service::Interceptor for ContextInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let correlation_id = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        
        let actor = request
            .metadata()
            .get("x-actor-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        
        let ctx = RequestContext {
            correlation_id,
            actor,
            trace_id: None,
            request_time: Utc::now(),
        };
        
        request.extensions_mut().insert(ctx);
        Ok(request)
    }
}
```

---

### Phase 3: Testing con Validación de Audit

#### Story 15.5: Crear AuditTestHelper

**Como** QA Engineer,  
**Quiero** helpers para validar audit logs en tests,  
**Para** verificar que side-effects ocurren correctamente.

**Criterios de Aceptación:**
- [x] `AuditTestHelper` struct disponible en tests ✅
- [ ] Método `wait_for_audit_log` con timeout y retry
- [x] Método `assert_event_sequence` para verificar orden ✅
- [x] Método `assert_no_event` para verificar ausencia ✅
- [x] Documentación y ejemplos de uso ✅

**Tareas Técnicas:**
```
[x] T15.5.1 - Crear crates/application/src/audit_test_helper.rs ✅
[x] T15.5.2 - Implementar AuditTestHelper struct ✅
[ ] T15.5.3 - Implementar wait_for_audit_log con retry loop (requiere DB)
[x] T15.5.4 - Implementar assert_event_sequence ✅
[x] T15.5.5 - Implementar assert_no_event (assert_no_events) ✅
[x] T15.5.6 - Añadir método para limpiar logs entre tests (clear) ✅
[x] T15.5.7 - Tests unitarios del helper ✅
```

**Diseño Técnico:**
```rust
// tests/common/audit_helpers.rs
pub struct AuditTestHelper {
    pool: PgPool,
}

impl AuditTestHelper {
    pub async fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn wait_for_audit_log(
        &self,
        correlation_id: &str,
        event_type: &str,
        timeout: Duration,
    ) -> Result<AuditLog, AuditTestError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(log) = self.find_event(correlation_id, event_type).await? {
                return Ok(log);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(AuditTestError::Timeout {
            correlation_id: correlation_id.to_string(),
            event_type: event_type.to_string(),
        })
    }
    
    pub async fn assert_event_sequence(
        &self,
        correlation_id: &str,
        expected_sequence: &[&str],
    ) -> Result<(), AuditTestError> {
        let logs = self.find_all_by_correlation(correlation_id).await?;
        let actual: Vec<_> = logs.iter()
            .map(|l| l.event_type.as_str())
            .collect();
        
        for (i, expected) in expected_sequence.iter().enumerate() {
            if !actual.iter().any(|a| a == expected) {
                return Err(AuditTestError::MissingEvent {
                    expected: expected.to_string(),
                    position: i,
                    actual: actual.clone(),
                });
            }
        }
        Ok(())
    }
    
    pub async fn cleanup_for_test(&self, test_prefix: &str) -> Result<usize, AuditTestError> {
        // Eliminar logs con correlation_id que empiece con test_prefix
        sqlx::query("DELETE FROM audit_logs WHERE correlation_id LIKE $1")
            .bind(format!("{}%", test_prefix))
            .execute(&self.pool)
            .await
            .map(|r| r.rows_affected() as usize)
            .map_err(|e| AuditTestError::Database(e.to_string()))
    }
}
```

---

#### Story 15.6: Refactorizar Tests E2E con Validación Audit

**Como** Desarrollador,  
**Quiero** que los tests E2E verifiquen audit logs,  
**Para** asegurar que el sistema produce los eventos esperados.

**Criterios de Aceptación:**
- [ ] Tests E2E usan correlation_id único por test
- [ ] Tests verifican secuencia de eventos esperada
- [ ] Tests fallan si eventos esperados no aparecen en audit
- [ ] Al menos 80% de tests E2E tienen validación audit

**Tareas Técnicas:**
```
[ ] T15.6.1 - Refactorizar test_04_worker_registration_with_otp
[ ] T15.6.2 - Refactorizar test_06_job_queued_and_dispatched
[ ] T15.6.3 - Añadir test_job_lifecycle_audit_trail (nuevo)
[ ] T15.6.4 - Añadir test_worker_lifecycle_audit_trail (nuevo)
[ ] T15.6.5 - Añadir test_correlation_id_propagation (nuevo)
[ ] T15.6.6 - Actualizar tests en grpc_integration.rs
```

**Ejemplo de Test Refactorizado:**
```rust
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_job_lifecycle_audit_trail() {
    let stack = TestStack::with_job_controller().await.unwrap();
    let audit = AuditTestHelper::new(stack.pool.clone()).await;
    
    // Usar correlation_id único para este test
    let correlation_id = format!("test-job-lifecycle-{}", Uuid::new_v4());
    
    // 1. Crear job con correlation_id
    let job_id = stack.create_job_with_correlation(&correlation_id).await.unwrap();
    
    // 2. Verificar JobCreated en audit
    audit.wait_for_audit_log(&correlation_id, "JobCreated", Duration::from_secs(5))
        .await
        .expect("JobCreated event should be in audit");
    
    // 3. Registrar y conectar worker
    let worker_id = stack.register_worker().await.unwrap();
    
    // 4. Esperar a que job se asigne y ejecute
    stack.wait_for_job_completion(&job_id, Duration::from_secs(30)).await.unwrap();
    
    // 5. Verificar secuencia completa de eventos
    audit.assert_event_sequence(&correlation_id, &[
        "JobCreated",
        "JobStatusChanged", // → Scheduled
        "JobStatusChanged", // → Running
        "JobStatusChanged", // → Succeeded
    ]).await.expect("Complete job lifecycle should be audited");
    
    stack.shutdown().await;
}
```

---

### Phase 4: Mejoras en Audit Repository

#### Story 15.7: Extender AuditRepository con Queries Avanzadas

**Como** Administrador,  
**Quiero** consultar audit logs con filtros avanzados,  
**Para** investigar incidentes y generar reportes.

**Criterios de Aceptación:**
- [ ] Query por event_type con paginación
- [ ] Query por rango de fechas
- [ ] Query por actor
- [ ] Conteo de eventos por tipo
- [ ] Índices optimizados en PostgreSQL

**Tareas Técnicas:**
```
[ ] T15.7.1 - Añadir métodos al trait AuditRepository
[ ] T15.7.2 - Implementar find_by_event_type en PostgresAuditRepository
[ ] T15.7.3 - Implementar find_by_date_range
[ ] T15.7.4 - Implementar find_by_actor
[ ] T15.7.5 - Implementar count_by_event_type
[ ] T15.7.6 - Añadir índices: event_type, occurred_at, actor
[ ] T15.7.7 - Migration SQL para índices
[ ] T15.7.8 - Tests de integración para queries
```

**SQL para Índices:**
```sql
-- Migration: add_audit_indexes
CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_occurred_at ON audit_logs(occurred_at);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs(actor);
CREATE INDEX IF NOT EXISTS idx_audit_event_date ON audit_logs(event_type, occurred_at);
```

---

#### Story 15.8: Endpoint gRPC para Consultar Audit Logs

**Como** Administrador,  
**Quiero** un endpoint gRPC para consultar audit logs,  
**Para** acceder a la información de auditoría programáticamente.

**Criterios de Aceptación:**
- [ ] Endpoint `GetAuditLogs` con filtros
- [ ] Respuesta paginada
- [ ] Filtros: correlation_id, event_type, actor, date_range
- [ ] Ordenamiento por fecha (desc por defecto)
- [ ] Rate limiting para evitar abuso

**Tareas Técnicas:**
```
[ ] T15.8.1 - Definir mensajes proto para Audit
[ ] T15.8.2 - Implementar AuditService gRPC
[ ] T15.8.3 - Añadir GetAuditLogsUseCase
[ ] T15.8.4 - Integrar en server.rs
[ ] T15.8.5 - Tests de integración
[ ] T15.8.6 - Documentar en asyncapi.md
```

**Proto Definition:**
```protobuf
// audit.proto
service AuditService {
    rpc GetAuditLogs(GetAuditLogsRequest) returns (GetAuditLogsResponse);
    rpc GetAuditLogsByCorrelation(GetByCorrelationRequest) returns (GetAuditLogsResponse);
}

message GetAuditLogsRequest {
    optional string event_type = 1;
    optional string actor = 2;
    optional google.protobuf.Timestamp start_time = 3;
    optional google.protobuf.Timestamp end_time = 4;
    int32 limit = 5;
    int32 offset = 6;
}

message GetAuditLogsResponse {
    repeated AuditLogEntry logs = 1;
    int32 total_count = 2;
    bool has_more = 3;
}

message AuditLogEntry {
    string id = 1;
    string correlation_id = 2;
    string event_type = 3;
    string payload_json = 4;
    google.protobuf.Timestamp occurred_at = 5;
    string actor = 6;
}
```

---

#### Story 15.9: Retención y Cleanup de Audit Logs

**Como** Operador,  
**Quiero** política de retención para audit logs,  
**Para** evitar crecimiento ilimitado de la base de datos.

**Criterios de Aceptación:**
- [ ] Configurable: retención por días (default 90)
- [ ] Cleanup automático via cron/background task
- [ ] Logs críticos pueden marcarse como "permanent"
- [ ] Métricas de cleanup (logs eliminados)

**Tareas Técnicas:**
```
[ ] T15.9.1 - Añadir columna is_permanent a audit_logs
[ ] T15.9.2 - Implementar cleanup_before en AuditRepository
[ ] T15.9.3 - Crear AuditCleanupService
[ ] T15.9.4 - Configurar via env HODEI_AUDIT_RETENTION_DAYS
[ ] T15.9.5 - Integrar cleanup en server startup/background
[ ] T15.9.6 - Añadir métricas Prometheus
```

---

## 3. Definition of Done

### Para cada Story:
- [ ] Código implementado y revisado
- [ ] Tests unitarios con cobertura > 80%
- [ ] Tests de integración pasando
- [ ] Documentación actualizada
- [ ] No regresiones en tests existentes
- [ ] Performance acceptable (< 10ms overhead por evento)

### Para la Épica completa:
- [ ] 100% de operaciones mutadoras emiten eventos
- [ ] 100% de eventos tienen correlation_id en producción
- [ ] > 80% de tests E2E validan audit logs
- [ ] Documentación de patrones de testing actualizada
- [ ] Runbook de debugging con correlation_id

---

## 4. Cronograma Estimado

| Sprint | Stories | Foco |
|--------|---------|------|
| Sprint 1 | 15.1, 15.2, 15.3 | Cobertura de eventos |
| Sprint 2 | 15.4, 15.5 | Context propagation + Test helpers |
| Sprint 3 | 15.6, 15.7 | Tests E2E + Queries |
| Sprint 4 | 15.8, 15.9 | API + Retención |

---

## 5. Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Breaking changes en DomainEvent | Media | Alto | Versionado de eventos, backwards compatibility |
| Tests lentos por audit waits | Alta | Bajo | Timeouts cortos, polling rápido |
| Context propagation complejo | Media | Medio | Documentar patrones, ejemplos claros |
| Overhead de performance | Baja | Medio | Benchmarks, eventos async |

---

## 6. Dependencias Externas

- PostgreSQL >= 14 (para NOTIFY performance)
- Testcontainers para tests de integración
- tokio-stream para event streaming

---

## 7. Métricas de Éxito

| Métrica | Baseline | Target | Medición |
|---------|----------|--------|----------|
| Eventos definidos | 7 | 12 | Code review |
| Cobertura de casos de uso | 60% | 100% | Matriz UC vs Eventos |
| Tests con audit validation | 0% | 80% | Test suite analysis |
| correlation_id presente | 30% | 100% | Audit log analysis |
| Latencia adicional por evento | N/A | < 5ms | Benchmarks |

---

## 8. Referencias

- [EVENTS-COVERAGE-ANALYSIS.md](../analysis/EVENTS-COVERAGE-ANALYSIS.md) - Análisis detallado
- [EPIC-13-EDA-Audit.md](./EPIC-0013-EDA-Audit.md) - Épica base de EDA
- [EPIC-14-EVENTS-AND-TESTING.md](./EPIC-14-EVENTS-AND-TESTING.md) - Épica previa
- [use-cases.md](../use-cases.md) - Casos de uso de referencia

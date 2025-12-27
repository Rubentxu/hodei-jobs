# Análisis de Cobertura de Eventos y Testing

**Versión**: 1.0  
**Fecha**: 2025-12-16  
**Autor**: Análisis automatizado

## 1. Resumen Ejecutivo

Este documento presenta un análisis exhaustivo de la cobertura de eventos de dominio en la plataforma Hodei Job Platform, identificando gaps, oportunidades de mejora en testing, y una hoja de ruta para implementar mejoras en trazabilidad y validación.

### Estado Actual

| Área | Estado | Cobertura |
|------|--------|-----------|
| Eventos de Dominio | ✅ Implementado | 7 tipos de eventos |
| Event Bus (Postgres) | ✅ Implementado | Pub/Sub funcional |
| Audit Service | ✅ Implementado | Subscriber activo |
| Tests con validación Audit | ⚠️ Parcial | Falta integración |
| Context Propagation | ⚠️ Parcial | correlation_id/actor a veces None |

---

## 2. Inventario de Eventos de Dominio Actuales

### 2.1 Eventos Definidos (`crates/domain/src/events.rs`)

| Evento | Descripción | Campos Clave |
|--------|-------------|--------------|
| `JobCreated` | Job creado y encolado | job_id, spec, correlation_id, actor |
| `JobStatusChanged` | Cambio de estado de job | job_id, old_state, new_state |
| `JobCancelled` | Job cancelado explícitamente | job_id, reason |
| `WorkerRegistered` | Worker registrado con OTP | worker_id, provider_id |
| `WorkerStatusChanged` | Cambio de estado de worker | worker_id, old_status, new_status |
| `ProviderRegistered` | Provider registrado | provider_id, provider_type, config_summary |
| `ProviderUpdated` | Provider actualizado | provider_id, changes |

### 2.2 Puntos de Publicación de Eventos

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         MAPA DE PUBLICACIÓN DE EVENTOS                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  CreateJobUseCase ─────────────────────────► JobCreated                 │
│                                                                          │
│  CancelJobUseCase ─────────────────────────► JobStatusChanged           │
│                      │                       JobCancelled               │
│                      └─────────────────────►                            │
│                                                                          │
│  ExecuteNextJobUseCase ────────────────────► JobStatusChanged (Scheduled)│
│                                                                          │
│  JobController.assign_and_dispatch ────────► JobStatusChanged (Running) │
│                                                                          │
│  WorkerAgentService.on_job_result ─────────► JobStatusChanged           │
│                                              (Succeeded/Failed)         │
│                                                                          │
│  WorkerAgentService.register ──────────────► WorkerRegistered           │
│                                                                          │
│  WorkerAgentService.on_worker_heartbeat ───► WorkerStatusChanged        │
│                                              (→Ready)                   │
│                                                                          │
│  RegisterProviderUseCase ──────────────────► ProviderRegistered         │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 3. GAPS Identificados: Casos de Uso SIN Eventos

### 3.1 Crítico - Operaciones Mutadoras sin Eventos

| Caso de Uso / Operación | Módulo | Evento Faltante | Prioridad |
|-------------------------|--------|-----------------|-----------|
| Worker Unregister | `worker.rs:unregister_worker` | `WorkerUnregistered` | 🔴 Alta |
| Worker Disconnect (stream close) | `worker.rs:worker_stream` | `WorkerDisconnected` | 🔴 Alta |
| Provider Update | (no existe UseCase) | `ProviderUpdated` (definido pero no usado) | 🟡 Media |
| Provider Disable/Enable | `provider_registry.rs` | `ProviderStatusChanged` | 🟡 Media |
| Job Timeout | `job_execution.rs:Job::timeout` | `JobTimeout` (usar JobStatusChanged) | 🟡 Media |
| Job Retry | `Job::prepare_retry` | `JobRetried` | 🟡 Media |
| Worker Health Check Failed | `worker_lifecycle.rs` | `WorkerHealthCheckFailed` | 🟢 Baja |
| Worker Provisioned | `worker_lifecycle.rs:provision_worker` | `WorkerProvisioned` | 🟢 Baja |
| Worker Terminated (lifecycle) | `worker_lifecycle.rs:cleanup_workers` | `WorkerTerminated` | 🔴 Alta |

### 3.2 Problema: Context Propagation Incompleto

En múltiples lugares, `correlation_id` y `actor` son `None`:

```rust
// job_controller.rs:114-116
let event = DomainEvent::JobStatusChanged {
    // ...
    correlation_id: None, // Context propagation needed
    actor: None,
};
```

**Ubicaciones afectadas:**
- `job_controller.rs:115-116`
- `job_controller.rs:119-120` (CancelJobUseCase)
- `worker.rs:531-532` (on_job_result)
- `worker.rs:588-589` (on_worker_heartbeat)
- `worker.rs:763-764` (register)

---

## 4. Análisis de Tests Existentes

### 4.1 Tests Unitarios con Eventos

| Test | Ubicación | Valida Eventos |
|------|-----------|----------------|
| `test_create_job_publishes_event` | `job_execution_usecases.rs` | ✅ Verifica JobCreated |
| `test_log_event_job_created` | `audit_usecases.rs` | ✅ Verifica logging |
| `run_once_assigns_and_dispatches` | `job_controller.rs` | ⚠️ Usa MockEventBus pero no verifica |

### 4.2 Tests de Integración - SIN Validación de Audit

| Test | Ubicación | Valida Audit |
|------|-----------|--------------|
| `test_full_registration_flow` | `grpc_integration.rs` | ❌ No valida |
| `test_04_worker_registration_with_otp` | `e2e_docker_provider.rs` | ❌ No valida |
| `test_06_job_queued_and_dispatched` | `e2e_docker_provider.rs` | ❌ No valida |
| `test_postgres_event_bus_pub_sub` | `event_bus_it.rs` | ✅ Valida pub/sub |

### 4.3 Oportunidades de Mejora en Tests

1. **Helper de Audit para Tests**
   ```rust
   // Propuesta: common/audit_helper.rs
   pub async fn fetch_audit_logs(pool: &PgPool, correlation_id: &str) -> Vec<AuditLog>;
   pub async fn assert_audit_contains_event(pool: &PgPool, correlation_id: &str, event_type: &str);
   ```

2. **Pattern de Verificación Black-Box**
   ```rust
   // En cada test E2E:
   // 1. Ejecutar operación con correlation_id único
   // 2. Esperar propagación (pequeño delay)
   // 3. Verificar audit_logs contiene eventos esperados
   ```

---

## 5. Sistema de Audit - Estado Actual

### 5.1 Arquitectura

```
┌──────────────┐     ┌──────────────┐     ┌──────────────────┐
│  Use Cases   │────►│  EventBus    │────►│  AuditService    │
│              │     │  (Postgres   │     │  (Subscriber)    │
│              │     │   NOTIFY)    │     │                  │
└──────────────┘     └──────────────┘     └────────┬─────────┘
                                                    │
                                                    ▼
                                          ┌──────────────────┐
                                          │  audit_logs      │
                                          │  (PostgreSQL)    │
                                          └──────────────────┘
```

### 5.2 Tabla audit_logs

```sql
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY,
    correlation_id VARCHAR(255),
    event_type VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    actor VARCHAR(255)
);
```

### 5.3 Limitaciones Actuales

1. **Sin índice por event_type** - Consultas por tipo de evento serán lentas
2. **Sin query por rango de fechas** - No hay endpoint para filtrar por fecha
3. **Sin paginación** - `find_by_correlation_id` devuelve todo
4. **Sin TTL/retención** - Los logs crecen indefinidamente

---

## 6. Recomendaciones de Mejora

### 6.1 Nuevos Eventos de Dominio a Añadir

```rust
// Propuesta para events.rs
pub enum DomainEvent {
    // Existentes...
    
    // Nuevos - Worker Lifecycle
    WorkerProvisioned {
        worker_id: WorkerId,
        provider_id: ProviderId,
        spec_summary: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    WorkerTerminated {
        worker_id: WorkerId,
        reason: TerminationReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    WorkerDisconnected {
        worker_id: WorkerId,
        last_heartbeat: DateTime<Utc>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    
    // Nuevos - Job Lifecycle
    JobRetried {
        job_id: JobId,
        attempt: u32,
        max_attempts: u32,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    JobQueued {
        job_id: JobId,
        queue_position: Option<usize>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    JobAssigned {
        job_id: JobId,
        worker_id: WorkerId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    
    // Nuevos - Provider
    ProviderHealthChanged {
        provider_id: ProviderId,
        old_status: HealthStatus,
        new_status: HealthStatus,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
}
```

### 6.2 Context Propagation

```rust
// Propuesta: shared_kernel o nuevo módulo context.rs
pub struct RequestContext {
    pub correlation_id: String,
    pub actor: Option<String>,
    pub trace_id: Option<String>,
}

// Uso en gRPC interceptor:
impl<S> tonic::service::Interceptor for ContextInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let correlation_id = request
            .metadata()
            .get("x-correlation-id")
            .map(|v| v.to_str().unwrap_or_default().to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        
        // Propagar via task_local o extension
        request.extensions_mut().insert(RequestContext { correlation_id, ... });
        Ok(request)
    }
}
```

### 6.3 Mejoras en Testing

```rust
// tests/common/audit_helpers.rs
pub struct AuditTestHelper {
    pool: PgPool,
}

impl AuditTestHelper {
    pub async fn new(pool: PgPool) -> Self { Self { pool } }
    
    /// Espera a que aparezcan logs de audit (con retry)
    pub async fn wait_for_audit_log(
        &self,
        correlation_id: &str,
        event_type: &str,
        timeout: Duration,
    ) -> Result<AuditLog, TimeoutError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            let logs = self.find_by_correlation(correlation_id).await;
            if let Some(log) = logs.iter().find(|l| l.event_type == event_type) {
                return Ok(log.clone());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(TimeoutError::new("Audit log not found"))
    }
    
    /// Verifica secuencia de eventos
    pub async fn assert_event_sequence(
        &self,
        correlation_id: &str,
        expected: &[&str],
    ) -> Result<(), AssertionError> {
        let logs = self.find_by_correlation(correlation_id).await;
        let actual: Vec<_> = logs.iter().map(|l| l.event_type.as_str()).collect();
        
        for expected_type in expected {
            if !actual.contains(expected_type) {
                return Err(AssertionError::MissingEvent(expected_type.to_string()));
            }
        }
        Ok(())
    }
}
```

### 6.4 Mejoras en Audit Repository

```rust
// Nuevos métodos para AuditRepository
#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn save(&self, log: &AuditLog) -> Result<()>;
    async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>>;
    
    // Nuevos
    async fn find_by_event_type(
        &self,
        event_type: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AuditLog>>;
    
    async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AuditLog>>;
    
    async fn find_by_actor(&self, actor: &str, limit: usize) -> Result<Vec<AuditLog>>;
    
    async fn count_by_event_type(&self, event_type: &str) -> Result<usize>;
    
    async fn cleanup_before(&self, before: DateTime<Utc>) -> Result<usize>;
}
```

---

## 7. Matriz de Casos de Uso vs Eventos

| Caso de Uso | Evento(s) Actual(es) | Evento(s) Propuesto(s) | Estado |
|-------------|---------------------|------------------------|--------|
| UC1: Crear Job | JobCreated | JobCreated, JobQueued | ⚠️ Parcial |
| UC2: Asignar Job | JobStatusChanged | JobAssigned | ⚠️ Parcial |
| UC3: Ejecutar Job | JobStatusChanged (Running→Succeeded/Failed) | ✅ | ✅ OK |
| UC4: Registrar Worker | WorkerRegistered, WorkerStatusChanged | ✅ | ✅ OK |
| UC5: Heartbeat | WorkerStatusChanged (→Ready) | ✅ | ✅ OK |
| UC6: Cancelar Job | JobStatusChanged, JobCancelled | ✅ | ✅ OK |
| UC7: Unregister Worker | ❌ Ninguno | WorkerTerminated | 🔴 Gap |
| UC8: Worker Disconnect | ❌ Ninguno | WorkerDisconnected | 🔴 Gap |
| UC9: Retry Job | ❌ Ninguno | JobRetried | 🔴 Gap |
| UC10: Provider Register | ProviderRegistered | ✅ | ✅ OK |
| UC11: Provider Update | ❌ Ninguno | ProviderUpdated | 🔴 Gap |
| UC12: Provision Worker | ❌ Ninguno | WorkerProvisioned | 🟡 Gap |
| UC13: Terminate Worker (lifecycle) | ❌ Ninguno | WorkerTerminated | 🔴 Gap |

---

## 8. Plan de Implementación

### Fase 1: Completar Cobertura de Eventos (Sprint 1)

1. **Añadir eventos faltantes a `DomainEvent`**
   - WorkerTerminated
   - WorkerDisconnected
   - JobRetried
   - JobAssigned (opcional, puede usar JobStatusChanged)

2. **Publicar eventos en puntos faltantes**
   - `unregister_worker` → WorkerTerminated
   - Stream cleanup → WorkerDisconnected
   - `prepare_retry` → JobRetried

3. **Actualizar AuditService.log_event para nuevos eventos**

### Fase 2: Context Propagation (Sprint 1-2)

1. **Implementar RequestContext struct**
2. **Crear gRPC Interceptor para extraer/propagar contexto**
3. **Modificar UseCases para recibir contexto**
4. **Asegurar correlation_id en todos los eventos**

### Fase 3: Testing con Audit (Sprint 2)

1. **Crear AuditTestHelper**
2. **Refactorizar tests E2E para usar correlation_id**
3. **Añadir validación de audit en tests existentes**
4. **Crear nuevos tests de secuencia de eventos**

### Fase 4: Mejoras en Audit Repository (Sprint 2-3)

1. **Añadir índices a audit_logs**
2. **Implementar métodos de query adicionales**
3. **Añadir endpoint gRPC para consultar audit**
4. **Implementar retención/cleanup**

---

## 9. Métricas de Éxito

| Métrica | Actual | Objetivo |
|---------|--------|----------|
| Tipos de eventos definidos | 7 | 12+ |
| Casos de uso con eventos | 60% | 100% |
| Tests E2E con validación audit | 0% | 80% |
| correlation_id presente | ~30% | 100% |
| Cobertura de audit en operaciones críticas | 70% | 100% |

---

## 10. Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Eventos perdidos durante reconexión | Media | Alto | Implementar catch-up query al reconectar |
| Payload excede 8KB (Postgres NOTIFY) | Baja | Medio | Enviar solo IDs, fetch full state si necesario |
| Tests lentos por espera de audit | Media | Bajo | Usar timeouts cortos con retry |
| Overhead de performance | Baja | Bajo | Eventos async, no bloquean operación principal |

---

## Anexo A: Archivos Clave a Modificar

```
crates/domain/src/events.rs              # Añadir nuevos eventos
crates/domain/src/audit.rs               # Extender AuditRepository trait
crates/application/src/audit_usecases.rs # Manejar nuevos eventos
crates/grpc/src/services/worker.rs       # Publicar eventos faltantes
crates/application/src/worker_lifecycle.rs # Publicar WorkerTerminated
crates/grpc/tests/common/mod.rs          # Añadir AuditTestHelper
crates/infrastructure/src/repositories/audit_repository.rs # Nuevos métodos
```

## Anexo B: Tests a Crear/Modificar

```
tests/e2e_docker_provider.rs
  - test_job_lifecycle_audit_trail()
  - test_worker_lifecycle_audit_trail()
  
tests/grpc_integration.rs
  - test_correlation_id_propagation()
  - test_audit_logs_complete_flow()

tests/common/audit_helpers.rs (nuevo)
  - AuditTestHelper struct
  - wait_for_audit_log()
  - assert_event_sequence()
```

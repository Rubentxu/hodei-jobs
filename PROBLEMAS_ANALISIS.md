# Reporte de Problemas - Hodei Job Platform
## Fecha: 19 de diciembre de 2025 (Actualizado)

## Estado Actual
- ✅ Servidor gRPC funcionando con Docker provider configurado
- ✅ Jobs se crean y guardan en tabla `jobs`
- ✅ Jobs se encolan atómicamente en tabla `job_queue`
- ✅ JobController procesa jobs pendientes
- ✅ Workers se provisionan automáticamente
- ✅ EventBus integrado con pg_notify
- ⚠️ Gestión de secretos requiere mejora (ver EPIC-16)

## Problemas Resueltos

### 1. ✅ Jobs no se encolan (`job_queue` vacía) - RESUELTO
**Descripción original:**
- CLI reportaba "Job queued successfully"
- Jobs aparecían en tabla `jobs` con estado PENDING
- Pero tabla `job_queue` permanecía vacía

**Solución implementada:**
- `PostgresJobRepository::save()` ahora usa transacción atómica
- Si el job está en estado PENDING, automáticamente se inserta en `job_queue`
- Código en `crates/server/infrastructure/src/persistence/postgres/job_repository.rs:161-175`

```rust
// Atomic Enqueue if Pending
if matches!(job.state(), JobState::Pending) {
    sqlx::query(r#"
        INSERT INTO job_queue (job_id)
        VALUES ($1)
        ON CONFLICT (job_id) DO NOTHING
    "#)
    .bind(job.id.0)
    .execute(&mut *tx)
    .await?;
}
tx.commit().await?;
```

### 2. ✅ EventBus no integrado en ciclo de vida - RESUELTO
**Descripción original:**
- EventBus usaba `pg_notify` para publicar eventos
- No había suscriptores activos

**Solución implementada:**
- EventBus completamente integrado
- 13 eventos de dominio definidos
- AuditService como suscriptor
- JobController reactivo a eventos
- Tests de integración verificando pg_notify

### 3. ✅ JobController polling vacío - RESUELTO
**Descripción original:**
- JobController ejecutaba `run_once()` cada 500ms
- No encontraba jobs porque `job_queue` estaba vacía

**Solución:**
- Con el enqueue atómico, ahora `job_queue` tiene jobs
- JobController los procesa correctamente

### 4. ✅ Provisioning service no usado - RESUELTO
**Descripción original:**
- DockerProvider inicializado correctamente
- Pero no se activaba provisioning

**Solución:**
- Con jobs en cola, provisioning se activa automáticamente
- Workers se crean según demanda

---

## Problemas Pendientes

### 🔴 1. Gestión de Secretos Insegura

**Descripción:**
Los secretos actualmente se inyectan como variables de entorno con prefijo `SECRET_`. 
Esto es inseguro porque:
- Cualquier proceso en el contenedor puede leer `/proc/1/environ`
- Los secretos pueden aparecer accidentalmente en logs
- No hay rotación de secretos
- No hay auditoría de acceso

**Código afectado:** `crates/worker/infrastructure/src/executor.rs:51-67`

**Solución propuesta:**
- Ver [EPIC-16: Gestión Segura de Secretos](docs/epics/EPIC-16-SECURE-SECRETS-MANAGEMENT.md)
- Ver [Propuesta de Arquitectura](docs/proposals/PROPOSAL-SECURE-SECRETS-MANAGEMENT.md)

**Prioridad:** Alta

---

## Flujo Actual (Funcionando)

```
CLI → JobExecutionService.queue_job()
    ↓ ✅
Handler gRPC ejecuta CreateJobRequest
    ↓ ✅
CreateJobUseCase.execute()
    ↓ ✅
Job guardado en DB + encolado (transacción atómica)
    ↓ ✅
EventBus.publish(JobCreated)
    ↓ ✅
JobController.run_once() encuentra job en cola
    ↓ ✅
JobController busca workers disponibles
    ↓ ✅
Si no hay workers → ProvisioningService.provision_worker()
    ↓ ✅
Worker creado (Docker/K8s/Firecracker)
    ↓ ✅
Worker se registra con OTP
    ↓ ✅
JobController asigna job a worker
    ↓ ✅
Worker ejecuta job
    ↓ ✅
Worker envía logs via streaming
    ↓ ✅
Job completa, resultado persistido
    ↓ ✅
EventBus.publish(JobCompleted)
```

---

## Métricas del Sistema

### Tests
- 238+ tests pasando
- 0 failures
- ~30 tests ignorados (requieren infraestructura específica)

### Cobertura de Eventos
- 13 eventos de dominio implementados
- 100% de operaciones mutadoras emiten eventos
- Auditoría completa en AuditService

---

## Próximos Pasos

### Inmediato
1. Revisar y aprobar EPIC-16 (Gestión Segura de Secretos)
2. Implementar Sprint 1 de EPIC-16 (Infraestructura Base)

### Corto Plazo
1. Completar EPIC-9 (Tests E2E Docker)
2. Implementar integración con HashiCorp Vault

### Largo Plazo
1. Dashboard web para gestión de jobs
2. Métricas y alertas avanzadas
3. Multi-tenancy

---

## Documentación Relacionada

- [EPIC-15: Event Traceability](docs/epics/EPIC-15-EVENTS-TRACEABILITY-TESTING.md) - ✅ Completado
- [EPIC-16: Secure Secrets](docs/epics/EPIC-16-SECURE-SECRETS-MANAGEMENT.md) - Planificado
- [Propuesta Secrets](docs/proposals/PROPOSAL-SECURE-SECRETS-MANAGEMENT.md) - Nuevo

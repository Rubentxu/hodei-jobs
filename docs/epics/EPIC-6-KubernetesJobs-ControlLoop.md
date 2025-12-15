# ÉPICA 6: Semántica Kubernetes Jobs + Control Loop (production-ready)

## Estado: 🚧 EN PROGRESO

## Resumen
Implementar un flujo end-to-end **tipo Kubernetes Jobs**: el cliente crea/lanza ejecuciones persistidas y el control-plane reconcilia automáticamente hasta completar (`Succeeded/Failed`), sin seeding manual, sin fallbacks in-memory como fuente de verdad y minimizando cambios drásticos.

Esta épica consolida y evoluciona las piezas ya existentes (WorkerRegistry, Scheduler, JobOrchestrator, Postgres persistence, WorkerAgentService) para cerrar el loop:

- Job persistido/encolado
- Scheduling + (opcional) provisioning
- Worker handshake + heartbeat
- Dispatch real (`RunJobCommand`) por stream gRPC
- Resultado del worker → persistencia de job + liberación/terminación del worker

Además introduce el encaje **tipo Jenkins** de “job reutilizable” separando **JobTemplate** (definición) de **JobRun** (ejecución), con estrategia incremental para **no romper el API gRPC existente**.

Referencia: `docs/kubernetes-jobs-semantics.md`

---

## Objetivos

- Hodei se comporta como un **controller** (reconciliación) y no como un flujo manual.
- Producción: **Postgres** como fuente de verdad para jobs/workers/tokens (sin in-memory como fallback).
- Mantener el API actual (gRPC) operativo; cambios solo si son:
  - aditivos, o
  - mejoras justificadas con plan de migración y deprecación.
- Eliminar la necesidad de `seed-demo-worker` en la demo cuando el loop esté completo.

## No-objetivos (por ahora)

- Multi-tenant completo.
- Object storage de artefactos.
- UI.
- HA completa del controller (se deja planificado en hardening).

---

## Estado actual (estudio previo)

### Piezas ya implementadas y reutilizables

- **Persistencia production** en `crates/grpc/src/bin/server.rs`:
  - `PostgresJobRepository`, `PostgresJobQueue`, `PostgresWorkerRegistry`.
- **WorkerRegistry (Domain)** completo:
  - `crates/domain/src/worker_registry.rs`.
- **Scheduler + SmartScheduler + JobOrchestrator (Application)**:
  - `crates/application/src/smart_scheduler.rs`
  - `crates/application/src/job_orchestrator.rs`
  - `crates/application/src/worker_lifecycle.rs`

### Gaps relevantes

- `WorkerAgentServiceImpl` (`crates/grpc/src/services/worker.rs`):
  - Mantiene tokens OTP y workers en memoria.
  - No actualiza `WorkerRegistry` persistente (READY/heartbeat/state).
  - El modo dev acepta tokens `dev-*`/vacíos.
- `SchedulerServiceImpl` (`crates/grpc/src/services/scheduler.rs`):
  - En `ProvisionWorker` responde: "Worker provisioning is not configured".
  - No hace dispatch al worker (solo persiste metadata/estado y asigna en registry).
- `JobExecutionServiceImpl` (`crates/grpc/src/services/job_execution.rs`):
  - `AssignJob`/`StartJob`/`CompleteJob`/`FailJob` funcionan como endpoints manuales.
  - No existe el cierre de loop automático desde `JobResultMessage`.
- `JobOrchestrator` (application):
  - Asigna jobs y marca `JobState::Running`, pero no hace dispatch real al worker.
  - `provision_and_assign` hace `sleep(2s)` (no event-driven).

---

## Principios de diseño (DDD + production-ready)

- **Source of Truth**:
  - Persistencia (Postgres) para: jobs, workers, OTP/tokens, scheduling state.
  - In-memory solo para: canales de stream activos (conectividad), caches no autoritativas.
- **Hexagonal**:
  - Application define puertos (traits) para dispatch y bootstrap.
  - gRPC implementa adapters.
- **Compatibilidad**:
  - Mantener RPCs existentes; si se añaden, que sea de forma aditiva.
  - Deprecación explícita (docs) antes de eliminar.

---

## Historias de Usuario

### HU-6.1: Worker registration/heartbeat actualiza WorkerRegistry persistente

**Como** control-plane
**quiero** que cuando el worker se registre o envíe heartbeat, su estado se refleje en `WorkerRegistry` (Postgres)
**para** que scheduler/orchestrator puedan seleccionar workers sin seeding manual.

Criterios de aceptación:

- Al completar `RegisterWorker`, el worker existe en `WorkerRegistry`.
- Tras el primer `Heartbeat`, el worker pasa a estado `READY` (o equivalente) en `WorkerRegistry`.
- Cada `Heartbeat` actualiza `last_heartbeat` persistente.
- No hay escritura en memoria que sea necesaria para scheduling.

Notas de implementación:

- Inyectar `Arc<dyn WorkerRegistry>` en `WorkerAgentServiceImpl`.
- En `worker_stream` al recibir `Heartbeat`:
  - `registry.heartbeat(worker_id)`
  - (opcional) `registry.update_state(worker_id, Ready)` si procede.

#### Checks de validación (HU-6.1)

 - [x] Compilación y tests (workspace) en verde:

 ```bash
 cargo test --workspace
 ```

 - [x] Test integración Postgres (manual, requiere Docker/Testcontainers):

 ```bash
 cargo test -p hodei-jobs-grpc services::worker::tests::hu_6_1_register_and_heartbeat_updates_registry -- --ignored
 ```

 - [x] Wiring production:
   - `crates/grpc/src/bin/server.rs` inyecta `PostgresWorkerRegistry` en `WorkerAgentServiceImpl::with_registry_and_log_service(...)`.

---

### HU-6.2: OTP/tokens de bootstrap persistentes (sin in-memory como fuente de verdad)

**Como** plataforma
**quiero** que los OTP de bootstrap sean persistentes y auditables
**para** poder reiniciar el control-plane sin romper el registro de workers y evitar fallbacks in-memory.

Criterios de aceptación:

- Validación OTP no depende de `HashMap` in-memory.
- OTP es one-shot y expira.
- Existe mecanismo de cleanup/GC de tokens expirados.
- `HODEI_DEV_MODE` deja de aceptar tokens vacíos/`dev-*` en el binario production.

Notas de implementación:

- Crear un port en domain/application (p.ej. `WorkerBootstrapTokenStore`).
- Implementación Postgres en `crates/infrastructure`.
- `WorkerAgentServiceImpl::validate_otp` consulta el store persistente.

#### Checks de validación (HU-6.2)

- [x] Compilación y tests (workspace) en verde:

```bash
cargo test --workspace
```

- [x] Port de dominio creado:
  - `crates/domain/src/otp_token_store.rs` define `OtpToken` (newtype) y trait `WorkerBootstrapTokenStore`.

- [x] Implementación Postgres:
  - `crates/infrastructure/src/persistence.rs` implementa `PostgresWorkerBootstrapTokenStore`.
  - Tabla `worker_bootstrap_tokens` con columnas: `token`, `worker_id`, `expires_at`, `consumed`.
  - Métodos: `issue()`, `consume()`, `cleanup_expired()`.

- [x] Integración en WorkerAgentServiceImpl:
  - `crates/grpc/src/services/worker.rs` usa `token_store: Option<Arc<dyn WorkerBootstrapTokenStore>>`.
  - `generate_otp()` retorna `Result<String, Status>` y valida UUID de `worker_id`.
  - `validate_otp()` consume token del store persistente si está configurado.

- [x] Hardening dev-mode:
  - Tokens `dev-*` solo aceptados si `cfg!(debug_assertions) && HODEI_DEV_MODE=1`.
  - En release builds, tokens dev son rechazados.

- [x] Wiring production:
  - `crates/grpc/src/bin/server.rs` inyecta `PostgresWorkerBootstrapTokenStore` en `WorkerAgentServiceImpl`.

- [x] Tests actualizados:
  - Tests usan UUIDs para `worker_id` (alineado con producción).
  - `generate_otp()` retorna `Result` y tests manejan el tipo correctamente.

---

### HU-6.3: Port de dispatch (RunJobCommand) y adapter gRPC

**Como** application/orchestrator
**quiero** un puerto para enviar comandos al worker
**para** no acoplar la capa application a tonic/streams.

Criterios de aceptación:

- Existe un trait de application (p.ej. `WorkerCommandSender`) con una operación de dispatch.
- La implementación gRPC usa `WorkerAgentServiceImpl::send_to_worker`.
- Errores de conectividad se traducen a re-scheduling/retry controlados (no panic).

---

### HU-6.4: Control loop/JobController que procesa JobQueue y ejecuta el flujo completo

**Como** operador
**quiero** que el sistema procese jobs en cola automáticamente
**para** no depender de llamadas manuales a `AssignJob/StartJob/CompleteJob`.

Criterios de aceptación:

- Existe un proceso/tarea en el server (binario prod) que:
  - consume jobs pendientes (`JobQueue`/`JobRepository.find_pending`)
  - usa scheduler/orchestrator para decidir
  - ejecuta provisioning si aplica
  - hace dispatch al worker
  - persiste transiciones de estado
- En caso de caída del server, al reiniciar se retoma desde estado persistido.

Notas:

- Implementar en application un `JobController` (loop con intervalo configurable).
- El wiring en `crates/grpc/src/bin/server.rs` arranca el loop.

#### Checks de validación (HU-6.3+HU-6.4)

- [x] Compilación y tests (workspace) en verde:

```bash
cargo test --workspace
```

- [x] Unit test (application):

```bash
cargo test -p hodei-jobs-application job_controller::tests::run_once_assigns_and_dispatches
```

- [x] Integration test (gRPC):

```bash
cargo test -p hodei-jobs-grpc --test job_controller_integration
```

- [x] Wiring production (loop background):
  - Variables de entorno:
    - `HODEI_JOB_CONTROLLER_ENABLED=1` (default)
    - `HODEI_JOB_CONTROLLER_INTERVAL_MS=500` (default)
  - Implementado en: `crates/grpc/src/bin/server.rs`.

---

### HU-6.5: Cierre de loop: JobResultMessage actualiza JobRepository y libera worker

**Como** sistema
**quiero** persistir el resultado reportado por el worker y liberar el worker
**para** que los jobs finalicen automáticamente.

Criterios de aceptación:

- Al recibir `WorkerPayload::Result`:
  - el job se marca `SUCCEEDED/FAILED` en `JobRepository`
  - el worker se libera en `WorkerRegistry.release_from_job`
  - si el modelo es efímero, el lifecycle manager puede terminar el worker
- `CompleteJob`/`FailJob` siguen existiendo por compatibilidad pero se documentan como legacy.

#### Checks de validación (HU-6.5)

- [x] Compilación y tests (workspace) en verde:

```bash
cargo test --workspace
```

- [x] Unit test específico (rápido):

```bash
cargo test -p hodei-jobs-grpc services::worker::tests::hu_6_5_job_result_updates_job_repository
```

- [x] Wiring production:
  - `crates/grpc/src/bin/server.rs` inyecta `JobRepository` en `WorkerAgentServiceImpl::with_registry_job_repository_and_log_service(...)`.

---

### HU-6.6: Provisioning real integrado (sin “not configured”) en SchedulerService

**Como** usuario
**quiero** que si el scheduler decide `ProvisionWorker`, el sistema lo haga
**para** que el flujo sea end-to-end.

Criterios de aceptación:

- `SchedulingDecision::ProvisionWorker` produce provisioning vía `WorkerLifecycleManager`/providers.
- Se genera OTP y se inyecta al worker.
- No hay respuestas “stub” en producción.

Notas:

- Mantener `SchedulerService` como API de interacción, pero delegar en `JobOrchestrator`/controller.

#### Checks de validación (HU-6.6)

- [x] Compilación y tests (workspace) en verde:

```bash
cargo test --workspace
```

- [x] Port de aplicación creado:
  - `crates/application/src/worker_provisioning.rs` define trait `WorkerProvisioningService` y `ProvisioningResult`.

- [x] Implementación concreta:
  - `crates/application/src/worker_provisioning_impl.rs` implementa `DefaultWorkerProvisioningService`.
  - Usa `WorkerRegistry` para registro, `WorkerBootstrapTokenStore` para OTP, y providers para creación.
  - Configuración via `ProvisioningConfig` (Builder Pattern).

- [x] Integración en SchedulerServiceImpl:
  - `crates/grpc/src/services/scheduler.rs` usa `provisioning_service: Option<Arc<dyn WorkerProvisioningService>>`.
  - Constructor `with_provisioning()` para inyectar el servicio.
  - `ProvisionWorker` decision ahora ejecuta provisioning real.

- [x] Wiring production:
  - `crates/grpc/src/bin/server.rs` inicializa `DockerProvider` si disponible.
  - Crea `DefaultWorkerProvisioningService` con providers, registry y token store.
  - Variable de entorno `HODEI_PROVISIONING_ENABLED` (default: "1").
  - Variables adicionales: `HODEI_SERVER_HOST`, `HODEI_WORKER_IMAGE`.

- [x] Sin respuestas stub:
  - Si provisioning no está configurado, retorna `success: false` con mensaje claro.
  - Si está configurado, ejecuta provisioning real via provider.

---

### HU-6.7: Jobs reutilizables estilo Jenkins (JobTemplate + JobRun) sin romper API

**Como** usuario
**quiero** poder reutilizar la definición de un job para ejecuciones posteriores
**para** tener un comportamiento similar a Jenkins (Job/Build) pero con semántica controller.

Estrategia incremental:

- Fase A (compat): mantener `QueueJob/ScheduleJob` creando `JobRun` ad-hoc.
- Fase B (aditiva): añadir soporte explícito de `JobTemplate`:
  - repositorio persistente
  - RPCs nuevos (aditivos) para crear template y lanzar run

Criterios de aceptación (fase A):

- Un `JobRun` siempre es una instancia; tiene `job_id` único.
- Se puede re-ejecutar con mismos criterios creando otro `JobRun` (aunque el template sea implícito).

Criterios de aceptación (fase B):

- Existe `JobTemplate` persistente.
- Existe operación para crear `JobRun` desde `JobTemplate`.

#### Checks de validación (HU-6.7)

- [x] Compilación y tests (workspace) en verde:

```bash
cargo test --workspace
```

- [x] Domain model creado:
  - `crates/domain/src/job_template.rs` define:
    - `JobTemplateId` (Newtype Pattern)
    - `JobTemplateStatus` (Active, Disabled, Archived)
    - `JobTemplate` aggregate con Builder Pattern
    - `JobTemplateRepository` trait (port)

- [x] Funcionalidad del modelo:
  - `create_run()` crea un `Job` (JobRun) desde el template
  - Metadata del job incluye `template_id`, `template_name`, `template_version`
  - Versionado automático en `update_spec()`
  - Estadísticas: `run_count`, `success_count`, `failure_count`, `success_rate()`
  - Lifecycle: `enable()`, `disable()`, `archive()`

- [x] Implementación Postgres:
  - `crates/infrastructure/src/persistence.rs` implementa `PostgresJobTemplateRepository`
  - Tabla `job_templates` con índices para name, status y labels (GIN)
  - Operaciones CRUD completas

- [x] Compatibilidad (Fase A):
  - API existente (`QueueJob`/`ScheduleJob`) sigue funcionando
  - Jobs creados directamente son JobRuns implícitos
  - No se rompe ningún endpoint existente

- [x] Tests unitarios:
  - Tests en `job_template.rs` cubren creación, builder, create_run, lifecycle, success_rate

---

### HU-6.8: Deprecación planificada de endpoints manuales

**Como** mantenedor
**quiero** deprecar endpoints que ya no son necesarios
**para** simplificar el API sin romper integraciones existentes.

Criterios de aceptación:

- Documentar deprecación de:
  - `StartJob`, `CompleteJob`, `FailJob`, `UpdateProgress` (si el worker lo reporta por stream)
- Mantenerlos durante al menos una versión menor, o hasta migración.

---

## Plan de pruebas (mínimo)

- Tests de integración con Postgres:
  - worker register → aparece en WorkerRegistry
  - queue_job → controller procesa → dispatch → job termina
- Tests de regresión de API:
  - las RPCs actuales siguen respondiendo, aunque algunas queden en modo legacy.

---

## Checklist production-ready

- Sin tokens dev en producción.
- Sin OTP in-memory como fuente de verdad.
- Sin seeding manual de workers.
- Sin “stub responses” en rutas de producción.
- Logs estructurados y métricas mínimas.

---

*Última actualización: 2025-12-13*

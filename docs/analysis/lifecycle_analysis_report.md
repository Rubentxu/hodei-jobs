# Análisis Profundo de Ciclos de Vida y Sistema Reactivo

**Fecha:** 2025-12-28  
**Versión:** 1.0  
**Autor:** Análisis Automatizado  
**Alcance:** Flujos de Jobs, Workers, Eventos y Auditoría

---

## Tabla de Contenidos

1. [Resumen Ejecutivo](#1-resumen-ejecutivo)
2. [Máquina de Estados del Job](#2-máquina-de-estados-del-job)
3. [Máquina de Estados del Worker](#3-máquina-de-estados-del-worker)
4. [Sistema Reactivo Basado en Eventos](#4-sistema-reactivo-basado-en-eventos)
5. [Coherencia de Transiciones de Estado](#5-coherencia-de-transiciones-de-estado)
6. [Sistema de Auditoría y Trazabilidad](#6-sistema-de-auditoría-y-trazabilidad)
7. [Análisis de Gaps y Vulnerabilidades](#7-análisis-de-gaps-y-vulnerabilidades)
8. [Propuestas de Mejora](#8-propuestas-de-mejora)
9. [Conclusiones](#9-conclusiones)

---

## 1. Resumen Ejecutivo

### 1.1 Estado General

| Aspecto | Estado | Puntuación |
|---------|--------|------------|
| **Máquina de Estados Job** | ✅ Implementado | 85% |
| **Máquina de Estados Worker** | ✅ Implementado | 80% |
| **Sistema Reactivo (EPIC-29)** | ✅ Implementado | 90% |
| **Auditoría de Eventos** | ⚠️ Parcial | 70% |
| **Trazabilidad de Errores** | ⚠️ Parcial | 65% |
| **Protección de Ciclos de Vida** | ⚠️ Necesita Mejora | 60% |

### 1.2 Hallazgos Clave

1. **✅ Fortaleza:** Sistema reactivo EPIC-29 elimina polling con EventBus PostgreSQL NOTIFY/LISTEN
2. **✅ Fortaleza:** Transiciones de estado validadas en Domain (State Machine Pattern)
3. **⚠️ Gap:** No hay validación de transiciones inválidas en Worker aggregate
4. **⚠️ Gap:** Falta de correlation_id consistente en flujos completos
5. **❌ Riesgo:** Jobs pueden quedar en estados intermedios sin recovery automático
6. **❌ Riesgo:** Outbox Poller puede fallar silenciosamente

---

## 2. Máquina de Estados del Job

### 2.1 Estados Definidos

```rust
// Fuente: crates/shared/src/states.rs:1-15
pub enum JobState {
    Pending,    // Recién creado, en cola esperando scheduler
    Assigned,   // Scheduler seleccionó provider Y worker (ALTA CONFIANZA)
    Scheduled,  // Scheduler seleccionó provider, esperando worker (MEDIA CONFIANZA)
    Running,    // Worker ejecutando activamente
    Succeeded,  // Completado con éxito
    Failed,     // Falló durante ejecución
    Cancelled,  // Cancelado por usuario
    Timeout,    // Excedió timeout
}
```

### 2.2 Diagrama de Transiciones

```
                         ┌─────────────────────────────────────────────────────────┐
                         │                                                         │
                         ▼                                                         │
┌─────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │
│ CREATED │───►│   PENDING   │───►│  ASSIGNED   │───►│   RUNNING   │──┬──────────┤
└─────────┘    └─────────────┘    └─────────────┘    └─────────────┘  │          │
                     │                   │                   │         │          │
                     │                   │                   │         ▼          │
                     │            ┌──────┴──────┐     ┌──────┴──────┐ ┌──────────┐│
                     │            │  SCHEDULED  │     │  SUCCEEDED  │ │  FAILED  ││
                     │            └─────────────┘     └─────────────┘ └──────────┘│
                     │                   │                                        │
                     │                   │         ┌──────────────┐               │
                     │                   │         │  CANCELLED   │◄──────────────┤
                     │                   │         └──────────────┘               │
                     │                   │                                        │
                     │                   │         ┌──────────────┐               │
                     └───────────────────┴────────►│   TIMEOUT    │◄──────────────┘
                                                   └──────────────┘
```

### 2.3 Validación de Transiciones

```rust
// Fuente: crates/shared/src/states.rs:21-74
pub fn can_transition_to(&self, new_state: &JobState) -> bool {
    match (self, new_state) {
        // Mismo estado - no válido
        (s, n) if s == n => false,

        // Desde Pending
        (JobState::Pending, JobState::Assigned) => true,
        (JobState::Pending, JobState::Scheduled) => true,
        (JobState::Pending, JobState::Failed) => true,
        (JobState::Pending, JobState::Cancelled) => true,
        (JobState::Pending, JobState::Timeout) => true,

        // Desde Assigned (ALTA CONFIANZA)
        (JobState::Assigned, JobState::Running) => true,
        (JobState::Assigned, JobState::Failed) => true,
        (JobState::Assigned, JobState::Timeout) => true,
        (JobState::Assigned, JobState::Cancelled) => true,

        // Desde Scheduled (MEDIA CONFIANZA)
        (JobState::Scheduled, JobState::Running) => true,
        (JobState::Scheduled, JobState::Failed) => true,
        (JobState::Scheduled, JobState::Timeout) => true,
        (JobState::Scheduled, JobState::Cancelled) => true,

        // Desde Running
        (JobState::Running, JobState::Succeeded) => true,
        (JobState::Running, JobState::Failed) => true,
        (JobState::Running, JobState::Cancelled) => true,
        (JobState::Running, JobState::Timeout) => true,

        // Estados terminales - no hay transiciones salientes
        _ => false,
    }
}
```

**✅ Evaluación:** La máquina de estados está bien definida y validada en el dominio.

### 2.4 Implementación en Aggregate

```rust
// Fuente: crates/server/domain/src/jobs/aggregate.rs:693-756
pub fn mark_running(&mut self) -> Result<()> {
    let new_state = JobState::Running;
    if !self.state.can_transition_to(&new_state) {
        return Err(DomainError::InvalidStateTransition {
            job_id: self.id.clone(),
            from_state: self.state.clone(),
            to_state: new_state,
        });
    }
    self.state = new_state;
    self.started_at = Some(Utc::now());
    Ok(())
}
```

**✅ Evaluación:** Cada método de transición valida antes de cambiar estado.

---

## 3. Máquina de Estados del Worker

### 3.1 Estados Definidos

```rust
// Fuente: crates/shared/src/states.rs:178-186
pub enum WorkerState {
    Creating,     // Provider creando recurso
    Connecting,   // Agent iniciando conexión gRPC
    Ready,        // Listo para recibir jobs
    Busy,         // Ejecutando un job
    Draining,     // No acepta nuevos jobs, terminando actual
    Terminating,  // En proceso de terminación
    Terminated,   // Terminado (estado final)
}
```

### 3.2 Diagrama de Transiciones

```
┌───────────┐    ┌─────────────┐    ┌─────────┐    ┌──────────┐
│ CREATING  │───►│ CONNECTING  │───►│  READY  │───►│   BUSY   │
└───────────┘    └─────────────┘    └─────────┘    └──────────┘
                                         │              │
                                         │              ▼
                                         │         ┌──────────┐
                                         │         │ DRAINING │
                                         │         └──────────┘
                                         │              │
                                         ▼              ▼
                                    ┌─────────────────────┐
                                    │    TERMINATING      │
                                    └─────────────────────┘
                                              │
                                              ▼
                                    ┌─────────────────────┐
                                    │    TERMINATED       │
                                    └─────────────────────┘
```

### 3.3 Transiciones Implementadas

```rust
// Fuente: crates/server/domain/src/workers/aggregate.rs:759-850

/// Worker conectando (agent arrancando gRPC)
pub fn mark_connecting(&mut self) -> Result<()> {
    match &self.state {
        WorkerState::Creating => {
            self.state = WorkerState::Connecting;
            self.updated_at = Utc::now();
            Ok(())
        }
        _ => Err(DomainError::WorkerNotAvailable {
            worker_id: self.id.clone(),
        }),
    }
}

/// Worker está listo (agent registrado)
pub fn mark_ready(&mut self) -> Result<()> {
    match &self.state {
        WorkerState::Connecting | WorkerState::Busy | WorkerState::Draining => {
            self.state = WorkerState::Ready;
            self.updated_at = Utc::now();
            self.last_heartbeat = Utc::now();
            Ok(())
        }
        _ => Err(DomainError::WorkerNotAvailable {
            worker_id: self.id.clone(),
        }),
    }
}

/// Asignar job al worker
pub fn assign_job(&mut self, job_id: JobId) -> Result<()> {
    if !self.state.can_accept_jobs() {
        return Err(DomainError::WorkerNotAvailable {
            worker_id: self.id.clone(),
        });
    }
    self.current_job_id = Some(job_id);
    self.state = WorkerState::Busy;
    self.updated_at = Utc::now();
    Ok(())
}
```

### 3.4 Gap Identificado: Validación Incompleta

**⚠️ PROBLEMA:** A diferencia de `JobState.can_transition_to()`, `WorkerState` no tiene un método centralizado de validación de transiciones.

```rust
// FALTA en crates/shared/src/states.rs
impl WorkerState {
    // ❌ NO EXISTE - Solo hay:
    pub fn can_accept_jobs(&self) -> bool { ... }
    pub fn is_active(&self) -> bool { ... }
    pub fn is_terminated(&self) -> bool { ... }
    
    // ❌ DEBERÍA EXISTIR:
    // pub fn can_transition_to(&self, new_state: &WorkerState) -> bool { ... }
}
```

**Impacto:** Cada método en Worker aggregate hace su propia validación, lo que puede llevar a inconsistencias.

---

## 4. Sistema Reactivo Basado en Eventos

### 4.1 Arquitectura EPIC-29

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                       SISTEMA REACTIVO (EPIC-29)                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐          │
│  │  Job Creation    │───►│   EventBus       │───►│  JobCoordinator  │          │
│  │  (gRPC Service)  │    │  (PG NOTIFY)     │    │  (Subscriber)    │          │
│  └──────────────────┘    └──────────────────┘    └──────────────────┘          │
│           │                      │                        │                     │
│           │                      │                        │                     │
│           ▼                      ▼                        ▼                     │
│  ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐          │
│  │  Publica:        │    │  Channels:       │    │  Reacciona a:    │          │
│  │  - JobQueued     │    │  - job.queue     │    │  - JobQueued     │          │
│  │  - JobCreated    │    │  - worker.ready  │    │  - WorkerReady   │          │
│  └──────────────────┘    │  - hodei_events  │    └──────────────────┘          │
│                          └──────────────────┘                                   │
│                                                                                 │
│  ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐          │
│  │  Worker Ready    │───►│   EventBus       │───►│  JobDispatcher   │          │
│  │  (Registration)  │    │  (PG NOTIFY)     │    │  (Handler)       │          │
│  └──────────────────┘    └──────────────────┘    └──────────────────┘          │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Implementación del EventBus

```rust
// Fuente: crates/server/infrastructure/src/messaging/postgres.rs:52-112

#[async_trait]
impl EventBus for PostgresEventBus {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        // 1. Serializar evento
        let payload = serde_json::to_string(event)?;
        
        // 2. Persistir en tabla domain_events (Audit Trail)
        sqlx::query(r#"
            INSERT INTO domain_events
            (id, occurred_at, event_type, aggregate_id, correlation_id, actor, payload)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#)
        .bind(event_id)
        .bind(occurred_at)
        .bind(event_type)
        .bind(aggregate_id)
        .bind(correlation_id)
        .bind(actor)
        .bind(payload_json)
        .execute(&self.pool)
        .await?;
        
        // 3. NOTIFY para suscriptores reactivos
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind("hodei_events")
            .bind(payload)
            .execute(&mut *conn)
            .await?;
            
        Ok(())
    }
}
```

**✅ Evaluación:** El sistema NO usa polling. Usa PostgreSQL NOTIFY/LISTEN para reactividad.

### 4.3 Procesamiento Reactivo en JobCoordinator

```rust
// Fuente: crates/server/application/src/jobs/coordinator.rs:85-145

async fn start_reactive_event_processing(&mut self) -> anyhow::Result<()> {
    let mut job_queue_stream = event_bus.subscribe("job.queue").await?;
    let mut worker_ready_stream = event_bus.subscribe("worker.ready").await?;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Procesar JobQueued events
                event_result = job_queue_stream.next() => {
                    if let Some(Ok(DomainEvent::JobQueued { job_id, .. })) = event_result {
                        dispatcher.handle_job_queued(&job_id).await;
                    }
                }
                
                // Procesar WorkerReadyForJob events
                event_result = worker_ready_stream.next() => {
                    if let Some(Ok(DomainEvent::WorkerReadyForJob { worker_id, .. })) = event_result {
                        dispatcher.dispatch_pending_job_to_worker(&worker_id).await;
                    }
                }
            }
        }
    });
    
    Ok(())
}
```

**✅ Evaluación:** `tokio::select!` proporciona procesamiento concurrente reactivo.

### 4.4 Eventos Clave del Sistema

| Evento | Producer | Consumer | Propósito |
|--------|----------|----------|-----------|
| `JobCreated` | SchedulerService | AuditService | Registro de creación |
| `JobQueued` | SchedulerService | JobCoordinator | Trigger dispatch reactivo |
| `JobAssigned` | JobDispatcher | AuditService | Registro de asignación |
| `JobStatusChanged` | Job Aggregate | AuditService, UI | Cambios de estado |
| `WorkerRegistered` | RegistryService | JobCoordinator | Worker disponible |
| `WorkerReadyForJob` | RegistryService | JobDispatcher | Trigger dispatch |
| `WorkerProvisioningRequested` | JobDispatcher | ProviderManager | Crear worker |
| `WorkerHeartbeat` | Worker Agent | LifecycleManager | Monitoreo |
| `JobExecutionError` | Worker Agent | AuditService | Registro de errores |

---

## 5. Coherencia de Transiciones de Estado

### 5.1 Flujo de Job: Happy Path

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                        FLUJO COMPLETO DE JOB                                   │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  1. CREACIÓN                                                                  │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ gRPC QueueJob → Job::new(PENDING) → JobRepository::save()    │         │
│     │                                   → EventBus::publish(JobCreated) │     │
│     │                                   → EventBus::publish(JobQueued)  │     │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  2. DISPATCH                                                                  │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ JobCoordinator::handle_job_queued()                          │         │
│     │   → JobDispatcher::dispatch_once()                           │         │
│     │     → query_healthy_workers()                                │         │
│     │     → make_scheduling_decision()                             │         │
│     │     → dispatch_job_to_worker() [Job → ASSIGNED]              │         │
│     │       → assign_and_dispatch()                                │         │
│     │         → job.assign_to_provider()                           │         │
│     │         → job_repository.update()                            │         │
│     │         → worker_command_sender.send_run_job()               │         │
│     │         → EventBus::publish(JobAssigned)                     │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  3. EJECUCIÓN                                                                 │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ Worker recibe RUN_JOB                                        │         │
│     │   → Envía ACK (Job → RUNNING)                                │         │
│     │   → JobExecutor::execute()                                   │         │
│     │   → Envía logs via LogStream                                 │         │
│     │   → Completa ejecución                                       │         │
│     │   → Envía resultado (Job → SUCCEEDED/FAILED)                 │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  4. FINALIZACIÓN                                                              │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ Server recibe JobResult                                      │         │
│     │   → job.complete(result)                                     │         │
│     │   → job_repository.update()                                  │         │
│     │   → EventBus::publish(JobStatusChanged)                      │         │
│     │   → Worker → TERMINATED (efímero)                            │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Flujo de Worker: Happy Path

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                      FLUJO COMPLETO DE WORKER                                  │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  1. PROVISIONING                                                              │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ WorkerProvisioningService::provision_worker()                │         │
│     │   → Provider.create_worker(spec) [Worker → CREATING]         │         │
│     │   → WorkerRegistry.register()                                │         │
│     │   → EventBus::publish(WorkerProvisioned)                     │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  2. CONEXIÓN                                                                  │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ Worker Agent inicia                                          │         │
│     │   → Conecta gRPC stream [Worker → CONNECTING]                │         │
│     │   → Envía RegisterWorker                                     │         │
│     │   → worker.mark_ready() [Worker → READY]                     │         │
│     │   → EventBus::publish(WorkerReady)                           │         │
│     │   → EventBus::publish(WorkerReadyForJob)                     │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  3. ASIGNACIÓN                                                                │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ JobDispatcher::dispatch_pending_job_to_worker()              │         │
│     │   → worker.assign_job(job_id) [Worker → BUSY]                │         │
│     │   → send_run_job()                                           │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                    │                                          │
│                                    ▼                                          │
│  4. TERMINACIÓN (Modelo Efímero EPIC-21/26)                                   │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │ Job completado                                               │         │
│     │   → worker.complete_job() [Worker → TERMINATED]              │         │
│     │   → EventBus::publish(WorkerTerminated)                      │         │
│     │   → Provider.destroy_worker()                                │         │
│     └──────────────────────────────────────────────────────────────┘         │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 5.3 Análisis de Coherencia

| Transición | Validación Domain | Evento Publicado | Persistencia | Auditoría |
|------------|-------------------|------------------|--------------|-----------|
| PENDING → ASSIGNED | ✅ `can_transition_to()` | ✅ JobAssigned | ✅ Repository.update() | ✅ EventBus |
| ASSIGNED → RUNNING | ✅ `can_transition_to()` | ✅ JobStatusChanged | ✅ Repository.update() | ✅ EventBus |
| RUNNING → SUCCEEDED | ✅ `can_transition_to()` | ✅ JobStatusChanged | ✅ Repository.update() | ✅ EventBus |
| RUNNING → FAILED | ✅ `can_transition_to()` | ✅ JobStatusChanged | ✅ Repository.update() | ✅ EventBus |
| CREATING → CONNECTING | ⚠️ Pattern matching | ⚠️ Parcial | ✅ Registry.update() | ⚠️ Parcial |
| READY → BUSY | ⚠️ `can_accept_jobs()` | ⚠️ Parcial | ✅ Registry.update() | ⚠️ Parcial |
| BUSY → TERMINATED | ⚠️ Pattern matching | ✅ WorkerTerminated | ✅ Registry.update() | ✅ EventBus |

---

## 6. Sistema de Auditoría y Trazabilidad

### 6.1 Componentes de Auditoría

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         SISTEMA DE AUDITORÍA                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                     CAPA 1: Domain Events Table                         │   │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│  │  │ PostgresEventBus::publish()                                      │    │   │
│  │  │   → INSERT INTO domain_events (                                  │    │   │
│  │  │       id, occurred_at, event_type, aggregate_id,                 │    │   │
│  │  │       correlation_id, actor, payload                             │    │   │
│  │  │     )                                                            │    │   │
│  │  └─────────────────────────────────────────────────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                     CAPA 2: Transactional Outbox                        │   │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│  │  │ OutboxRepository::insert_events()                                │    │   │
│  │  │   → INSERT INTO outbox_events (                                  │    │   │
│  │  │       aggregate_id, aggregate_type, event_type, payload,         │    │   │
│  │  │       metadata, idempotency_key, status                          │    │   │
│  │  │     )                                                            │    │   │
│  │  └─────────────────────────────────────────────────────────────────┘    │   │
│  │                                                                          │   │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│  │  │ OutboxPoller::poll_and_publish()                                 │    │   │
│  │  │   → SELECT * FROM outbox_events WHERE status = 'pending'         │    │   │
│  │  │   → EventBus::publish()                                          │    │   │
│  │  │   → UPDATE status = 'published'                                  │    │   │
│  │  └─────────────────────────────────────────────────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                     CAPA 3: Audit Service                               │   │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│  │  │ AuditService::log_event()                                        │    │   │
│  │  │   → AuditRepository::save(AuditLog)                              │    │   │
│  │  │                                                                  │    │   │
│  │  │ Consultas:                                                       │    │   │
│  │  │   → find_by_correlation_id()                                     │    │   │
│  │  │   → find_by_event_type()                                         │    │   │
│  │  │   → find_by_date_range()                                         │    │   │
│  │  │   → find_by_actor()                                              │    │   │
│  │  └─────────────────────────────────────────────────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Modelo de Datos de Auditoría

```rust
// Fuente: crates/server/domain/src/audit/model.rs:7-16
pub struct AuditLog {
    pub id: Uuid,
    pub correlation_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub actor: Option<String>,
}
```

### 6.3 Trazabilidad: correlation_id

```rust
// Fuente: crates/server/domain/src/events.rs - Todos los eventos soportan:
pub struct DomainEvent {
    // ...
    correlation_id: Option<String>,  // ✅ Para tracking end-to-end
    actor: Option<String>,           // ✅ Para auditoría de quién inició
}
```

**⚠️ Gap Identificado:** No hay generación automática de `correlation_id` en todos los flujos.

---

## 7. Análisis de Gaps y Vulnerabilidades

### 7.1 Gap 1: Falta de can_transition_to en WorkerState

**Ubicación:** `crates/shared/src/states.rs`

**Problema:** WorkerState NO tiene validación centralizada como JobState.

**Impacto:** Cada método en Worker aggregate valida transiciones de forma ad-hoc.

**Riesgo:** MEDIO

### 7.2 Gap 2: Jobs Huérfanos en Estado ASSIGNED

**Ubicación:** `crates/server/application/src/jobs/dispatcher.rs:640-675`

**Problema:** Si falla el gRPC send_run_job(), el job puede quedar en estado inconsistente.

**Impacto:** Jobs pueden quedar atascados en ASSIGNED si hay fallo parcial.

**Riesgo:** ALTO

### 7.3 Gap 3: Reconexión de EventBus Stream

**Ubicación:** `crates/server/application/src/jobs/coordinator.rs:120-130`

**Problema:** Solo hay un sleep, no hay lógica de reconexión real.

**Impacto:** Si el stream de PostgreSQL LISTEN se desconecta, el sistema pierde eventos.

**Riesgo:** ALTO

### 7.4 Gap 4: Outbox Poller Fallo Silencioso

**Problema:** El OutboxPoller puede fallar al publicar eventos sin recovery.

**Impacto:** Eventos quedan en estado "pending" indefinidamente.

**Riesgo:** MEDIO

### 7.5 Gap 5: Falta de Timeout de Estado

**Problema:** No hay mecanismo para detectar jobs en estados intermedios.

**Impacto:** Jobs pueden quedar atascados sin recovery.

**Riesgo:** ALTO

### 7.6 Gap 6: correlation_id No Propagado

**Problema:** `correlation_id` no se genera automáticamente y no se propaga.

**Impacto:** Trazabilidad incompleta de flujos end-to-end.

**Riesgo:** MEDIO

---

## 8. Propuestas de Mejora

### 8.1 Alta Prioridad

1. **Validación Centralizada de WorkerState** - Agregar `can_transition_to()`
2. **Timeout de Estados Intermedios** - Implementar JobStateTimeoutMonitor
3. **Reconexión Automática de EventBus** - Implementar ResilientSubscriber

### 8.2 Media Prioridad

4. **Generación Automática de correlation_id** - Middleware gRPC
5. **Dead Letter Queue para Outbox** - Manejo de eventos fallidos
6. **Circuit Breaker para Dispatch** - Protección contra cascada de fallos

### 8.3 Código Deprecado para Eliminación

Con la implementación de EPIC-29 (sistema reactivo) y EPIC-23 (Extension Objects pattern), existe código legacy que debería eliminarse para reducir la deuda técnica y mejorar la mantenibilidad.

#### 8.3.1 Modo Polling Legacy (ELIMINAR)

**Archivo:** `crates/server/application/src/jobs/coordinator.rs:165-195`

```rust
// ❌ DEPRECADO - Eliminar cuando EPIC-29 esté estabilizado
pub async fn start_polling_mode(&mut self) -> anyhow::Result<()> {
    info!("🚀 JobCoordinator: Starting job processing system (Legacy Polling Mode)");
    // ... polling loop con sleep(500ms)
}
```

**Razón:** El sistema reactivo EPIC-29 usa PostgreSQL NOTIFY/LISTEN. El polling ya no es necesario.

**Impacto:** Eliminar ~30 líneas. Verificar que ningún deployment use `start_polling_mode()`.

---

#### 8.3.2 Campo `kubernetes` en WorkerSpec (ELIMINAR)

**Archivo:** `crates/server/domain/src/workers/aggregate.rs:229-231`

```rust
// ❌ DEPRECADO - Usar provider_config en su lugar
/// @deprecated Use `provider_config` instead. This field will be removed in a future version.
#[serde(default)]
pub kubernetes: KubernetesWorkerConfig,
```

**Razón:** EPIC-23 introdujo el patrón Extension Objects con `provider_config`. El campo `kubernetes` directo viola el principio de agnóstico de infraestructura.

**Impacto:** 
- Eliminar campo `kubernetes` de `WorkerSpec`
- Eliminar struct `KubernetesWorkerConfig` (líneas 233-260)
- Eliminar métodos deprecados (ver 8.3.3)
- Migrar cualquier uso a `ProviderConfig::Kubernetes(KubernetesConfigExt)`

---

#### 8.3.3 Métodos `with_kubernetes_*` (ELIMINAR)

**Archivo:** `crates/server/domain/src/workers/aggregate.rs:554-604`

```rust
// ❌ TODOS DEPRECADOS - 6 métodos a eliminar
/// @deprecated Use `with_provider_config(KubernetesConfigExt)` instead
pub fn with_kubernetes_annotation(...) -> Self { ... }
pub fn with_kubernetes_label(...) -> Self { ... }
pub fn with_kubernetes_node_selector(...) -> Self { ... }
pub fn with_kubernetes_service_account(...) -> Self { ... }
pub fn with_kubernetes_init_container(...) -> Self { ... }
pub fn with_kubernetes_sidecar_container(...) -> Self { ... }
```

**Razón:** Reemplazados por `with_provider_config(ProviderConfig::Kubernetes(...))`.

**Impacto:** Eliminar ~50 líneas. Buscar usos en tests y migrar.

---

#### 8.3.4 Legacy Exports en lib.rs (LIMPIAR)

**Archivos:** 
- `crates/server/domain/src/lib.rs:27` - "Legacy exports para retrocompatibilidad durante la migración"
- `crates/server/application/src/lib.rs:13` - "Legacy exports para retrocompatibilidad"
- `crates/server/infrastructure/src/lib.rs:14` - "Legacy"

**Acción:** Revisar si los exports legacy aún son necesarios. Eliminar los que ya no se usen.

---

#### 8.3.5 Fallbacks de Event Publishing (LIMPIAR)

**Archivos:**
- `crates/server/application/src/workers/lifecycle.rs:318` - "Legacy: Direct event publishing"
- `crates/server/application/src/workers/lifecycle.rs:368` - "Legacy: Direct event publishing"
- `crates/server/interface/src/grpc/worker.rs:348` - "Legacy: Direct event publishing (fallback)"
- `crates/server/interface/src/grpc/worker.rs:588` - "Legacy: Direct event publishing (fallback)"

**Razón:** Con Transactional Outbox estabilizado, los fallbacks de publicación directa deberían eliminarse para garantizar consistencia.

**Impacto:** Convertir fallbacks en errores cuando outbox no esté configurado, o hacer outbox obligatorio.

---

#### 8.3.6 TODOs Pendientes de Implementar o Eliminar

| Archivo | Línea | TODO | Acción Recomendada |
|---------|-------|------|-------------------|
| `jobs/coordination.rs` | 320 | "Implementar limpieza de logs antiguos" | Implementar o documentar como out-of-scope |
| `jobs/coordination.rs` | 331 | "Implementar búsqueda en logs" | Implementar con LogStreamService |
| `jobs/aggregate.rs` | 1088 | "Implementar en job" | Revisar contexto y completar |
| `providers/health_monitor.rs` | 282 | "Publish alert event" | Implementar alerting |
| `grpc/worker_command_sender.rs` | 83 | "Implement secret injection" | Prioridad media - seguridad |
| `grpc/log_stream.rs` | 426 | "Implement pagination" | Implementar para logs grandes |
| `grpc/worker.rs` | 1173 | "Store log_ref in database" | Implementar persistencia |
| `providers/kubernetes.rs` | 1499-1596 | Múltiples TODOs de env_from, ports, security_context | Completar para K8s production-ready |

---

#### 8.3.7 Endpoints gRPC Legacy

**Archivo:** `proto/src/generated/hodei.worker.rs:564-748`

```protobuf
/// Legacy: Obtener workers disponibles para scheduling
/// Legacy: Drenar worker (preparar para shutdown)  
/// Legacy: Desregistrar worker
/// Legacy: Stream para actualizaciones de estado de workers
```

**Acción:** Evaluar si estos endpoints siguen en uso. Si están reemplazados por nuevos endpoints en EPIC-29, marcar para deprecación y eventual eliminación.

---

### 8.4 Resumen de Código a Eliminar

| Categoría | Archivos Afectados | Líneas Aprox. | Prioridad |
|-----------|-------------------|---------------|-----------|
| Polling Mode | coordinator.rs | ~30 | P1 (tras validar EPIC-29) |
| kubernetes field | workers/aggregate.rs | ~80 | P2 |
| Legacy exports | lib.rs (3 archivos) | ~15 | P3 |
| Direct event fallbacks | lifecycle.rs, worker.rs | ~40 | P2 |
| TODOs sin implementar | 8+ archivos | N/A | Evaluar |
| gRPC Legacy | hodei.worker.rs | ~100 | P3 |

**Total estimado:** ~265 líneas de código muerto/deprecado

---

## 9. Conclusiones

### 9.1 Fortalezas del Sistema Actual

1. **✅ Sistema Reactivo EPIC-29:** Elimina polling con PostgreSQL NOTIFY/LISTEN
2. **✅ State Machine Pattern:** Validación de transiciones en JobState
3. **✅ Eventos de Dominio Ricos:** 50+ eventos con metadata completa
4. **✅ Transactional Outbox:** Garantiza consistencia eventos/persistencia
5. **✅ Modelo Efímero EPIC-21/26:** Workers se terminan tras job

### 9.2 Áreas de Mejora Críticas

1. **❌ WorkerState sin validación centralizada**
2. **❌ Jobs huérfanos en ASSIGNED**
3. **❌ Reconexión de EventBus**
4. **❌ correlation_id inconsistente**

### 9.3 Deuda Técnica: Código a Eliminar

| Categoría | Líneas | Prioridad |
|-----------|--------|-----------|
| Polling Mode Legacy | ~30 | P1 |
| kubernetes field deprecado | ~80 | P2 |
| Direct event fallbacks | ~40 | P2 |
| Legacy exports | ~15 | P3 |
| gRPC Legacy endpoints | ~100 | P3 |
| **Total** | **~265** | - |

### 9.4 Roadmap de Mejoras

| Prioridad | Mejora | Esfuerzo | Impacto |
|-----------|--------|----------|---------|
| P0 | Timeout de estados intermedios | 2 días | Alto |
| P0 | Reconexión EventBus | 1 día | Alto |
| P1 | can_transition_to() en WorkerState | 0.5 días | Medio |
| P1 | correlation_id automático | 1 día | Medio |
| P1 | Eliminar polling mode legacy | 0.5 días | Bajo (limpieza) |
| P2 | Eliminar kubernetes field deprecado | 1 día | Bajo (limpieza) |
| P2 | Dead Letter Queue | 2 días | Medio |
| P2 | Circuit Breaker | 1.5 días | Medio |
| P3 | Limpiar legacy exports y fallbacks | 0.5 días | Bajo (limpieza) |

---

*Documento generado automáticamente. Para más detalles, revisar el código fuente en los archivos referenciados.*

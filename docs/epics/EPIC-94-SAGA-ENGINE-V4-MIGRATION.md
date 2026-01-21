# EPIC-94: Saga Engine v4.0 - Librería de Durable Execution Reutilizable

> **Estado**: En Desarrollo | **Versión**: v4.0 | **Prioridad**: Crítica
> **Última Actualización**: 2026-01-21 | **Progreso Total**: 0%

## ⚠️ Epica Dividida en 4 Sub-Epicas

Esta épica ha sido dividida en 4 sub-épicas más enfocadas para facilitar la gestión del proyecto y el tracking de progreso:

| # | Épica | Enfoque | Duración | Prioridad |
|---|-------|---------|----------|-----------|
| **A** | [EPIC-94-A: Saga Engine v4 Core](EPIC-94-A-SAGA-ENGINE-V4-CORE.md) | Core library: DurableWorkflow, SagaEngine, Worker | 4-5 sem | Crítica |
| **B** | [EPIC-94-B: Infrastructure Adapters](EPIC-94-B-SAGA-ENGINE-V4-INFRASTRUCTURE.md) | PostgreSQL, NATS, CommandBus, EventBus, Outbox | 3-4 sem | Alta |
| **C** | [EPIC-94-C: Hodei-Jobs Migration](EPIC-94-C-HODEI-JOBS-MIGRATION.md) | Migración de workflows, gRPC, services | 4-6 sem | Crítica |
| **D** | [EPIC-94-D: Testing & Observability](EPIC-94-D-TESTING-OBSERVABILITY.md) | Testing harness, Prometheus, OpenTelemetry | 2-3 sem | Alta |

---

## 🎯 Visión de la Épica (Resumen)

Transformar `saga-engine` en una **librería de Durable Execution reutilizable** inspirada en Temporal.io, que abstraiga completamente las complejidades de patrones saga, command bus, event bus, outbox e infraestructura del código cliente.

**Principio Fundamental**: *"El código cliente solo define workflows y actividades. Todo lo demás (persistencia, mensajería, resiliencia, replay) es responsabilidad de la librería."*

---

## 📊 Resumen de Análisis

### Lo que YA Existe ✅

| Capa | Componentes | Estado |
|------|-------------|--------|
| **Core** | Activity, WorkflowDefinition, WorkflowContext, EventStore, TaskQueue, TimerStore, HistoryReplayer | ✅ Listo |
| **PostgreSQL** | PostgresEventStore, PostgresTimerStore, PostgresReplayer | ✅ Listo |
| **NATS** | NatsTaskQueue, SignalDispatcher | ✅ Listo |
| **Hodei-jobs** | ValidateProviderActivity, CheckConnectivityActivity, types | ✅ Listo |

### Lo que FALTA implementar ❌

| Feature | Prioridad | Épica |
|---------|-----------|-------|
| `DurableWorkflow` trait | Crítica | **A** |
| `execute_activity()` en Context | Crítica | **A** |
| `SagaEngine` | Crítica | **A** |
| `CommandBus` abstracción | Alta | **B** |
| `EventBus` abstracción | Alta | **B** |
| `Outbox` abstracción | Alta | **B** |
| `ActivityRegistry` | Media | **A** |
| `Worker` | Alta | **A** |
| Migration de workflows | Crítica | **C** |
| Integration tests | Alta | **D** |
| Métricas Prometheus | Media | **D** |

---

## 🔄 Estrategia de Evolución

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    EVOLUCIÓN STEP-LIST → WORKFLOW-AS-CODE                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────┐     │
│   │                    EXISTENTE (Mantener)                          │     │
│   │   trait WorkflowDefinition { fn steps(&self) -> &[Box<dyn...>] } │     │
│   │   trait Activity { async fn execute(...) }                       │     │
│   │   struct WorkflowContext { ... }                                 │     │
│   │   trait EventStore, TaskQueue, TimerStore { ... }               │     │
│   └──────────────────────────────────────────────────────────────────┘     │
│                              ↓ EXTENDER                                     │
│   ┌──────────────────────────────────────────────────────────────────┐     │
│   │                    NUEVO (Añadir en Épica A)                     │     │
│   │   trait DurableWorkflow { async fn run(...) }  ← ⭐ PRINCIPAL    │     │
│   │   impl WorkflowContext { fn execute_activity(...) }              │     │
│   │   struct SagaEngine { ... }                                      │     │
│   │   struct ActivityRegistry { ... }                                │     │
│   │   struct Worker { ... }                                          │     │
│   └──────────────────────────────────────────────────────────────────┘     │
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────┐     │
│   │                    NUEVO (Añadir en Épica B)                     │     │
│   │   trait CommandBus { ... }                                       │     │
│   │   trait EventBus { ... }                                         │     │
│   │   trait OutboxRepository { ... }                                 │     │
│   │   struct PostgresSagaEngine { ... }                              │     │
│   └──────────────────────────────────────────────────────────────────┘     │
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────┐     │
│   │                    NUEVO (Añadir en Épica C)                     │     │
│   │   ProvisioningWorkflow (DurableWorkflow)                         │     │
│   │   RecoveryWorkflow (DurableWorkflow)                             │     │
│   │   JobController adaptado                                         │     │
│   │   gRPC services con WorkflowClient                               │     │
│   └──────────────────────────────────────────────────────────────────┘     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🚀 Orden de Implementación Recomendado

```
Semana 1-4  │  Épica A: SagaEngine Core
            │  ├── DurableWorkflow trait
            │  ├── execute_activity() en Context
            │  ├── SagaEngine con replay
            │  ├── ActivityRegistry
            │  └── Worker
            │
Semana 5-7  │  Épica B: Infrastructure Adapters
            │  ├── CommandBus abstraction
            │  ├── EventBus abstraction
            │  ├── Outbox + OutboxRelay
            │  └── PostgresSagaEngine wrapper
            │
Semana 8-12 │  Épica C: Hodei-Jobs Migration
            │  ├── ProvisioningWorkflow migration
            │  ├── RecoveryWorkflow migration
            │  ├── JobController adaptation
            │  ├── gRPC integration
            │  └── Legacy code removal
            │
Semana 13-15│  Épica D: Testing & Observability
            │  ├── WorkflowTestHarness
            │  ├── InMemorySagaEngine
            │  ├── Prometheus metrics
            │  ├── OpenTelemetry tracing
            │  └── Integration tests
```

---

## 📈 Métricas de Éxito

| Métrica | Objetivo | Épica |
|---------|----------|-------|
| Coverage tests | > 90% | D |
| gRPC tests | 100% passing | C, D |
| Integration tests | 100% passing | D |
| Legacy code removed | 100% | C |
| Performance | Mejor que v3 | A, B |

---

## 🔗 Documentación Relacionada

- [EPIC-94-A: Core Library](docs/epics/EPIC-94-A-SAGA-ENGINE-V4-CORE.md)
- [EPIC-94-B: Infrastructure](docs/epics/EPIC-94-B-SAGA-ENGINE-V4-INFRASTRUCTURE.md)
- [EPIC-94-C: Hodei-Jobs Migration](docs/epics/EPIC-94-C-HODEI-JOBS-MIGRATION.md)
- [EPIC-94-D: Testing & Observability](docs/epics/EPIC-94-D-TESTING-OBSERVABILITY.md)

---

## 📋 Plan de Implementación Detallado (v2.0)

### Fase 1: Core - DurableWorkflow Trait (Semanas 1-2)

#### US-94.1: Nuevo trait `DurableWorkflow` (Código-as-Code)

**Objetivo**: Añadir un nuevo trait que permita definir workflows como código Rust nativo.

```rust
// crates/saga-engine/core/src/workflow/durable.rs - NUEVO

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Trait para workflows estilo "Temporal" (Workflow-as-Code)
///
/// A diferencia de `WorkflowDefinition` que usa lista de pasos (`steps()`),
/// `DurableWorkflow` usa un método `run()` que es código Rust real.
///
/// # Ejemplo
///
/// ```rust
/// #[async_trait]
/// impl DurableWorkflow for ProvisioningWorkflow {
///     const TYPE_ID: &'static str = "provisioning";
///     const VERSION: u32 = 1;
///
///     type Input = ProvisioningInput;
///     type Output = WorkerId;
///     type Error = ProvisioningError;
///
///     async fn run(
///         &self,
///         ctx: &mut WorkflowContext,
///         input: Self::Input,
///     ) -> Result<Self::Output, Self::Error> {
///         // Código Rust real con await, if, match, etc.
///         ctx.execute_activity(&ValidateProvider, input.provider_id).await?;
///         let worker_id = ctx.execute_activity(&CreateInfra, input.spec).await?;
///         Ok(worker_id)
///     }
/// }
/// ```
#[async_trait]
pub trait DurableWorkflow: Send + Sync + 'static {
    /// Unique identifier for this workflow type
    const TYPE_ID: &'static str;
    /// Version number for this workflow definition
    const VERSION: u32;

    /// Type-safe input for this workflow
    type Input: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;
    /// Type-safe output for this workflow
    type Output: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;
    /// Error type for this workflow
    type Error: std::error::Error + Send + Sync + 'static;

    /// Main workflow method - called by the engine for execution
    ///
    /// This method should call `ctx.execute_activity()` for any long-running
    /// operations. The engine will:
    /// 1. Check if the activity was already completed (replay)
    /// 2. If not, publish a task and pause this workflow
    /// 3. Resume when the task completes
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}

/// Error thrown when a workflow needs to pause
#[derive(Debug, Clone)]
pub struct WorkflowPaused {
    /// The activity that was scheduled
    pub activity_type: &'static str,
    /// The saga/execution ID
    pub execution_id: SagaId,
}

impl std::fmt::Display for WorkflowPaused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Workflow paused waiting for {} (execution: {})",
            self.activity_type, self.execution_id
        )
    }
}

impl std::error::Error for WorkflowPaused {}
```

#### US-94.2: `execute_activity()` en `WorkflowContext`

**Objetivo**: Añadir método para ejecutar actividades de forma interceptada.

```rust
// AÑADIR a crates/saga-engine/core/src/workflow/mod.rs

impl WorkflowContext {
    // ... existente ...

    /// Execute an activity in a durable manner
    ///
    /// This method:
    /// 1. Checks if activity was already completed (replay from history)
    /// 2. If not completed, publishes task to queue and returns Err(WorkflowPaused)
    /// 3. When task completes, engine will replay and continue
    ///
    /// # Type Parameters
    ///
    /// * `A` - The activity type (must implement `Activity`)
    ///
    /// # Arguments
    ///
    /// * `activity` - The activity instance to execute
    /// * `input` - The activity input
    ///
    /// # Returns
    ///
    /// * `Ok(output)` - Activity completed successfully
    /// * `Err(WorkflowPaused)` - Activity scheduled, workflow should pause
    /// * `Err(ActivityError)` - Activity failed permanently
    pub async fn execute_activity<A: Activity>(
        &mut self,
        activity: &A,
        input: A::Input,
    ) -> Result<A::Output, ExecuteActivityError<A::Error>> {
        let activity_id = self.get_activity_id(A::TYPE_ID, &input);

        // Check if already completed (replay case)
        if let Some(cached) = self.get_cached_activity_output(&activity_id).await {
            return cached;
        }

        // Schedule activity and pause
        self.schedule_activity(A::TYPE_ID, &activity_id, &input).await?;

        Err(ExecuteActivityError::Paused(WorkflowPaused {
            activity_type: A::TYPE_ID,
            execution_id: self.execution_id.clone(),
        }))
    }

    /// Execute activity with options (timeout, retry, etc.)
    pub async fn execute_activity_with_options<A: Activity>(
        &mut self,
        activity: &A,
        input: A::Input,
        options: ActivityOptions,
    ) -> Result<A::Output, ExecuteActivityError<A::Error>> {
        // ... implementación con opciones ...
    }

    // Helper methods (privados)
    fn get_activity_id(&self, activity_type: &str, input: &impl Serialize) -> String {
        // Generar ID único basado en tipo + input hash
        format!("{}-{}", activity_type, hash_input(input))
    }

    async fn get_cached_activity_output<A: Activity>(
        &self,
        activity_id: &str,
    ) -> Option<Result<A::Output, ExecuteActivityError<A::Error>>> {
        // Buscar en step_outputs el resultado cacheado
        self.step_outputs
            .get(activity_id)
            .map(|v| serde_json::from_value(v.clone()).map_err(ExecuteActivityError::Serialization))
    }

    async fn schedule_activity(
        &mut self,
        activity_type: &'static str,
        activity_id: &str,
        input: &impl Serialize,
    ) -> Result<(), EventStoreError<...>> {
        // Publicar evento de actividad programada
        // Publicar task en TaskQueue
        Ok(())
    }
}

/// Error types for execute_activity
#[derive(Debug, thiserror::Error)]
pub enum ExecuteActivityError<E: std::error::Error + Send + Sync + 'static> {
    #[error("Activity completed successfully")]
    Ok(serde_json::Value),

    #[error("Workflow paused waiting for activity")]
    Paused(WorkflowPaused),

    #[error("Activity failed: {0}")]
    Failed(E),

    #[error("Activity timeout: {0}")]
    Timeout(Duration),

    #[error("Activity cancelled")]
    Cancelled,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Activity execution options
#[derive(Debug, Clone)]
pub struct ActivityOptions {
    pub timeout: Duration,
    pub retry_policy: RetryPolicy,
    pub heartbeat_interval: Option<Duration>,
}

impl Default for ActivityOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            retry_policy: RetryPolicy::default(),
            heartbeat_interval: None,
        }
    }
}

impl ActivityOptions {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }
}
```

### Fase 2: Core - SagaEngine (Semanas 3-4)

#### US-94.3: `SagaEngine` con Replay+Ejecución Incremental

**Objetivo**: Motor que maneja la ejecución durable.

```rust
// crates/saga-engine/core/src/engine/mod.rs - NUEVO

/// Motor central de durable execution
///
/// Maneja:
/// 1. Replay de historial de eventos
/// 2. Ejecución de workflows hasta el siguiente await
/// 3. Publicación de tasks de actividades
/// 4. Reanudación de workflows cuando tasks completan
pub struct SagaEngine<ES, TQ, TS, R>
where
    ES: EventStore,
    TQ: TaskQueue,
    TS: TimerStore,
    R: HistoryReplayer<WorkflowContext>,
{
    event_store: Arc<ES>,
    task_queue: Arc<TQ>,
    timer_store: Arc<TS>,
    replayer: Arc<R>,
    activity_registry: Arc<dyn ActivityRegistry>,
    metrics: Arc<dyn SagaMetrics>,
    config: SagaEngineConfig,
}

impl<ES, TQ, TS, R> SagaEngine<ES, TQ, TS, R>
where
    ES: EventStore + 'static,
    TQ: TaskQueue + 'static,
    TS: TimerStore + 'static,
    R: HistoryReplayer<WorkflowContext, Error = ES::Error> + 'static,
{
    /// Start a new workflow execution
    pub async fn start_workflow<W: DurableWorkflow>(
        &self,
        workflow: &W,
        input: W::Input,
        idempotency_key: Option<String>,
    ) -> Result<SagaId, W::Error> {
        let execution_id = SagaId::new();

        // 1. Create start event
        let start_event = create_workflow_started_event(&execution_id, &input, &idempotency_key);

        // 2. Persist event
        self.event_store
            .append_event(&execution_id, 0, &start_event)
            .await
            .map_err(|e| W::Error::from(format!("Event store error: {:?}", e)))?;

        // 3. Create workflow context
        let config = workflow.configuration();
        let mut ctx = WorkflowContext::new(
            execution_id.clone(),
            WorkflowTypeId::new::<W>(),
            config,
        );

        // 4. Execute workflow (will pause on first activity)
        let _ = self.execute_workflow(workflow, &mut ctx, input).await;

        Ok(execution_id)
    }

    /// Resume a paused workflow (called by Worker when task completes)
    pub async fn resume_workflow<W: DurableWorkflow>(
        &self,
        workflow: &W,
        execution_id: &SagaId,
    ) -> Result<W::Output, W::Error> {
        // 1. Replay context from event store
        let mut ctx = self
            .replayer
            .get_current_state(execution_id, None)
            .await
            .map_err(|e| W::Error::from(format!("Replay error: {:?}", e)))?
            .state;

        // 2. Get pending activity result from event store
        let activity_result = self.get_pending_activity_result(execution_id).await?;

        // 3. Apply result to context
        self.apply_activity_result(&mut ctx, &activity_result)?;

        // 4. Continue execution
        let input = self.get_workflow_input::<W>(execution_id).await?;
        self.execute_workflow(workflow, &mut ctx, input).await
    }

    /// Internal execution loop
    async fn execute_workflow<W: DurableWorkflow>(
        &self,
        workflow: &W,
        ctx: &mut WorkflowContext,
        input: W::Input,
    ) -> Result<W::Output, W::Error> {
        loop {
            match workflow.run(ctx, input.clone()).await {
                Ok(output) => {
                    // Workflow completed
                    self.persist_workflow_completed(ctx, &output).await;
                    return Ok(output);
                }
                Err(ExecuteActivityError::Paused(paused)) => {
                    // Workflow paused, save state
                    self.persist_workflow_state(ctx).await;
                    return Err(W::Error::from("Paused - worker will resume"));
                }
                Err(e) => {
                    return Err(W::Error::from(format!("Execution error: {:?}", e)));
                }
            }
        }
    }
}

/// Configuration for SagaEngine
#[derive(Debug, Clone)]
pub struct SagaEngineConfig {
    pub snapshot_interval: usize,
    pub max_concurrent_workflows: usize,
    pub replay_timeout: Duration,
}

impl Default for SagaEngineConfig {
    fn default() -> Self {
        Self {
            snapshot_interval: 100,
            max_concurrent_workflows: 1000,
            replay_timeout: Duration::from_secs(30),
        }
    }
}

/// Metrics trait for SagaEngine (reemplaza WorkflowMetrics)
pub trait SagaMetrics: Send + Sync + 'static {
    fn workflow_started(&self, workflow_type: &str);
    fn workflow_completed(&self, workflow_type: &str, duration_ms: u64);
    fn workflow_failed(&self, workflow_type: &str, error: &str);
    fn activity_started(&self, activity_type: &str);
    fn activity_completed(&self, activity_type: &str, duration_ms: u64);
    fn activity_failed(&self, activity_type: &str, error: &str);
    fn replay_duration(&self, workflow_type: &str, duration_ms: u64);
    fn snapshot_taken(&self, workflow_type: &str, events_count: u64);
}
```

### Fase 3: Worker y ActivityRegistry (Semanas 5-6)

#### US-94.4: Worker (Poller de Task Queues)

```rust
// crates/saga-engine/core/src/worker/mod.rs - NUEVO

/// Worker que hace polling de task queues y ejecuta actividades
pub struct Worker<ES, TQ, R>
where
    ES: EventStore,
    TQ: TaskQueue,
    R: HistoryReplayer<WorkflowContext>,
{
    engine: Arc<SagaEngine<ES, TQ, PostgresTimerStore, R>>,
    task_queue: Arc<TQ>,
    activity_registry: Arc<dyn ActivityRegistry>,
    config: WorkerConfig,
    shutdown: broadcast::Sender<()>,
}

impl<ES, TQ, R> Worker<ES, TQ, R>
where
    ES: EventStore + 'static,
    TQ: TaskQueue + 'static,
    R: HistoryReplayer<WorkflowContext, Error = ES::Error> + 'static,
{
    /// Create a new worker
    pub fn new(
        engine: Arc<SagaEngine<ES, TQ, PostgresTimerStore, R>>,
        task_queue: Arc<TQ>,
        activity_registry: Arc<dyn ActivityRegistry>,
    ) -> Self {
        Self {
            engine,
            task_queue,
            activity_registry,
            config: WorkerConfig::default(),
            shutdown: broadcast::channel(1).0,
        }
    }

    /// Start the worker (blocking)
    pub async fn run(&self) -> Result<(), WorkerError> {
        // Ensure consumer exists
        self.task_queue
            .ensure_consumer("saga-worker", &ConsumerConfig::default())
            .await
            .map_err(WorkerError::TaskQueue)?;

        let mut receiver = self.task_queue.subscribe().await.map_err(WorkerError::TaskQueue)?;

        tracing::info!("Worker started, polling for tasks...");

        loop {
            tokio::select! {
                result = receiver.recv() => {
                    match result {
                        Ok(messages) => {
                            for msg in messages {
                                if let Err(e) = self.process_task(&msg).await {
                                    tracing::error!("Task processing failed: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Task receive error: {}", e);
                        }
                    }
                }
                _ = self.shutdown.recv() => {
                    tracing::info!("Worker shutting down...");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Process a single task
    async fn process_task(&self, message: &TaskMessage) -> Result<(), WorkerError> {
        let task = &message.task;

        // Deserialize activity input
        let input: ActivityInput = serde_json::from_slice(&task.payload)
            .map_err(WorkerError::Deserialization)?;

        // Get activity from registry
        let activity = self.activity_registry
            .get(&input.activity_type)
            .ok_or(WorkerError::ActivityNotFound(input.activity_type))?;

        // Execute activity
        let start = std::time::Instant::now();
        let result = activity.execute(input.input).await;
        let duration_ms = start.elapsed().as_millis();

        match &result {
            Ok(output) => {
                // Persist activity result
                self.engine
                    .persist_activity_result(&task.saga_id, &input.activity_id, output)
                    .await?;

                // Ack the task
                self.task_queue.ack(&message.message_id).await?;
            }
            Err(e) => {
                // Nak the task for retry
                self.task_queue.nak(&message.message_id, None).await?;
            }
        }

        Ok(())
    }

    /// Stop the worker gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown.send(());
    }
}

/// Worker configuration
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub max_concurrent_tasks: usize,
    pub poll_batch_size: u64,
    pub poll_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 10,
            poll_batch_size: 10,
            poll_timeout: Duration::from_secs(5),
        }
    }
}
```

#### US-94.5: ActivityRegistry

```rust
// crates/saga-engine/core/src/activity/mod.rs - NUEVO

/// Registry for activities used by the Worker
#[derive(Default)]
pub struct ActivityRegistry {
    activities: dashmap::DashMap<&'static str, Arc<dyn DynActivity>>,
}

impl ActivityRegistry {
    /// Register an activity type
    pub fn register<A: Activity + Clone + Send + Sync + 'static>(&mut self) {
        self.activities.insert(A::TYPE_ID, Arc::new(DynActivityAdapter::<A>::new()));
    }

    /// Get an activity by type ID
    pub fn get(&self, type_id: &str) -> Option<Arc<dyn DynActivity>> {
        self.activities.get(type_id).map(Arc::clone)
    }

    /// Check if an activity is registered
    pub fn contains(&self, type_id: &str) -> bool {
        self.activities.contains_key(type_id)
    }
}

/// Dynamic activity (erased type for storage in registry)
#[async_trait]
pub trait DynActivity: Send + Sync {
    fn type_id(&self) -> &'static str;
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// Adapter for Activity -> DynActivity
struct DynActivityAdapter<A: Activity>(PhantomData<A>);

impl<A: Activity> DynActivityAdapter<A> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

#[async_trait]
impl<A: Activity> DynActivity for DynActivityAdapter<A> {
    fn type_id(&self) -> &'static str {
        A::TYPE_ID
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, String> {
        let input: A::Input = serde_json::from_value(input).map_err(|e| e.to_string())?;
        let output = A::execute(&A::default(), input).await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(output).map_err(|e| e.to_string())
    }
}

/// Activity input/output types for task queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityInput {
    pub activity_type: &'static str,
    pub activity_id: String,
    pub saga_id: String,
    pub input: serde_json::Value,
}
```

### Fase 4: Abstracciones de Infraestructura (Semanas 7-8)

#### US-94.6: CommandBus abstracción

```rust
// crates/saga-engine/core/src/port/command_bus.rs - NUEVO

#[async_trait]
pub trait CommandBus: Send + Sync {
    async fn send<C: Command>(&self, command: C) -> Result<C::Result, CommandBusError>
    where
        C: Command + Send;
}

pub trait Command: Send {
    type Result;
}

#[async_trait]
pub trait CommandHandler<C: Command>: Send {
    async fn handle(&self, command: C) -> Result<C::Result, CommandBusError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CommandBusError {
    #[error("Timeout")]
    Timeout,
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),
    #[error("Handler error: {0}")]
    HandlerError(String),
}
```

#### US-94.7: EventBus abstracción

```rust
// crates/saga-engine/core/src/port/event_bus.rs - NUEVO

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish<E: DomainEvent>(&self, event: E) -> Result<(), EventBusError>
    where
        E: DomainEvent + Send;

    async fn subscribe<E: DomainEvent>(
        &self,
        handler: Arc<dyn EventHandler<E>>,
    ) -> Result<SubscriptionId, EventBusError>
    where
        E: DomainEvent + Send;
}

pub trait DomainEvent: Send + Sync + Clone {
    fn event_type(&self) -> &'static str;
    fn aggregate_id(&self) -> &str;
}

#[async_trait]
pub trait EventHandler<E: DomainEvent>: Send {
    async fn handle(&self, event: E) -> Result<(), EventBusError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("Publish error: {0}")]
    Publish(String),
    #[error("Subscription error: {0}")]
    Subscription(String),
}
```

#### US-94.8: Outbox abstracción

```rust
// crates/saga-engine/core/src/port/outbox.rs - NUEVO

/// Repository trait para persistencia de mensajes outbox
#[async_trait]
pub trait OutboxRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Escribir mensaje al outbox (dentro de transacción)
    async fn write(&self, message: OutboxMessage) -> Result<(), Self::Error>;
    
    /// Obtener mensajes pendientes de publicar
    async fn get_pending(&self, batch_size: usize) -> Result<Vec<OutboxMessage>, Self::Error>;
    
    /// Marcar mensaje como publicado
    async fn mark_published(&self, message_id: Uuid) -> Result<(), Self::Error>;
    
    /// Marcar mensaje como fallido
    async fn mark_failed(&self, message_id: Uuid, error: &str) -> Result<(), Self::Error>;
    
    /// Reintentar mensajes fallidos (actualizar retry_count)
    async fn retry_failed(&self, max_retries: u32) -> Result<usize, Self::Error>;
}

/// Mensaje a publicar vía outbox pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub id: Uuid,
    pub topic: String,
    pub payload: serde_json::Value,
    pub headers: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub retry_count: u32,
    pub status: OutboxStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    Failed,
}
```

#### US-94.8b: OutboxRelay (background worker)

> [!IMPORTANT]
> **NUEVA ESPECIFICACIÓN**: El `OutboxRelay` es el worker background que lee del outbox y publica a NATS/EventBus.

```rust
// crates/saga-engine/core/src/relay/outbox_relay.rs - NUEVO

/// Trait para el relay que procesa mensajes del outbox
#[async_trait]
pub trait OutboxRelay: Send + Sync {
    /// Iniciar el loop de procesamiento
    async fn start(&self, shutdown: broadcast::Receiver<()>) -> Result<(), RelayError>;
    
    /// Procesar un batch de mensajes pendientes
    async fn process_batch(&self, batch_size: usize) -> Result<ProcessResult, RelayError>;
    
    /// Obtener métricas del relay
    fn metrics(&self) -> &OutboxRelayMetrics;
}

/// Configuración del OutboxRelay
#[derive(Debug, Clone)]
pub struct OutboxRelayConfig {
    /// Intervalo de polling (fallback si LISTEN/NOTIFY falla)
    pub poll_interval: Duration,
    /// Tamaño del batch por ciclo
    pub batch_size: usize,
    /// Máximo de reintentos por mensaje
    pub max_retries: u32,
    /// Backoff exponencial
    pub backoff: BackoffConfig,
    /// Habilitar LISTEN/NOTIFY de PostgreSQL
    pub use_pg_notify: bool,
}

impl Default for OutboxRelayConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            batch_size: 100,
            max_retries: 5,
            backoff: BackoffConfig::default(),
            use_pg_notify: true,
        }
    }
}

/// Resultado de procesar un batch
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub processed: usize,
    pub published: usize,
    pub failed: usize,
    pub duration: Duration,
}

/// Métricas del OutboxRelay
#[derive(Debug, Default)]
pub struct OutboxRelayMetrics {
    pub total_processed: AtomicU64,
    pub total_published: AtomicU64,
    pub total_failed: AtomicU64,
    pub last_batch_duration_ms: AtomicU64,
}

/// Errores del relay
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("Repository error: {0}")]
    Repository(String),
    #[error("Publisher error: {0}")]
    Publisher(String),
    #[error("Shutdown requested")]
    Shutdown,
}
```

**Implementaciones:**
- `saga-engine-pg`: `PostgresOutboxRelay` con LISTEN/NOTIFY + polling fallback
- `saga-engine-testing`: `InMemoryOutboxRelay` para tests

---

#### US-94.9: WorkflowClient y WorkflowHandle

> [!IMPORTANT]
> El `WorkflowClient` es la interfaz principal para interactuar con workflows desde fuera del engine.

```rust
// crates/saga-engine/core/src/client/mod.rs - NUEVO

/// Cliente para interactuar con el Saga Engine
/// Equivalente a Temporal's `temporal.NewClient()`
#[async_trait]
pub trait WorkflowClient: Send + Sync {
    /// Iniciar un nuevo workflow
    async fn start_workflow<W: Workflow>(
        &self,
        workflow_id: &str,
        input: W::Input,
        options: StartWorkflowOptions,
    ) -> Result<WorkflowHandle<W>, ClientError>;
    
    /// Obtener handle a un workflow existente
    async fn get_workflow<W: Workflow>(
        &self,
        workflow_id: &str,
        run_id: Option<&str>,
    ) -> Result<WorkflowHandle<W>, ClientError>;
    
    /// Señalar un workflow
    async fn signal_workflow(
        &self,
        workflow_id: &str,
        signal_name: &str,
        payload: serde_json::Value,
    ) -> Result<(), ClientError>;
    
    /// Cancelar un workflow
    async fn cancel_workflow(
        &self,
        workflow_id: &str,
        reason: &str,
    ) -> Result<(), ClientError>;
    
    /// Terminar un workflow (forzado, sin compensación)
    async fn terminate_workflow(
        &self,
        workflow_id: &str,
        reason: &str,
    ) -> Result<(), ClientError>;
    
    /// Listar workflows por tipo o estado
    async fn list_workflows(
        &self,
        query: WorkflowQuery,
    ) -> Result<Vec<WorkflowInfo>, ClientError>;
}

/// Handle a un workflow en ejecución
pub struct WorkflowHandle<W: Workflow> {
    workflow_id: String,
    run_id: String,
    client: Arc<dyn WorkflowClient>,
    _phantom: PhantomData<W>,
}

impl<W: Workflow> WorkflowHandle<W> {
    /// Esperar resultado del workflow
    pub async fn result(&self) -> Result<W::Output, ClientError> {
        // Poll hasta completar o timeout
    }
    
    /// Obtener estado actual (sin bloquear)
    pub async fn describe(&self) -> Result<WorkflowDescription, ClientError> {
        // Query estado sin esperar
    }
    
    /// Enviar señal al workflow
    pub async fn signal(&self, signal_name: &str, payload: serde_json::Value) -> Result<(), ClientError> {
        self.client.signal_workflow(&self.workflow_id, signal_name, payload).await
    }
    
    /// Hacer query al workflow (lectura de estado)
    pub async fn query(&self, query_type: &str, args: serde_json::Value) -> Result<serde_json::Value, ClientError> {
        // Query handler registrado en el workflow
    }
    
    /// Cancelar workflow
    pub async fn cancel(&self, reason: &str) -> Result<(), ClientError> {
        self.client.cancel_workflow(&self.workflow_id, reason).await
    }
}

/// Opciones para iniciar workflow
#[derive(Debug, Clone, Default)]
pub struct StartWorkflowOptions {
    /// Task queue donde ejecutar
    pub task_queue: String,
    /// Tiempo máximo de ejecución del workflow
    pub workflow_execution_timeout: Option<Duration>,
    /// Tiempo máximo de cada task del workflow
    pub workflow_task_timeout: Option<Duration>,
    /// ID-empotency key
    pub idempotency_key: Option<String>,
    /// Permitir workflow duplicado con mismo ID
    pub id_reuse_policy: IdReusePolicy,
    /// Retry policy para el workflow
    pub retry_policy: Option<RetryPolicy>,
    /// Cron schedule (ej: "0 * * * *")
    pub cron_schedule: Option<String>,
    /// Search attributes para visibility
    pub search_attributes: HashMap<String, serde_json::Value>,
    /// Memo (metadata no indexable)
    pub memo: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum IdReusePolicy {
    #[default]
    AllowDuplicate,
    AllowDuplicateFailedOnly,
    RejectDuplicate,
    TerminateIfRunning,
}

/// Información de un workflow
#[derive(Debug, Clone)]
pub struct WorkflowInfo {
    pub workflow_id: String,
    pub run_id: String,
    pub workflow_type: String,
    pub status: WorkflowStatus,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub task_queue: String,
}

/// Estados posibles del workflow
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkflowStatus {
    Running,
    Completed,
    Failed,
    Canceled,
    Terminated,
    ContinuedAsNew,
    TimedOut,
}
```

---

#### US-94.10: Activity Heartbeat

> [!TIP]
> El heartbeat permite que activities de larga duración reporten progreso y detectar si el worker murió.

```rust
// crates/saga-engine/core/src/activity/heartbeat.rs - NUEVO

/// Contexto de actividad con soporte para heartbeat
pub struct ActivityContext {
    /// ID de la actividad
    activity_id: String,
    /// Task token para identificar la ejecución
    task_token: TaskToken,
    /// Canal para enviar heartbeats
    heartbeat_sender: mpsc::Sender<HeartbeatPayload>,
    /// Receiver de cancelación
    cancellation: CancellationToken,
    /// Último heartbeat details (para recovery)
    last_heartbeat_details: Option<serde_json::Value>,
}

impl ActivityContext {
    /// Registrar heartbeat con detalles de progreso
    /// 
    /// # Ejemplo
    /// ```rust
    /// async fn process_large_file(ctx: &ActivityContext, path: &str) -> Result<()> {
    ///     let file = File::open(path).await?;
    ///     let total_lines = count_lines(&file);
    ///     
    ///     for (i, line) in file.lines().enumerate() {
    ///         process_line(line)?;
    ///         
    ///         // Reportar progreso cada 1000 líneas
    ///         if i % 1000 == 0 {
    ///             ctx.heartbeat(json!({
    ///                 "processed": i,
    ///                 "total": total_lines,
    ///                 "percent": (i * 100) / total_lines
    ///             })).await?;
    ///         }
    ///         
    ///         // Verificar si fue cancelado
    ///         if ctx.is_cancelled() {
    ///             return Err(ActivityError::Cancelled);
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub async fn heartbeat(&self, details: serde_json::Value) -> Result<(), HeartbeatError> {
        self.heartbeat_sender.send(HeartbeatPayload {
            task_token: self.task_token.clone(),
            details,
        }).await.map_err(|_| HeartbeatError::ChannelClosed)
    }
    
    /// Verificar si la actividad fue cancelada
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    
    /// Obtener detalles del último heartbeat (para retomar progreso)
    pub fn last_heartbeat_details(&self) -> Option<&serde_json::Value> {
        self.last_heartbeat_details.as_ref()
    }
    
    /// Obtener info de la actividad
    pub fn info(&self) -> &ActivityInfo {
        &self.info
    }
}

/// Información de la actividad en ejecución
#[derive(Debug, Clone)]
pub struct ActivityInfo {
    pub activity_id: String,
    pub activity_type: String,
    pub workflow_id: String,
    pub workflow_type: String,
    pub task_queue: String,
    pub attempt: u32,
    pub scheduled_time: DateTime<Utc>,
    pub started_time: DateTime<Utc>,
    pub heartbeat_timeout: Duration,
    pub schedule_to_close_timeout: Duration,
    pub start_to_close_timeout: Duration,
}

/// Configuración de heartbeat para actividades
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Intervalo máximo entre heartbeats (si se excede, activity timeout)
    pub heartbeat_timeout: Duration,
    /// Intervalo de throttling (no enviar más frecuente que esto)
    pub throttle_interval: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(30),
            throttle_interval: Duration::from_secs(1),
        }
    }
}
```

---

#### US-94.11: Local Activities

> [!NOTE]
> Local Activities son actividades ligeras que se ejecutan en el mismo proceso que el workflow, sin pasar por la task queue.

```rust
// crates/saga-engine/core/src/activity/local.rs - NUEVO

/// Trait para actividades locales (sin task queue)
#[async_trait]
pub trait LocalActivity: Send + Sync + 'static {
    const TYPE_ID: &'static str;
    
    type Input: Serialize + for<'de> Deserialize<'de> + Send;
    type Output: Serialize + for<'de> Deserialize<'de> + Send;
    type Error: std::error::Error + Send + Sync;
    
    /// Ejecutar localmente (sin workflow context completo)
    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

/// Extensión de WorkflowContext para local activities
impl WorkflowContext {
    /// Ejecutar local activity (en el mismo proceso, sin task queue)
    /// 
    /// **Cuándo usar**:
    /// - Operaciones muy rápidas (< 10s)
    /// - No necesitan retry complejo
    /// - No requieren heartbeat
    /// - Código que debe ser determinista pero necesita I/O menor
    /// 
    /// **Cuándo NO usar**:
    /// - Operaciones lentas (> 10s)
    /// - Necesitan heartbeat
    /// - Alto riesgo de fallos
    pub async fn execute_local_activity<A: LocalActivity>(
        &mut self,
        activity: &A,
        input: A::Input,
        options: LocalActivityOptions,
    ) -> Result<A::Output, LocalActivityError<A::Error>> {
        // 1. Verificar si ya se ejecutó (replay)
        let activity_id = self.next_local_activity_id(A::TYPE_ID);
        if let Some(cached) = self.get_cached_local_activity(&activity_id) {
            return Ok(cached);
        }
        
        // 2. Ejecutar directamente (sin task queue)
        let result = tokio::time::timeout(
            options.start_to_close_timeout,
            activity.execute(input)
        ).await;
        
        // 3. Persistir resultado
        match result {
            Ok(Ok(output)) => {
                self.record_local_activity_completed(&activity_id, &output);
                Ok(output)
            }
            Ok(Err(e)) => Err(LocalActivityError::Failed(e)),
            Err(_) => Err(LocalActivityError::Timeout),
        }
    }
}

/// Opciones para local activities
#[derive(Debug, Clone)]
pub struct LocalActivityOptions {
    /// Timeout para la ejecución completa
    pub start_to_close_timeout: Duration,
    /// Número de reintentos locales
    pub retry_attempts: u32,
    /// Backoff entre reintentos
    pub retry_backoff: Duration,
}

impl Default for LocalActivityOptions {
    fn default() -> Self {
        Self {
            start_to_close_timeout: Duration::from_secs(10),
            retry_attempts: 3,
            retry_backoff: Duration::from_millis(100),
        }
    }
}
```

---

#### US-94.12: Workflow Queries

> [!TIP]
> Las queries permiten leer el estado del workflow sin mutarlo, útil para UIs y monitoreo.

```rust
// crates/saga-engine/core/src/workflow/query.rs - NUEVO

/// Trait para workflows que soportan queries
pub trait QueryableWorkflow: Workflow {
    /// Manejar una query (lectura de estado, NO DEBE MUTAR)
    fn handle_query(
        &self,
        ctx: &WorkflowContext,
        query_type: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, QueryError>;
}

/// Macro para registrar query handlers
/// 
/// ```rust
/// struct OrderWorkflow {
///     order_status: OrderStatus,
///     items: Vec<OrderItem>,
/// }
/// 
/// impl QueryableWorkflow for OrderWorkflow {
///     fn handle_query(&self, ctx: &WorkflowContext, query_type: &str, args: Value) -> Result<Value, QueryError> {
///         match query_type {
///             "get_status" => Ok(json!({ "status": self.order_status })),
///             "get_items" => Ok(json!({ "items": self.items })),
///             "get_total" => {
///                 let total: f64 = self.items.iter().map(|i| i.price).sum();
///                 Ok(json!({ "total": total }))
///             }
///             _ => Err(QueryError::UnknownQueryType(query_type.to_string())),
///         }
///     }
/// }
/// ```

/// Extensión de WorkflowContext para queries
impl WorkflowContext {
    /// Registrar un query handler (llamar en run())
    pub fn set_query_handler<F, T>(&mut self, query_type: &str, handler: F)
    where
        F: Fn(&serde_json::Value) -> Result<T, QueryError> + Send + Sync + 'static,
        T: Serialize,
    {
        self.query_handlers.insert(query_type.to_string(), Box::new(move |args| {
            handler(args).map(|r| serde_json::to_value(r).unwrap())
        }));
    }
}

/// Errores de queries
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Unknown query type: {0}")]
    UnknownQueryType(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}
```

---

#### US-94.13: Continue-as-New

> [!IMPORTANT]
> Continue-as-New permite reiniciar un workflow con un historial limpio, crucial para workflows de larga duración.

```rust
// crates/saga-engine/core/src/workflow/continue_as_new.rs - NUEVO

/// Extensión de WorkflowContext para continue-as-new
impl WorkflowContext {
    /// Continuar el workflow con un nuevo run (historial limpio)
    /// 
    /// **Cuándo usar**:
    /// - Workflows infinitos (ej: cron jobs)
    /// - Historial muy grande (> 10,000 eventos)
    /// - Cambio de versión del workflow
    /// 
    /// # Ejemplo
    /// ```rust
    /// async fn run(&self, ctx: &mut WorkflowContext, input: Input) -> Result<Output, Error> {
    ///     loop {
    ///         // Procesar batch
    ///         let batch = ctx.execute_activity(&FetchBatch, input.cursor).await?;
    ///         
    ///         process_batch(&batch).await?;
    ///         
    ///         if batch.has_more {
    ///             // Continuar con nuevo historial
    ///             ctx.continue_as_new(Input { cursor: batch.next_cursor })?;
    ///         } else {
    ///             break;
    ///         }
    ///     }
    ///     Ok(Output::done())
    /// }
    /// ```
    pub fn continue_as_new<I: Serialize>(&mut self, input: I) -> Result<!, ContinueAsNewError> {
        // Serialize input
        let input_json = serde_json::to_value(input)?;
        
        // Emit ContinueAsNew event
        self.pending_commands.push(WorkflowCommand::ContinueAsNew {
            input: input_json,
            task_queue: self.config.task_queue.clone(),
            workflow_type: self.workflow_type.clone(),
        });
        
        // Return special error to stop execution
        Err(ContinueAsNewError::Requested)
    }
    
    /// Obtener sugerencia si debería hacer continue-as-new
    pub fn should_continue_as_new(&self) -> bool {
        self.metadata.replay_count > self.config.max_history_events.unwrap_or(10_000)
    }
}

/// Error especial para continue-as-new (no es un error real)
#[derive(Debug)]
pub enum ContinueAsNewError {
    Requested,  // Normal flow, not an error
    Serialization(serde_json::Error),
}
```

---

#### US-94.14: Side Effects

> [!WARNING]
> Side Effects permiten ejecutar código no-determinista de forma controlada (ej: random, timestamps).

```rust
// crates/saga-engine/core/src/workflow/side_effect.rs - NUEVO

impl WorkflowContext {
    /// Ejecutar side effect (código no-determinista que se ejecuta una vez y se recuerda)
    /// 
    /// **Cuándo usar**:
    /// - Generar UUIDs
    /// - Obtener timestamps "ahora"
    /// - Números aleatorios
    /// - Cualquier código que daría resultados diferentes en replay
    /// 
    /// # Ejemplo
    /// ```rust
    /// async fn run(&self, ctx: &mut WorkflowContext, input: Input) -> Result<Output, Error> {
    ///     // ❌ MALO: No determinista, falla en replay
    ///     let bad_id = Uuid::new_v4();
    ///     
    ///     // ✅ BUENO: Side effect recordado
    ///     let good_id: Uuid = ctx.side_effect(|| Uuid::new_v4());
    ///     
    ///     // ❌ MALO: Diferente en cada replay
    ///     let bad_now = Utc::now();
    ///     
    ///     // ✅ BUENO: Tiempo determinista
    ///     let good_now = ctx.current_time();
    /// }
    /// ```
    pub fn side_effect<T, F>(&mut self, func: F) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
        F: FnOnce() -> T,
    {
        let side_effect_id = self.next_side_effect_id();
        
        // En replay, retornar valor guardado
        if let Some(cached) = self.get_cached_side_effect(&side_effect_id) {
            return serde_json::from_value(cached).unwrap();
        }
        
        // Primera ejecución: ejecutar y guardar
        let result = func();
        self.record_side_effect(&side_effect_id, &serde_json::to_value(&result).unwrap());
        result
    }
    
    /// Obtener tiempo actual determinista (guardado en primer run)
    pub fn current_time(&mut self) -> DateTime<Utc> {
        self.side_effect(|| Utc::now())
    }
    
    /// Generar UUID determinista
    pub fn random_uuid(&mut self) -> Uuid {
        self.side_effect(|| Uuid::new_v4())
    }
}
```

---

#### US-94.15: Workflow Versioning

> [!IMPORTANT]
> Workflow Versioning permite cambiar código de workflows sin romper ejecuciones en vuelo.

```rust
// crates/saga-engine/core/src/workflow/versioning.rs - NUEVO

impl WorkflowContext {
    /// Obtener versión para branching de código
    /// 
    /// Permite evolucionar workflows sin romper ejecuciones existentes.
    /// 
    /// # Ejemplo: Añadir nuevo paso
    /// ```rust
    /// async fn run(&self, ctx: &mut WorkflowContext, input: Input) -> Result<Output, Error> {
    ///     let step1 = ctx.execute_activity(&Step1, input.clone()).await?;
    ///     
    ///     // Versión 1: sin step intermedio
    ///     // Versión 2: nuevo step intermedio añadido
    ///     let version = ctx.get_version("add-validation-step", 1, 2);
    ///     
    ///     let step2_input = if version >= 2 {
    ///         // Nuevo código para workflows iniciados con v2+
    ///         ctx.execute_activity(&ValidationStep, step1.clone()).await?
    ///     } else {
    ///         // Código viejo para workflows en vuelo (v1)
    ///         step1.clone()
    ///     };
    ///     
    ///     let result = ctx.execute_activity(&Step2, step2_input).await?;
    ///     Ok(result)
    /// }
    /// ```
    /// 
    /// # Parámetros
    /// - `change_id`: Identificador único del cambio
    /// - `min_supported`: Versión mínima que el código actual soporta
    /// - `max_supported`: Versión máxima (actual)
    pub fn get_version(&mut self, change_id: &str, min_supported: i32, max_supported: i32) -> i32 {
        // En replay: retornar versión guardada
        if let Some(version) = self.get_recorded_version(change_id) {
            if version < min_supported || version > max_supported {
                panic!(
                    "Version {} for change '{}' is outside supported range [{}, {}]",
                    version, change_id, min_supported, max_supported
                );
            }
            return version;
        }
        
        // Primera ejecución: usar versión máxima y guardar
        self.record_version(change_id, max_supported);
        max_supported
    }
    
    /// Marcar un cambio como deprecado (para eliminar código viejo)
    pub fn deprecate_version(&mut self, change_id: &str, deprecated_version: i32) {
        let version = self.get_recorded_version(change_id);
        if version == Some(deprecated_version) {
            tracing::warn!(
                change_id = change_id,
                version = deprecated_version,
                "Workflow is running deprecated version"
            );
        }
    }
}
```

---

#### US-94.16: Child Workflows

```rust
// crates/saga-engine/core/src/workflow/child.rs - NUEVO

impl WorkflowContext {
    /// Ejecutar un workflow hijo
    /// 
    /// # Ejemplo
    /// ```rust
    /// async fn run(&self, ctx: &mut WorkflowContext, order: Order) -> Result<OrderResult, Error> {
    ///     // Ejecutar workflow de pago como hijo
    ///     let payment = ctx.execute_child_workflow::<PaymentWorkflow>(
    ///         &format!("payment-{}", order.id),
    ///         PaymentInput { amount: order.total, ... },
    ///         ChildWorkflowOptions::default(),
    ///     ).await?;
    ///     
    ///     // Ejecutar workflow de envío en paralelo
    ///     let shipping_handle = ctx.start_child_workflow::<ShippingWorkflow>(
    ///         &format!("shipping-{}", order.id),
    ///         ShippingInput { address: order.address, ... },
    ///         ChildWorkflowOptions::default(),
    ///     ).await?;
    ///     
    ///     // Esperar resultado después
    ///     let shipping = shipping_handle.result().await?;
    ///     
    ///     Ok(OrderResult { payment, shipping })
    /// }
    /// ```
    pub async fn execute_child_workflow<W: Workflow>(
        &mut self,
        workflow_id: &str,
        input: W::Input,
        options: ChildWorkflowOptions,
    ) -> Result<W::Output, ChildWorkflowError<W::Error>> {
        let handle = self.start_child_workflow::<W>(workflow_id, input, options).await?;
        handle.result().await
    }
    
    /// Iniciar workflow hijo sin esperar resultado
    pub async fn start_child_workflow<W: Workflow>(
        &mut self,
        workflow_id: &str,
        input: W::Input,
        options: ChildWorkflowOptions,
    ) -> Result<ChildWorkflowHandle<W>, ChildWorkflowError<W::Error>> {
        // 1. Verificar si ya se inició (replay)
        let child_id = self.next_child_workflow_id(W::TYPE_ID, workflow_id);
        if let Some(run_id) = self.get_started_child_workflow(&child_id) {
            return Ok(ChildWorkflowHandle::new(workflow_id, run_id, self));
        }
        
        // 2. Emitir comando para iniciar hijo
        self.pending_commands.push(WorkflowCommand::StartChildWorkflow {
            workflow_id: workflow_id.to_string(),
            workflow_type: W::TYPE_ID.to_string(),
            input: serde_json::to_value(input)?,
            options,
        });
        
        // 3. Retornar handle (result() bloqueará hasta completar)
        Ok(ChildWorkflowHandle::pending(workflow_id, self))
    }
}

/// Opciones para child workflows
#[derive(Debug, Clone, Default)]
pub struct ChildWorkflowOptions {
    pub task_queue: Option<String>,
    pub execution_timeout: Option<Duration>,
    pub parent_close_policy: ParentClosePolicy,
    pub retry_policy: Option<RetryPolicy>,
}

/// Qué hacer con el hijo cuando el padre termina
#[derive(Debug, Clone, Copy, Default)]
pub enum ParentClosePolicy {
    #[default]
    Terminate,    // Terminar el hijo
    Abandon,      // Dejar corriendo
    RequestCancel, // Enviar cancel y esperar
}
```

---

#### US-94.17: Worker y Registries

```rust
// crates/saga-engine/core/src/worker/mod.rs - NUEVO

/// Worker que procesa workflows y activities
pub struct SagaWorker {
    /// Task queue que escucha
    task_queue: String,
    /// Registry de workflows
    workflow_registry: WorkflowRegistry,
    /// Registry de activities
    activity_registry: ActivityRegistry,
    /// Configuración
    config: WorkerConfig,
    /// Dependencies
    event_store: Arc<dyn EventStore>,
    task_queue_client: Arc<dyn TaskQueue>,
    timer_store: Arc<dyn TimerStore>,
}

impl SagaWorker {
    pub fn builder(task_queue: &str) -> WorkerBuilder {
        WorkerBuilder::new(task_queue)
    }
    
    /// Registrar un workflow
    pub fn register_workflow<W: Workflow + 'static>(&mut self, workflow: W) {
        self.workflow_registry.register::<W>(workflow);
    }
    
    /// Registrar una activity
    pub fn register_activity<A: Activity + 'static>(&mut self, activity: A) {
        self.activity_registry.register::<A>(activity);
    }
    
    /// Iniciar el worker (loop principal)
    pub async fn run(&self, shutdown: broadcast::Receiver<()>) -> Result<(), WorkerError> {
        loop {
            tokio::select! {
                // Procesar workflow tasks
                Some(task) = self.fetch_workflow_task() => {
                    self.process_workflow_task(task).await?;
                }
                // Procesar activity tasks
                Some(task) = self.fetch_activity_task() => {
                    self.process_activity_task(task).await?;
                }
                // Procesar timers vencidos
                Some(timer) = self.fetch_expired_timers() => {
                    self.process_timer(timer).await?;
                }
                // Shutdown
                _ = shutdown.recv() => {
                    tracing::info!("Worker shutdown requested");
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Registry de workflows
#[derive(Default)]
pub struct WorkflowRegistry {
    workflows: HashMap<&'static str, Arc<dyn DynWorkflow>>,
}

impl WorkflowRegistry {
    pub fn register<W: Workflow + 'static>(&mut self, workflow: W) {
        self.workflows.insert(W::TYPE_ID, Arc::new(WorkflowAdapter(workflow)));
    }
    
    pub fn get(&self, type_id: &str) -> Option<Arc<dyn DynWorkflow>> {
        self.workflows.get(type_id).cloned()
    }
}

/// Registry de activities
#[derive(Default)]
pub struct ActivityRegistry {
    activities: HashMap<&'static str, Arc<dyn DynActivity>>,
}

impl ActivityRegistry {
    pub fn register<A: Activity + 'static>(&mut self, activity: A) {
        self.activities.insert(A::TYPE_ID, Arc::new(ActivityAdapter(activity)));
    }
    
    pub fn get(&self, type_id: &str) -> Option<Arc<dyn DynActivity>> {
        self.activities.get(type_id).cloned()
    }
}

/// Configuración del worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Número máximo de workflow tasks concurrentes
    pub max_concurrent_workflow_tasks: usize,
    /// Número máximo de activity tasks concurrentes  
    pub max_concurrent_activity_tasks: usize,
    /// Timeout para workflow task poll
    pub workflow_task_poll_timeout: Duration,
    /// Timeout para activity task poll
    pub activity_task_poll_timeout: Duration,
    /// Intervalo para verificar timers
    pub timer_check_interval: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workflow_tasks: 100,
            max_concurrent_activity_tasks: 100,
            workflow_task_poll_timeout: Duration::from_secs(60),
            activity_task_poll_timeout: Duration::from_secs(60),
            timer_check_interval: Duration::from_secs(1),
        }
    }
}
```

---

#### US-94.18: SagaEngine Central

```rust
// crates/saga-engine/core/src/engine.rs - NUEVO

/// Engine principal de sagas (orchestrator central)
/// 
/// # Ejemplo de uso
/// ```rust
/// // Crear engine
/// let engine = SagaEngine::builder()
///     .with_event_store(postgres_event_store)
///     .with_task_queue(nats_task_queue)
///     .with_timer_store(postgres_timer_store)
///     .with_outbox_relay(hybrid_outbox_relay)
///     .build()?;
/// 
/// // Crear client
/// let client = engine.client();
/// 
/// // Iniciar workflow
/// let handle = client.start_workflow::<OrderWorkflow>(
///     "order-123",
///     OrderInput { items: vec![...] },
///     StartWorkflowOptions::default(),
/// ).await?;
/// 
/// // Esperar resultado
/// let result = handle.result().await?;
/// ```
pub struct SagaEngine {
    event_store: Arc<dyn EventStore>,
    task_queue: Arc<dyn TaskQueue>,
    timer_store: Arc<dyn TimerStore>,
    outbox_relay: Arc<dyn OutboxRelay>,
    signal_dispatcher: Arc<dyn SignalDispatcher>,
    replayer: Arc<dyn HistoryReplayer<WorkflowContext>>,
    metrics: Arc<dyn WorkflowMetrics>,
}

impl SagaEngine {
    pub fn builder() -> SagaEngineBuilder {
        SagaEngineBuilder::default()
    }
    
    /// Obtener client para interactuar con workflows
    pub fn client(&self) -> impl WorkflowClient {
        SagaEngineClient::new(self.clone())
    }
    
    /// Crear un worker para esta engine
    pub fn worker(&self, task_queue: &str) -> WorkerBuilder {
        WorkerBuilder::new(task_queue)
            .with_event_store(self.event_store.clone())
            .with_task_queue(self.task_queue.clone())
            .with_timer_store(self.timer_store.clone())
            .with_replayer(self.replayer.clone())
    }
    
    /// Iniciar servicios background (timers, outbox relay)
    pub async fn start_background_services(
        &self,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        let outbox_handle = tokio::spawn({
            let relay = self.outbox_relay.clone();
            let shutdown = shutdown.resubscribe();
            async move { relay.start(shutdown).await }
        });
        
        let timer_handle = tokio::spawn({
            let timer_store = self.timer_store.clone();
            let task_queue = self.task_queue.clone();
            let shutdown = shutdown.resubscribe();
            async move { 
                timer_scheduler_loop(timer_store, task_queue, shutdown).await 
            }
        });
        
        // Esperar shutdown
        shutdown.recv().await?;
        outbox_handle.await??;
        timer_handle.await??;
        
        Ok(())
    }
}

/// Builder para SagaEngine
#[derive(Default)]
pub struct SagaEngineBuilder {
    event_store: Option<Arc<dyn EventStore>>,
    task_queue: Option<Arc<dyn TaskQueue>>,
    timer_store: Option<Arc<dyn TimerStore>>,
    outbox_relay: Option<Arc<dyn OutboxRelay>>,
    signal_dispatcher: Option<Arc<dyn SignalDispatcher>>,
    replayer: Option<Arc<dyn HistoryReplayer<WorkflowContext>>>,
    metrics: Option<Arc<dyn WorkflowMetrics>>,
}

impl SagaEngineBuilder {
    pub fn with_event_store<E: EventStore + 'static>(mut self, store: E) -> Self {
        self.event_store = Some(Arc::new(store));
        self
    }
    
    pub fn with_task_queue<Q: TaskQueue + 'static>(mut self, queue: Q) -> Self {
        self.task_queue = Some(Arc::new(queue));
        self
    }
    
    // ... más métodos with_*
    
    pub fn build(self) -> Result<SagaEngine, BuilderError> {
        Ok(SagaEngine {
            event_store: self.event_store.ok_or(BuilderError::MissingEventStore)?,
            task_queue: self.task_queue.ok_or(BuilderError::MissingTaskQueue)?,
            timer_store: self.timer_store.ok_or(BuilderError::MissingTimerStore)?,
            // ... defaults para opcionales
        })
    }
}
```

---

#### US-94.19: WorkflowTestEnvironment (Time Skipping)

> [!IMPORTANT]
> **CRÍTICO para testabilidad**: Permite testear workflows que duran "meses" en milisegundos mediante salto de tiempo virtual.

```rust
// crates/saga-engine/testing/src/test_env.rs - NUEVO

/// Entorno de test con control de tiempo virtual
/// 
/// # Ejemplo
/// ```rust
/// #[tokio::test]
/// async fn test_timeout_workflow() {
///     let env = WorkflowTestEnvironment::new();
///     
///     // Registrar workflow y activities
///     env.register_workflow::<RenewalWorkflow>();
///     env.register_activity::<SendReminderActivity>();
///     
///     // Iniciar workflow
///     let handle = env.start_workflow::<RenewalWorkflow>(
///         "renewal-123",
///         RenewalInput { subscription_id: "sub-1" },
///     ).await.unwrap();
///     
///     // Avanzar tiempo virtual 30 días (sin esperar!)
///     env.skip_time(Duration::from_days(30)).await;
///     
///     // Verificar que se envió reminder
///     assert!(env.activity_was_called::<SendReminderActivity>());
///     
///     // Avanzar 30 días más
///     env.skip_time(Duration::from_days(30)).await;
///     
///     // Workflow debería haber terminado
///     let result = handle.result().await.unwrap();
///     assert_eq!(result.status, "renewed");
/// }
/// ```
pub struct WorkflowTestEnvironment {
    /// Reloj virtual controlable
    virtual_clock: Arc<VirtualClock>,
    /// Event store in-memory
    event_store: Arc<InMemoryEventStore>,
    /// Task queue in-memory
    task_queue: Arc<InMemoryTaskQueue>,
    /// Timer store con tiempo virtual
    timer_store: Arc<VirtualTimerStore>,
    /// Registries
    workflow_registry: WorkflowRegistry,
    activity_registry: ActivityRegistry,
    /// Historial de llamadas a activities (para assertions)
    activity_call_log: Arc<RwLock<Vec<ActivityCallRecord>>>,
    /// Interceptores para mock de activities
    activity_mocks: Arc<RwLock<HashMap<String, Box<dyn ActivityMock>>>>,
}

impl WorkflowTestEnvironment {
    pub fn new() -> Self {
        Self::with_clock(VirtualClock::new())
    }
    
    /// Crear con reloj fijo en un timestamp específico
    pub fn with_start_time(start: DateTime<Utc>) -> Self {
        Self::with_clock(VirtualClock::starting_at(start))
    }
    
    /// Registrar workflow
    pub fn register_workflow<W: Workflow + 'static>(&mut self, workflow: W) {
        self.workflow_registry.register::<W>(workflow);
    }
    
    /// Registrar activity real
    pub fn register_activity<A: Activity + 'static>(&mut self, activity: A) {
        self.activity_registry.register::<A>(activity);
    }
    
    /// Mockear una activity para retornar valor específico
    pub fn mock_activity<A: Activity>(&mut self, result: A::Output)
    where
        A::Output: Clone + 'static,
    {
        self.activity_mocks.write().insert(
            A::TYPE_ID.to_string(),
            Box::new(FixedResultMock::new(result)),
        );
    }
    
    /// Mockear activity con función custom
    pub fn mock_activity_with<A, F>(&mut self, handler: F)
    where
        A: Activity,
        F: Fn(A::Input) -> Result<A::Output, A::Error> + Send + Sync + 'static,
    {
        self.activity_mocks.write().insert(
            A::TYPE_ID.to_string(),
            Box::new(FnMock::new(handler)),
        );
    }
    
    /// Iniciar workflow
    pub async fn start_workflow<W: Workflow>(
        &self,
        workflow_id: &str,
        input: W::Input,
    ) -> Result<TestWorkflowHandle<W>, TestError> {
        // Crear tarea inicial
        let saga_id = SagaId(workflow_id.to_string());
        
        // Emitir evento de inicio
        self.event_store.append_event(
            &saga_id,
            0,
            &HistoryEvent::new(
                EventId(0),
                saga_id.clone(),
                EventType::WorkflowExecutionStarted,
                EventCategory::Workflow,
                serde_json::to_value(&input)?,
            ),
        ).await?;
        
        // Encolar task
        self.task_queue.publish(&Task {
            id: TaskId(format!("wf-{}", workflow_id)),
            saga_id: saga_id.clone(),
            task_type: TaskType::WorkflowTask,
            payload: serde_json::to_value(&input)?,
            created_at: self.virtual_clock.now(),
        }).await?;
        
        // Procesar task inmediatamente
        self.process_pending_tasks().await?;
        
        Ok(TestWorkflowHandle {
            workflow_id: workflow_id.to_string(),
            env: self.clone(),
            _phantom: PhantomData,
        })
    }
    
    /// ⚡ SALTAR TIEMPO VIRTUAL
    /// 
    /// Avanza el reloj virtual y dispara todos los timers que venzan en ese intervalo
    pub async fn skip_time(&self, duration: Duration) {
        let end_time = self.virtual_clock.now() + duration;
        
        loop {
            // Buscar próximo timer
            let next_timer = self.timer_store.next_timer_before(end_time).await;
            
            match next_timer {
                Some(timer) => {
                    // Avanzar reloj hasta el timer
                    self.virtual_clock.advance_to(timer.scheduled_at);
                    
                    // Disparar timer
                    self.timer_store.fire_timer(&timer.id).await.unwrap();
                    
                    // Encolar workflow task
                    self.enqueue_workflow_task(&timer.saga_id).await;
                    
                    // Procesar tasks pendientes
                    self.process_pending_tasks().await.unwrap();
                }
                None => {
                    // No más timers, avanzar al final
                    self.virtual_clock.advance_to(end_time);
                    break;
                }
            }
        }
    }
    
    /// Verificar si una activity fue llamada
    pub fn activity_was_called<A: Activity>(&self) -> bool {
        self.activity_call_log.read()
            .iter()
            .any(|r| r.activity_type == A::TYPE_ID)
    }
    
    /// Obtener número de veces que se llamó una activity
    pub fn activity_call_count<A: Activity>(&self) -> usize {
        self.activity_call_log.read()
            .iter()
            .filter(|r| r.activity_type == A::TYPE_ID)
            .count()
    }
    
    /// Obtener inputs con los que se llamó una activity
    pub fn activity_inputs<A: Activity>(&self) -> Vec<A::Input>
    where
        A::Input: DeserializeOwned,
    {
        self.activity_call_log.read()
            .iter()
            .filter(|r| r.activity_type == A::TYPE_ID)
            .map(|r| serde_json::from_value(r.input.clone()).unwrap())
            .collect()
    }
    
    /// Obtener tiempo actual virtual
    pub fn current_time(&self) -> DateTime<Utc> {
        self.virtual_clock.now()
    }
    
    /// Procesar todas las tasks pendientes
    async fn process_pending_tasks(&self) -> Result<(), TestError> {
        while let Some(task) = self.task_queue.try_fetch().await? {
            match task.task_type {
                TaskType::WorkflowTask => {
                    self.process_workflow_task(&task).await?;
                }
                TaskType::ActivityTask => {
                    self.process_activity_task(&task).await?;
                }
            }
        }
        Ok(())
    }
}

/// Reloj virtual para tests
#[derive(Debug)]
pub struct VirtualClock {
    current_time: AtomicU64, // millis since epoch
}

impl VirtualClock {
    pub fn new() -> Self {
        Self { current_time: AtomicU64::new(0) }
    }
    
    pub fn starting_at(time: DateTime<Utc>) -> Self {
        Self { current_time: AtomicU64::new(time.timestamp_millis() as u64) }
    }
    
    pub fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.current_time.load(Ordering::SeqCst) as i64).unwrap()
    }
    
    pub fn advance(&self, duration: Duration) {
        self.current_time.fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
    }
    
    pub fn advance_to(&self, time: DateTime<Utc>) {
        self.current_time.store(time.timestamp_millis() as u64, Ordering::SeqCst);
    }
}

/// Registro de llamada a activity
#[derive(Debug, Clone)]
pub struct ActivityCallRecord {
    pub activity_type: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}
```

---

#### US-94.20: Determinism Safeguards

> [!CAUTION]
> **CRÍTICO para correctitud**: Prevenir código no-determinista en workflows que causaría fallos de replay.

```rust
// crates/saga-engine/macros/src/lib.rs - NUEVO

/// Macro para validar y anotar workflows
/// 
/// Realiza las siguientes comprobaciones en tiempo de compilación:
/// 1. Detecta uso de `std::time::Instant::now()` o `chrono::Utc::now()`
/// 2. Detecta uso de `rand::*`
/// 3. Detecta iteración sobre `HashMap` (orden no determinista)
/// 4. Inyecta panic hook para detectar violaciones en runtime
/// 
/// # Ejemplo
/// ```rust
/// #[workflow]  // ← Aplica safeguards
/// impl Workflow for OrderWorkflow {
///     async fn run(&self, ctx: &mut WorkflowContext, input: OrderInput) -> Result<OrderOutput, OrderError> {
///         // ❌ ERROR DE COMPILACIÓN: uso de Utc::now() detectado
///         // let now = Utc::now();
///         
///         // ✅ CORRECTO: usar método del contexto
///         let now = ctx.current_time();
///         
///         // ❌ ERROR DE COMPILACIÓN: HashMap iteration order is non-deterministic
///         // for (k, v) in my_hashmap { ... }
///         
///         // ✅ CORRECTO: usar BTreeMap o sorted keys
///         for k in my_map.keys().sorted() { ... }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn workflow(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    
    // Verificar que implementa Workflow trait
    validate_workflow_impl(&input);
    
    // Escanear cuerpo del método run() buscando patrones prohibidos
    let violations = scan_for_determinism_violations(&input);
    
    if !violations.is_empty() {
        return compile_error_for_violations(violations);
    }
    
    // Inyectar wrapper con panic hook
    wrap_with_determinism_guard(input)
}

// Runtime determinism guard
pub struct DeterminismGuard {
    workflow_id: String,
    is_replaying: bool,
}

impl DeterminismGuard {
    /// Instalar como panic hook durante ejecución del workflow
    pub fn install(workflow_id: &str, is_replaying: bool) -> DeterminismGuardHandle {
        let guard = Arc::new(Self {
            workflow_id: workflow_id.to_string(),
            is_replaying,
        });
        
        // Interceptar llamadas prohibidas (via thread-local)
        CURRENT_GUARD.with(|g| *g.borrow_mut() = Some(guard.clone()));
        
        DeterminismGuardHandle { guard }
    }
    
    /// Verificar si estamos en contexto de workflow
    pub fn check_determinism(operation: &str) {
        CURRENT_GUARD.with(|g| {
            if let Some(guard) = g.borrow().as_ref() {
                if guard.is_replaying {
                    panic!(
                        "Non-deterministic operation '{}' detected in workflow '{}' during replay! \
                         Use WorkflowContext methods instead.",
                        operation, guard.workflow_id
                    );
                } else {
                    tracing::warn!(
                        workflow_id = %guard.workflow_id,
                        operation = operation,
                        "Potentially non-deterministic operation in workflow"
                    );
                }
            }
        });
    }
}

thread_local! {
    static CURRENT_GUARD: RefCell<Option<Arc<DeterminismGuard>>> = RefCell::new(None);
}

/// Wrapper para Utc::now() que verifica contexto
pub fn checked_utc_now() -> DateTime<Utc> {
    DeterminismGuard::check_determinism("Utc::now()");
    Utc::now()
}

/// Lista de patrones prohibidos a detectar
const FORBIDDEN_PATTERNS: &[(&str, &str)] = &[
    ("Utc::now()", "Use ctx.current_time() instead"),
    ("Instant::now()", "Use ctx.current_time() instead"),
    ("thread_rng()", "Use ctx.random() instead"),
    ("rand::random()", "Use ctx.random() instead"),
    ("Uuid::new_v4()", "Use ctx.random_uuid() instead"),
    ("HashMap::iter()", "Use BTreeMap or .keys().sorted()"),
    ("HashSet::iter()", "Use BTreeSet or .iter().sorted()"),
    ("std::thread::spawn", "Use activities for concurrent work"),
    ("tokio::spawn", "Use ctx.execute_activity() for async work"),
];
```

---

#### US-94.21: Error Classification System

> [!WARNING]
> **Esencial para retry logic**: Distinguir errores de negocio vs infraestructura para saber cuándo reintentar.

```rust
// crates/saga-engine/core/src/error/classification.rs - NUEVO

/// Clasificación de errores para determinar estrategia de retry
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorClassification {
    /// Error de lógica de negocio - NO reintentar automáticamente
    /// Ejemplo: "Usuario no encontrado", "Saldo insuficiente"
    Business,
    
    /// Error de infraestructura transitorio - SÍ reintentar
    /// Ejemplo: "Connection refused", "Timeout", "Service unavailable"
    Transient,
    
    /// Error de infraestructura permanente - NO reintentar
    /// Ejemplo: "Invalid credentials", "Permission denied"
    Permanent,
    
    /// Pánico en el worker - Reintentar en OTRO worker
    WorkerCrash,
    
    /// Cancelación explícita - NO reintentar
    Cancelled,
}

/// Trait para clasificar errores
pub trait ClassifiableError: std::error::Error {
    fn classification(&self) -> ErrorClassification;
    
    fn is_retryable(&self) -> bool {
        matches!(self.classification(), ErrorClassification::Transient | ErrorClassification::WorkerCrash)
    }
}

/// Error de ejecución de activity con clasificación
#[derive(Debug, thiserror::Error)]
pub enum ActivityExecutionError<E: std::error::Error> {
    /// Error de negocio del activity
    #[error("Activity business error: {0}")]
    Business(E),
    
    /// Timeout de ejecución
    #[error("Activity timed out after {duration:?}")]
    Timeout { duration: Duration },
    
    /// Error de red/conexión
    #[error("Activity network error: {message}")]
    Network { message: String, is_transient: bool },
    
    /// Worker murió durante ejecución
    #[error("Activity worker crashed")]
    WorkerCrash,
    
    /// Actividad cancelada
    #[error("Activity cancelled")]
    Cancelled,
    
    /// Heartbeat timeout (worker probablemente muerto)
    #[error("Activity heartbeat timeout")]
    HeartbeatTimeout,
    
    /// Serialización falló
    #[error("Activity serialization error: {0}")]
    Serialization(String),
}

impl<E: std::error::Error> ClassifiableError for ActivityExecutionError<E> {
    fn classification(&self) -> ErrorClassification {
        match self {
            Self::Business(_) => ErrorClassification::Business,
            Self::Timeout { .. } => ErrorClassification::Transient,
            Self::Network { is_transient, .. } => {
                if *is_transient { ErrorClassification::Transient } else { ErrorClassification::Permanent }
            }
            Self::WorkerCrash => ErrorClassification::WorkerCrash,
            Self::HeartbeatTimeout => ErrorClassification::WorkerCrash,
            Self::Cancelled => ErrorClassification::Cancelled,
            Self::Serialization(_) => ErrorClassification::Permanent,
        }
    }
}

/// Error de workflow con clasificación
#[derive(Debug, thiserror::Error)]
pub enum WorkflowExecutionError<E: std::error::Error> {
    #[error("Workflow business error: {0}")]
    Business(E),
    
    #[error("Workflow timed out")]
    Timeout,
    
    #[error("Workflow cancelled")]
    Cancelled,
    
    #[error("Non-determinism detected: {message}")]
    NonDeterminism { message: String },
    
    #[error("Activity failed: {0}")]
    ActivityFailed(#[from] Box<dyn ClassifiableError + Send + Sync>),
    
    #[error("Child workflow failed: {workflow_id}")]
    ChildWorkflowFailed { workflow_id: String },
}

/// Configuración de retry basada en clasificación
impl RetryPolicy {
    /// Determinar si debe reintentar basado en clasificación de error
    pub fn should_retry<E: ClassifiableError>(&self, error: &E, attempt: u32) -> bool {
        if attempt >= self.max_attempts {
            return false;
        }
        
        match error.classification() {
            ErrorClassification::Transient => true,
            ErrorClassification::WorkerCrash => true,
            ErrorClassification::Business => false,
            ErrorClassification::Permanent => false,
            ErrorClassification::Cancelled => false,
        }
    }
    
    /// Calcular delay para siguiente retry
    pub fn next_retry_delay(&self, attempt: u32) -> Duration {
        match self.backoff {
            BackoffStrategy::Constant(d) => d,
            BackoffStrategy::Linear { initial, increment } => initial + increment * attempt,
            BackoffStrategy::Exponential { initial, multiplier, max } => {
                let delay = initial.as_millis() as f64 * multiplier.powi(attempt as i32);
                Duration::from_millis(delay.min(max.as_millis() as f64) as u64)
            }
        }
    }
}
```

---

#### US-94.22: Task Queue Optimization (SKIP LOCKED)

> [!TIP]
> Evitar contención en PostgreSQL con `SELECT ... FOR UPDATE SKIP LOCKED`.

```rust
// crates/saga-engine/pg/src/task_queue/optimized.rs - NUEVO

impl PostgresTaskQueue {
    /// Fetch tasks usando SKIP LOCKED para evitar contención
    /// 
    /// Múltiples workers pueden hacer polling concurrente sin bloquearse
    pub async fn fetch_with_skip_locked(
        &self,
        consumer_id: &str,
        batch_size: u32,
        visibility_timeout: Duration,
    ) -> Result<Vec<Task>, TaskQueueError> {
        let now = Utc::now();
        let visible_until = now + visibility_timeout;
        
        // Query con SKIP LOCKED para evitar contención
        let tasks = sqlx::query_as::<_, TaskRow>(r#"
            UPDATE saga_tasks
            SET 
                status = 'processing',
                consumer_id = $1,
                visible_until = $2,
                started_at = $3,
                attempt = attempt + 1
            WHERE id IN (
                SELECT id FROM saga_tasks
                WHERE status = 'pending'
                  AND (visible_until IS NULL OR visible_until < $3)
                  AND task_queue = $4
                ORDER BY priority DESC, created_at ASC
                LIMIT $5
                FOR UPDATE SKIP LOCKED  -- ⚡ Clave para evitar contención
            )
            RETURNING *
        "#)
        .bind(consumer_id)
        .bind(visible_until)
        .bind(now)
        .bind(&self.queue_name)
        .bind(batch_size as i32)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(tasks.into_iter().map(|r| r.into()).collect())
    }
    
    /// Optimización: Usar índice parcial para tasks pendientes
    /// 
    /// Migración SQL recomendada:
    /// ```sql
    /// CREATE INDEX CONCURRENTLY idx_saga_tasks_pending 
    /// ON saga_tasks (task_queue, priority DESC, created_at ASC)
    /// WHERE status = 'pending';
    /// ```
}

/// Configuración de polling optimizado
#[derive(Debug, Clone)]
pub struct PollingConfig {
    /// Batch size por poll
    pub batch_size: u32,
    /// Tiempo de visibilidad (cuánto tiempo tiene el worker para procesar)
    pub visibility_timeout: Duration,
    /// Intervalo entre polls cuando no hay trabajo
    pub idle_poll_interval: Duration,
    /// Intervalo entre polls cuando hay trabajo
    pub busy_poll_interval: Duration,
    /// Backoff exponencial si hay errores de conexión
    pub error_backoff: BackoffConfig,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            visibility_timeout: Duration::from_secs(300), // 5 min
            idle_poll_interval: Duration::from_secs(1),
            busy_poll_interval: Duration::from_millis(10),
            error_backoff: BackoffConfig {
                initial: Duration::from_millis(100),
                max: Duration::from_secs(30),
                multiplier: 2.0,
            },
        }
    }
}
```

---

#### US-94.23: Type-Safe Activity Execution

> [!NOTE]
> Mejora de ergonomía: Evitar serialización manual constante con wrapper tipado.

```rust
// crates/saga-engine/core/src/workflow/activity_exec.rs - MEJORADO

impl WorkflowContext {
    /// Ejecutar activity con tipado fuerte (sin serialización manual)
    /// 
    /// # Ejemplo
    /// ```rust
    /// // ✅ Tipado fuerte, el compilador verifica tipos
    /// let result = ctx.execute::<FetchUserActivity>(FetchUserInput { 
    ///     user_id: "123" 
    /// }).await?;
    /// 
    /// // result es FetchUserOutput, no Value
    /// println!("User: {}", result.name);
    /// ```
    pub async fn execute<A>(&mut self, input: A::Input) -> Result<A::Output, ActivityExecutionError<A::Error>>
    where
        A: Activity,
    {
        self.execute_with_options::<A>(input, ActivityOptions::default()).await
    }
    
    /// Ejecutar activity con opciones personalizadas
    pub async fn execute_with_options<A>(
        &mut self,
        input: A::Input,
        options: ActivityOptions,
    ) -> Result<A::Output, ActivityExecutionError<A::Error>>
    where
        A: Activity,
    {
        // Generar ID secuencial (determinista)
        let activity_seq = self.next_activity_sequence();
        let activity_id = format!("{}:{}", A::TYPE_ID, activity_seq);
        
        // Verificar si ya tenemos resultado en caché (replay)
        if let Some(cached) = self.get_cached_activity_result(&activity_id) {
            return match cached {
                CachedActivityResult::Completed(value) => {
                    Ok(serde_json::from_value(value)?)
                }
                CachedActivityResult::Failed(error) => {
                    Err(self.deserialize_error::<A>(error)?)
                }
            };
        }
        
        // No en caché, necesitamos ejecutar
        if self.is_replaying {
            return Err(ActivityExecutionError::NonDeterminism {
                message: format!(
                    "Activity '{}' not found in history during replay. \
                     This indicates non-deterministic workflow code.",
                    activity_id
                ),
            });
        }
        
        // Serializar input (type-safe)
        let input_value = serde_json::to_value(&input)?;
        
        // Encolar tarea de activity
        self.pending_commands.push(WorkflowCommand::ScheduleActivity {
            activity_id: activity_id.clone(),
            activity_type: A::TYPE_ID.to_string(),
            input: input_value,
            options: options.clone(),
        });
        
        // Retornar suspensión (el worker nos despertará cuando complete)
        Err(ActivityExecutionError::Scheduled { activity_id })
    }
    
    /// Helper interno para secuencia de IDs
    fn next_activity_sequence(&mut self) -> u64 {
        let seq = self.activity_sequence;
        self.activity_sequence += 1;
        seq
    }
}

/// Opciones de ejecución de activity
#[derive(Debug, Clone)]
pub struct ActivityOptions {
    /// Timeout desde que se programa hasta que completa
    pub schedule_to_close_timeout: Option<Duration>,
    /// Timeout desde que inicia hasta que completa
    pub start_to_close_timeout: Option<Duration>,
    /// Timeout desde que se programa hasta que inicia
    pub schedule_to_start_timeout: Option<Duration>,
    /// Intervalo de heartbeat (para activities largas)
    pub heartbeat_timeout: Option<Duration>,
    /// Política de retry
    pub retry_policy: Option<RetryPolicy>,
    /// Task queue específica (override default)
    pub task_queue: Option<String>,
}

impl Default for ActivityOptions {
    fn default() -> Self {
        Self {
            schedule_to_close_timeout: Some(Duration::from_secs(300)),
            start_to_close_timeout: Some(Duration::from_secs(60)),
            schedule_to_start_timeout: Some(Duration::from_secs(60)),
            heartbeat_timeout: None,
            retry_policy: Some(RetryPolicy::default()),
            task_queue: None,
        }
    }
}

impl ActivityOptions {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.start_to_close_timeout = Some(timeout);
        self
    }
    
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }
    
    pub fn with_heartbeat(mut self, interval: Duration) -> Self {
        self.heartbeat_timeout = Some(interval);
        self
    }
    
    pub fn no_retry(mut self) -> Self {
        self.retry_policy = None;
        self
    }
}
```

---

#### US-94.24: Replay Cycle Documentation

> [!TIP]
> Diagrama visual del ciclo de replay para facilitar comprensión del equipo.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     CICLO DE REPLAY DEL SAGA ENGINE                     │
└─────────────────────────────────────────────────────────────────────────┘

1. WORKFLOW TASK RECIBIDO
   ┌─────────────────┐
   │ Task Queue      │ ──➤ Worker recibe tarea
   └─────────────────┘

2. CARGAR HISTORIAL
   ┌─────────────────┐
   │ Event Store     │ ──➤ SELECT * FROM saga_events WHERE saga_id = ?
   └─────────────────┘     ORDER BY event_id ASC
                          
   Resultado:
   ┌────────────────────────────────────────────────────────────────┐
   │ [0] WorkflowExecutionStarted { input: {...} }                  │
   │ [1] ActivityScheduled { activity_id: "fetch:0", input: {...} } │
   │ [2] ActivityCompleted { activity_id: "fetch:0", output: {...} }│
   │ [3] TimerStarted { timer_id: "t1", duration: 30s }             │
   │ [4] TimerFired { timer_id: "t1" }                              │
   │ [5] ActivityScheduled { activity_id: "send:1", input: {...} }  │
   │ [6] ActivityCompleted { activity_id: "send:1", output: {...} } │
   └────────────────────────────────────────────────────────────────┘

3. RECONSTRUIR ESTADO (REPLAY)
   ┌──────────────────────────────────────────────────────────────────────┐
   │ WorkflowContext {                                                    │
   │   activity_sequence: 2,           // Próximo ID de activity          │
   │   cached_results: {               // Resultados ya ejecutados        │
   │     "fetch:0" => Completed({...}),                                   │
   │     "send:1" => Completed({...}),                                    │
   │   },                                                                 │
   │   current_event_index: 7,         // Siguiente evento a grabar       │
   │   is_replaying: true,             // ⚠️ Modo replay activo           │
   │ }                                                                    │
   └──────────────────────────────────────────────────────────────────────┘

4. RE-EJECUTAR CÓDIGO DEL WORKFLOW (DETERMINÍSTICO)
   ```rust
   async fn run(ctx, input) {
       // [0] WorkflowExecutionStarted - ya procesado
       
       // [1-2] Retorna cached result inmediatamente
       let user = ctx.execute::<FetchUser>(input).await?; // ⚡ Cache hit!
       
       // [3-4] Timer ya disparado, no bloquea
       ctx.sleep(Duration::from_secs(30)).await; // ⚡ Cache hit!
       
       // [5-6] Retorna cached result inmediatamente  
       ctx.execute::<SendEmail>(email_input).await?; // ⚡ Cache hit!
       
       // [7] ← AQUÍ ESTAMOS: Próxima actividad es NUEVA
       ctx.is_replaying = false;
       
       // Esta actividad SÍ se programa en Task Queue
       ctx.execute::<UpdateDatabase>(db_input).await?; // ➤ Schedule!
   }
   ```

5. NUEVA ACTIVIDAD DETECTADA
   ┌─────────────────────────────────────────────────────────────┐
   │ pending_commands: [                                         │
   │   ScheduleActivity { activity_id: "update:2", ... }         │
   │ ]                                                           │
   └─────────────────────────────────────────────────────────────┘
   
6. PERSISTIR Y ENCOLAR
   ┌─────────────────┐     ┌─────────────────┐
   │ Event Store     │ ◀── │ [7] Activity-   │
   │ (append)        │     │     Scheduled   │
   └─────────────────┘     └─────────────────┘
   
   ┌─────────────────┐
   │ Task Queue      │ ◀── Encolar ActivityTask para worker de activities
   │ (publish)       │
   └─────────────────┘

7. SUSPENDER WORKFLOW (esperando activity)
   └── Workflow task completa. Cuando activity termine,
       se encolará nuevo WorkflowTask y repetiremos desde paso 1.

═══════════════════════════════════════════════════════════════════════════
INVARIANTES CRÍTICOS:
- El código del run() DEBE ser determinista
- Misma entrada + mismo historial = mismos pending_commands
- Side effects (Uuid, tiempo) deben usar ctx.* no llamadas directas
═══════════════════════════════════════════════════════════════════════════
```

| Componente | PostgreSQL | NATS | In-Memory |
|------------|------------|------|-----------|
| CommandBus | `saga-engine-pg` | `saga-engine-nats` | `saga-engine-testing` |
| EventBus | `saga-engine-pg` | `saga-engine-nats` | `saga-engine-testing` |
| Outbox | `saga-engine-pg` | - | `saga-engine-testing` |

### Fase 6: Integración hodei-jobs (Semanas 11-12)

#### US-94.9: Migrar workflows existentes

```rust
// ANTIGUO (Step-List) - server/application/src/saga/workflows/provisioning.rs
impl WorkflowDefinition for ProvisioningWorkflow {
    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
        vec![Box::new(ValidateProviderStep), ...]
    }
}

// NUEVO (Workflow-as-Code)
#[async_trait]
impl DurableWorkflow for ProvisioningWorkflow {
    const TYPE_ID: &'static str = "provisioning";
    const VERSION: u32 = 1;

    type Input = ProvisioningWorkflowInput;
    type Output = ProvisioningWorkflowOutput;
    type Error = ProvisioningWorkflowError;

    async fn run(
        &self,
        ctx: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Usar execute_activity directamente
        ctx.execute_activity(&ValidateProviderActivity::new(self.registry.clone()), input.provider_id).await?;
        let worker_id = ctx.execute_activity(&CreateInfrastructureActivity::new(self.provisioner.clone()), input.spec).await?;
        Ok(ProvisioningWorkflowOutput { worker_id: worker_id.to_string(), ... })
    }
}
```

---

## 📦 Estructura de Archivos Propuesta

```
crates/saga-engine/
├── core/src/
│   ├── lib.rs                              # Exports actualizados
│   ├── workflow/
│   │   ├── mod.rs                          # WorkflowDefinition, Activity, WorkflowContext (existente)
│   │   └── durable.rs                      # ⭐ NUEVO: DurableWorkflow trait
│   ├── engine/
│   │   └── mod.rs                          # ⭐ NUEVO: SagaEngine
│   ├── activity/
│   │   ├── mod.rs                          # ⭐ NUEVO: ActivityRegistry, DynActivity
│   │   └── traits.rs                       # Activity, DynActivity traits
│   ├── worker/
│   │   └── mod.rs                          # ⭐ NUEVO: Worker, WorkerConfig
│   ├── port/
│   │   ├── mod.rs                          # Exports actualizados
│   │   ├── command_bus.rs                  # ⭐ NUEVO
│   │   ├── event_bus.rs                    # ⭐ NUEVO
│   │   ├── outbox.rs                       # ⭐ NUEVO
│   │   ├── event_store.rs                  # existente
│   │   ├── task_queue.rs                   # existente
│   │   ├── timer_store.rs                  # existente
│   │   ├── replay.rs                       # existente
│   │   └── signal_dispatcher.rs            # existente
│   ├── event/                              # existente (bien diseñado)
│   ├── codec/                              # existente
│   └── metrics/                            # ⭐ NUEVO: SagaMetrics
│
├── testing/src/
│   ├── lib.rs
│   ├── in_memory/
│   │   ├── command_bus.rs                  # ⭐ NUEVO
│   │   ├── event_bus.rs                    # ⭐ NUEVO
│   │   ├── outbox.rs                       # ⭐ NUEVO
│   │   ├── event_store.rs                  # existente
│   │   ├── task_queue.rs                   # existente
│   │   └── timer_store.rs                  # existente
│
├── pg/src/
│   ├── lib.rs
│   ├── command_bus.rs                      # ⭐ NUEVO
│   ├── event_bus.rs                        # ⭐ NUEVO
│   ├── outbox.rs                           # ⭐ NUEVO
│   ├── event_store.rs                      # existente
│   ├── timer_store.rs                      # existente
│   └── replayer.rs                         # existente
│
└── nats/src/
    ├── lib.rs
    ├── command_bus.rs                      # ⭐ NUEVO
    ├── event_bus.rs                        # ⭐ NUEVO
    ├── task_queue.rs                       # existente
    └── signal_dispatcher.rs                # existente
```

---

## 📋 Resumen de User Stories (Actualizado)

| Fase | US | Título | Dependencias | Estado |
|------|-----|--------|--------------|--------|
| 1 | US-94.1 | `DurableWorkflow` trait | - | ⏳ Pending |
| 1 | US-94.2 | `execute_activity()` | US-94.1 | ⏳ Pending |
| 2 | US-94.3 | `SagaEngine` | US-94.1, US-94.2 | ⏳ Pending |
| 3 | US-94.4 | `Worker` (Poller) | US-94.3 | ⏳ Pending |
| 3 | US-94.5 | `ActivityRegistry` | - | ⏳ Pending |
| 4 | US-94.6 | `CommandBus` abstracción | - | ⏳ Pending |
| 4 | US-94.7 | `EventBus` abstracción | - | ⏳ Pending |
| 4 | US-94.8 | `Outbox` abstracción | - | ⏳ Pending |
| 5 | US-94.9 | Implementaciones PG/NATS | US-94.6, US-94.7, US-94.8 | ⏳ Pending |
| 6 | US-94.10 | Integración hodei-jobs | US-94.3, US-94.4 | ⏳ Pending |

---

## 🎯 API Final del Cliente

```rust
// ============ CÓDIGO DEL CLIENTE (hodei-jobs) ============

// 1. Definir activities (YA EXISTEN - solo marcar con trait marker)
#[async_trait]
impl Activity for ValidateProviderActivity { /* existente */ }

// 2. Definir workflow con código-as-code
#[async_trait]
impl DurableWorkflow for ProvisioningWorkflow {
    const TYPE_ID: &'static str = "provisioning";
    const VERSION: u32 = 1;

    type Input = ProvisioningInput;
    type Output = WorkerId;
    type Error = ProvisioningError;

    async fn run(&self, ctx: &mut WorkflowContext, input: Self::Input) -> Result<Self::Output, Self::Error> {
        ctx.execute_activity(&ValidateProviderActivity::new(self.registry.clone()), input.provider_id).await?;
        let worker_id = ctx.execute_activity(&CreateInfraActivity::new(self.provisioner.clone()), input.spec).await?;
        Ok(worker_id)
    }
}

// 3. Inicializar (una vez al inicio)
let registry = ActivityRegistry::default();
registry.register::<ValidateProviderActivity>();
registry.register::<CreateInfraActivity>();

let engine = SagaEngine::builder()
    .with_event_store(postgres_event_store)
    .with_task_queue(nats_task_queue)
    .with_metrics(metrics.clone())
    .build();

let worker = Worker::new(Arc::new(engine), Arc::new(nats_task_queue), Arc::new(registry));
tokio::spawn(worker.run());

// 4. Iniciar workflow (desde cualquier parte del código)
let input = ProvisioningInput { provider_id: "...", spec: ... };
let execution_id = engine.start_workflow(&ProvisioningWorkflow, input, None).await?;
```

---

## ✅ Criterios de Éxito

### Reutilización de Código Existente
- [ ] `Activity` trait sin cambios
- ✅ `EventStore`, `TaskQueue`, `TimerStore` sin cambios
- ✅ `HistoryEvent`, `EventType` sin cambios
- ✅ `WorkflowContext` extendido, no reemplazado

### Funcionales
- [ ] Workflows definidos como código Rust nativo
- [ ] Actividades type-safe con Input/Output generics
- [ ] Replay determinista de workflows
- [ ] CommandBus/EventBus/Outbox abstraídos

### No Funcionales
- [ ] Replay < 50ms para workflows típicos
- [ ] Documentación completa en docs.rs
- [ ] API estable (SemVer)

---

## 📚 Referencias

- [Temporal.io Documentation](https://docs.temporal.io/)
- [Event Sourcing pattern](https://martinfowler.com/eaaDev/EventSourcing.html)

---

## 📚 Apéndice: Análisis Profundo del Código Actual (2026-01-20)

### A.1 Inventario de Componentes Existentes

Tras un análisis exhaustivo del código actual de `saga-engine` y la capa de aplicación, se han identificado los siguientes componentes:

#### A.1.1 saga-engine/core ✅ (Alta madurez)

| Componente | Ubicación | Estado | Calidad |
|------------|-----------|--------|---------|
| `HistoryEvent` + `EventType` (~100 tipos) | `core/src/event/mod.rs` | ✅ Excelente | Producción |
| `EventCategory` (12 categorías) | `core/src/event/mod.rs` | ✅ Excelente | Producción |
| `EventId`, `SagaId` | `core/src/event/mod.rs` | ✅ Excelente | Producción |
| `EventStore` port + `EventStoreError::Conflict` | `core/src/port/event_store.rs` | ✅ Excelente | Producción |
| `TaskQueue` port (NATS JetStream Pull) | `core/src/port/task_queue.rs` | ✅ Excelente | Producción |
| `TimerStore` port | `core/src/port/timer_store.rs` | ✅ Excelente | Producción |
| `SignalDispatcher` port | `core/src/port/signal_dispatcher.rs` | ✅ Excelente | Producción |
| `HistoryReplayer` + `Applicator` trait | `core/src/port/replay.rs` | ✅ Excelente | Producción |
| `Activity` trait (TYPE_ID, Input, Output, Error) | `core/src/workflow/mod.rs:L391` | ✅ Excelente | Producción |
| `WorkflowDefinition` trait | `core/src/workflow/mod.rs:L620` | ✅ Bueno | Producción |
| `WorkflowStep` + `DynWorkflowStep` | `core/src/workflow/mod.rs:L519-575` | ✅ Bueno | Producción |
| `WorkflowContext` | `core/src/workflow/mod.rs:L70` | ⚠️ Parcial | Requiere mejoras |
| `DurableWorkflowRuntime` | `core/src/workflow/mod.rs:L752` | ⚠️ Parcial | Requiere mejoras |
| `WorkflowMetrics` trait | `core/src/workflow/mod.rs:L706` | ✅ Bueno | Producción |
| `RetryPolicy` (Exponential, Fixed) | `core/src/workflow/mod.rs:L401` | ✅ Bueno | Producción |
| `WorkflowResult`, `WorkflowState` | `core/src/workflow/mod.rs:L269-379` | ✅ Excelente | Producción |

#### A.1.2 saga-engine/pg ✅ (Alta madurez)

| Componente | Ubicación | Estado |
|------------|-----------|--------|
| `PostgresEventStore` | `pg/src/event_store.rs` | ✅ Implementado |
| `PostgresTimerStore` | `pg/src/timer_store.rs` | ✅ Implementado |
| `PostgresHistoryReplayer` | `pg/src/replayer.rs` | ✅ Implementado |

#### A.1.3 saga-engine/nats ✅ (Alta madurez)

| Componente | Ubicación | Estado |
|------------|-----------|--------|
| `NatsTaskQueue` (JetStream Pull) | `nats/src/task_queue.rs` | ✅ Implementado |
| `NatsSignalDispatcher` (Core Pub/Sub) | `nats/src/signal_dispatcher.rs` | ✅ Implementado |

#### A.1.4 saga-engine/testing ✅

| Componente | Ubicación | Estado |
|------------|-----------|--------|
| `InMemoryEventStore` | `testing/src/memory_event_store.rs` | ✅ Implementado |
| `InMemoryTimerStore` | `testing/src/memory_timer_store.rs` | ✅ Implementado |
| `InMemoryReplayer` | `testing/src/in_memory_replayer.rs` | ✅ Implementado |

#### A.1.5 Aplicación (server/application/src/saga/) ✅

| Componente | Ubicación | Estado | Notas |
|------------|-----------|--------|-------|
| `SagaPort<W>` trait | `saga/port/mod.rs` | ✅ Excelente | Type-safe, extensible |
| `SagaPortExt<W>` | `saga/port/mod.rs` | ✅ Excelente | Convenience methods |
| `CommandBusActivity<C>` | `saga/bridge/command_bus.rs` | ✅ Excelente | Puente Commands→Activities |
| `CommandBusActivityRegistry` | `saga/bridge/command_bus.rs` | ✅ Bueno | Registry parcial |
| 7 Workflows completos | `saga/workflows/*.rs` | ✅ Bueno | provisioning, execution, recovery, cancellation, cleanup, timeout |
| Feature flags | `saga/feature_flags.rs` | ✅ Excelente | Migración gradual |
| Circuit Breaker | `saga/ports/circuit_breaker.rs` | ✅ Implementado | |
| Rate Limiter | `saga/ports/rate_limiter.rs` | ✅ Implementado | |
| Stuck Detection | `saga/ports/stuck_detection.rs` | ✅ Implementado | |

---

### A.2 Análisis de la Propuesta EPIC-94 vs Código Actual

#### A.2.1 Lo que YA EXISTE y NO necesita implementarse ✅

| Propuesta EPIC-94 | Estado Actual | Recomendación |
|-------------------|---------------|---------------|
| `Activity` trait con TYPE_ID, Input, Output, Error | ✅ Existe exactamente igual | **REUTILIZAR** |
| `EventStore` port con Conflict error | ✅ Existe exactamente igual | **REUTILIZAR** |
| `TaskQueue` port (NATS JetStream Pull) | ✅ Existe exactamente igual | **REUTILIZAR** |
| `TimerStore` port (PostgreSQL) | ✅ Existe exactamente igual | **REUTILIZAR** |
| `SignalDispatcher` port | ✅ Existe exactamente igual | **REUTILIZAR** |
| `HistoryReplayer` | ✅ Existe exactamente igual | **REUTILIZAR** |
| `WorkflowMetrics` trait | ✅ Existe exactamente igual | **REUTILIZAR** |
| `RetryPolicy` | ✅ Existe exactamente igual | **REUTILIZAR** |
| PostgreSQL EventStore implementation | ✅ saga-engine-pg | **REUTILIZAR** |
| NATS TaskQueue implementation | ✅ saga-engine-nats | **REUTILIZAR** |
| InMemory implementations (testing) | ✅ saga-engine-testing | **REUTILIZAR** |
| `ActivityOptions` (timeout, retry, heartbeat) | ⚠️ Parcial en `WorkflowConfig` | **MEJORAR** |

#### A.2.2 Lo que SÍ necesita implementarse ⚠️

> [!NOTE]
> Componentes identificados comparando con Temporal.io's architecture para un engine de durable execution completo.

**🔴 Crítico - Core Workflow-as-Code**

| Componente | Gap | Existe en Temporal | Prioridad |
|------------|-----|-------------------|-----------|
| **`Workflow` trait con `run()`** | `WorkflowDefinition` usa `steps()` | ✅ Core feature | 🔴 |
| **`execute_activity()` en WorkflowContext** | ❌ No existe | ✅ `workflow.ExecuteActivity()` | 🔴 |
| **`SagaEngine` (orchestrator central)** | ❌ No unificado | ✅ `temporal.NewClient()` | 🔴 |
| **Deterministic Replay** | ⚠️ Parcial | ✅ Core feature | 🔴 |

**🟠 Alta - Infraestructura Worker**

| Componente | Gap | Existe en Temporal | Prioridad |
|------------|-----|-------------------|-----------|
| **`Worker` (Poller loop)** | ⚠️ Parcial en `DurableWorkflowRuntime` | ✅ `worker.New()` | 🟠 |
| **`ActivityRegistry`** | ⚠️ Parcial | ✅ `worker.RegisterActivity()` | 🟠 |
| **`WorkflowRegistry`** | ❌ No existe | ✅ `worker.RegisterWorkflow()` | 🟠 |
| **`WorkflowClient` / `WorkflowHandle`** | ❌ No existe | ✅ Client SDK para iniciar/query workflows | 🟠 |

**🟠 Alta - Outbox Pattern**

| Componente | Gap | Existe Otro Lugar | Prioridad |
|------------|-----|-------------------|-----------|
| **`Outbox` trait** | ❌ No en saga-engine | ✅ `domain/outbox` | 🟠 |
| **`OutboxRepository` trait** | ❌ No en saga-engine | ✅ `domain/outbox` | 🟠 |
| **`OutboxRelay` (background worker)** | ❌ No abstracción | ✅ `HybridOutboxRelay` en infra | 🟠 |

**🟠 Alta - Messaging Abstractions**

| Componente | Gap | Existe Otro Lugar | Prioridad |
|------------|-----|-------------------|-----------|
| **`CommandBus`** | ❌ No en saga-engine | ✅ `domain/command` | 🟠 |
| **`EventBus`** | ❌ No en saga-engine | ❌ No como trait | 🟠 |

**🟢 Media - Activity Features**

| Componente | Gap | Existe en Temporal | Prioridad |
|------------|-----|-------------------|-----------|
| **Activity Heartbeat** | ❌ No existe | ✅ `activity.RecordHeartbeat()` | 🟢 |
| **Activity Cancellation** | ⚠️ Solo via workflow cancel | ✅ Context cancellation | 🟢 |
| **Local Activities** | ❌ No existe | ✅ Short-lived, no task queue | 🟢 |

**🟢 Media - Workflow Features**

| Componente | Gap | Existe en Temporal | Prioridad |
|------------|-----|-------------------|-----------|
| **Workflow Queries** | ❌ No existe | ✅ `workflow.SetQueryHandler()` | 🟢 |
| **Continue-as-New** | ❌ No existe | ✅ Large history mitigation | 🟢 |
| **Child Workflows** | ⚠️ Solo eventos | ✅ `workflow.ExecuteChildWorkflow()` | 🟢 |
| **Workflow Versioning** | ❌ No existe | ✅ `workflow.GetVersion()` | 🟢 |
| **Side Effects** | ❌ No existe | ✅ `workflow.SideEffect()` | 🟢 |
| **Snapshot Compaction** | ⚠️ No automático | ✅ History compaction | 🟢 |

**🟡 Baja - Observability & Advanced**

| Componente | Gap | Existe en Temporal | Prioridad |
|------------|-----|-------------------|-----------|
| **Visibility / Search Attributes** | ❌ No existe | ✅ Query workflows by attributes | 🟡 |
| **Cron Schedules (in engine)** | ⚠️ Solo en app layer | ✅ `workflow.SetCronSchedule()` | 🟡 |
| **Workflow Update** | ❌ No existe | ✅ Synchronous updates | 🟡 |
| **Nexus (cross-namespace)** | ❌ No existe | ✅ Service orchestration | 🟡 |

#### A.2.3 Migración Gradual de Workflows Existentes 🔄

> [!IMPORTANT]
> Se implementará migración gradual de los 7 workflows a Workflow-as-Code manteniendo compatibilidad hacia atrás.

| Workflow Existente | Complejidad | Prioridad Migración | Estrategia |
|--------------------|-------------|---------------------|------------|
| `ProvisioningWorkflow` | Media | 🔴 Alta | Migrar primero (piloto) |
| `ExecutionWorkflow` | Baja | 🔴 Alta | Segundo piloto |
| `RecoveryWorkflow` | Alta | 🟠 Media | Después de validar patrón |
| `CancellationWorkflow` | Media | 🟠 Media | Migrar en paralelo |
| `TimeoutWorkflow` | Media | 🟠 Media | Migrar en paralelo |
| `CleanupWorkflow` | Baja | 🟢 Baja | Último grupo |
| `DispatcherWorkflow` | Media | 🟢 Baja | Último grupo |

**Estrategia de Migración en 3 Fases**:

1. **Fase A - Infraestructura** (Sprints 1-4): Implementar `Workflow` trait con `run()` y `execute_activity()` SIN tocar workflows existentes
2. **Fase B - Migración Piloto** (Sprints 5-6): Migrar `ProvisioningWorkflow` y `ExecutionWorkflow` como pilotos
3. **Fase C - Migración Completa** (Sprints 7-9): Migrar workflows restantes gradualmente

#### A.2.4 Lo que puede POSPONERSE ⏳

| Propuesta EPIC-94 | Razón para Posponer |
|-------------------|---------------------|
| Schema PostgreSQL nuevo | ⚠️ Ya existe schema funcional, solo añadir campos si necesario |
| Timer Sharding schema | ⚠️ Optimización prematura, no hay evidencia de bottleneck |

---

### A.3 Propuesta de Mejoras Refinada

#### 🔴 Crítico (Sprint 1) - execute_activity() en WorkflowContext

**Gap principal**: El `WorkflowContext` actual no tiene método `execute_activity()`. Los workflows llaman actividades directamente, perdiendo:
- Replay determinista
- Persistencia automática de resultados
- Timeout/retry interceptados

**Solución propuesta**:

```rust
// saga-engine/core/src/workflow/mod.rs - AÑADIR a WorkflowContext

impl WorkflowContext {
    /// Ejecutar actividad con interceptación por el motor
    /// 
    /// Este método:
    /// 1. Verifica si la actividad ya se ejecutó (replay del historial)
    /// 2. Si no, programa la actividad en la task queue
    /// 3. Espera el resultado y lo persiste
    pub async fn execute_activity<A: Activity>(
        &mut self,
        activity: &A,
        input: A::Input,
        event_store: &dyn EventStore,
        task_queue: &dyn TaskQueue,
    ) -> Result<A::Output, ActivityExecutionError<A::Error>> {
        // 1. Generar activity_id determinista
        let activity_id = format!("{}:{}", A::TYPE_ID, self.current_activity_sequence);
        self.current_activity_sequence += 1;
        
        // 2. Buscar en historial si ya se completó
        if let Some(cached) = self.get_cached_activity_output(&activity_id) {
            return Ok(cached);
        }
        
        // 3. Si no existe, programar ejecución
        let task = Task::new(
            A::TYPE_ID.to_string(),
            self.execution_id.clone(),
            activity_id.clone(),
            serde_json::to_vec(&input)?,
        );
        task_queue.publish(&task).await?;
        
        // 4. Esperar resultado (el Worker lo completará)
        Err(ActivityExecutionError::Pending { activity_id })
    }
}
```

**Impacto**: Mínimo en código existente. Los workflows pueden seguir usando `steps()` y gradualmente adoptar `execute_activity()`.

---

#### 🟠 Alta (Sprint 2) - ActivityRegistry Global

**Gap**: `CommandBusActivityRegistry` existe pero es específico para commands. Necesitamos un registry global.

**Solución**: Mover implementación de `CommandBusActivityRegistry` a saga-engine-core y generalizar:

```rust
// saga-engine/core/src/activity/mod.rs - NUEVO ARCHIVO

pub struct ActivityRegistry {
    activities: DashMap<&'static str, Arc<dyn DynActivity>>,
}

impl ActivityRegistry {
    pub fn register<A: Activity + 'static>(&self, activity: A) {
        self.activities.insert(A::TYPE_ID, Arc::new(DynActivityAdapter(activity)));
    }
    
    pub fn get(&self, type_id: &str) -> Option<Arc<dyn DynActivity>> {
        self.activities.get(type_id).map(|r| r.clone())
    }
}
```

---

#### 🟠 Alta (Sprint 3) - Abstracciones CommandBus/EventBus/Outbox en saga-engine

**Gap**: Estas abstracciones existen en `domain` e `infrastructure` pero no en saga-engine. Esto rompe la portabilidad de la librería.

**Solución**: Crear ports abstractos en saga-engine-core:

```
saga-engine/core/src/port/
├── command_bus.rs   # ⭐ NUEVO: Command trait + CommandBus trait
├── event_bus.rs     # ⭐ NUEVO: DomainEvent trait + EventBus trait  
├── outbox.rs        # ⭐ NUEVO: OutboxMessage + Outbox trait
```

Luego, los adaptadores en `server/infrastructure` implementan estos traits.

---

#### 🟢 Media (Sprint 4) - Worker/Poller Unificado

**Gap**: `DurableWorkflowRuntime` tiene lógica de ejecución pero no un Worker loop claro.

**Solución**: Extraer y unificar:

```rust
// saga-engine/core/src/worker/mod.rs - NUEVO

pub struct SagaWorker<ES, TQ, TS, R> {
    event_store: Arc<ES>,
    task_queue: Arc<TQ>,
    timer_store: Arc<TS>,
    replayer: Arc<R>,
    activity_registry: Arc<ActivityRegistry>,
}

impl SagaWorker {
    /// Loop principal del worker
    pub async fn run(&self, shutdown: broadcast::Receiver<()>) {
        loop {
            tokio::select! {
                Some(msg) = self.task_queue.fetch(...) => {
                    self.process_task(msg).await;
                }
                _ = shutdown.recv() => break,
            }
        }
    }
}
```

---

### A.4 Estrategia de Migración: Step-List → Workflow-as-Code

| Aspecto | Step-List (Actual) | Workflow-as-Code (Objetivo) | Plan |
|---------|-------------------|------------------------------|------|
| **Código cliente** | `fn steps() -> Vec<Box<dyn DynWorkflowStep>>` | `async fn run(&self, ctx, input) -> Result<Output>` | Soportar **ambos** durante transición |
| **Pros** | Simple, testeable, ya implementado | Más expresivo, loops/conditionals, Temporal-compatible | - |
| **Cons** | Menos flexible para lógica compleja | Requiere replay determinista | - |
| **Código existente** | 7 workflows implementados | 0 workflows | **Migrar gradualmente** |

**Conclusión**: **Migración agresiva a Workflow-as-Code** sin mantener código legacy:

> [!CAUTION]
> Estrategia agresiva: NO se mantiene backward compatibility. Los workflows se migran y el código legacy se elimina.

1. ⚡ **Sin Backward Compatibility**: `WorkflowDefinition` se elimina tras migración
2. ⚡ **Sin Dual Support**: Solo `Workflow` trait con `run()`
3. ⚡ **Migración Atómica**: Cada workflow se migra completamente antes de pasar al siguiente
4. ⚡ **Limpieza Agresiva**: Código step-list se elimina inmediatamente tras migrar

#### A.4.1 Ejemplo de Migración: ProvisioningWorkflow

```rust
// ❌ ELIMINAR: Step-List Pattern (código legacy a borrar)
impl WorkflowDefinition for ProvisioningWorkflow {
    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] { ... }
}

// ✅ REEMPLAZAR CON: Workflow-as-Code Pattern
#[async_trait]
impl Workflow for ProvisioningWorkflow {
    const TYPE_ID: &'static str = "provisioning";
    const VERSION: u32 = 2;
    
    type Input = ProvisioningWorkflowInput;
    type Output = ProvisioningWorkflowOutput;
    type Error = ProvisioningWorkflowError;
    
    async fn run(
        &self,
        ctx: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Paso 1: Validar provider
        ctx.execute_activity(
            &ValidateProviderActivity::new(self.service.clone()),
            ValidateProviderInput { provider_id: input.provider_id.clone() },
        ).await?;
        
        // Paso 2: Validar spec
        let spec_output = ctx.execute_activity(
            &ValidateWorkerSpecActivity::new(self.service.clone()),
            input.spec.clone(),
        ).await?;
        
        // Paso 3: Provisionar
        let worker = ctx.execute_activity_with_options(
            &ProvisionWorkerActivity::new(self.service.clone()),
            ProvisionInput { spec: spec_output.validated_spec },
            ActivityOptions::default()
                .with_retry(RetryPolicy::ExponentialBackoff { max_retries: 3, .. }),
        ).await?;
        
        Ok(ProvisioningWorkflowOutput {
            worker_id: worker.id,
            status: "provisioned".to_string(),
        })
    }
}
```

#### A.4.2 Proceso de Migración por Workflow

Cada workflow sigue este proceso atómico:

```
1. Crear nuevo archivo: `workflow_name_v2.rs` con `impl Workflow`
2. Copiar activities existentes (reutilizables)
3. Implementar `async fn run()` con lógica equivalente
4. Actualizar imports en coordinadores/consumers
5. ✅ Verificar compilación
6. ❌ ELIMINAR archivo legacy `workflow_name.rs` con `impl WorkflowDefinition`
7. ✅ Verificar tests pasan
```

---

### A.5 Plan de Implementación (Migración Agresiva)

> [!WARNING]
> Plan agresivo sin backward compatibility. Cada sprint debe compilar y pasar tests.

#### Fase A: Nuevo Engine (Sprints 1-3)

| Sprint | US | Descripción | Días |
|--------|-----|-------------|------|
| 1 | US-94.A | `Workflow` trait con `run()` | 2 |
| 1 | US-94.B | `execute_activity()` en WorkflowContext | 2 |
| 1 | US-94.C | `SagaEngine` central (solo Workflow-as-Code) | 2 |
| 2 | US-94.D | `ActivityRegistry` global | 2 |
| 2 | US-94.E | `SagaWorker` unificado | 3 |
| 3 | US-94.F | `CommandBus`/`EventBus`/`Outbox` ports | 4 |
| 3 | US-94.G | Integración end-to-end | 2 |

**Subtotal Fase A**: 3 sprints (~17 días)

#### Fase B: Migración + Eliminación (Sprints 4-6)

| Sprint | Workflow | Acción | Días |
|--------|----------|--------|------|
| 4 | `ProvisioningWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 3 |
| 4 | `ExecutionWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 3 |
| 5 | `RecoveryWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 3 |
| 5 | `CancellationWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 3 |
| 6 | `TimeoutWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 2 |
| 6 | `CleanupWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 2 |
| 6 | `DispatcherWorkflow` | Migrar a `run()` + ❌ eliminar legacy | 2 |

**Subtotal Fase B**: 3 sprints (~18 días)

#### Fase C: Limpieza Final (Sprint 7)

| Sprint | US | Descripción | Días |
|--------|-----|-------------|------|
| 7 | US-94.H | ❌ Eliminar `WorkflowDefinition` trait | 1 |
| 7 | US-94.I | ❌ Eliminar `WorkflowStep`/`DynWorkflowStep` | 1 |
| 7 | US-94.J | ❌ Eliminar `SyncWorkflowExecutor` | 1 |
| 7 | US-94.K | ❌ Eliminar código puente legacy | 1 |
| 7 | US-94.L | Actualizar documentación | 1 |

**Subtotal Fase C**: 1 sprint (~5 días)

#### Resumen del Plan Agresivo

| Fase | Sprints | Días | Objetivo |
|------|---------|------|----------|
| **A - Nuevo Engine** | 1-3 | 17 | Workflow trait, SagaEngine, Worker |
| **B - Migración** | 4-6 | 18 | 7 workflows migrados + legacy eliminado |
| **C - Limpieza** | 7 | 5 | Eliminar todo código step-list |
| **TOTAL** | **7 sprints** | **~40 días** | **100% Workflow-as-Code, 0% legacy** |

> [!IMPORTANT]
> **Regla de oro**: Cada commit debe compilar. Migrar workflow → eliminar legacy → verificar tests → siguiente workflow.

---

### A.6 Métricas de Éxito (Migración Agresiva)

| Criterio | Baseline | Objetivo | Cómo medir |
|----------|----------|----------|------------|
| **Workflows migrados a `run()`** | 0/7 | 7/7 | Code review |
| **Código legacy eliminado** | 0% | 100% | `git diff --stat` |
| **`WorkflowDefinition` uses** | ~20 | 0 | `grep -r "WorkflowDefinition"` |
| **`WorkflowStep` uses** | ~30 | 0 | `grep -r "WorkflowStep"` |
| **Tests passing** | ✅ | ✅ | CI pipeline |
| **Compilación limpia** | ✅ | ✅ | `cargo build` |

---

### A.7 Riesgos y Mitigaciones (Estrategia Agresiva)

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Errores de compilación durante migración | Alta | Medio | Migrar 1 workflow → compilar → siguiente |
| Regresión funcional en workflows | Media | Alto | Tests exhaustivos antes de eliminar legacy |
| Bloqueo del equipo durante migración | Media | Alto | Migrar en branch separado, merge atómico |
| Pérdida de funcionalidad en step-list | Baja | Alto | Revisar cada `compensate()` se porta a error handling |
| Replay determinista incorrecto | Media | Alto | Tests de replay para cada workflow migrado |

---

## Apéndice B: Consolidación Técnica y Decisiones de Diseño

> [!IMPORTANT]
> Este apéndice consolida el feedback técnico recibido, resuelve inconsistencias identificadas y establece los diseños definitivos para implementación.

---

### B.1 Issues Identificados y Resoluciones

| # | Issue | Severidad | Resolución |
|---|-------|-----------|------------|
| 1 | `execute_activity()` inconsistente entre US-94.2 y US-94.23 | 🔴 Alta | Unificar en diseño B.2.1 |
| 2 | `SagaEngine` con 4 genéricos demasiado verboso | 🟠 Media | Usar trait objects + builder (B.2.2) |
| 3 | Falta transacción en replay | 🔴 Alta | Implementar `TransactionalReplayer` (B.2.3) |
| 4 | `ActivityRegistry` DashMap vs HashMap | 🟡 Baja | Usar `DashMap` (concurrente) |
| 5 | Determinism safeguards solo compile-time | 🟠 Media | Añadir runtime enforcer (B.2.6) |
| 6 | Sistema de errores duplicado | 🟠 Media | Consolidar en `ActivityError` único (B.2.1) |
| 7 | Sin plan para migrar compensaciones | 🔴 Alta | Sistema Saga Pattern (B.2.4) |
| 8 | Worker hardcodea PostgresTimerStore | 🟠 Media | Worker modular (B.2.7) |
| 9 | TestEnvironment God Object | 🟡 Baja | Separar en componentes (B.2.8) |
| 10 | Sin estrategia de snapshotting | 🟠 Media | Implementar snapshots (B.2.5) |

---

### B.2 Diseños Consolidados Definitivos

#### B.2.1 API Unificada de `execute_activity()` ✅

> [!CAUTION]
> **DISEÑO DEFINITIVO** - Reemplaza US-94.2 y US-94.23

```rust
// crates/saga-engine/core/src/workflow/context.rs - DISEÑO FINAL

impl WorkflowContext {
    /// Ejecutar actividad de forma durable
    /// 
    /// # Comportamiento
    /// - En primera ejecución: programa actividad y suspende workflow
    /// - En replay: retorna resultado cacheado inmediatamente
    /// - El motor maneja la suspensión/reanudación automáticamente
    /// 
    /// # Ejemplo
    /// ```rust
    /// let user = ctx.execute::<FetchUserActivity>(input).await?;
    /// println!("User: {}", user.name);  // Tipado fuerte
    /// ```
    pub async fn execute<A: Activity>(
        &mut self,
        input: A::Input,
    ) -> Result<A::Output, ActivityError>
    where
        A::Error: Into<ActivityError>,
    {
        let activity_seq = self.next_activity_sequence();
        let activity_id = format!("{}:{}", A::TYPE_ID, activity_seq);
        
        // 1. Check replay cache
        if let Some(result) = self.get_cached_activity_result(&activity_id) {
            return result.map_err(|e| e.into());
        }
        
        // 2. Verificar determinismo en replay
        if self.is_replaying {
            return Err(ActivityError::NonDeterminism {
                message: format!(
                    "Activity '{}' not found in history during replay. \
                     Check workflow code for non-determinism.",
                    activity_id
                ),
            });
        }
        
        // 3. Serializar y programar
        let input_value = serde_json::to_value(&input)
            .map_err(|e| ActivityError::Serialization(e.to_string()))?;
        
        // 4. Registrar comando pendiente (engine lo procesará)
        self.pending_commands.push(WorkflowCommand::ScheduleActivity {
            activity_id: activity_id.clone(),
            activity_type: A::TYPE_ID.to_string(),
            input: input_value,
            options: ActivityOptions::default(),
        });
        
        // 5. Señalar suspensión
        Err(ActivityError::Scheduled { activity_id })
    }
    
    /// Ejecutar actividad con opciones personalizadas
    pub async fn execute_with_options<A: Activity>(
        &mut self,
        input: A::Input,
        options: ActivityOptions,
    ) -> Result<A::Output, ActivityError>
    where
        A::Error: Into<ActivityError>,
    {
        // ... mismo patrón con options personalizadas
    }
}

/// Error unificado de actividades (DISEÑO FINAL)
#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    /// Actividad necesita ser programada (suspender workflow)
    #[error("Activity scheduled: {activity_id}")]
    Scheduled { activity_id: String },
    
    /// Error de negocio de la actividad
    #[error("Activity business error: {message}")]
    Business { message: String, retryable: bool },
    
    /// Timeout de ejecución
    #[error("Activity timeout after {duration:?}")]
    Timeout { duration: Duration },
    
    /// Error de red transitorio (reintentar)
    #[error("Activity transient error: {message}")]
    Transient { message: String },
    
    /// Error permanente (no reintentar)
    #[error("Activity permanent error: {message}")]
    Permanent { message: String },
    
    /// Worker murió
    #[error("Activity worker crashed")]
    WorkerCrash,
    
    /// Cancelado
    #[error("Activity cancelled")]
    Cancelled,
    
    /// No-determinismo detectado
    #[error("Non-determinism: {message}")]
    NonDeterminism { message: String },
    
    /// Error de serialización
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl ActivityError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            Self::Transient { .. } | 
            Self::Timeout { .. } | 
            Self::WorkerCrash
        )
    }
    
    pub fn classification(&self) -> ErrorClassification {
        match self {
            Self::Business { retryable: false, .. } => ErrorClassification::Business,
            Self::Business { retryable: true, .. } => ErrorClassification::Transient,
            Self::Transient { .. } | Self::Timeout { .. } => ErrorClassification::Transient,
            Self::Permanent { .. } | Self::Serialization(_) => ErrorClassification::Permanent,
            Self::WorkerCrash => ErrorClassification::WorkerCrash,
            Self::Cancelled => ErrorClassification::Cancelled,
            Self::Scheduled { .. } | Self::NonDeterminism { .. } => ErrorClassification::Permanent,
        }
    }
}
```

---

#### B.2.2 SagaEngine Simplificado con Builder ✅

> [!TIP]
> Elimina complejidad de 4 parámetros genéricos usando trait objects.

```rust
// crates/saga-engine/core/src/engine.rs - DISEÑO SIMPLIFICADO

/// Engine principal (SIN genéricos excesivos)
pub struct SagaEngine {
    runtime: Arc<dyn SagaRuntime>,
    config: EngineConfig,
    metrics: Arc<dyn EngineMetrics>,
}

/// Runtime unificado (trait object para flexibilidad)
#[async_trait]
pub trait SagaRuntime: Send + Sync {
    fn event_store(&self) -> &dyn EventStore;
    fn task_queue(&self) -> &dyn TaskQueue;
    fn timer_store(&self) -> &dyn TimerStore;
    fn history_replayer(&self) -> &dyn HistoryReplayer;
    fn outbox_relay(&self) -> &dyn OutboxRelay;
    fn signal_dispatcher(&self) -> &dyn SignalDispatcher;
}

/// Builder fluido para configuración simple
pub struct SagaEngineBuilder {
    runtime: Option<Arc<dyn SagaRuntime>>,
    config: EngineConfig,
    metrics: Option<Arc<dyn EngineMetrics>>,
}

impl SagaEngineBuilder {
    pub fn new() -> Self {
        Self {
            runtime: None,
            config: EngineConfig::default(),
            metrics: None,
        }
    }
    
    /// Configurar con PostgreSQL + NATS (producción)
    pub async fn with_postgres_nats(
        mut self,
        postgres_url: &str,
        nats_url: &str,
    ) -> Result<Self, BuilderError> {
        let pool = PgPool::connect(postgres_url).await?;
        let nats = async_nats::connect(nats_url).await?;
        
        self.runtime = Some(Arc::new(PostgresNatsRuntime::new(pool, nats).await?));
        Ok(self)
    }
    
    /// Configurar in-memory (testing)
    pub fn with_in_memory(mut self) -> Self {
        self.runtime = Some(Arc::new(InMemoryRuntime::default()));
        self
    }
    
    /// Configuración de concurrencia
    pub fn max_concurrent_workflows(mut self, n: usize) -> Self {
        self.config.max_concurrent_workflows = n;
        self
    }
    
    /// Timeout de replay
    pub fn replay_timeout(mut self, duration: Duration) -> Self {
        self.config.replay_timeout = duration;
        self
    }
    
    pub fn build(self) -> Result<SagaEngine, BuilderError> {
        let runtime = self.runtime.ok_or(BuilderError::RuntimeNotConfigured)?;
        
        Ok(SagaEngine {
            runtime,
            config: self.config,
            metrics: self.metrics.unwrap_or_else(|| Arc::new(NoopMetrics)),
        })
    }
}

// USO SIMPLE (5 líneas máximo)
let engine = SagaEngine::builder()
    .with_postgres_nats("postgresql://...", "nats://...")
    .await?
    .max_concurrent_workflows(100)
    .build()?;

// Para tests
let engine = SagaEngine::builder()
    .with_in_memory()
    .build()?;
```

---

#### B.2.3 Transactional Replayer ✅

> [!WARNING]
> **CRÍTICO**: Garantiza atomicidad entre replay y persistencia.

```rust
// crates/saga-engine/core/src/replay/transactional.rs - NUEVO

/// Replayer con garantías transaccionales
pub struct TransactionalReplayer {
    event_store: Arc<dyn TransactionalEventStore>,
    task_queue: Arc<dyn TaskQueue>,
}

/// EventStore con soporte transaccional
#[async_trait]
pub trait TransactionalEventStore: EventStore {
    type Transaction: Send;
    
    async fn begin_transaction(&self) -> Result<Self::Transaction, Error>;
    async fn commit(&self, tx: Self::Transaction) -> Result<(), Error>;
    async fn rollback(&self, tx: Self::Transaction) -> Result<(), Error>;
    
    async fn append_event_in_tx(
        &self,
        tx: &mut Self::Transaction,
        saga_id: &SagaId,
        event: &HistoryEvent,
    ) -> Result<EventId, Error>;
}

impl TransactionalReplayer {
    /// Replay y ejecutar con garantías ACID
    pub async fn replay_and_execute<W: Workflow>(
        &self,
        workflow: &W,
        saga_id: &SagaId,
    ) -> Result<WorkflowOutcome<W::Output>, Error> {
        // 1. Iniciar transacción
        let mut tx = self.event_store.begin_transaction().await?;
        
        // 2. Replay dentro de transacción (lectura consistente)
        let (mut ctx, input) = self.replay_in_transaction(&tx, saga_id).await?;
        
        // 3. Ejecutar workflow
        let result = workflow.run(&mut ctx, input).await;
        
        // 4. Manejar resultado atómicamente
        match result {
            Ok(output) => {
                // Persistir evento de completado
                self.persist_completion(&mut tx, &ctx, &output).await?;
                tx = self.event_store.commit(tx).await?;
                Ok(WorkflowOutcome::Completed(output))
            }
            
            Err(ActivityError::Scheduled { activity_id }) => {
                // Persistir estado de pausa + encolar actividad
                self.persist_scheduled_activity(&mut tx, &ctx, &activity_id).await?;
                tx = self.event_store.commit(tx).await?;
                
                // Encolar DESPUÉS del commit (eventual consistency OK aquí)
                self.enqueue_activity_task(&activity_id, &ctx).await?;
                
                Ok(WorkflowOutcome::Suspended)
            }
            
            Err(e) => {
                // Rollback en caso de error
                self.event_store.rollback(tx).await?;
                Err(e.into())
            }
        }
    }
}

pub enum WorkflowOutcome<T> {
    Completed(T),
    Suspended,
    Failed(Error),
}
```

---

#### B.2.4 Sistema de Compensaciones para Workflow-as-Code ✅

> [!IMPORTANT]
> Migración del patrón `compensate()` de step-list a Workflow-as-Code.

```rust
// crates/saga-engine/core/src/workflow/compensation.rs - NUEVO

/// Trait para definir compensaciones
#[async_trait]
pub trait Compensation<A: Activity>: Send + Sync + Clone {
    async fn compensate(&self, output: A::Output) -> Result<(), CompensationError>;
}

/// Extensión de contexto para saga pattern
impl WorkflowContext {
    /// Ejecutar actividad con compensación automática
    /// 
    /// Si el workflow falla después de esta actividad, la compensación
    /// se ejecutará automáticamente en orden inverso (saga pattern).
    /// 
    /// # Ejemplo
    /// ```rust
    /// // Reservar recursos
    /// let reservation = ctx.execute_compensable::<ReserveInventory>(
    ///     ReserveInput { item_id, quantity },
    ///     CancelReservation { reservation_id },  // Compensación
    /// ).await?;
    /// 
    /// // Cobrar (si falla, la reserva se cancela automáticamente)
    /// ctx.execute::<ChargePayment>(payment_input).await?;
    /// ```
    pub async fn execute_compensable<A, C>(
        &mut self,
        input: A::Input,
        compensation: C,
    ) -> Result<A::Output, ActivityError>
    where
        A: Activity,
        C: Compensation<A> + 'static,
        A::Error: Into<ActivityError>,
    {
        let output = self.execute::<A>(input).await?;
        
        // Registrar compensación en el contexto
        self.compensation_stack.push(CompensationRecord {
            activity_type: A::TYPE_ID.to_string(),
            output: serde_json::to_value(&output)?,
            compensator: Box::new(TypedCompensator::<A, C>::new(compensation)),
        });
        
        Ok(output)
    }
    
    /// Ejecutar compensaciones manualmente (para errores de negocio)
    pub async fn compensate_all(&mut self, reason: &str) -> Result<(), CompensationError> {
        tracing::warn!(reason = reason, "Executing compensations");
        
        // Ejecutar en orden inverso (LIFO)
        while let Some(record) = self.compensation_stack.pop() {
            record.compensator.execute(record.output).await?;
            
            // Registrar evento de compensación
            self.pending_commands.push(WorkflowCommand::CompensationExecuted {
                activity_type: record.activity_type,
                reason: reason.to_string(),
            });
        }
        
        Ok(())
    }
}

/// Ejemplo de migración de step-list a Workflow-as-Code
/// 
/// ANTES (step-list):
/// ```rust
/// struct ProvisionStep { ... }
/// impl WorkflowStep for ProvisionStep {
///     async fn execute(...) -> Result<Value, StepError> { ... }
///     async fn compensate(...) -> Result<(), StepError> { ... }
/// }
/// ```
/// 
/// DESPUÉS (Workflow-as-Code):
/// ```rust
/// #[workflow]
/// impl Workflow for ProvisioningWorkflow {
///     async fn run(&self, ctx, input) -> Result<Output, Error> {
///         // Con compensación automática
///         let infra = ctx.execute_compensable::<CreateInfraActivity>(
///             CreateInfraInput { ... },
///             DestroyInfraCompensation { ... },
///         ).await?;
///         
///         // Si falla aquí, infra se destruye automáticamente
///         let worker = ctx.execute::<RegisterWorkerActivity>(infra.id).await?;
///         
///         Ok(Output { worker_id: worker.id })
///     }
/// }
/// ```
```

---

#### B.2.5 Snapshotting para Replay Eficiente ✅

> [!NOTE]
> Optimización para workflows con miles de eventos.

```rust
// crates/saga-engine/core/src/snapshot/mod.rs - MEJORADO

/// Snapshot del estado del workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub saga_id: SagaId,
    pub snapshot_id: Uuid,
    pub last_event_id: EventId,
    pub context_state: WorkflowContextState,
    pub activity_cache: HashMap<String, CachedActivityResult>,
    pub compensation_stack: Vec<CompensationRecord>,
    pub created_at: DateTime<Utc>,
    pub checksum: String,  // Para verificar integridad
}

/// Estado serializable del contexto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowContextState {
    pub activity_sequence: u64,
    pub timer_sequence: u64,
    pub side_effect_sequence: u64,
    pub current_step: Option<String>,
    pub metadata: WorkflowMetadata,
}

/// Política de snapshotting
#[derive(Debug, Clone)]
pub struct SnapshotPolicy {
    /// Crear snapshot cada N eventos
    pub events_threshold: u64,
    /// O cada N segundos de ejecución
    pub time_threshold: Duration,
    /// Retener últimos N snapshots
    pub retention_count: usize,
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            events_threshold: 1000,
            time_threshold: Duration::from_secs(300),
            retention_count: 3,
        }
    }
}

/// Replayer optimizado con snapshots
impl HistoryReplayer {
    pub async fn replay_from_snapshot(
        &self,
        saga_id: &SagaId,
    ) -> Result<WorkflowContext, ReplayError> {
        // 1. Intentar cargar snapshot más reciente
        if let Some(snapshot) = self.load_latest_snapshot(saga_id).await? {
            // Verificar integridad
            if !self.verify_snapshot_checksum(&snapshot) {
                tracing::warn!("Snapshot checksum failed, falling back to full replay");
                return self.replay_full(saga_id).await;
            }
            
            // 2. Reconstruir desde snapshot
            let mut ctx = WorkflowContext::from_snapshot(snapshot.context_state);
            ctx.activity_cache = snapshot.activity_cache;
            ctx.compensation_stack = snapshot.compensation_stack;
            
            // 3. Replay solo eventos DESPUÉS del snapshot
            let events = self.event_store
                .get_events_after(saga_id, snapshot.last_event_id)
                .await?;
            
            tracing::info!(
                saga_id = %saga_id,
                snapshot_event = snapshot.last_event_id.0,
                remaining_events = events.len(),
                "Replaying from snapshot"
            );
            
            for event in events {
                ctx.apply_event(&event)?;
            }
            
            return Ok(ctx);
        }
        
        // Fallback: replay completo
        self.replay_full(saga_id).await
    }
    
    /// Crear snapshot si cumple política
    pub async fn maybe_create_snapshot(
        &self,
        ctx: &WorkflowContext,
        policy: &SnapshotPolicy,
    ) -> Result<Option<WorkflowSnapshot>, Error> {
        let should_snapshot = 
            ctx.event_count() % policy.events_threshold == 0 ||
            ctx.elapsed_since_last_snapshot() > policy.time_threshold;
        
        if should_snapshot {
            let snapshot = WorkflowSnapshot {
                saga_id: ctx.saga_id.clone(),
                snapshot_id: Uuid::new_v4(),
                last_event_id: ctx.last_event_id(),
                context_state: ctx.to_state(),
                activity_cache: ctx.activity_cache.clone(),
                compensation_stack: ctx.compensation_stack.clone(),
                created_at: Utc::now(),
                checksum: ctx.compute_checksum(),
            };
            
            self.save_snapshot(&snapshot).await?;
            
            // Cleanup old snapshots
            self.cleanup_old_snapshots(&ctx.saga_id, policy.retention_count).await?;
            
            return Ok(Some(snapshot));
        }
        
        Ok(None)
    }
}
```

---

#### B.2.6 Determinism Enforcer Mejorado ✅

> [!CAUTION]
> Detección de no-determinismo en RUNTIME además de compile-time.

```rust
// crates/saga-engine/core/src/determinism/enforcer.rs - NUEVO

/// Verificador de determinismo en runtime
pub struct DeterminismEnforcer {
    saga_id: SagaId,
    expected_events: VecDeque<HistoryEvent>,
    current_index: usize,
    is_replaying: bool,
    violations: Vec<DeterminismViolation>,
}

impl DeterminismEnforcer {
    /// Verificar que la próxima actividad coincide con el historial
    pub fn check_activity_schedule(
        &mut self,
        activity_type: &str,
        input: &serde_json::Value,
    ) -> Result<(), DeterminismViolation> {
        if !self.is_replaying {
            return Ok(());  // Primera ejecución, no hay que verificar
        }
        
        let expected = self.expected_events.get(self.current_index)
            .ok_or(DeterminismViolation::ExtraActivity {
                activity_type: activity_type.to_string(),
            })?;
        
        match &expected.event_type {
            EventType::ActivityScheduled => {
                let expected_type: String = expected.payload.get("activity_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                
                let expected_input = expected.payload.get("input");
                
                if activity_type != expected_type {
                    return Err(DeterminismViolation::ActivityTypeMismatch {
                        expected: expected_type,
                        actual: activity_type.to_string(),
                    });
                }
                
                if Some(input) != expected_input {
                    return Err(DeterminismViolation::ActivityInputMismatch {
                        activity_type: activity_type.to_string(),
                        expected: expected_input.cloned().unwrap_or_default(),
                        actual: input.clone(),
                    });
                }
                
                self.current_index += 1;
                Ok(())
            }
            _ => Err(DeterminismViolation::UnexpectedEventType {
                expected: format!("{:?}", expected.event_type),
                actual: "ActivityScheduled".to_string(),
            }),
        }
    }
    
    /// Log detallado de violaciones para debugging
    pub fn log_violations(&self) {
        for violation in &self.violations {
            tracing::error!(
                saga_id = %self.saga_id,
                violation = ?violation,
                "Determinism violation detected"
            );
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeterminismViolation {
    #[error("Activity type mismatch: expected '{expected}', got '{actual}'")]
    ActivityTypeMismatch { expected: String, actual: String },
    
    #[error("Activity input mismatch for '{activity_type}'")]
    ActivityInputMismatch { 
        activity_type: String,
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    
    #[error("Extra activity scheduled: '{activity_type}'")]
    ExtraActivity { activity_type: String },
    
    #[error("Missing expected activity")]
    MissingActivity,
    
    #[error("Unexpected event type: expected '{expected}', got '{actual}'")]
    UnexpectedEventType { expected: String, actual: String },
}
```

---

#### B.2.7 Worker Modular ✅

> [!TIP]
> Arquitectura de plugins para extensibilidad.

```rust
// crates/saga-engine/core/src/worker/modular.rs - NUEVO

/// Worker basado en módulos intercambiables
pub struct ModularWorker {
    modules: Vec<Box<dyn WorkerModule>>,
    config: WorkerConfig,
    shutdown: ShutdownSignal,
}

/// Trait para módulos de worker
#[async_trait]
pub trait WorkerModule: Send + Sync {
    fn name(&self) -> &'static str;
    async fn start(&self, ctx: &WorkerContext) -> Result<(), WorkerError>;
    async fn tick(&self, ctx: &WorkerContext) -> Result<TickResult, WorkerError>;
    async fn stop(&self) -> Result<(), WorkerError>;
}

pub enum TickResult {
    Idle,           // No work done
    Busy,           // Work done, check again immediately
    Sleep(Duration), // Sleep before next check
}

/// Módulo de workflows
pub struct WorkflowModule {
    engine: Arc<SagaEngine>,
    registry: Arc<WorkflowRegistry>,
    task_queue: Arc<dyn TaskQueue>,
}

#[async_trait]
impl WorkerModule for WorkflowModule {
    fn name(&self) -> &'static str { "workflows" }
    
    async fn tick(&self, _ctx: &WorkerContext) -> Result<TickResult, WorkerError> {
        if let Some(task) = self.task_queue.try_fetch().await? {
            self.process_workflow_task(task).await?;
            Ok(TickResult::Busy)
        } else {
            Ok(TickResult::Idle)
        }
    }
}

/// Módulo de activities
pub struct ActivityModule {
    registry: Arc<ActivityRegistry>,
    task_queue: Arc<dyn TaskQueue>,
}

/// Módulo de timers
pub struct TimerModule {
    timer_store: Arc<dyn TimerStore>,  // ← Trait object, no hardcoded
    task_queue: Arc<dyn TaskQueue>,
}

// Builder para configuración modular
impl ModularWorker {
    pub fn builder() -> ModularWorkerBuilder {
        ModularWorkerBuilder::new()
    }
}

impl ModularWorkerBuilder {
    pub fn with_module<M: WorkerModule + 'static>(mut self, module: M) -> Self {
        self.modules.push(Box::new(module));
        self
    }
    
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.config.max_concurrent_tasks = n;
        self
    }
}

// Uso
let worker = ModularWorker::builder()
    .with_module(WorkflowModule::new(engine, registry, task_queue))
    .with_module(ActivityModule::new(activity_registry, activity_queue))
    .with_module(TimerModule::new(timer_store, task_queue)) // Timer store genérico
    .with_concurrency(10)
    .build();

worker.run(shutdown_signal).await?;
```

---

#### B.2.8 Testing Mejorado ✅

> [!NOTE]
> Separación de TestEnvironment en componentes + test harness fluido.

```rust
// crates/saga-engine/testing/src/harness.rs - NUEVO

/// Harness fluido para tests de workflows
pub struct WorkflowTestHarness<W: Workflow> {
    workflow: W,
    env_config: TestEnvConfig,
    activity_mocks: HashMap<&'static str, Box<dyn ActivityMock>>,
    assertions: Vec<Box<dyn WorkflowAssertion>>,
}

impl<W: Workflow> WorkflowTestHarness<W> {
    pub fn new(workflow: W) -> Self {
        Self {
            workflow,
            env_config: TestEnvConfig::default(),
            activity_mocks: HashMap::new(),
            assertions: Vec::new(),
        }
    }
    
    /// Mockear actividad
    pub fn mock_activity<A: Activity>(mut self, result: A::Output) -> Self
    where
        A::Output: Clone + 'static,
    {
        self.activity_mocks.insert(A::TYPE_ID, Box::new(FixedMock(result)));
        self
    }
    
    /// Configurar tiempo inicial
    pub fn with_start_time(mut self, time: DateTime<Utc>) -> Self {
        self.env_config.start_time = time;
        self
    }
    
    /// Assertion: actividad fue llamada N veces
    pub fn expect_activity_calls<A: Activity>(mut self, count: usize) -> Self {
        self.assertions.push(Box::new(ActivityCallAssertion {
            activity_type: A::TYPE_ID,
            expected_count: count,
        }));
        self
    }
    
    /// Assertion: output cumple predicado
    pub fn expect_output<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&W::Output) -> bool + 'static,
    {
        self.assertions.push(Box::new(OutputAssertion(Box::new(predicate))));
        self
    }
    
    /// Ejecutar test
    pub async fn run(self, input: W::Input) -> TestResult<W::Output> {
        // Crear environment con mocks
        let mut env = WorkflowTestEnvironment::with_config(self.env_config);
        env.set_activity_mocks(self.activity_mocks);
        
        // Ejecutar
        let result = env.execute(&self.workflow, input).await?;
        
        // Verificar assertions
        for assertion in self.assertions {
            assertion.verify(&env, &result)?;
        }
        
        Ok(result)
    }
}

// Uso fluido
#[tokio::test]
async fn test_provisioning_workflow() {
    let result = WorkflowTestHarness::new(ProvisioningWorkflow::new())
        .mock_activity::<ValidateProviderActivity>(ValidateOutput::valid())
        .mock_activity::<CreateInfraActivity>(InfraOutput { id: "infra-1" })
        .expect_activity_calls::<ValidateProviderActivity>(1)
        .expect_activity_calls::<CreateInfraActivity>(1)
        .expect_output(|o| o.worker_id.starts_with("wrk-"))
        .run(ProvisioningInput { ... })
        .await
        .unwrap();
    
    assert_eq!(result.status, "provisioned");
}
```

---

### B.3 Plan de Implementación Priorizado Actualizado

> [!IMPORTANT]
> Plan revisado incorporando todas las consolidaciones técnicas.

| Fase | Semanas | Foco | User Stories | Dependencias |
|------|---------|------|--------------|--------------|
| **1 - Core Estable** | 1-3 | API unificada, SagaEngine simple | B.2.1, B.2.2, B.2.3 | - |
| **2 - Testing First** | 4-5 | Harness, determinism enforcer | B.2.8, B.2.6 | Fase 1 |
| **3 - Migración Safe** | 6-7 | Compensaciones, workflows existentes | B.2.4, US-94.L-X | Fase 2 |
| **4 - Optimización** | 8-9 | Snapshots, worker modular | B.2.5, B.2.7, B.2.10 | Fase 3 |
| **5 - Producción** | 10-12 | Limpieza legacy, métricas | Cleanup, observability | Fase 4 |

---

### B.4 Issues Adicionales Identificados

| # | Issue | Severidad | Resolución |
|---|-------|-----------|------------|
| 11 | Outbox dentro del Engine viola "client solo define workflows" | 🟠 Media | **B.2.9** - Outbox opcional con NoOp default |
| 12 | Replay ciego (desde evento 0) no escala | 🔴 Alta | **B.2.10** - `checkpoint()` API explícita |
| 13 | Macro `#[workflow]` no detecta mutación global | 🟠 Media | **B.2.11** - `&self` inmutable + runtime checks |

---

#### B.2.9 Optional Outbox (Simplificación) ✅

> [!IMPORTANT]
> **Principio clave**: "El código cliente SOLO define workflows". 
> El Outbox debe ser **transparente** al usuario, no una configuración obligatoria.

```rust
// crates/saga-engine/core/src/outbox/optional.rs - NUEVO

/// Estrategia de publicación de eventos del workflow
#[derive(Debug, Clone, Default)]
pub enum OutboxStrategy {
    /// Sin outbox - eventos se pierden si falla después de commit
    /// Útil para desarrollo y tests
    #[default]
    NoOp,
    
    /// Outbox integrado - el engine maneja todo automáticamente
    /// El relay se inicia como background task del engine
    Integrated(IntegratedOutboxConfig),
    
    /// Outbox externo - el usuario configura su propio relay
    /// Para casos donde ya existe infraestructura de outbox
    External(Arc<dyn Outbox>),
}

/// Configuración de outbox integrado (0 fricción para el usuario)
#[derive(Debug, Clone)]
pub struct IntegratedOutboxConfig {
    /// Tabla de outbox (default: saga_outbox)
    pub table_name: String,
    /// Intervalo de poll (fallback si LISTEN/NOTIFY no disponible)
    pub poll_interval: Duration,
    /// Batch size por ciclo
    pub batch_size: usize,
}

impl Default for IntegratedOutboxConfig {
    fn default() -> Self {
        Self {
            table_name: "saga_outbox".to_string(),
            poll_interval: Duration::from_secs(1),
            batch_size: 100,
        }
    }
}

/// Builder con outbox opcional y transparente
impl SagaEngineBuilder {
    /// Sin outbox (default) - para desarrollo/tests
    pub fn without_outbox(mut self) -> Self {
        self.outbox_strategy = OutboxStrategy::NoOp;
        self
    }
    
    /// Outbox integrado - el engine maneja TODO automáticamente
    /// 
    /// # Zero Configuration
    /// No necesita configurar relay, bus, ni consumidores.
    /// El engine inicia el relay internamente.
    pub fn with_integrated_outbox(mut self) -> Self {
        self.outbox_strategy = OutboxStrategy::Integrated(Default::default());
        self
    }
    
    /// Outbox externo para casos avanzados
    pub fn with_external_outbox(mut self, outbox: Arc<dyn Outbox>) -> Self {
        self.outbox_strategy = OutboxStrategy::External(outbox);
        self
    }
}

// USO SIMPLIFICADO

// Desarrollo: sin outbox (0 config)
let dev_engine = SagaEngine::builder()
    .with_in_memory()
    .build()?;  // NoOp outbox por defecto

// Producción: outbox integrado (1 línea extra)
let prod_engine = SagaEngine::builder()
    .with_postgres_nats("postgresql://...", "nats://...")
    .await?
    .with_integrated_outbox()  // ← El engine maneja el relay internamente
    .build()?;

// El usuario SOLO define workflows, sin preocuparse por:
// - OutboxRelay
// - OutboxRepository
// - Consumidores de eventos
// - Configuración de LISTEN/NOTIFY
```

**Implicación arquitectónica:**
- `CommandBus` y `EventBus` se mueven a `saga-engine-ext` (extensión opcional)
- El core (`saga-engine-core`) solo expone `Outbox` como trait opcional
- La integración con NATS/mensajería es un "adapter" que el usuario puede o no activar

---

#### B.2.10 Checkpoint API (Snapshotting Explícito) ✅

> [!WARNING]
> **Crítico para workflows largos**: Un workflow de 6 meses con 50,000 eventos NO puede hacer replay desde evento 0.

```rust
// crates/saga-engine/core/src/workflow/checkpoint.rs - NUEVO

impl WorkflowContext {
    /// Crear checkpoint del estado actual
    /// 
    /// Guarda las variables del workflow para evitar replay desde evento 0.
    /// 
    /// # Cuándo usar
    /// - Después de operaciones costosas de recomputar
    /// - En loops largos (cada N iteraciones)
    /// - Antes de waits largos (timers, señales)
    /// 
    /// # Ejemplo: Workflow con loop largo
    /// ```rust
    /// #[workflow]
    /// impl Workflow for BatchProcessor {
    ///     async fn run(&self, ctx: &mut WorkflowContext, input: BatchInput) -> Result<(), Error> {
    ///         let mut processed = 0;
    ///         let mut cursor = input.start_cursor;
    ///         
    ///         loop {
    ///             // Procesar batch
    ///             let batch = ctx.execute::<FetchBatch>(cursor).await?;
    ///             
    ///             for item in batch.items {
    ///                 ctx.execute::<ProcessItem>(item).await?;
    ///                 processed += 1;
    ///             }
    ///             
    ///             cursor = batch.next_cursor;
    ///             
    ///             // ⚡ CHECKPOINT cada 1000 items
    ///             // Si el workflow se reanuda, empezará desde aquí
    ///             if processed % 1000 == 0 {
    ///                 ctx.checkpoint(json!({
    ///                     "processed": processed,
    ///                     "cursor": cursor,
    ///                 }));
    ///             }
    ///             
    ///             if batch.is_last {
    ///                 break;
    ///             }
    ///         }
    ///         
    ///         Ok(())
    ///     }
    /// }
    /// ```
    pub fn checkpoint(&mut self, memo: serde_json::Value) {
        let checkpoint = WorkflowCheckpoint {
            checkpoint_id: self.next_checkpoint_id(),
            activity_sequence: self.activity_sequence,
            timer_sequence: self.timer_sequence,
            side_effect_sequence: self.side_effect_sequence,
            memo,
            activity_cache: self.activity_cache.clone(),
            compensation_stack: self.compensation_stack.clone(),
            created_at: self.current_time(),
        };
        
        // Registrar comando para persistir checkpoint
        self.pending_commands.push(WorkflowCommand::SaveCheckpoint(checkpoint));
    }
    
    /// Obtener memo del último checkpoint (para reanudar estado)
    pub fn last_checkpoint_memo<T: DeserializeOwned>(&self) -> Option<T> {
        self.last_checkpoint
            .as_ref()
            .and_then(|cp| serde_json::from_value(cp.memo.clone()).ok())
    }
    
    /// Helper para saber si debemos hacer checkpoint
    pub fn should_checkpoint(&self, every_n_events: u64) -> bool {
        self.event_count() % every_n_events == 0
    }
}

/// Checkpoint persistible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCheckpoint {
    pub checkpoint_id: u64,
    pub activity_sequence: u64,
    pub timer_sequence: u64,
    pub side_effect_sequence: u64,
    pub memo: serde_json::Value,
    pub activity_cache: HashMap<String, CachedActivityResult>,
    pub compensation_stack: Vec<CompensationRecord>,
    pub created_at: DateTime<Utc>,
}

/// Replayer con soporte de checkpoints
impl HistoryReplayer {
    pub async fn replay_from_checkpoint(
        &self,
        saga_id: &SagaId,
    ) -> Result<WorkflowContext, ReplayError> {
        // 1. Buscar último checkpoint
        let checkpoint = self.event_store
            .get_latest_checkpoint(saga_id)
            .await?;
        
        match checkpoint {
            Some(cp) => {
                tracing::info!(
                    saga_id = %saga_id,
                    checkpoint_id = cp.checkpoint_id,
                    "Resuming from checkpoint"
                );
                
                // 2. Reconstruir contexto desde checkpoint
                let mut ctx = WorkflowContext::from_checkpoint(cp.clone());
                ctx.last_checkpoint = Some(cp.clone());
                
                // 3. Replay solo eventos DESPUÉS del checkpoint
                let events_after = self.event_store
                    .get_events_after_checkpoint(saga_id, cp.checkpoint_id)
                    .await?;
                
                for event in events_after {
                    ctx.apply_event(&event)?;
                }
                
                Ok(ctx)
            }
            None => {
                // Sin checkpoint, replay completo
                self.replay_full(saga_id).await
            }
        }
    }
}

/// Política de checkpoints automáticos
#[derive(Debug, Clone)]
pub struct AutoCheckpointPolicy {
    /// Crear checkpoint cada N eventos
    pub every_n_events: u64,
    /// O cada N segundos de ejecución virtual
    pub every_n_seconds: Option<Duration>,
    /// Máximo número de checkpoints a retener
    pub max_checkpoints: usize,
}

impl Default for AutoCheckpointPolicy {
    fn default() -> Self {
        Self {
            every_n_events: 1000,
            every_n_seconds: Some(Duration::from_secs(300)),
            max_checkpoints: 5,
        }
    }
}
```

**Comparativa de rendimiento:**

| Escenario | Sin Checkpoint | Con Checkpoint |
|-----------|----------------|----------------|
| 100 eventos | ~5ms | ~5ms |
| 1,000 eventos | ~50ms | ~10ms |
| 10,000 eventos | ~500ms | ~15ms |
| 50,000 eventos | ~2,500ms ❌ | ~20ms ✅ |

---

#### B.2.11 Enhanced #[workflow] Macro ✅

> [!CAUTION]
> Prevenir mutación de estado global y forzar inmutabilidad del workflow.

```rust
// crates/saga-engine/macros/src/workflow_attr.rs - MEJORADO

/// Macro `#[workflow]` mejorada
/// 
/// # Garantías
/// 1. `run()` recibe `&self` (inmutable), no `&mut self`
/// 2. Todo estado mutable debe ir en `WorkflowContext`
/// 3. Detección de patrones no-deterministas en tiempo de compilación
/// 4. Inyección de checks en runtime para edge cases
/// 
/// # Ejemplo correcto
/// ```rust
/// #[workflow]
/// impl Workflow for OrderWorkflow {
///     // ✅ &self es inmutable
///     async fn run(&self, ctx: &mut WorkflowContext, input: OrderInput) 
///         -> Result<OrderOutput, OrderError> 
///     {
///         // ✅ Estado en ctx, no en self
///         let order_id = ctx.random_uuid();
///         
///         // ✅ Memoization via checkpoint
///         let items = ctx.execute::<FetchItems>(order_id).await?;
///         
///         Ok(OrderOutput { order_id, items })
///     }
/// }
/// ```
/// 
/// # Errores de compilación
/// ```rust
/// #[workflow]
/// impl Workflow for BadWorkflow {
///     // ❌ ERROR: `&mut self` no permitido en workflows
///     async fn run(&mut self, ...) { ... }
/// }
/// 
/// struct BadWorkflow {
///     counter: Cell<i32>,  // ❌ WARNING: Interior mutability detected
/// }
/// ```
#[proc_macro_attribute]
pub fn workflow(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    
    // 1. Verificar que run() usa &self, no &mut self
    validate_immutable_self(&input)?;
    
    // 2. Detectar interior mutability (Cell, RefCell, Mutex fields)
    detect_interior_mutability(&input)?;
    
    // 3. Escanear patrones no-deterministas
    let violations = scan_non_deterministic_patterns(&input);
    if !violations.is_empty() {
        return compile_error(violations);
    }
    
    // 4. Inyectar runtime guards
    inject_determinism_guards(input)
}

/// Patrones detectados en tiempo de compilación
const COMPILE_TIME_CHECKS: &[(&str, &str)] = &[
    ("static mut", "Global mutable state is non-deterministic"),
    ("thread_local!", "Thread locals are non-deterministic"),
    ("lazy_static!", "Lazy statics may be non-deterministic"),
    ("once_cell::sync::Lazy", "Lazy initialization may be non-deterministic"),
    ("std::env::var", "Environment variables may change between replays"),
    ("std::fs::", "File system access is non-deterministic"),
    ("std::net::", "Network access should be in activities"),
    ("tokio::spawn", "Spawning tasks is non-deterministic, use activities"),
];

/// Patrón recomendado: Workflow inmutable con contexto mutable
/// 
/// ```rust
/// // EL WORKFLOW ES INMUTABLE (puede ser Clone, Debug, etc.)
/// #[derive(Clone, Debug)]
/// struct OrderWorkflow {
///     // Solo datos de configuración inmutables
///     default_timeout: Duration,
///     max_retries: u32,
/// }
/// 
/// // EL CONTEXTO ES MUTABLE (todo el estado va aquí)
/// impl Workflow for OrderWorkflow {
///     async fn run(&self, ctx: &mut WorkflowContext, input: OrderInput) -> Result<...> {
///         // self.default_timeout ← lectura OK
///         // ctx.checkpoint(...) ← mutación OK (es del contexto)
///     }
/// }
/// ```
```

---

### B.5 Criterios de Éxito Técnicos Actualizados

| Categoría | Criterio | Métrica | Cómo Medir |
|-----------|----------|---------|------------|
| **Correctitud** | Replay determinista | 100% | Tests de replay (US-94.24) |
| **Correctitud** | Transacciones atómicas | 0 inconsistencias | Tests de fallo forzado |
| **Correctitud** | Compensaciones funcionales | Equivalente a step-list | Tests de migración |
| **Performance** | Replay con checkpoint | **< 20ms para 50k eventos** | Benchmarks |
| **Performance** | Replay sin checkpoint | < 50ms para 100 eventos | Benchmarks |
| **Performance** | Throughput | > 1000 activities/min | Load tests |
| **Usabilidad** | API consistente | 1 forma de hacer cada cosa | Code review |
| **Usabilidad** | Setup básico | **≤ 3 líneas** (sin outbox config) | Docs + examples |
| **Usabilidad** | "Client solo define workflows" | 0 config de infra obligatoria | User testing |
| **Mantenibilidad** | Test coverage | > 80% en core | `cargo tarpaulin` |

---

### B.6 Principios de Diseño Finales

> [!NOTE]
> Estos principios guían TODAS las decisiones de implementación.

1. **"El cliente SOLO define workflows"**
   - Outbox, relays, buses son transparentes o opcionales
   - Setup mínimo: 3 líneas para development, 5 para producción

2. **"Inmutabilidad del Workflow"**
   - `&self` en `run()`, no `&mut self`
   - Todo estado mutable en `WorkflowContext`
   - Macro `#[workflow]` fuerza esto

3. **"Replay Inteligente"**
   - `checkpoint()` explícito para workflows largos
   - Snapshots automáticos cada N eventos
   - < 20ms para reanudar workflows de 50k eventos

4. **"Errores Clasificados"**
   - Business vs Transient vs Permanent
   - Retry automático solo para Transient
   - Mensajes claros para debugging

5. **"Testing First"**
   - `WorkflowTestEnvironment` con time skipping
   - Mocking de activities trivial
   - Determinism enforcer en runtime

---

**Creado**: 2026-01-20 | **Última actualización**: 2026-01-20 | **Owner**: Platform Team



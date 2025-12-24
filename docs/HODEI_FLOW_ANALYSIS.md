# Hodei Job Platform - Análisis Completo de Flujo y Problemas

## 1. FLUJO DE ENCOLADO DE JOBS

### 1.1 Punto de Entrada: CLI

**Archivo**: `crates/cli/src/main.rs`

Cuando usuario ejecuta:
```bash
hodei-jobs-cli job run --name "test" --command '["echo", "hi"]'
```

**Flujo en CLI**:
1. `handle_job()` → `JobAction::Run` → línea 156
2. Crea `QueueJobRequest` con `JobDefinition`
3. Llama a `JobExecutionServiceClient::queue_job()` vía gRPC
4. Si éxito, llama a `LogStreamServiceClient::subscribe_logs()`
5. Mantiene stream abierto para recibir logs en tiempo real

### 1.2 Servicio gRPC: JobExecutionService

**Archivo**: `crates/server/interface/src/grpc/job_execution.rs`

**Método**: `queue_job()` → línea 169

**Procesamiento**:
1. Extrae metadata (correlation_id, actor)
2. Parsea `JobDefinition` a `CreateJobRequest`
3. Mapea resource requirements (CPU, memory, disk)
4. **Llama a `CreateJobUseCase::execute()`** → línea 259

**Emite logs**: `=== QUEUE_JOB HANDLER CALLED ===`

### 1.3 Caso de Uso: CreateJobUseCase

**Archivo**: `crates/server/application/src/jobs/create.rs`

**Método**: `execute()` → línea 118

**Pasos**:
1. **Convierte request a JobSpec** (línea 120-151)
2. **Genera JobId** (usa UUID del request o genera nuevo) → línea 153-161
3. **Crea Job aggregate** → línea 164
4. **Almacena metadata** (correlation_id, actor) → línea 167-173
5. **Valida Job** → línea 176-190
6. **GUARDA EN REPOSITORY** (incluye enqueue atómico) → línea 192-195
   - `PostgresJobRepository::save()` automáticamente encola si estado es PENDING
7. **Publica evento `JobCreated`** → línea 201-210
8. **Publica evento `JobQueueDepthChanged`** → línea 213-228

**Eventos Publicados**:
- `JobCreated { job_id, spec, occurred_at, correlation_id, actor }`
- `JobQueueDepthChanged { queue_depth, threshold, ... }`

### 1.4 Event Bus

**Implementación**: `PostgresEventBus` con LISTEN/NOTIFY

**Función**: Publica eventos a PostgreSQL, listeners los reciben vía NOTIFY

### 1.5 JobController - Suscriptor de Eventos

**Archivo**: `crates/server/application/src/jobs/controller.rs`

**Método**: `subscribe_to_events()` → línea 32

**Eventos Escuchados**:
- `JobCreated` → línea 48 → `run_once()`
- `WorkerStatusChanged` → línea 54 → `run_once()`
- `WorkerRegistered` → línea 67 → `run_once()`
- `WorkerProvisioned` → línea 73 → `run_once()`

### 1.6 JobController - Procesamiento

**Método**: `run_once()` → línea 115

**Pasos**:
1. **Consulta workers disponibles** → línea 118
   ```rust
   let all_workers = self.worker_registry.find_available().await.map_err(|e| {
   ```
   
2. **Log de debug**: "Found X ready workers"

3. **Si no hay workers** → retorna 0 (línea 132-134)

4. **Desencola job de queue** → línea 137
   ```rust
   let Some(mut job) = self.job_queue.dequeue().await? else {
   ```

5. **Crea SchedulingContext** → línea 141-148
   ```rust
   let ctx = SchedulingContext {
       job: job.clone(),
       available_workers: all_workers,
       available_providers: Vec::new(), // ⚠️ NO PROVISIONING
       pending_jobs_count: 0,
       system_load: 0.0,
   };
   ```
   
   **PROBLEMA DETECTADO**: `available_providers: Vec::empty()` - Comentario dice "No provisioning in event-driven mode"

6. **Usa scheduler para decisión** → línea 150
   ```rust
   let worker_id = match self.scheduler.make_decision(ctx).await? {
   ```

### 1.7 SmartScheduler - Decisión

**Archivo**: `crates/server/application/src/scheduling/smart_scheduler.rs`

**Método**: `schedule()` → línea 158

**Configuración Default**:
```rust
prefer_existing_workers: true,
max_queue_depth: 100,
scale_up_load_threshold: 0.8,
```

**Lógica de Decisión** (línea 181-207):
1. **Check queue depth** → si > max_queue_depth → Reject
2. **Try existing workers** (si prefer_existing_workers) → línea 184-186
   ```rust
   if let Some(worker_id) = self.select_worker(&context.job, &context.available_workers) {
       return Ok(SchedulingDecision::AssignToWorker { job_id, worker_id });
   }
   ```
3. **Try provision new worker** → línea 189-195
   ```rust
   if let Some(provider_id) = self.select_provider(&context.job, &context.available_providers) {
       return Ok(SchedulingDecision::ProvisionWorker { job_id, provider_id });
   }
   ```
4. **Check system load** → línea 198-202
5. **Last resort: enqueue** → línea 205-209

### 1.8 Worker Selection

**Método**: `select_worker()` → línea 66

**Logs de Debug** (línea 71-78):
```rust
for (i, w) in workers.iter().enumerate() {
    let can_accept = w.state().can_accept_jobs();
    tracing::debug!(
        "Worker[{}] id={:?} state={:?} can_accept_jobs={}",
        i, w.id(), w.state(), can_accept
    );
}
```

**Estrategias Disponibles**:
- `FirstAvailable`: Retorna primer worker que puede aceptar jobs
- `LeastLoaded`: Workers con menos jobs (actualmente igual a FirstAvailable)
- `RoundRobin`: Rotación entre workers disponibles
- `MostCapacity`: Igual a LeastLoaded (sin implementar)
- `Affinity`: Busca workers con labels que coincidan

### 1.9 Asignación y Dispatch

**Si scheduler decide AssignToWorker** → línea 153

**Método**: `assign_and_dispatch()` → línea 172

**Pasos**:
1. **Obtiene worker del registry** → línea 180
2. **Crea ExecutionContext** → línea 188-193
3. **Marca job como Running** → línea 197
4. **Publica evento `JobAssigned`** → línea 201-210
5. **Publica evento `JobStatusChanged` (Running)** → línea 213-224
6. **Asigna worker a job en registry** → línea 227
7. **Actualiza job en repository** → línea 230
8. **ENVÍA COMANDO AL WORKER** → línea 236-242
   ```rust
   self.worker_command_sender.send_run_job(worker_id, job).await
   ```

## 2. FLUJO DE STREAMING DE LOGS

### 2.1 Suscripción desde CLI

**CLI**: `handle_job()` → línea 194-210

**Proceso**:
1. Crea `SubscribeLogsRequest` con `job_id`
2. Llama a `LogStreamServiceClient::subscribe_logs()`
3. Recibe stream de `JobLogEntry`
4. Muestra logs en tiempo real con timestamps

### 2.2 LogStreamService gRPC

**Archivo**: `crates/server/interface/src/grpc/log_stream.rs`

**Método**: `subscribe_logs()` → línea 194

**Proceso**:
1. **Envía logs históricos** (si include_history) → línea 203-217
2. **Suscribe a nuevos logs** → línea 220-242
   - Crea stream via `self.inner.subscribe(&job_id)`
   - Spawn task que convierte `LogEntry` a `JobLogEntry` con sequence
   - Envía a cliente vía channel

### 2.3 LogStreamService (Core)

**Archivo**: `crates/server/interface/src/grpc/log_stream.rs`

**Estructura**:
```rust
LogStreamService {
    logs: HashMap<String, Vec<BufferedLogEntry>>,        // Buffers por job
    subscribers: HashMap<String, Vec<mpsc::Sender>>,     // Suscriptores por job
    sequences: HashMap<String, u64>,                     // Contadores por job
}
```

**Métodos Clave**:
- `append_log()` → línea 59: Agrega log al buffer y notifica subscribers
- `subscribe()` → línea 106: Retorna stream de logs (buffer + nuevos)
- `get_logs()` → línea 137: Obtiene logs históricos

### 2.4 Logs desde Worker

**Worker → Server**: Via `worker_stream` gRPC bidirectional

**WorkerAgentServiceImpl**: `worker_stream()` → línea 892

**Cuando Worker envía `Log` o `LogBatch`** (línea 924-942):
```rust
WorkerPayload::Log(log) => {
    // Forward to LogStreamService for client subscribers
    if let Some(ref svc) = log_service {
        let entry = LogEntry {
            job_id: log.job_id,
            line: log.line,
            is_stderr: log.is_stderr,
            timestamp: log.timestamp,
        };
        svc.append_log(entry).await;
    }
}
```

## 3. MAPEO COMPLETO DE EVENTOS

### 3.1 Eventos Publicados

| Evento | Quién lo Publica | Cuándo |
|--------|------------------|--------|
| `JobCreated` | `CreateJobUseCase::execute()` | Job guardado en repository |
| `JobQueueDepthChanged` | `CreateJobUseCase::execute()` | Después de encolar job |
| `JobAssigned` | `JobController::assign_and_dispatch()` | Worker asignado a job |
| `JobStatusChanged` | `JobController::assign_and_dispatch()` | Job marcado como Running |
| `JobRetried` | `RetryJobUseCase::execute()` | Job reintentado |
| `WorkerRegistered` | `WorkerAgentServiceImpl::register()` | Worker registrado |
| `WorkerStatusChanged` | `WorkerAgentServiceImpl::on_worker_registered()` | Worker cambia estado |
| `WorkerProvisioned` | `ProviderManager` | Worker aprovisionado |
| `WorkerReconnected` | `WorkerAgentServiceImpl::register()` | Worker reconectado |
| `WorkerDisconnected` | `WorkerAgentServiceImpl::worker_stream()` | Worker desconectado |
| `WorkerTerminated` | `WorkerAgentServiceImpl::unregister_worker()` | Worker terminado |

### 3.2 Suscriptores de Eventos

| Suscriptor | Eventos Escuchados | Acción |
|------------|-------------------|--------|
| `JobController` | `JobCreated`, `WorkerStatusChanged`, `WorkerRegistered`, `WorkerProvisioned` | Llama `run_once()` |
| `ProviderManager` | `JobQueueDepthChanged`, `WorkerTerminated` | Auto-scaling, health monitoring |
| `JobController` (otra instancia) | Todos via event bus | Reacciona a cambios |

## 4. ANÁLISIS DE INCONSISTENCIAS

### 4.1 PROBLEMA RAÍZ IDENTIFICADO

**Error**: "1 available workers and 0 providers"

**Causa**:
En `JobController::run_once()` línea 144:
```rust
available_providers: Vec::new(), // No provisioning in event-driven mode
```

**Explicación**:
1. JobController recibe `JobCreated` event
2. Llama a `run_once()`
3. Consulta workers disponibles → encuentra 1 worker en estado READY
4. Crea SchedulingContext con `available_providers: Vec::empty()`
5. Scheduler recibe contexto con 1 worker, 0 providers
6. Como `prefer_existing_workers=true`, intenta `select_worker()`
7. **PROBLEMA**: Aunque hay 1 worker, scheduler lo rechaza

### 4.2 ¿Por qué Scheduler Rechaza el Worker?

**Hipótesis 1**: Worker no está en estado `can_accept_jobs()`
- **Verificación**: Logs muestran "Worker[0] id=... state=Creating can_accept_jobs=false"
- **Estado en DB**: Workers están en estado READY
- **Problema**: `map_row_to_worker()` no restaura estado correctamente

**Ya arreglado**: Se implementó `Worker::from_database()` y se arregló `map_row_to_worker()`

**Hipótesis 2**: Worker se registra pero luego se desconecta inmediatamente
- **Verificación**: Worker se registra via gRPC, se agrega a registry
- **Problema**: No mantiene conexión activa
- **Síntoma**: Worker en DB como READY pero sin canal gRPC activo

**Solución intentada**: Sistema de tracking de workers conectados
- **Revertido**: Interfería con aprovisionamiento

### 4.3 Inconsistencia Arquitectural

**Conflicto**: Event-Driven vs Request-Response

**Problema**:
- JobController opera en modo "event-driven" (sin provisioning)
- Pero aún necesita workers disponibles
- No consulta providers porque comentario dice "No provisioning"
- Pero sin providers, no puede asignar jobs correctamente

**Evidencia**:
```rust
// crates/server/application/src/jobs/controller.rs línea 144
available_providers: Vec::new(), // No provisioning in event-driven mode
```

### 4.4 Problemas Adicionales Detectados

1. **ExecuteNextJobUseCase**: No se usa, código comentado/deprecado
2. **ProviderRegistry**: No se consulta en JobController
3. **Auto-scaling**: ProviderManager escucha eventos pero JobController no usa providers
4. **Worker state**: Inconsistencia entre estado en DB y estado en memoria

## 5. HIPÓTESIS Y SOSPECHOSOS

### 5.1 Principal Sospechoso: SchedulingContext con Providers Vacío

**Archivo**: `crates/server/application/src/jobs/controller.rs` línea 141-148

**Problema**:
```rust
let ctx = SchedulingContext {
    job: job.clone(),
    available_workers: all_workers,
    available_providers: Vec::new(), // ⚠️ AQUÍ ESTÁ EL PROBLEMA
    pending_jobs_count: 0,
    system_load: 0.0,
};
```

**Impacto**:
- Scheduler no puede provisionar nuevos workers
- Depende 100% de workers existentes
- Si workers existentes no pueden aceptar jobs → failure

### 5.2 Segundo Sospechoso: Estado de Workers

**Problema**: Workers en DB como READY pero no procesan jobs

**Causas Posibles**:
1. Workers se registran y luego se desconectan inmediatamente
2. Estado en DB no coincide con estado real (bug en restore)
3. Workers están en estado READY pero no pueden aceptar jobs (bug en `can_accept_jobs()`)

### 5.3 Tercer Sospechoso: Filter en select_worker()

**Archivo**: `crates/server/application/src/scheduling/smart_scheduler.rs` línea 90-96

**Problema**:
```rust
let available: Vec<_> = workers
    .iter()
    .filter(|w| {
        let can = w.state().can_accept_jobs();
        tracing::debug!(
            "RoundRobin filter: worker {:?} can_accept={}",
            w.id(),
            can
        );
        can
    })
    .collect();
```

**Si `can_accept_jobs()` retorna false para todos los workers**:
- `available` vector queda vacío
- select_worker() retorna None
- Scheduler no puede asignar job

### 5.4 Cuarto Sospechoso: Race Condition

**Problema**:
1. Worker se registra en DB (estado READY)
2. JobController consulta workers (los encuentra)
3. JobController crea SchedulingContext (antes de que worker abra gRPC stream)
4. Worker abre gRPC stream DESPUÉS de que JobController ya procesó el job
5. Job queda encolado sin ejecutar

## 6. FLUJO ACTUAL (PROBLEMÁTICO)

```
CLI: job run
  ↓
gRPC: queue_job()
  ↓
CreateJobUseCase: execute()
  ↓
Repository: save() + enqueue
  ↓
EventBus: publish JobCreated
  ↓
JobController: subscribe_to_events() → run_once()
  ↓
WorkerRegistry: find_available()
  ↓ (encuentra 1 worker en READY)
SchedulingContext { workers: [1], providers: [] }
  ↓
SmartScheduler: schedule()
  ↓
select_worker() → encuentra worker
  ↓ ⚠️ PERO algo lo rechaza
SchedulerDecision: ?????
  ↓
❌ Job no se asigna, queda encolado
```

## 7. FLUJO ESPERADO (CORRECTO)

```
CLI: job run
  ↓
gRPC: queue_job()
  ↓
CreateJobUseCase: execute()
  ↓
Repository: save() + enqueue
  ↓
EventBus: publish JobCreated
  ↓
JobController: subscribe_to_events() → run_once()
  ↓
WorkerRegistry: find_available()
  ↓ (encuentra 1 worker en READY y CONECTADO)
SchedulingContext { workers: [1], providers: [...] }
  ↓
SmartScheduler: schedule()
  ↓
select_worker() → retorna worker_id
  ↓
SchedulingDecision: AssignToWorker { job_id, worker_id }
  ↓
JobController: assign_and_dispatch()
  ↓
WorkerCommandSender: send_run_job()
  ↓
Worker: recibe job via gRPC stream
  ↓
Worker: ejecuta job, envía logs
  ↓
LogStreamService: append_log()
  ↓
CLI: recibe logs en stream
```

## 8. PRUEBAS REQUERIDAS

### 8.1 Verificar Estado de Workers

```bash
# Query DB directly
SELECT worker_id, state, current_job_id, last_heartbeat 
FROM workers 
WHERE state = 'READY';
```

### 8.2 Verificar Event Flow

Agregar logs detallados en:
- `JobController::run_once()`: log de todos los workers encontrados
- `select_worker()`: log de cada worker evaluado
- `SchedulingContext` creation: log de counts

### 8.3 Verificar gRPC Connection

```bash
# Verificar que worker mantiene conexión
# Logs del servidor deben mostrar worker conectado
# Worker stream debe estar activo
```

## 9. SOLUCIONES PROPUESTAS

### 9.1 Solución Inmediata: Consultar Providers

Modificar `JobController::run_once()`:

```rust
// En lugar de:
available_providers: Vec::new(),

// Usar:
available_providers: self.provider_registry.list_available().await?,
```

**Beneficio**: Scheduler puede decidir entre usar worker existente o provisionar nuevo

### 9.2 Solución Media: Verificar Workers Conectados

No con tracking global, sino verificando canal gRPC:

```rust
let available_workers: Vec<_> = all_workers
    .into_iter()
    .filter(|w| {
        // Verificar si worker tiene canal gRPC activo
        self.worker_channels.contains_key(w.id())
    })
    .collect();
```

### 9.3 Solución Larga: Refactor Arquitectural

Separar modos:
- **Event-Driven Mode**: Solo reacciona a eventos, no hace scheduling
- **Controller Mode**: Hace scheduling real con providers y workers

## 10. CONCLUSIONES

**Problema Principal**: `JobController` crea `SchedulingContext` sin providers, confiando solo en workers existentes, pero algo impide que los workers sean seleccionados correctamente.

**Impacto**: Jobs se encolan exitosamente pero nunca se ejecutan, quedando en estado PENDING indefinidamente.

**Prioridad**: Alta - Sistema no funcional para ejecución de jobs.

**Tiempo Estimado de Fix**: 2-4 horas (consultar providers + verificar worker states)

---

He recibido el análisis del flujo y el código fuente completo de la plataforma **Hodei Job Platform**. Como **Lead Rust Code Auditor**, he procedido a realizar una auditoría profunda centrada en la deuda técnica, fallos de diseño en la capa de dominio y la causa raíz de los errores de programación reportados.

He generado el reporte de auditoría detallado. A continuación, presento el diagnóstico y la matriz de mejoras.

---

### 1. ESCANEO DE ESTRUCTURA Y DEPENDENCIAS

*   **Arquitectura:** El proyecto sigue una estructura de micro-crates (CLI, Server, Worker, Shared). Dentro del servidor, se observa un intento de **Hexagonal Architecture/DDD**, dividiendo en `domain`, `application`, `infrastructure` e `interface`.
*   **Violación de Límites:** Se detecta una fuga de responsabilidades en `hodei-server-infrastructure`. El `PostgresJobRepository` está asumiendo lógica de orquestación (encolado atómico), lo que crea una dependencia oculta: si se cambia el motor de persistencia, se rompe el sistema de colas.
*   **Acoplamiento gRPC-Dominio:** La capa de `interface` (gRPC) realiza mapeos manuales extensos de estados (`i32` a `Enum`), lo que introduce una alta fragilidad ante cambios en los archivos `.proto`.

---

### 2. ARTIFACT: REPORTE DE CALIDAD Y DEUDA

He generado el archivo en: `docs/audit/001_core_scheduling_audit.md`

#### Contenido del Reporte (Resumen Crítico):

**A. Análisis de Conascencia (Coupling):**
1.  **Connascence of Meaning (Causante del Bug de Selección):** En `scheduler.rs` y `smart_scheduler.rs`, se usan valores mágicos y comparaciones manuales de estados. El problema raíz reportado ("1 available worker but rejected") ocurre porque el `JobController` instancia un `SchedulingContext` con `available_providers: Vec::new()`. El `SmartScheduler`, al ver la lista de providers vacía, entra en un estado de decisión degradado donde cualquier fallo menor en los labels del worker descarta la ejecución.
2.  **Connascence of Algorithm:** La lógica de "qué es un worker listo" está duplicada en el `WorkerRegistry` (SQL) y en el `SmartScheduler` (Rust). Si el SQL considera `READY` algo que el `can_accept_jobs()` del dominio no, el Job se queda en un limbo de "dequeue -> reject -> enqueue".

**B. Violaciones DDD & SOLID:**
1.  **Anemic Domain Model:** El agregado `Job` en `domain/src/jobs/aggregate.rs` es principalmente una estructura de datos. La lógica de decisión de "cuándo un job debe ser reintentado" o "cómo se calcula su prioridad" está dispersa en Use Cases de la capa de aplicación.
2.  **God Object (JobController):** El `JobController` en `application` está intentando ser un orquestador de eventos, un scheduler y un despachador de comandos al mismo tiempo.

**C. Rust Idioms & Safety:**
1.  **Race Conditions en Estados:** En `JobController::assign_and_dispatch`, se actualiza el estado en la DB *después* de publicar eventos pero *antes* de enviar el comando gRPC. Si el `worker_command_sender` falla, el worker queda bloqueado en el `Registry` como `Busy` pero sin job asignado.
2.  **Error Handling:** Uso excesivo de `String` para errores en la capa de infraestructura, lo que impide que la capa de aplicación tome decisiones programáticas basadas en el tipo de error (ej. reintentar si es un error de conexión vs. fallar si es un error de configuración).

---

### 3. MATRIZ DE MEJORAS PROPUESTAS

| Prioridad | Refactorización | Tipo | Descripción |
| :--- | :--- | :--- | :--- |
| **CRÍTICA** | **Registry-Provider Integration** | Logic | Modificar `JobController::run_once` para que consulte el `ProviderRegistry` en lugar de pasar un vector vacío. Sin esto, el auto-scaling es imposible. |
| **CRÍTICA** | **Worker State Reconciliation** | Safety | Implementar un `Heartbeat` que no solo actualice un timestamp, sino que verifique la existencia del stream gRPC activo antes de reportar un worker como `Available`. |
| **IMPORTANTE** | **Strongly Typed Mapping** | Type-Safety | Eliminar los `match status { 2 => ... }` en `scheduler.rs`. Usar `TryFrom<i32>` para los Enums de Proto. |
| **IMPORTANTE** | **Domain Service Migration** | DDD | Mover `SmartScheduler` de `application` a `domain`. La selección de recursos es lógica de negocio pura, no de aplicación. |
| **OPCIONAL** | **Context Propagation** | Debt | Refactorizar el `RequestContext` para usar `tracing::Span` en lugar de pasarlo manualmente en cada `execute_with_context`. |

---

### 4. SOLICITUD DE ACCIÓN

He identificado que el problema de que los jobs no se ejecuten se debe a que el **`JobController` está operando en un modo "ciego"** (sin providers) y el **`SmartScheduler` tiene un filtro de afinidad demasiado estricto** que rechaza workers si no tienen labels específicos que el CLI no está enviando por defecto.

He generado el reporte de auditoría completo en `docs/audit/001_core_scheduling_audit.md`.

**¿Por cuál de las refactorizaciones priorizadas ('Críticas') quieres que empecemos a trabajar para limpiar la deuda técnica y solucionar el bloqueo de los Jobs?**

1.  **Solución al `JobController`** (Inyectar providers y arreglar contexto).
2.  **Sincronización de Estado Worker** (Asegurar que `READY` en DB signifique "Conexión gRPC Activa").
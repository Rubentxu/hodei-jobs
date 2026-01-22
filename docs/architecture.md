# Arquitectura del Sistema

**Versión**: 8.0
**Última Actualización**: 2025-12-18

## Domain-Driven Design (DDD)

El proyecto sigue una arquitectura DDD hexagonal con las siguientes capas:

```mermaid
graph TB
    subgraph "Presentation Layer Adapters"
        CLI[CLI]
        REST[REST API Axum]
        GRPC[gRPC Services]
        WORKER[Worker Agent]
    end

    subgraph "Application Layer"
        UC1[Job Execution Use Cases]
        UC2[Provider Registry]
        UC3[Worker Lifecycle Manager]
        UC4[Smart Scheduler]
        UC5[Job Controller]
        UC6[Worker Provisioning]
    end

    subgraph "Domain Layer"
        subgraph "Job Execution Context"
            JOB[Job Aggregate]
            SPEC[JobSpec + CommandType]
            QUEUE[JobQueue]
            TEMPLATE[JobTemplate]
        end

        subgraph "Worker Management Context"
            WORKER_DOM[Worker Aggregate]
            WPROV[WorkerProvider Trait]
            WREG[WorkerRegistry]
        end

        subgraph "Provider Context"
            PROV[ProviderConfig]
            CAP[ProviderCapabilities]
        end

        subgraph "Shared Kernel"
            ID[WorkerId, JobId, ProviderId]
            STATE[WorkerState, JobState]
            ERR[DomainError, ProviderError]
            OTP[OtpTokenStore]
        end
    end

    subgraph "Infrastructure Layer"
        DOCKER[DockerProvider]
        K8S[KubernetesProvider]
        FC[FirecrackerProvider]
        PG[(Postgres)]
        REPO[Repositories sqlx]
        LOGS[LogStreamService]
    end

    CLI --> UC1
    REST --> UC1
    REST --> UC2
    GRPC --> UC2
    WORKER --> GRPC

    UC1 --> JOB
    UC2 --> PROV
    UC3 --> WORKER_DOM
    UC4 --> WREG
    UC5 --> QUEUE
    UC6 --> WPROV

    JOB --> ID
    WORKER_DOM --> STATE
    WPROV --> DOCKER
    WPROV --> K8S
    WPROV --> FC

    WREG --> REPO
    UC1 --> LOGS
    REPO --> PG
```

## Worker Agent - Optimizaciones v8.0

### LogBatching & Backpressure (T1.1-T1.5)

El worker agent implementa un sistema de batching de logs para reducir significativamente la sobrecarga de red:

```mermaid
graph TD
    A[LogEntry generada] --> B{Buffer lleno?}
    B -->|No| C[Agregar a buffer]
    B -->|Sí| D[Flush LogBatch]
    D --> E[Enviar LogBatch vía gRPC]
    E --> F[Reset buffer]
    C --> G[Timer flush interval]
    G --> H{Timeout?}
    H -->|Sí| D
    H -->|No| I[Aguardar]
    I --> G

    J[ServerMessage] --> K[try_send()?]
    K -->|Full| L[Drop message<br/>Backpressure]
    K -->|OK| M[Continuar]
```

**Beneficios:**
- **90-99% reducción** en llamadas gRPC (de línea por línea a batches)
- **Backpressure handling** con `try_send()` para evitar bloqueos
- **Flush automático** por capacidad o intervalo de tiempo
- **Thread-safe** usando `Arc<Mutex<LogBatcher>>`

### Write-Execute Pattern (T1.6-T1.7)

Patrón robusto de ejecución de scripts inspirado en Jenkins/K8s:

```mermaid
flowchart TD
    A[Recibir RunJobCommand] --> B[Crear archivo temporal]
    B --> C[Escribir script + safety headers]
    C --> D[Hacer archivo ejecutable]
    D --> E[Ejecutar con Command]
    E --> F[Stream logs en tiempo real]
    F --> G[Enviar JobResult]
    G --> H[Limpiar archivo temporal<br/>async con tokio::spawn]

    subgraph "Safety Headers"
        I[set -e<br/>exit on error]
        J[set -u<br/>undefined variables error]
        K[set -o pipefail<br/>pipe failure detection]
    end

    C --> I
    C --> J
    C --> K
```

**Características:**
- Inyección automática de safety headers (`set -euo pipefail`)
- Gestión segura de archivos temporales
- Cleanup asíncrono no bloqueante
- Ejecución robusta con manejo de errores

### Secret Injection (T2.1-T2.3)

Inyección segura de secretos via stdin:

```mermaid
sequenceDiagram
    participant W as Worker
    participant E as JobExecutor

    W->>E: execute_script(script, env, secrets)
    E->>E: Serializar secrets a JSON
    E->>E: stdin.write_all(&json)
    E->>E: stdin.shutdown() [Write]
    E->>Command: spawn()
    Command->>Command: Leer secrets desde stdin
    Command->>Command: Ejecutar script con env
    Note over E,Command: Stdin cerrado después<br/>de inyección - seguridad
```

**Seguridad:**
- Secrets nunca aparecen en logs (redacción automática)
- Transmisión via stdin con cierre inmediato
- JSON serializado para múltiples secretos
- Auditoría de acceso a secretos

### Zero-Copy I/O (T3.1-T3.2)

Optimización de lectura de logs:

```rust
// FramedRead + BytesCodec para zero-copy
let mut framed = FramedRead::new(source, BytesCodec::new());
while let Some(chunk) = framed.next().await {
    // Direct Bytes slice - no copy
    let bytes = chunk?;
    // Process directly from Bytes
}
```

**Beneficios:**
- **Zero-copy** de datos de log
- **BytesCodec** para decodificación eficiente
- **FramedRead** para manejo de límites
- Reducción de allocaciones de memoria

### Métricas Asíncronas (T3.3-T3.5)

Sistema de métricas con cache TTL:

```mermaid
graph LR
    A[Métricas Request] --> B{Cache válido?}
    B -->|Sí| C[Return cached]
    B -->|No| D[Spawn blocking task]
    D --> E[Recolectar desde cgroups]
    E --> F[Update cache + timestamp]
    F --> G[Return fresh metrics]
    G --> H[Background update]

    subgraph "Cache TTL"
        I[35 segundos TTL]
        J[Instant timestamp]
        K[Atomic update]
    end

    F --> I
```

**Características:**
- **Cache TTL** de 35 segundos
- **spawn_blocking** para tareas intensivas
- **yield_now** para permitir preemption
- Integración cgroups para containers

### mTLS Infrastructure (T4.1-T4.5)

Infraestructura completa de mTLS para Zero Trust:

```mermaid
graph TD
    A[CertificatePaths] --> B[Load certs]
    B --> C[ServerTlsSettings]
    C --> D[CertificateExpiration checker]

    subgraph "PKI Infrastructure"
        E[CA Root Certificate]
        F[Server Certificate]
        G[Client Certificate]
        H[Certificate Revocation]
    end

    subgraph "Certificate Management"
        I[Auto-rotation]
        J[Validity tracking]
        K[Expiration alerts]
    end

    E --> F
    E --> G
    D --> I
    D --> J
    D --> K

    script[scripts/generate-certificates.sh]
    doc1[docs/security/PKI-DESIGN.md]
    doc2[docs/security/CERTIFICATE-MANAGEMENT.md]

    script --> E
    doc1 --> E
    doc2 --> E
```

**Nota:** Requiere upgrade a `tonic >= 0.15` para habilitar TLS features.

## Comunicación Server ↔ Worker Agent (PRD v8.0)

```mermaid
sequenceDiagram
    participant P as Provider (Docker)
    participant S as Server
    participant W as Worker Agent

    Note over P,W: 1. Provisioning
    S->>P: create_worker(WorkerSpec)
    P->>P: Start container with OTP token
    P-->>S: WorkerHandle + OTP

    Note over S,W: 2. Registration (OTP Auth)
    W->>S: Register(auth_token=OTP, worker_info)
    S->>S: validate_otp(token)
    S-->>W: RegisterResponse(session_id)

    Note over S,W: 3. Bidirectional Stream con Optimizaciones
    W->>S: WorkerStream (bidirectional)

    loop Heartbeat + Commands
        W->>S: WorkerHeartbeat (cached metrics)
        S-->>W: ACK / KeepAlive
        S->>W: RunJobCommand
        W->>W: Write-Execute Pattern
        W->>S: LogBatch (batched logs)
        W->>S: JobResultMessage (with secrets audit)
    end

    Note over S,W: 4. Shutdown
    W->>S: Unregister(reason)
    S->>P: destroy_worker(handle)
```

## Bounded Contexts

### 1. Job Execution Context

Responsable del ciclo de vida completo de un job.

```mermaid
classDiagram
    class Job {
        +JobId id
        +JobSpec spec
        +JobState state
        +ProviderId selected_provider
        +ExecutionContext execution_context
        +u32 attempts
        +u32 max_attempts
        +queue() Result
        +submit_to_provider() Result
        +mark_running() Result
        +complete() Result
        +fail() Result
        +cancel() Result
        +can_retry() bool
    }

    class JobSpec {
        +CommandType command
        +Option~String~ image
        +HashMap env
        +JobResources resources
        +u64 timeout_ms
        +Vec~Constraint~ constraints
        +Vec~ArtifactSource~ inputs
        +Vec~ArtifactDest~ outputs
    }

    class CommandType {
        <<enumeration>>
        Shell: cmd + args
        Script: interpreter + content
    }

    class JobResources {
        +f32 cpu_cores
        +u64 memory_mb
        +u64 storage_mb
        +bool gpu_required
        +String architecture
    }

    class JobPreferences {
        +Option~String~ preferred_provider
        +Option~String~ preferred_region
        +JobPriority priority
        +bool allow_retry
    }

    class ExecutionContext {
        +JobId job_id
        +ProviderId provider_id
        +String provider_execution_id
        +ExecutionStatus status
        +update_status()
    }

    Job --> JobSpec
    Job --> ExecutionContext
    JobSpec --> JobResources
    JobSpec --> JobPreferences
```

### 2. Provider Management Context

Gestiona los workers/providers que ejecutan jobs.

```mermaid
classDiagram
    class Provider {
        +ProviderId id
        +String name
        +ProviderType provider_type
        +ProviderStatus status
        +ProviderCapabilities capabilities
        +ProviderConfig config
        +u32 current_jobs
        +is_healthy() bool
        +is_available() bool
        +mark_healthy()
        +mark_unhealthy()
        +increment_job_count()
        +decrement_job_count()
    }

    class ProviderType {
        <<enumeration>>
        Lambda
        Kubernetes
        Docker
        AzureVm
        Ec2
        CloudRun
        BareMetal
        Custom
    }

    class ProviderCapabilities {
        +Option~u32~ max_cpu_cores
        +Option~u64~ max_memory_gb
        +bool supports_gpu
        +Vec~String~ supported_runtimes
        +Vec~String~ supported_architectures
        +Option~u32~ max_concurrent_jobs
    }

    class ProviderConfig {
        <<enumeration>>
        Lambda
        Kubernetes
        Docker
        AzureVm
        Ec2
        CloudRun
        BareMetal
    }

    Provider --> ProviderType
    Provider --> ProviderCapabilities
    Provider --> ProviderConfig
```

### 3. Shared Kernel

Tipos y conceptos compartidos entre contextos.

```mermaid
classDiagram
    class JobId {
        +Uuid value
        +new() JobId
    }

    class ProviderId {
        +Uuid value
        +new() ProviderId
    }

    class CorrelationId {
        +Uuid value
        +new() CorrelationId
    }

    class JobState {
        <<enumeration>>
        Pending
        Scheduled
        Running
        Succeeded
        Failed
        Cancelled
        Timeout
    }

    class WorkerState {
        <<enumeration>>
        Creating
        Connecting
        Ready
        Busy
        Draining
        Terminating
        Terminated
    }

    class ProviderStatus {
        <<enumeration>>
        Active
        Maintenance
        Disabled
        Overloaded
        Unhealthy
    }

    class DomainError {
        <<enumeration>>
        JobNotFound
        ProviderNotFound
        InvalidStateTransition
        ProviderUnhealthy
        MaxAttemptsExceeded
    }
```

## Estructura de Crates
No actualizado a la estructura actual. Tenerlo como referencia pero no es real.
```
hodei-job-platform/
├── crates/
│   ├── domain/           # hodei-jobs-domain - Lógica de negocio pura
│   │   ├── shared_kernel.rs      # JobId, WorkerId, ProviderId, States, Errors
│   │   ├── job_execution.rs      # Job aggregate, JobSpec, JobQueue trait
│   │   ├── job_template.rs       # JobTemplate aggregate
│   │   ├── worker.rs             # Worker aggregate, WorkerSpec, WorkerHandle
│   │   ├── worker_provider.rs    # WorkerProvider trait, ProviderCapabilities
│   │   ├── worker_registry.rs    # WorkerRegistry trait
│   │   ├── job_scheduler.rs      # Scheduling strategies
│   │   ├── provider_config.rs    # ProviderConfig
│   │   └── otp_token_store.rs    # OTP authentication
│   │
│   ├── application/      # hodei-jobs-application - Use Cases
│   │   ├── job_execution_usecases.rs  # CreateJob, CancelJob
│   │   ├── job_controller.rs          # Control loop
│   │   ├── smart_scheduler.rs         # Scheduling service
│   │   ├── worker_provisioning.rs     # Worker provisioning trait
│   │   ├── worker_provisioning_impl.rs # Default implementation
│   │   ├── worker_lifecycle.rs        # Worker lifecycle management
│   │   └── provider_registry.rs       # Provider management
│   │
│   ├── infrastructure/   # hodei-jobs-infrastructure - Adapters
│   │   ├── providers/
│   │   │   ├── docker.rs         # DockerProvider (bollard)
│   │   │   ├── kubernetes.rs     # KubernetesProvider (kube-rs)
│   │   │   └── firecracker.rs    # FirecrackerProvider (KVM)
│   │   ├── persistence.rs        # Postgres repositories (sqlx)
│   │   └── repositories.rs       # In-memory repositories
│   │
│   ├── grpc/             # hodei-jobs-grpc - gRPC Services
│   │   ├── services/             # Service implementations
│   │   ├── proto/                # Protocol Buffers (LogBatch message)
│   │   └── bin/
│   │       ├── server.rs         # Control plane server (mTLS ready)
│   │       └── worker.rs         # Worker agent (HPC-ready)
│   │
│   ├── interface/        # hodei-jobs-interface - REST API (Axum)
│   │
│   └── cli/              # hodei-jobs-cli - Command line interface
│
├── proto/                # Protocol Buffers definitions
├── deploy/
│   └── kubernetes/       # K8s manifests (RBAC, NetworkPolicy)
├── scripts/
│   ├── docker/           # Docker image build scripts
│   ├── kubernetes/       # K8s image build scripts
│   ├── firecracker/      # Firecracker rootfs build scripts
│   └── generate-certificates.sh  # PKI certificate generation
└── docs/security/
    ├── PKI-DESIGN.md     # mTLS PKI architecture
    └── CERTIFICATE-MANAGEMENT.md  # Operations guide
```

```mermaid
graph LR
    subgraph "Proto"
        PROTO[hodei-jobs<br/>Generated Types]
    end

    subgraph "Domain"
        DOM[hodei-jobs-domain<br/>Business Logic]
    end

    subgraph "Application"
        APP[hodei-jobs-application<br/>Use Cases]
    end

    subgraph "Infrastructure"
        INFRA[hodei-jobs-infrastructure<br/>Docker + K8s + Firecracker]
    end

    subgraph "Adapters"
        GRPC[hodei-jobs-grpc<br/>Server + Worker Agent]
        CLI[hodei-jobs-cli<br/>Command Line]
        REST[hodei-jobs-interface<br/>REST API]
    end

    PROTO --> GRPC
    PROTO --> CLI

    DOM --> APP
    APP --> GRPC
    APP --> REST

    INFRA --> APP
    INFRA --> GRPC
```

## Worker Agent - Componentes Internos (v8.0)

El worker agent es un sistema de alta performance con los siguientes componentes:

### Core Components

```mermaid
classDiagram
    class WorkerClient {
        +connection: GrpcConnection
        +stream: BidirectionalStream
        +register_worker() Result
        +start_worker_stream() Result
    }

    class LogBatcher {
        +tx: mpsc::Sender~WorkerMessage~
        +buffer: Vec~LogEntry~
        +capacity: usize
        +flush_interval: Duration
        +last_flush: Instant
        +flush() Task
        +add_entry() Result
    }

    class JobExecutor {
        +execute_shell() Result
        +execute_script_robust() Result
        +inject_secrets() Result
        +cleanup_temp_files() Task
    }

    class MetricsCollector {
        +cache: CachedResourceUsage
        +cache_ttl: Duration
        +get_usage() Result
        +spawn_blocking_task() Task
    }

    class CertificateManager {
        +paths: CertificatePaths
        +expiration: CertificateExpiration
        +load_certificates() Task
        +check_expiration() Task
    }

    WorkerClient --> LogBatcher
    WorkerClient --> JobExecutor
    WorkerClient --> MetricsCollector
    CertificateManager --> WorkerClient
```

### Data Structures

```rust
// LogBatcher - Batched log transmission
struct LogBatcher {
    tx: mpsc::Sender<WorkerMessage>,
    buffer: Vec<LogEntry>,
    capacity: usize,
    flush_interval: Duration,
    last_flush: Instant,
}

// CachedResourceUsage - TTL cache for metrics
struct CachedResourceUsage {
    usage: ResourceUsage,
    timestamp: Instant,
}
const METRICS_CACHE_TTL_SECS: u64 = 35;

// CertificatePaths - mTLS configuration
struct CertificatePaths {
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
    pub ca_cert_path: PathBuf,
}

// LogBatch message - Reduced network overhead
message LogBatch {
    string job_id = 1;
    repeated LogEntry entries = 2;
}
```

### Performance Characteristics

| Componente | Optimización | Beneficio |
|-----------|--------------|-----------|
| **LogBatcher** | Batch de 100 entries, flush cada 100ms | 90-99% reducción gRPC calls |
| **Backpressure** | try_send() con drop en full | Prevenir blocking del async runtime |
| **Zero-Copy I/O** | FramedRead + BytesCodec | Reducción allocaciones memoria |
| **Metrics Cache** | TTL 35s + spawn_blocking | Non-blocking metrics collection |
| **Write-Execute** | Temp files + async cleanup | Robustez en ejecución scripts |
| **Secret Injection** | stdin + JSON serialization | Seguridad sin exposición logs |

## Worker Providers

El sistema soporta múltiples providers para ejecutar workers:

| Provider | Aislamiento | Startup | GPU | Requisitos |
|----------|-------------|---------|-----|------------|
| **Docker** | Container | ~1s | Sí | Docker daemon |
| **Kubernetes** | Container (Pod) | ~5-15s | Sí | K8s cluster |
| **Firecracker** | Hardware (KVM) | ~125ms | No | Linux + KVM |

### WorkerProvider Trait (ISP Refactoring - DEBT-001)

**Estado**: 🟡 Fase 1 Completada - ISP traits segregados, trait combinado deprecated

```rust
// ISP Traits - Segregados (recomendado para nuevo código)
#[async_trait]
pub trait WorkerLifecycle: Send + Sync {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
}

#[async_trait]
pub trait WorkerHealth: Send + Sync {
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;
}

#[async_trait]
pub trait WorkerLogs: Send + Sync {
    async fn get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) 
        -> Result<Vec<LogEntry>, ProviderError>;
}

pub trait WorkerCost: Send + Sync {
    fn estimate_cost(&self, spec: &WorkerSpec, duration: Duration) -> Option<CostEstimate>;
    fn estimated_startup_time(&self) -> Duration;
}

// ... más ISP traits: WorkerEligibility, WorkerMetrics, WorkerEventSource, WorkerProviderIdentity

// Trait combinado (DEPRECATED - solo para backward compatibility)
#[deprecated(
    since = "0.83.0",
    note = "Use specific ISP traits (WorkerLifecycle, WorkerHealth, etc.) instead"
)]
#[async_trait]
pub trait WorkerProvider:
    WorkerProviderIdentity
    + WorkerLifecycle
    + WorkerLogs
    + WorkerCost
    + WorkerHealth
    + WorkerEligibility
    + WorkerMetrics
    + WorkerEventSource
    + Send
    + Sync
{
}
```

**Uso Recomendado (ISP-compliant)**:
```rust
// Cliente que solo necesita crear/destruir workers
struct SagaWorkerProvisioner {
    provider: Arc<dyn WorkerLifecycle + WorkerProviderIdentity>,
}

// Cliente que solo necesita health checks
struct MonitoringService {
    providers: Vec<Arc<dyn WorkerHealth>>,
}
```

**Migración**: Ver `docs/analysis/TECHNICAL_DEBT_SOLID_DDD.md#debt-001` para guía completa

### Variables de Entorno por Provider

**Docker:**
```bash
HODEI_DOCKER_ENABLED=1
HODEI_WORKER_IMAGE=hodei-worker:latest
```

**Kubernetes:**
```bash
HODEI_K8S_ENABLED=1
HODEI_K8S_NAMESPACE=hodei-workers
HODEI_K8S_KUBECONFIG=/path/to/kubeconfig  # opcional
```

**Firecracker:**
```bash
HODEI_FC_ENABLED=1
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4
HODEI_FC_USE_JAILER=true
```

## Servicios gRPC

| Servicio | Descripción | RPCs |
|----------|-------------|------|
| **WorkerAgentService** | Comunicación Server↔Worker | `Register`, `WorkerStream`, `UpdateWorkerStatus`, `UnregisterWorker` |
| **JobExecutionService** | Ciclo de vida de ejecución | `QueueJob`, `AssignJob`, `StartJob`, `UpdateProgress`, `CompleteJob`, `FailJob`, `CancelJob`, `ExecutionEventStream` |
| **SchedulerService** | Scheduling inteligente | `ScheduleJob`, `GetSchedulingDecision`, `ConfigureScheduler`, `GetQueueStatus`, `GetAvailableWorkers`, `SchedulingDecisionStream` |
| **ProviderManagementService** | Gestión de providers | `RegisterProvider`, `ListProviders`, `GetProviderHealth` |
| **MetricsService** | Métricas | `StreamMetrics`, `GetAggregatedMetrics`, `GetTimeSeriesMetrics` |
| **LogStreamService** | Logs de job | `SubscribeLogs`, `GetLogs` |

> Nota: el `MetricsService` requiere un backend explícito. Por defecto el backend está deshabilitado.

## Persistencia (Postgres)

El sistema persiste su estado en Postgres mediante repositorios SQLx (infraestructura):

- `JobRepository` (jobs)
- `JobQueue` (cola de jobs)
- `WorkerRegistry` (workers)
- `ProviderConfigRepository` (providers)

Tanto el servidor gRPC como el adaptador REST ejecutan migraciones al arrancar.

### Variables de entorno (DB)

- `HODEI_DATABASE_URL` (o `DATABASE_URL`) **obligatoria**
- `HODEI_DB_MAX_CONNECTIONS` (default `10`)
- `HODEI_DB_CONNECTION_TIMEOUT_SECS` (default `30`)

## Puertos y entrypoints

- gRPC server: `crates/grpc/src/bin/server.rs` (default `GRPC_PORT=50051`)
- Worker Agent: `crates/grpc/src/bin/worker.rs` (conecta a `HODEI_SERVER`, default `http://localhost:50051`)
- REST API: `crates/interface` (Axum). Requiere Postgres (mismas variables DB).

### WorkerAgentService - Flujo Principal

```protobuf
service WorkerAgentService {
    // Registro con OTP token (PRD v6.0)
    rpc Register(RegisterWorkerRequest) returns (RegisterWorkerResponse);

    // Stream bidireccional para comandos y respuestas
    rpc WorkerStream(stream WorkerMessage) returns (stream ServerMessage);

    // Legacy RPCs
    rpc UpdateWorkerStatus(UpdateWorkerStatusRequest) returns (UpdateWorkerStatusResponse);
    rpc UnregisterWorker(UnregisterWorkerRequest) returns (UnregisterWorkerResponse);
}
```

### Mensajes del Stream

**Worker → Server:**
- `WorkerHeartbeat` - Estado y recursos
- `LogEntry` - Logs de ejecución en tiempo real
- `JobResultMessage` - Resultado de job completado
- `WorkerStatsMessage` - Estadísticas del worker

**Server → Worker:**
- `RunJobCommand` - Ejecutar un job
- `CancelJobCommand` - Cancelar job en ejecución
- `AckMessage` - Confirmación de recepción
- `KeepAliveMessage` - Mantener conexión activa

## Comunicación entre Componentes

```mermaid
sequenceDiagram
    participant C as Client
    participant G as gRPC Gateway
    participant S as Service Layer
    participant A as Application Layer
    participant D as Domain Layer
    participant I as Infrastructure

    C->>G: gRPC Request
    G->>S: Parse & Validate
    S->>A: Execute Use Case
    A->>D: Domain Operations
    D->>D: Business Rules
    D-->>A: Domain Result
    A->>I: Persist Changes
    I-->>A: Confirmation
    A-->>S: Use Case Result
    S-->>G: Build Response
    G-->>C: gRPC Response
```

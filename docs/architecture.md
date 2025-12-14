# Arquitectura del Sistema

**Versión**: 7.0  
**Última Actualización**: 2025-12-14

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

## Comunicación Server ↔ Worker Agent (PRD v6.0)

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
    
    Note over S,W: 3. Bidirectional Stream
    W->>S: WorkerStream (bidirectional)
    loop Heartbeat + Commands
        W->>S: WorkerHeartbeat
        S-->>W: ACK / KeepAlive
        S->>W: RunJobCommand
        W->>S: LogEntry (streaming)
        W->>S: JobResultMessage
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
│   │   └── bin/
│   │       ├── server.rs         # Control plane server
│   │       └── worker.rs         # Worker agent
│   │
│   ├── interface/        # hodei-jobs-interface - REST API (Axum)
│   │
│   └── cli/              # hodei-jobs-cli - Command line interface
│
├── proto/                # Protocol Buffers definitions
├── deploy/
│   └── kubernetes/       # K8s manifests (RBAC, NetworkPolicy)
└── scripts/
    ├── docker/           # Docker image build scripts
    ├── kubernetes/       # K8s image build scripts
    └── firecracker/      # Firecracker rootfs build scripts
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

## Worker Providers

El sistema soporta múltiples providers para ejecutar workers:

| Provider | Aislamiento | Startup | GPU | Requisitos |
|----------|-------------|---------|-----|------------|
| **Docker** | Container | ~1s | Sí | Docker daemon |
| **Kubernetes** | Container (Pod) | ~5-15s | Sí | K8s cluster |
| **Firecracker** | Hardware (KVM) | ~125ms | No | Linux + KVM |

### WorkerProvider Trait

```rust
#[async_trait]
pub trait WorkerProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn provider_type(&self) -> ProviderType;
    fn capabilities(&self) -> &ProviderCapabilities;
    
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
    async fn get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) -> Result<Vec<LogEntry>, ProviderError>;
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;
    fn estimated_startup_time(&self) -> Duration;
}
```

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

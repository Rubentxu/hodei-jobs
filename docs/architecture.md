# Architecture Documentation - Hodei Job Platform

**Version**: 8.0  
**Last Updated**: 2026-01-09  
**Architecture Style**: Hexagonal + Event-Driven + DDD  

---

## Table of Contents

1. [Overview](#overview)
2. [Architectural Principles](#architectural-principles)
3. [High-Level Architecture](#high-level-architecture)
4. [Workspace Structure](#workspace-structure)
5. [Domain Layer (Server)](#domain-layer-server)
6. [Application Layer (Server)](#application-layer-server)
7. [Infrastructure Layer (Server)](#infrastructure-layer-server)
8. [Interface Layer (Server)](#interface-layer-server)
9. [Worker Architecture](#worker-architecture)
10. [Bounded Contexts](#bounded-contexts)
11. [Domain Events Architecture](#domain-events-architecture)
12. [Job Execution Flow](#job-execution-flow)
13. [Worker Lifecycle Management](#worker-lifecycle-management)
14. [Worker Providers](#worker-providers)
15. [gRPC Services](#grpc-services)
16. [Messaging & Event Bus](#messaging--event-bus)
17. [Persistence](#persistence)
18. [Security (Zero Trust)](#security-zero-trust)
19. [Performance Optimizations (v8.0)](#performance-optimizations-v80)
20. [Testing Strategy](#testing-strategy)
21. [Configuration Management](#configuration-management)
22. [Deployment Architecture](#deployment-architecture)

---

## Overview

Hodei Job Platform is a **production-ready, HPC-ready distributed job execution platform** built with Rust. It provides a universal abstraction for executing computational workloads across multiple infrastructure providers (Docker, Kubernetes, Firecracker) with automatic worker provisioning and Zero Trust security.

### Key Characteristics

- **Event-Driven Architecture**: Reacts to domain events in real-time
- **Hexagonal Architecture**: Clear separation between domain logic and infrastructure
- **Domain-Driven Design**: Organized around bounded contexts
- **CQRS Pattern**: Separate command and query responsibilities
- **Saga Pattern**: Distributed transaction coordination
- **Zero Trust Security**: mTLS with PKI infrastructure
- **HPC-Ready**: Optimized for high-performance computing workloads

### Technology Stack

- **Language**: Rust 2024 Edition (1.83+)
- **Runtime**: Tokio async runtime
- **Communication**: gRPC (Tonic) + Protocol Buffers
- **Database**: PostgreSQL with SQLx
- **Messaging**: PostgreSQL Outbox + NATS (optional)
- **Providers**: Docker (Bollard), Kubernetes (kube-rs), Firecracker (KVM)

---

## Architectural Principles

### 1. Inside-Out Communication
**Principle**: All agents initiate connections towards the control plane. Never open inbound ports on workers.

```
Worker ────OUTBOUND────▶ Server (Control Plane)
   ▲                           │
   │                           │
   └──────────gRPC─────────────┘
```

### 2. Domain-Driven Design
- **Bounded Contexts**: Each domain area has clear boundaries
- **Ubiquitous Language**: Consistent terminology across layers
- **Aggregate Roots**: Ensure consistency within aggregates
- **Domain Events**: Capture important domain changes

### 3. Hexagonal Architecture
```
┌─────────────────────────────────────────────────┐
│              Presentation Layer                  │
│         (gRPC, REST, CLI, Worker Agent)          │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│            Application Layer                     │
│     (Use Cases, Sagas, Coordinators)             │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│              Domain Layer                         │
│   (Aggregates, Value Objects, Domain Events)     │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│          Infrastructure Layer                     │
│  (Repositories, Providers, Event Bus, DB)       │
└──────────────────────────────────────────────────┘
```

### 4. SOLID Principles
- **Single Responsibility**: Each component has one reason to change
- **Open/Closed**: Open for extension, closed for modification
- **Liskov Substitution**: Subtypes must be substitutable
- **Interface Segregation**: Clients shouldn't depend on unused interfaces
- **Dependency Inversion**: Depend on abstractions, not concretions

### 5. Reactive & Event-Driven
- **Reactive Streams**: Backpressure-aware async operations
- **Domain Events**: React to state changes asynchronously
- **Saga Pattern**: Coordinate distributed transactions
- **Actor Model**: Isolated worker state management

---

## High-Level Architecture

### Component Overview

```mermaid
graph TB
    subgraph "Clients"
        CLI[CLI]
        WEB[Web Dashboard]
        API[REST/gRPC Clients]
    end

    subgraph "Control Plane (Server)"
        subgraph "Interface Layer"
            GRPC[gRPC Services]
            REST[REST API]
            WEBSOCKET[WebSocket Streams]
        end

        subgraph "Application Layer"
            JOB_UC[Job Use Cases]
            WORKER_UC[Worker Use Cases]
            PROV_UC[Provider Use Cases]
            SCHED_UC[Scheduler]
            SAGA[Saga Orchestrator]
        end

        subgraph "Domain Layer"
            JOB_CTX[Job Context]
            WORKER_CTX[Worker Context]
            PROV_CTX[Provider Context]
            IAM_CTX[IAM Context]
        end
    end

    subgraph "Data Plane (Workers)"
        WORKER[Worker Agent]
        EXEC[Job Executor]
        MONITOR[Resource Monitor]
    end

    subgraph "Infrastructure"
        PG[(PostgreSQL)]
        NATS[NATS Broker]
        DOCKER[Docker Daemon]
        K8S[Kubernetes API]
        FIRECRACKER[Firecracker]
    end

    CLI --> GRPC
    WEB --> REST
    API --> GRPC

    GRPC --> JOB_UC
    REST --> JOB_UC
    GRPC --> WORKER_UC

    JOB_UC --> JOB_CTX
    WORKER_UC --> WORKER_CTX
    PROV_UC --> PROV_CTX
    SCHED_UC --> JOB_CTX

    SAGA --> JOB_UC
    SAGA --> WORKER_UC

    JOB_UC --> PG
    WORKER_UC --> PG
    SAGA --> PG

    WORKER --> GRPC
    WORKER --> EXEC
    EXEC --> MONITOR

    WORKER_UC --> DOCKER
    WORKER_UC --> K8S
    WORKER_UC --> FIRECRACKER

    JOB_UC --> NATS
    WORKER_UC --> NATS
```

### Communication Flows

#### 1. Job Submission Flow
```
Client ──QueueJob──▶ gRPC Server ──CreateJobUseCase──▶ Domain
                          │                              │
                          ▼                              ▼
                    JobQueue ◀──────────JobCreatedEvent
                          │
                          ▼
                    WorkerProvisioning
                          │
                          ▼
                    Provider (Docker/K8s/FC)
                          │
                          ▼
                    Worker Agent ◀─────────WorkerStartedEvent
                          │
                          ▼
                    JobExecution
                          │
                          ▼
                    JobCompletedEvent
```

#### 2. Worker Registration Flow (OTP Authentication)
```
Provider ──create_worker──▶ Server ──GenerateOTP──▶ TokenStore
    │                                             │
    ▼                                             ▼
Container (with OTP) ────────────────────────────▶ Server
         │
         ▼
    Worker Agent
         │
         ▼
    Register(OTP) ──▶ Server (ValidateOTP) ──▶ WorkerRegisteredEvent
```

---

## Workspace Structure

### Crates Organization

```
hodei-jobs/
├── crates/
│   ├── shared/                          # Shared types & utilities
│   │   ├── src/
│   │   │   ├── ids.rs                   # JobId, WorkerId, ProviderId, etc.
│   │   │   ├── states.rs                # JobState, WorkerState, ProviderStatus
│   │   │   ├── error.rs                 # Common error types
│   │   │   └── event_topics.rs          # Event topic definitions
│   │
│   ├── server/                          # Server-side components
│   │   ├── domain/                      # Domain Layer (DDD)
│   │   │   ├── src/
│   │   │   │   ├── shared_kernel/        # Shared domain concepts
│   │   │   │   ├── jobs/                # Job bounded context
│   │   │   │   ├── workers/             # Worker bounded context
│   │   │   │   ├── providers/           # Provider bounded context
│   │   │   │   ├── scheduling/          # Scheduling strategies
│   │   │   │   ├── templates/           # Job templates (EPIC-34)
│   │   │   │   ├── iam/                 # Identity & Access Management
│   │   │   │   ├── audit/               # Audit logging
│   │   │   │   ├── credentials/         # Credential management
│   │   │   │   ├── saga/                # Saga pattern support
│   │   │   │   ├── logging/             # Logging domain
│   │   │   │   ├── command/             # Command bus support
│   │   │   │   ├── domain_events/       # Modular domain events
│   │   │   │   ├── event_bus.rs         # Event bus traits
│   │   │   │   ├── outbox/              # Outbox pattern
│   │   │   │   ├── telemetry/           # Tracing & metrics
│   │   │   │   └── events.rs            # Legacy monolithic events
│   │   │
│   │   ├── application/                 # Application Layer (Use Cases)
│   │   │   ├── src/
│   │   │   │   ├── core/                # CQRS infrastructure
│   │   │   │   ├── command/             # Command bus implementation
│   │   │   │   ├── jobs/                # Job use cases
│   │   │   │   ├── workers/             # Worker use cases
│   │   │   │   ├── providers/           # Provider use cases
│   │   │   │   ├── scheduling/          # Scheduler service
│   │   │   │   ├── saga/                # Saga orchestrators
│   │   │   │   ├── resilience/          # Circuit breakers, retries
│   │   │   │   └── metrics/             # Metrics collection
│   │   │
│   │   ├── infrastructure/              # Infrastructure Layer
│   │   │   ├── src/
│   │   │   │   ├── persistence/         # Database implementations
│   │   │   │   │   └── postgres/        # PostgreSQL repositories
│   │   │   │   ├── providers/           # Provider implementations
│   │   │   │   │   ├── docker.rs        # Docker provider
│   │   │   │   │   ├── kubernetes/      # Kubernetes provider
│   │   │   │   │   └── firecracker/     # Firecracker provider
│   │   │   │   ├── messaging/           # Event bus implementations
│   │   │   │   │   ├── postgres/        # PostgreSQL outbox
│   │   │   │   │   └── nats/            # NATS event bus
│   │   │   │   ├── observability/       # Logging & monitoring
│   │   │   │   ├── metrics/             # Metrics collectors
│   │   │   │   └── reconciliation/      # State reconciliation
│   │   │   └── migrations/              # Database migrations
│   │   │
│   │   ├── interface/                   # Interface Layer (Adapters)
│   │   │   ├── src/
│   │   │   │   ├── grpc/                # gRPC service implementations
│   │   │   │   ├── mappers/             # gRPC ↔ Domain mappers
│   │   │   │   ├── log_buffer.rs        # Log streaming buffer
│   │   │   │   └── log_persistence.rs   # Log storage backend
│   │   │
│   │   ├── bin/                         # Server executables
│   │   │   └── src/
│   │   │       └── main.rs              # gRPC server entry point
│   │   │
│   │   └── tests/                       # Integration tests
│   │
│   ├── worker/                          # Worker-side components
│   │   ├── domain/                      # Worker domain (minimal)
│   │   ├── application/                 # Worker application layer
│   │   ├── infrastructure/              # Worker infrastructure
│   │   │   └── src/
│   │   │       ├── executor.rs          # Job executor
│   │   │       ├── metrics.rs           # Metrics collector
│   │   │       ├── log_batcher.rs       # Log batching
│   │   │       └── client.rs            # gRPC client
│   │   └── bin/                         # Worker executables
│   │       └── src/
│   │           └── main.rs              # Worker agent entry point
│   │
│   └── cli/                             # Command-line interface
│       └── src/
│
├── proto/                               # Protocol Buffers definitions
│   ├── hodei_all_in_one.proto          # All services combined
│   ├── worker_agent.proto              # Worker ↔ Server API
│   ├── job_execution.proto             # Job execution API
│   ├── scheduler.proto                 # Scheduling API
│   ├── metrics.proto                   # Metrics API
│   ├── job_templates.proto             # Templates API
│   └── provider_management.proto       # Provider management API
│
└── docs/                                # Documentation
    ├── architecture.md                  # This file
    ├── analysis/                        # Architectural analysis
    └── epics/                           # Refactoring épicas
```

---

## Domain Layer (Server)

### Purpose

The domain layer contains the **business logic and core concepts** of the platform. It is completely independent of external concerns (databases, frameworks, APIs).

### Bounded Contexts

#### 1. Jobs Context (`domain/src/jobs/`)

**Responsibility**: Job lifecycle management

**Key Components**:

**Aggregate Root: Job**

**📋 Responsabilidad (SOLID)**:
- **Single Responsibility Principle (SRP)**: El agregado `Job` es responsable de mantener la consistencia del ciclo de vida de un job. Es el único lugar donde se pueden modificar las invariantes de negocio relacionadas con un job.
- **Open/Closed Principle (OCP)**: Abierto para extensión (nuevos estados, nuevas validaciones) pero cerrado para modificación (métodos públicos estables).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs**
- Es el **Aggregate Root** del contexto de Jobs, lo que significa que:
- Todas las modificaciones a un Job y sus entidades relacionadas deben pasar por este agregado
- Garantiza la consistencia transaccional dentro del agregado
- Implementa las reglas de negocio del dominio

**🔗 Dependencias**:
- **Internal**: `JobId`, `JobSpec`, `JobState`, `ExecutionContext` (Value Objects del mismo contexto)
- **No dependencies externas**: Puro dominio, sin dependencias de frameworks o infraestructura

**💡 Motivación**:
Se creó para encapsular la lógica de negocio completa de un job y garantizar que:
1. Los jobs siempre transicionan de estado de manera válida
2. Las invariantes de negocio (ej. max_attempts, timeout) se respeten
3. Las operaciones atómicas en el agregado sean consistentes

**🎨 Patrones Aplicados**:
- **Aggregate Pattern**: Agrupa entidades relacionadas bajo un root para garantizar consistencia
- **Encapsulation**: Los campos son privados, se acceden solo a través de métodos públicos
- **Invariant Enforcement**: Los métodos públicos aseguran que el estado siempre sea válido

```rust
// Aggregate Root
pub struct Job {
    id: JobId,
    spec: JobSpec,
    state: JobState,
    selected_provider: Option<ProviderId>,
    execution_context: ExecutionContext,
    attempts: u32,
    max_attempts: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

---

**Value Objects: JobSpec y relacionados**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada Value Object tiene una única responsabilidad: representar un concepto de dominio inmutable y validado.
- **Liskov Substitution Principle (LSP)**: Los Value Objects son substituibles por igualdad (si tienen mismos valores, son el mismo objeto).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs**
- Son componentes internos del agregado Job
- No tienen identidad propia, se identifican por sus atributos
- Son inmutables para evitar efectos secundarios

**🔗 Dependencias**:
- `JobSpec`: Depende de `CommandType`, `JobResources`, `JobPreferences`
- `CommandType`: Enum de tipos de comandos soportados
- `JobResources`: Representa requerimientos de recursos computacionales

**💡 Motivación**:
Se crearon para:
1. Capturar conceptos de dominio complejos en objetos cohesivos
2. Evitar **Primitive Obsession** (usar strings, ints en lugar de objetos de dominio)
3. Garantizar inmutabilidad de configuraciones críticas
4. Facilitar validaciones en un solo lugar

**🎨 Patrones Aplicados**:
- **Value Object Pattern**: Objetos sin identidad, definidos por sus atributos
- **Immutability**: Los objetos no pueden modificarse después de creación
- **Type Safety**: Enums en lugar de strings para tipos de comandos

```rust
// Value Objects
pub struct JobSpec {
    command: CommandType,
    image: Option<String>,
    environment: HashMap<String, String>,
    resources: JobResources,
    timeout_ms: u64,
    constraints: Vec<Constraint>,
    inputs: Vec<ArtifactSource>,
    outputs: Vec<ArtifactDest>,
    preferences: JobPreferences,
}

pub enum CommandType {
    Shell { cmd: String, args: Vec<String> },
    Script { interpreter: String, content: String },
}

pub struct JobResources {
    cpu_cores: f32,
    memory_mb: u64,
    storage_mb: u64,
    gpu_required: bool,
    architecture: String,
}
```

---

**Repository Traits: JobRepository y JobQueue**

**📋 Responsabilidad (SOLID)**:
- **Interface Segregation Principle (ISP)**: Cada trait tiene métodos cohesivos. `JobRepository` para persistencia, `JobQueue` para operaciones de cola. Los clientes no dependen de métodos que no usan.
- **Dependency Inversion Principle (DIP)**: Las capas superiores dependen de abstracciones (traits), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Infrastructure Layer Implementation of Domain Abstractions**
- Son **puertos** de la arquitectura hexagonal
- Definen contratos que debe cumplir cualquier implementación de infraestructura
- Permiten cambiar implementaciones (Postgres, MySQL, en memoria) sin afectar el dominio

**🔗 Dependencias**:
- **Domain Types**: `Job`, `JobId`, `JobState`, `Result<Job>`
- **Async Traits**: `#[async_trait]` para métodos asíncronos
- **Concurrency**: `Send + Sync` para permitir concurrencia segura

**💡 Motivación**:
Se crearon para:
1. Separar la lógica de negocio (dominio) de la persistencia (infraestructura)
2. Permitir múltiples implementaciones (Postgres, en memoria, caching)
3. Facilitar testing con mocks y fakes
4. Seguir el patrón Repository para encapsular acceso a datos

**🎨 Patrones Aplicados**:
- **Repository Pattern**: Abstrae el almacenamiento y recuperación de objetos de dominio
- **Dependency Inversion**: Depende de abstracciones, no de implementaciones
- **Interface Segregation**: Interfaces pequeñas y enfocadas

```rust
// Repository Traits
#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>>;
    async fn find_by_state(&self, state: &JobState) -> Result<Vec<Job>>;
    async fn find_pending(&self) -> Result<Vec<Job>>;
    async fn delete(&self, id: &JobId) -> Result<()>;
    // ... more methods
}

#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: Job) -> Result<()>;
    async fn dequeue(&self) -> Result<Option<Job>>;
    async fn peek(&self) -> Result<Option<Job>>;
    async fn len(&self) -> Result<usize>;
    // ... more methods
}
```

**Domain Events: JobEvent**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada variante del enum representa un evento específico que ocurrió en el dominio. El enum como un todo es responsable de representar todos los eventos posibles de jobs.
- **OCP**: Nuevos eventos pueden agregarse sin modificar el código existente que maneja eventos (mediante pattern matching exhaustivo).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs - Event Bus**
- Representa **algo que pasó** en el dominio que es relevante para otras partes del sistema
- Son inmutables por naturaleza (los hechos del pasado no cambian)
- Publicados por el agregado cuando ocurren cambios de estado importantes

**🔗 Dependencias**:
- **Domain Types**: `JobId`, `JobSpec`, `JobState`, `JobExecutionResult`
- **Serialization**: `#[derive(Serialize, Deserialize)]` para serialización JSON
- **No dependencies externas**: Solo tipos de dominio

**💡 Motivación**:
Se creó para:
1. **Desacoplar bounded contexts**: Los cambios en Jobs no impactan directamente a Workers, Scheduling, etc.
2. **Implementar Event-Driven Architecture**: Permitir reacción asíncrona a cambios
3. **Auditoría y tracking**: Historia completa de lo que pasó con cada job
4. **Integración con sistemas externos**: Notificaciones, logs, métricas

**🎨 Patrones Aplicados**:
- **Domain Events Pattern**: Captura hechos importantes del dominio
- **Event Sourcing (parcial)**: Eventos representan cambios de estado
- **Pub/Sub**: Productores (agregados) publican, consumidores suscriben
- **Type Safety**: Enum asegura que solo se manejen eventos válidos

**⚠️ Consideraciones de Diseño**:
- **Connascence of Name**: El nombre del evento debe ser claro y descriptivo
- **Temporal Coupling**: Los consumidores deben manejar el orden correcto de eventos
- **Idempotency**: Los manejadores de eventos deben ser idempotentes

```rust
pub enum JobEvent {
    JobCreated {
        job_id: JobId,
        spec: JobSpec,
        queued_by: String,
    },
    JobStatusChanged {
        job_id: JobId,
        old_state: JobState,
        new_state: JobState,
    },
    JobCompleted {
        job_id: JobId,
        result: JobExecutionResult,
    },
    JobFailed {
        job_id: JobId,
        error: String,
        attempt: u32,
    },
    // ... more events
}
```

#### 2. Workers Context (`domain/src/workers/`)

**Responsibility**: Worker lifecycle management

**🏗️ Ámbito en DDD**: **Bounded Context: Workers**
- Maneja el ciclo de vida completo de workers (provisioning, registro, ejecución, terminación)
- Define los contratos para providers de infraestructura
- Coordina con el contexto de Jobs para asignación de trabajos

**📋 Responsabilidad General (SOLID)**:
- **SRP**: El contexto está dedicado exclusivamente a la gestión de workers
- **ISP**: Cada trait tiene métodos cohesivos específicos para su propósito
- **DIP**: Las capas superiores dependen de abstracciones, no de implementaciones concretas

**Key Components**:

---

**Aggregate Root: Worker**

**📋 Responsabilidad (SOLID)**:
- **SRP**: El agregado `Worker` es responsable de mantener la consistencia del estado de un worker durante todo su ciclo de vida.
- **OCP**: Abierto para agregar nuevos estados o políticas de TTL sin modificar código existente.

**🏗️ Ámbito en DDD**: **Aggregate Root - Workers Context**
- Es el punto de consistencia para todas las operaciones relacionadas con un worker
- Garantiza que el estado del worker y sus recursos sean consistentes
- Coordina transiciones de estado validadas

**🔗 Dependencias**:
- **Internal**: `WorkerHandle`, `WorkerSpec`, `WorkerState`, `ResourceUsage`, `WorkerTTLConfig`
- **Cross-context**: `JobId` (referencia opcional al job asignado)
- **No dependencies externas**: Puro dominio

**💡 Motivación**:
Se creó para:
1. Encapsular toda la lógica de estado de un worker en un solo lugar
2. Implementar máquina de estados robusta para transiciones de worker
3. Garantizar que el worker no exceda sus límites de recursos
4. Manejar timeouts y cleanup automático

**🎨 Patrones Aplicados**:
- **State Machine**: WorkerState define estados válidos y transiciones
- **Aggregate Pattern**: Garantiza consistencia del worker
- **Invariant Enforcement**: Los métodos públicos aseguran estado válido
- **Resource Management**: Tracking de uso de recursos

```rust
// Aggregate Root
pub struct Worker {
    handle: WorkerHandle,
    spec: WorkerSpec,
    state: WorkerState,
    current_job_id: Option<JobId>,
    resources: ResourceUsage,
    heartbeat: Option<DateTime<Utc>>,
    ttl_config: WorkerTTLConfig,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

---

**Value Objects: WorkerHandle, WorkerSpec**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada Value Object tiene una única responsabilidad de representar un concepto específico.
- **LSP**: Los Value Objects son substituibles por igualdad.

**🏗️ Ámbito en DDD**: **Components - Workers Context**
- Son inmutables por definición
- No tienen identidad propia, se identifican por sus valores
- `WorkerHandle`: Identificador único y referencia al provider
- `WorkerSpec`: Especificación completa de configuración del worker

**🔗 Dependencias**:
- `WorkerHandle`: `WorkerId`, `ProviderType`, `String`
- `WorkerSpec`: `ProviderId`, `ProviderType`, `WorkerResources`, `WorkerTTLConfig`
- **Cross-context**: `JobId` (referencia al job que motivó el provisioning)

**💡 Motivación**:
Se crearon para:
1. Capturar la complejidad de configuración de workers en objetos tipados
2. Evitar Primitive Obsession (no usar strings/HashMaps crudos)
3. Facilitar validaciones centralizadas de configuraciones
4. Garantizar inmutabilidad de configuraciones críticas

**🎨 Patrones Aplicados**:
- **Value Object Pattern**: Objetos sin identidad, definidos por atributos
- **Type Safety**: Enums en lugar de strings para ProviderType
- **Builder Pattern**: (implícito) Construcción gradual de specs complejas

```rust
// Value Objects
pub struct WorkerHandle {
    worker_id: WorkerId,
    provider_type: ProviderType,
    provider_execution_id: String,
    connection_id: Option<String>,
}

pub struct WorkerSpec {
    provider_id: ProviderId,
    provider_type: ProviderType,
    image: String,
    resources: WorkerResources,
    ttl_config: WorkerTTLConfig,
    labels: HashMap<String, String>,
}
```

---

**Enums: ProviderType y WorkerState**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada enum tiene una única responsabilidad: definir tipos válidos.
- **OCP**: Nuevos providers o estados pueden agregarse sin modificar código existente (con pattern matching).

**🏗️ Ámbito en DDD**: **Core Types - Workers Context**
- `ProviderType`: Define los tipos de infraestructura soportados
- `WorkerState`: Define los estados válidos en la máquina de estados del worker

**🔗 Dependencias**:
- **No dependencies**: Enums primitivos, sin dependencias externas
- **Self-referential**: WorkerState puede referenciar a sí mismo en transiciones

**💡 Motivación**:
Se crearon para:
1. **Type Safety**: Compilador asegura que solo se usen tipos válidos
2. **Exhaustive Pattern Matching**: Rust obliga a manejar todos los casos
3. **Documentación Inline**: Los enums sirven como documentación de dominio
4. **Prevenir Errores**: No hay strings mágicos, solo tipos válidos

**🎨 Patrones Aplicados**:
- **Type-safe Enum**: Alternativa a strings mágicos
- **State Machine**: WorkerState define estados válidos
- **Strategy Pattern**: ProviderType permite diferentes estrategias de provisioning

```rust
pub enum ProviderType {
    Docker,
    Kubernetes,
    Firecracker,
}

pub enum WorkerState {
    Creating,
    Connecting,
    Ready,
    Busy,
    Draining,
    Terminating,
    Terminated,
}
```

// Traits
#[async_trait]
pub trait WorkerProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn provider_type(&self) -> ProviderType;
    fn capabilities(&self) -> &ProviderCapabilities;

    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
    // ... more methods
}

#[async_trait]
pub trait WorkerRegistry: Send + Sync {
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec, job_id: JobId) -> Result<Worker>;
    async fn unregister(&self, worker_id: &WorkerId) -> Result<()>;
    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>>;
    async fn find_available(&self) -> Result<Vec<Worker>>;
    // ... more methods
}
```

**Domain Events**:

```rust
pub enum WorkerEvent {
    WorkerRegistered {
        worker_id: WorkerId,
        provider_id: ProviderId,
        spec: WorkerSpec,
    },
    WorkerStatusChanged {
        worker_id: WorkerId,
        old_state: WorkerState,
        new_state: WorkerState,
    },
    WorkerHeartbeat {
        worker_id: WorkerId,
        resources: ResourceUsage,
    },
    WorkerTerminated {
        worker_id: WorkerId,
        reason: TerminationReason,
    },
    // ... more events
}
```

#### 3. Providers Context (`domain/src/providers/`)

**Responsibility**: Provider configuration and management

**Key Components**:

```rust
// Aggregate Root
pub struct ProviderConfig {
    id: ProviderId,
    name: String,
    provider_type: ProviderType,
    config: ProviderSpecificConfig,
    status: ProviderStatus,
    capabilities: ProviderCapabilities,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub enum ProviderSpecificConfig {
    Docker(DockerConfig),
    Kubernetes(KubernetesConfig),
    Firecracker(FirecrackerConfig),
}

pub struct ProviderCapabilities {
    max_cpu_cores: Option<u32>,
    max_memory_gb: Option<u64>,
    supports_gpu: bool,
    supported_runtimes: Vec<String>,
    supported_architectures: Vec<String>,
    max_concurrent_jobs: Option<u32>,
}

#[async_trait]
pub trait ProviderConfigRepository: Send + Sync {
    async fn save(&self, config: &ProviderConfig) -> Result<()>;
    async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>>;
    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>>;
    // ... more methods
}
```

#### 4. Scheduling Context (`domain/src/scheduling/`)

**Responsibility**: Job-to-worker assignment strategies

**Key Components**:

```rust
pub trait Scheduler: Send + Sync {
    async fn select_worker(
        &self,
        job: &Job,
        available_workers: &[Worker],
        providers: &[ProviderConfig],
    ) -> Result<Option<WorkerId>, SchedulingError>;
}

pub struct SmartScheduler {
    strategy: Box<dyn SchedulingStrategy>,
    policy: SchedulingPolicy,
}

pub trait SchedulingStrategy: Send + Sync {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64;
}

pub enum SchedulingStrategy {
    RoundRobin,
    LeastLoaded,
    PriorityBased,
    ResourceAware,
    CostOptimized,
}
```

#### 5. Templates Context (`domain/src/templates/`)

**Responsibility**: Job template management (EPIC-34)

**Key Components**:

```rust
pub struct JobTemplate {
    id: TemplateId,
    name: String,
    description: Option<String>,
    spec: JobSpec,
    parameters: Vec<TemplateParameter>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub struct TemplateParameter {
    name: String,
    param_type: ParameterType,
    default_value: Option<serde_json::Value>,
    required: bool,
    description: Option<String>,
}

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn save(&self, template: &JobTemplate) -> Result<()>;
    async fn find_by_id(&self, id: &TemplateId) -> Result<Option<JobTemplate>>;
    async fn find_by_name(&self, name: &Str) -> Result<Option<JobTemplate>>;
    // ... more methods
}
```

#### 6. IAM Context (`domain/src/iam/`)

**Responsibility**: Authentication and authorization

**Key Components**:

```rust
pub struct BootstrapToken {
    token: OtpToken,
    worker_id: WorkerId,
    provider_id: ProviderId,
    spec_hash: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[async_trait]
pub trait OtpTokenStore: Send + Sync {
    async fn generate(&self, worker_id: WorkerId, provider_id: ProviderId, ttl: Duration) -> Result<BootstrapToken>;
    async fn validate(&self, token: &OtpToken) -> Result<WorkerRegistration>;
    async fn consume(&self, token: &OtpToken) -> Result<()>;
}
```

#### 7. Audit Context (`domain/src/audit/`)

**Responsibility**: Audit logging

**Key Components**:

```rust
pub struct AuditLog {
    id: AuditLogId,
    actor: String,
    action: String,
    target_type: String,
    target_id: String,
    changes: Option<serde_json::Value>,
    timestamp: DateTime<Utc>,
    metadata: AuditMetadata,
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn log(&self, entry: &AuditLog) -> Result<()>;
    async fn find_by_actor(&self, actor: &str, limit: usize) -> Result<Vec<AuditLog>>;
    // ... more methods
}
```

#### 8. Credentials Context (`domain/src/credentials/`)

**Responsibility**: Secret and credential management

**Key Components**:

```rust
pub struct Secret {
    id: SecretId,
    name: String,
    value: SecretValue,
    secret_type: SecretType,
    version: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn get(&self, name: &str) -> Result<Secret>;
    async fn list(&self) -> Result<Vec<Secret>>;
    // ... more methods
}
```

#### 9. Saga Context (`domain/src/saga/`)

**Responsibility**: Distributed transaction coordination

**Key Components**:

```rust
pub struct Saga<Steps> {
    id: SagaId,
    state: SagaState,
    current_step: usize,
    steps: Steps,
    compensation_data: HashMap<String, serde_json::Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub enum SagaState {
    Pending,
    InProgress,
    Compensating,
    Completed,
    Failed,
}

#[async_trait]
pub trait SagaStep: Send + Sync {
    type Input;
    type Output;
    type CompensatingInput;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, SagaError>;
    async fn compensate(&self, input: Self::CompensatingInput) -> Result<(), SagaError>;
}

#[async_trait]
pub trait SagaRepository: Send + Sync {
    async fn save(&self, saga: &Saga<dyn Any>) -> Result<()>;
    async fn find_by_id(&self, id: &SagaId) -> Result<Option<Saga<dyn Any>>>;
    async fn find_pending(&self) -> Result<Vec<Saga<dyn Any>>>;
    // ... more methods
}
```

### Shared Kernel (`domain/src/shared_kernel/`)

**Purpose**: Types and concepts shared across all bounded contexts

```rust
// IDs
pub struct JobId(pub Uuid);
pub struct WorkerId(pub Uuid);
pub struct ProviderId(pub Uuid);
pub struct TemplateId(pub Uuid);
pub struct SagaId(pub Uuid);

// States
pub enum JobState {
    Pending,
    Scheduled,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
}

pub enum WorkerState {
    Creating,
    Connecting,
    Ready,
    Busy,
    Draining,
    Terminating,
    Terminated,
}

// Errors
pub type Result<T> = std::result::Result<T, DomainError>;

pub enum DomainError {
    JobNotFound(JobId),
    WorkerNotFound(WorkerId),
    ProviderNotFound(ProviderId),
    InvalidStateTransition { current: String, target: String },
    ProviderUnhealthy(ProviderId),
    MaxAttemptsExceeded { job_id: JobId, attempts: u32 },
    // ... more errors
}
```

---

## Application Layer (Server)

### Purpose

The application layer orchestrates **use cases** using domain logic. It coordinates between bounded contexts and infrastructure.

### CQRS Infrastructure (`application/src/core/`)

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada trait tiene una única responsabilidad: `CommandBus` para comandos, `QueryBus` para queries, `Command`/`Query` para tipar mensajes.
- **ISP**: Los clientes dependen solo de las interfaces que usan (CommandBus o QueryBus, no ambos).
- **DIP**: Las capas superiores dependen de estas abstracciones, no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Application Layer - CQRS Pattern**
- Implementa el patrón **Command Query Responsibility Segregation**
- Separa operaciones de escritura (comandos) de lectura (queries)
- Permite optimizar cada lado independientemente (caching, sharding, etc.)

**🔗 Dependencias**:
- **Domain Types**: `Result<T>` del shared kernel
- **Concurrency**: `Send + Sync` para thread safety
- **Generics**: `C: Command`, `Q: Query` para type safety
- **No dependencies externas**: Puro Rust standard library

**💡 Motivación**:
Se creó para:
1. **Implementar CQRS**: Separar claramente comandos y queries
2. **Type Safety**: Compilador asegura tipos correctos de comandos y resultados
3. **Extensibilidad**: Fácil agregar nuevos comandos y queries
4. **Testing**: Mocks simples para CommandBus y QueryBus
5. **Performance**: Permitir optimizaciones específicas por tipo de operación

**🎨 Patrones Aplicados**:
- **CQRS Pattern**: Separación de responsabilidad de comandos y queries
- **Bus Pattern**: Desacoplamiento entre emisor y receptor de mensajes
- **Generic Programming**: Traits genéricos para type safety en compile-time
- **Dependency Inversion**: Depende de traits, no de implementaciones

**⚠️ Consideraciones de Diseño**:
- **Async/Await**: Todos los métodos son asíncronos para no bloquear
- **Error Handling**: Usa `Result<T>` para errores tipados
- **Type Erasure**: El trait object permite dispatch dinámico

```rust
// Command Bus
pub trait CommandBus: Send + Sync {
    async fn dispatch<C>(&self, command: C) -> Result<C::Result>
    where
        C: Command + Send + 'static;
}

// Query Bus
pub trait QueryBus: Send + Sync {
    async fn execute<Q>(&self, query: Q) -> Result<Q::Result>
    where
        Q: Query + Send + 'static;
}

// Command Trait
pub trait Command: Send + Sync {
    type Result: Send + Sync;
}

// Query Trait
pub trait Query: Send + Sync {
    type Result: Send + Sync;
}
```

### Job Use Cases (`application/src/jobs/`)

**🏗️ Ámbito en DDD**: **Application Layer - Jobs Context**
- Orquesta use cases relacionados con jobs
- Coordina entre bounded contexts (Jobs, Workers, Providers)
- Implementa flujos de negocio complejos que cruzan agregados

**📋 Responsabilidad General (SOLID)**:
- **SRP**: Cada Use Case tiene una única responsabilidad (create, dispatch, control)
- **OCP**: Abierto para extender con nuevos Use Cases sin modificar existentes
- **DIP**: Depende de abstracciones de dominio, no de implementaciones concretas

#### 1. CreateJobUseCase

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: crear y validar jobs según reglas de negocio.
- **OCP**: Abierto para agregar nuevas validaciones sin modificar el caso de uso existente.
- **DIP**: Depende de `JobQueue`, `JobRepository`, `Scheduler`, `EventBus` (abstracciones), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Application Layer - Jobs Context**
- Orquesta la creación de jobs interactuando con múltiples bounded contexts
- Coordina entre: Jobs (crear), Queue (enqueue), Scheduling (select), Workers (provision)

**🔗 Dependencias**:
- **Domain (Jobs)**: `JobQueue`, `JobRepository`
- **Domain (Scheduling)**: `Scheduler`
- **Domain (Event Bus)**: `EventBus`
- **Concurrency**: `Arc<T>` para compartir estado entre threads

**💡 Motivación**:
Se creó para:
1. **Encapsular flujo de negocio**: Creación de job es más que simple INSERT
2. **Orquestar múltiples bounded contexts**: Validar, crear, encolar, programar
3. **Publicar eventos**: Notificar al sistema sobre nuevo job
4. **Manejar errores**: Traducir errores técnicos a errores de dominio

**🎨 Patrones Aplicados**:
- **Use Case Pattern**: Encapsula un caso de uso específico del dominio
- **Orchestration Pattern**: Coordina múltiples bounded contexts
- **Dependency Injection**: Recibe dependencias por constructor
- **Event Publishing**: Publica eventos al finalizar para integración

**⚠️ Consideraciones de Diseño**:
- **Transactional Boundary**: Debe ser transaccional (create + enqueue + publish)
- **Error Handling**: Debe traducir todos los errores a `Result<Job>`
- **Validation**: Valida en la aplicación, no en la capa de infraestructura

```rust
pub struct CreateJobUseCase {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    scheduler: Arc<dyn Scheduler>,
    event_bus: Arc<dyn EventBus>,
}

impl CreateJobUseCase {
    pub async fn execute(&self, request: CreateJobRequest) -> Result<Job> {
        // 1. Validate request
        // 2. Create job aggregate
        // 3. Persist job
        // 4. Publish JobCreated event
        // 5. Trigger provisioning if needed
    }
}
```

---

#### 2. JobController

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: control loop que procesa jobs pendientes continuamente.
- **OCP**: Abierto para agregar nuevas estrategias de procesamiento sin modificar el loop principal.
- **DIP**: Depende de abstracciones, no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Application Layer - Jobs Context**
- Implementa el **control loop** que mantiene el sistema procesando jobs
- Coordinador principal entre: Queue, Workers, Dispatcher, Events

**🔗 Dependencias**:
- **Domain (Jobs)**: `JobQueue`, `JobRepository`
- **Domain (Workers)**: `WorkerRegistry`
- **Application**: `JobDispatcher`
- **Domain (Event Bus)**: `EventBus`

**💡 Motivación**:
Se creó para:
1. **Implementar reactor pattern**: Reaccionar continuamente a cambios en cola
2. **Separar concerns**: Loop control vs lógica de dispatch
3. **Facilitar testing**: Loop puede testearse aisladamente
4. **Monitoreo**: Punto central para métricas de procesamiento

**🎨 Patrones Aplicados**:
- **Controller Pattern**: Controla el flujo principal del sistema
- **Reactor Pattern**: Reacciona a eventos (jobs en cola)
- **Async Loop**: Usa `tokio::spawn` para concurrencia no bloqueante
- **Dependency Injection**: Todas las dependencias inyectadas

**⚠️ Consideraciones de Diseño**:
- **Backpressure**: Debe manejar cuando no hay workers disponibles
- **Error Recovery**: Debe recuperar gracefully de errores en el loop
- **Shutdown Graceful**: Debe poder detenerse limpiamente
- **Idempotency**: Procesar el mismo job múltiples veces no debe causar problemas

```rust
pub struct JobController {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    dispatcher: Arc<JobDispatcher>,
    event_bus: Arc<dyn EventBus>,
}

impl JobController {
    pub async fn start(&self) -> Result<()> {
        // Start control loop
        tokio::spawn(async move {
            loop {
                self.process_pending_jobs().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn process_pending_jobs(&self) -> Result<()> {
        let pending = self.job_queue.peek().await?;
        if let Some(job) = pending {
            self.dispatch_job(job).await?;
        }
        Ok(())
    }
}
```

#### 2. JobController

```rust
pub struct JobController {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    dispatcher: Arc<JobDispatcher>,
    event_bus: Arc<dyn EventBus>,
}

impl JobController {
    pub async fn start(&self) -> Result<()> {
        // Start control loop
        tokio::spawn(async move {
            loop {
                self.process_pending_jobs().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn process_pending_jobs(&self) -> Result<()> {
        let pending = self.job_queue.peek().await?;
        if let Some(job) = pending {
            self.dispatch_job(job).await?;
        }
        Ok(())
    }
}
```

#### 3. JobDispatcher

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: asignar jobs a workers según políticas de scheduling.
- **OCP**: Abierto para agregar nuevas estrategias de dispatch sin modificar lógica principal.
- **DIP**: Depende de abstracciones (`WorkerRegistry`, `JobRepository`, `EventBus`), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Application Layer - Cross-Context Coordination**
- Coordina entre Jobs y Workers bounded contexts
- Implementa la lógica de **asignación** de jobs a workers
- Publica eventos para integración con otros contextos

**🔗 Dependencias**:
- **Domain (Workers)**: `WorkerRegistry`
- **Domain (Jobs)**: `JobRepository`
- **Domain (Providers)**: `ProviderRegistry`
- **Domain (Event Bus)**: `EventBus`
- **Scheduling**: Algoritmos de selección de worker

**💡 Motivación**:
Se creó para:
1. **Separar concerns**: Scheduling es lógica compleja que merece su propio componente
2. **Encapsular políticas**: Todas las decisiones de asignación en un solo lugar
3. **Facilitar testing**: Lógica de dispatch puede testearse aisladamente
4. **Publish events**: Notificar al sistema cuando un job es asignado

**🎨 Patrones Aplicados**:
- **Dispatcher Pattern**: Asigna jobs a workers según políticas
- **Strategy Pattern**: Diferentes estrategias de asignación (least loaded, round robin, etc.)
- **Event Publishing**: Notifica cambios de estado al sistema
- **Error Handling**: Maneja casos donde no hay workers disponibles

**⚠️ Consideraciones de Diseño**:
- **Race Conditions**: Debe manejar workers que cambian de estado durante dispatch
- **Backpressure**: Debe manejar cuando no hay workers disponibles
- **Timeouts**: Debe implementar timeouts para dispatch
- **Idempotency**: Dispatch del mismo job multiple veces debe ser idempotente

```rust
pub struct JobDispatcher {
    worker_registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    event_bus: Arc<dyn EventBus>,
    provider_registry: Arc<ProviderRegistry>,
}

impl JobDispatcher {
    pub async fn dispatch(&self, job: Job) -> Result<DispatchResult> {
        // 1. Find available worker
        let worker = self.find_worker(&job).await?;

        // 2. Assign job to worker
        self.assign_job(&worker, &job).await?;

        // 3. Send job to worker via gRPC
        self.send_to_worker(&worker, &job).await?;

        // 4. Publish JobDispatched event
        self.event_bus.publish(JobDispatchedEvent {
            job_id: job.id.clone(),
            worker_id: worker.handle().worker_id.clone(),
        }).await?;

        Ok(DispatchResult::Success)
    }
}
```

### Worker Use Cases (`application/src/workers/`)

#### 1. WorkerProvisioningService

```rust
pub struct WorkerProvisioningService {
    worker_registry: Arc<dyn WorkerRegistry>,
    provider_registry: Arc<ProviderRegistry>,
    otp_store: Arc<dyn OtpTokenStore>,
    event_bus: Arc<dyn EventBus>,
}

impl WorkerProvisioningService {
    pub async fn provision_worker(&self, job: &Job) -> Result<Worker> {
        // 1. Select provider based on job requirements
        let provider = self.select_provider(job).await?;

        // 2. Create worker spec
        let spec = self.create_worker_spec(job, &provider).await?;

        // 3. Generate OTP token
        let token = self.otp_store.generate(
            WorkerId::new(),
            provider.id.clone(),
            Duration::from_secs(300),
        ).await?;

        // 4. Provision worker via provider
        let handle = provider.create_worker(&spec).await?;

        // 5. Register worker
        let worker = self.worker_registry.register(
            handle.clone(),
            spec,
            job.id.clone(),
        ).await?;

        // 6. Publish WorkerProvisioned event
        self.event_bus.publish(WorkerProvisionedEvent {
            worker_id: worker.handle().worker_id.clone(),
            provider_id: provider.id.clone(),
        }).await?;

        Ok(worker)
    }
}
```

#### 2. WorkerSupervisor (Actor Model)

```rust
pub struct WorkerSupervisor {
    workers: Arc<DashMap<WorkerId, WorkerActor>>,
    event_bus: Arc<dyn EventBus>,
    config: WorkerSupervisorConfig,
}

pub struct WorkerActor {
    worker_id: WorkerId,
    state: WorkerState,
    heartbeat_timeout: Duration,
    last_heartbeat: Instant,
}

impl WorkerSupervisor {
    pub async fn handle_heartbeat(&self, worker_id: WorkerId, heartbeat: WorkerHeartbeat) {
        if let Some(actor) = self.workers.get(&worker_id) {
            actor.update_heartbeat(heartbeat).await;
        }
    }

    pub async fn monitor_timeouts(&self) {
        tokio::spawn(async move {
            loop {
                for entry in self.workers.iter() {
                    let actor = entry.value();
                    if actor.is_timeout().await {
                        self.handle_timeout(actor.worker_id.clone()).await;
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
}
```

#### 3. AutoScalingService

```rust
pub struct AutoScalingService {
    worker_registry: Arc<dyn WorkerRegistry>,
    job_queue: Arc<dyn JobQueue>,
    provider_registry: Arc<ProviderRegistry>,
    config: AutoScalingConfig,
}

impl AutoScalingService {
    pub async fn evaluate_scaling(&self) -> Result<ScalingDecision> {
        // 1. Get queue length
        let queue_len = self.job_queue.len().await?;

        // 2. Get available workers
        let available_workers = self.worker_registry.find_available().await?;

        // 3. Calculate desired worker count
        let desired = self.calculate_desired_count(queue_len, available_workers.len());

        // 4. Scale up or down
        if desired > available_workers.len() as i32 {
            self.scale_up((desired - available_workers.len() as i32) as usize).await?;
        } else if desired < available_workers.len() as i32 {
            self.scale_down((available_workers.len() as i32 - desired) as usize).await?;
        }

        Ok(ScalingDecision {
            current: available_workers.len(),
            desired,
        })
    }
}
```

### Saga Use Cases (`application/src/saga/`)

#### 1. ProvisioningSagaCoordinator

```rust
pub struct ProvisioningSagaCoordinator {
    orchestrator: Arc<SagaOrchestrator>,
    worker_provisioning: Arc<WorkerProvisioningService>,
    worker_registry: Arc<dyn WorkerRegistry>,
}

impl ProvisioningSagaCoordinator {
    pub async fn start_provisioning(&self, job_id: JobId) -> Result<SagaId> {
        let saga = Saga::builder()
            .step(GenerateOTPStep::new())
            .step(ProvisionWorkerStep::new())
            .step(WaitForRegistrationStep::new())
            .build();

        self.orchestrator.execute(saga).await
    }
}
```

#### 2. ExecutionSagaDispatcher

```rust
pub struct ExecutionSagaDispatcher {
    orchestrator: Arc<SagaOrchestrator>,
    job_dispatcher: Arc<JobDispatcher>,
    job_repository: Arc<dyn JobRepository>,
}

impl ExecutionSagaDispatcher {
    pub async fn dispatch_job(&self, job: Job) -> Result<SagaId> {
        let saga = Saga::builder()
            .step(AssignWorkerStep::new())
            .step(SendToWorkerStep::new())
            .step(MonitorExecutionStep::new())
            .step(CompleteJobStep::new())
            .build();

        self.orchestrator.execute(saga).await
    }
}
```

### Scheduling Use Cases (`application/src/scheduling/`)

```rust
pub struct SmartScheduler {
    strategies: Vec<Box<dyn SchedulingStrategy>>,
    policy: SchedulingPolicy,
}

impl SmartScheduler {
    pub async fn select_worker(
        &self,
        job: &Job,
        available_workers: &[Worker],
        providers: &[ProviderConfig],
    ) -> Result<Option<WorkerId>> {
        // Apply strategies in priority order
        for strategy in &self.strategies {
            if let Some(worker_id) = strategy.select(job, available_workers, providers).await? {
                return Ok(Some(worker_id));
            }
        }

        Ok(None)
    }
}
```

---

## Infrastructure Layer (Server)

### Purpose

The infrastructure layer provides **concrete implementations** of domain and application interfaces. It handles external concerns like databases, messaging, and providers.

### Persistence (`infrastructure/src/persistence/`)

**🏗️ Ámbito en DDD**: **Infrastructure Layer - Adapters**
- Implementa los **puertos** definidos en la capa de dominio
- Es responsable de la persistencia de datos (PostgreSQL, en memoria, etc.)
- Es un **adapter** que conecta el dominio con tecnologías externas

**📋 Responsabilidad General (SOLID)**:
- **SRP**: Cada repository es responsable de un solo agregado
- **OCP**: Abierto para extender con nuevos tipos de persistencia sin modificar dominio
- **DIP**: Depende de abstracciones de dominio (`JobRepository`, `WorkerRegistry`), no al revés

#### PostgreSQL Repositories

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada repository es responsable de persistir un solo tipo de agregado (`Job` o `Worker`).
- **OCP**: Abierto para agregar nuevos métodos de query sin modificar existentes.
- **DIP**: Implementa abstracciones de dominio, depende de ellas, no las crea.

**🏗️ Ámbito en DDD**: **Infrastructure Layer - Persistence Adapter**
- Son implementaciones concretas de los **puertos** definidos en dominio
- Conectan el dominio puro con la tecnología de base de datos (PostgreSQL)
- Mapean tipos de dominio a esquemas de base de datos

**🔗 Dependencias**:
- **Domain**: Implementa `JobRepository`, `WorkerRegistry` (traits de dominio)
- **External**: `sqlx` (PostgreSQL async client), `PgPool`
- **Serialization**: `serde_json` para serializar structs complejos a JSON
- **Concurrency**: `async/await` para operaciones no bloqueantes

**💡 Motivación**:
Se crearon para:
1. **Separar dominio de infraestructura**: El dominio no sabe de SQL
2. **Facilitar testing**: Puedo usar implementaciones en memoria para tests
3. **Performance**: Aprovechar PostgreSQL features (indexes, transactions, etc.)
4. **Type Safety**: SQLx genera queries compiladas con type checking

**🎨 Patrones Aplicados**:
- **Repository Pattern**: Abstrae el almacenamiento y recuperación de objetos de dominio
- **Adapter Pattern**: Conecta interfaces de dominio con tecnología externa
- **Data Mapper**: Mapea tipos de dominio a esquema de base de datos
- **Connection Pooling**: `PgPool` para reuso eficiente de conexiones

**⚠️ Consideraciones de Diseño**:
- **Upsert**: `ON CONFLICT DO UPDATE` para idempotencia
- **Async Operations**: Todas las operaciones son asíncronas
- **Error Handling**: Convierte `sqlx::Error` a `Result<T>` del dominio
- **JSON Columns**: Usa `JSONB` para tipos complejos (JobSpec, WorkerSpec)
- **Indexes**: Queries de `find_by_id` usan primary key, `find_available` usa index en `state`

**🔌 Integration con Domain**:
```mermaid
graph LR
    A[Application Layer] -->|Uses| B[JobRepository Trait]
    B -->|Implemented by| C[PostgresJobRepository]
    C -->|Persists to| D[(PostgreSQL)]
    
    E[Application Layer] -->|Uses| F[WorkerRegistry Trait]
    F -->|Implemented by| G[PostgresWorkerRegistry]
    G -->|Persists to| D
```

```rust
// Job Repository
pub struct PostgresJobRepository {
    pool: PgPool,
}

#[async_trait]
impl JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO jobs (id, spec, state, selected_provider, attempts, max_attempts, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                state = EXCLUDED.state,
                updated_at = EXCLUDED.updated_at
            "#,
            job.id.as_uuid(),
            serde_json::to_value(&job.spec)?,
            job.state as i32,
            job.selected_provider.as_ref().map(|p| p.as_uuid()),
            job.attempts as i32,
            job.max_attempts as i32,
            job.created_at,
            job.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>> {
        let row = sqlx::query_as!(
            JobRow,
            "SELECT * FROM jobs WHERE id = $1",
            id.as_uuid()
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| self.row_to_job(row)?))
    }
}

// Worker Registry
pub struct PostgresWorkerRegistry {
    pool: PgPool,
}

#[async_trait]
impl WorkerRegistry for PostgresWorkerRegistry {
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec, job_id: JobId) -> Result<Worker> {
        let worker = Worker::new(handle.clone(), spec);
        
        sqlx::query!(
            r#"
            INSERT INTO workers (worker_id, handle, spec, state, current_job_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            handle.worker_id.as_uuid(),
            serde_json::to_value(&handle)?,
            serde_json::to_value(&spec)?,
            WorkerState::Creating as i32,
            job_id.as_uuid(),
            Utc::now(),
            Utc::now(),
        )
        .execute(&self.pool)
        .await?;

        Ok(worker)
    }

    async fn find_available(&self) -> Result<Vec<Worker>> {
        let rows = sqlx::query_as!(
            WorkerRow,
            "SELECT * FROM workers WHERE state = $1 OR state = $2",
            WorkerState::Ready as i32,
            WorkerState::Idle as i32,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter()
            .map(|row| self.row_to_worker(row))
            .collect::<Result<Vec<_>>>()?)
    }
}
```

#### Job Queue

```rust
pub struct PostgresJobQueue {
    pool: PgPool,
}

#[async_trait]
impl JobQueue for PostgresJobQueue {
    async fn enqueue(&self, job: Job) -> Result<()> {
        sqlx::query!(
            "INSERT INTO job_queue (job_id, priority, created_at) VALUES ($1, $2, $3)",
            job.id.as_uuid(),
            job.spec.preferences.priority as i32,
            Utc::now(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>> {
        // Use SKIP LOCKED for concurrent dequeues
        let row = sqlx::query_as!(
            JobQueueRow,
            r#"
            DELETE FROM job_queue
            WHERE job_id = (
                SELECT job_id FROM job_queue
                ORDER BY priority DESC, created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING *
            "#
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(queue_row) = row {
            let job = self.job_repo.find_by_id(&JobId(queue_row.job_id)).await?;
            Ok(job)
        } else {
            Ok(None)
        }
    }
}
```

#### Outbox Pattern

```rust
pub struct PostgresOutboxRepository {
    pool: PgPool,
}

#[async_trait]
impl OutboxRepository for PostgresOutboxRepository {
    async fn save(&self, event: &DomainEvent) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO outbox (event_id, event_type, payload, aggregate_id, aggregate_type, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            Uuid::new_v4(),
            event.event_type(),
            serde_json::to_value(event)?,
            event.aggregate_id(),
            event.aggregate_type(),
            Utc::now(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_unpublished(&self, limit: usize) -> Result<Vec<OutboxEntry>> {
        let rows = sqlx::query_as!(
            OutboxEntryRow,
            "SELECT * FROM outbox WHERE published = FALSE ORDER BY created_at ASC LIMIT $1",
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| self.row_to_entry(row)).collect())
    }

    async fn mark_published(&self, event_id: Uuid) -> Result<()> {
        sqlx::query!(
            "UPDATE outbox SET published = TRUE, published_at = $1 WHERE event_id = $2",
            Utc::now(),
            event_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
```

### Messaging (`infrastructure/src/messaging/`)

#### PostgreSQL Event Bus

```rust
pub struct PostgresEventBus {
    outbox: Arc<dyn OutboxRepository>,
    poller: OutboxPoller,
}

impl PostgresEventBus {
    pub async fn start(&self) -> Result<()> {
        self.poller.start().await
    }
}

#[async_trait]
impl EventBus for PostgresEventBus {
    async fn publish<E>(&self, event: E) -> Result<()>
    where
        E: DomainEvent + Send + Sync + 'static,
    {
        self.outbox.save(&event).await
    }

    async fn subscribe<E, H>(&self, handler: H) -> Result<Subscription>
    where
        E: DomainEvent + Send + Sync + 'static,
        H: EventHandler<E> + Send + Sync + 'static,
    {
        self.poller.add_handler(event_type_name::<E>(), Box::new(handler)).await
    }
}
```

#### Outbox Poller

```rust
pub struct OutboxPoller {
    outbox: Arc<dyn OutboxRepository>,
    handlers: HashMap<String, Vec<Box<dyn EventHandlerDyn>>>,
    config: PollerConfig,
}

impl OutboxPoller {
    pub async fn start(&self) -> Result<()> {
        loop {
            let events = self.outbox.find_unpublished(self.config.batch_size).await?;
            
            for event in events {
                self.handle_event(event).await?;
            }

            tokio::time::sleep(self.config.poll_interval).await;
        }
    }

    async fn handle_event(&self, event: OutboxEntry) -> Result<()> {
        if let Some(handlers) = self.handlers.get(&event.event_type) {
            for handler in handlers {
                handler.handle(&event.payload).await?;
            }
        }

        self.outbox.mark_published(event.event_id).await
    }
}
```

### Worker Providers (`infrastructure/src/providers/`)

#### Docker Provider

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: proveer workers usando Docker containers.
- **OCP**: Abierto para extender (nuevas configuraciones) sin modificar lógica principal.
- **LSP**: `DockerProvider` es substituible por cualquier implementación de `WorkerProvider`.
- **ISP**: Solo implementa métodos relevantes del trait `WorkerProvider`.

**🏗️ Ámbito en DDD**: **Infrastructure Layer - Provider Adapter**
- Es un **adapter** que implementa el puerto `WorkerProvider` definido en dominio
- Conecta el dominio con la tecnología Docker
- No contiene lógica de negocio, solo orquestación de Docker API

**🔗 Dependencias**:
- **Domain**: Implementa `WorkerProvider` trait
- **Domain Types**: `WorkerSpec`, `WorkerHandle`, `ProviderId`, `ProviderType`
- **External**: `bollard::Docker` (Docker HTTP API client)
- **Configuration**: `DockerConfig`

**💡 Motivación**:
Se creó para:
1. **Abstraer Docker**: Permitir cambiar de proveedor sin afectar dominio
2. **Production Ready**: Docker es estable y ampliamente usado
3. **Development Friendly**: Fácil de usar en local y CI/CD
4. **Fast Startup**: Containers inician en ~1 segundo

**🎨 Patrones Aplicados**:
- **Adapter Pattern**: Adapta Docker API a interface de dominio
- **Factory Pattern**: (implícito) Crea containers desde specs
- **Resource Management**: Maneja lifecycle de containers (create, start, remove)
- **Error Translation**: Convierte `bollard::Error` a `ProviderError`

**⚠️ Consideraciones de Diseño**:
- **Container Naming**: Usa naming convention para identificación
- **Resource Limits**: Debe respetar specs de recursos del worker
- **Cleanup**: Debe remover containers siempre, incluso en errores
- **Network**: Configura networking para worker pueda comunicarse con servidor
- **Labels**: Usa labels para tracking y filtering

**🔌 Docker Integration Flow**:
```mermaid
graph LR
    A[Application Layer] -->|WorkerProvider trait| B[DockerProvider]
    B -->|HTTP API| C[Docker Daemon]
    C -->|Creates| D[Container]
    D -->|Returns| B
    B -->|WorkerHandle| A
```

```rust
pub struct DockerProvider {
    client: Docker,
    config: DockerConfig,
    provider_id: ProviderId,
}

#[async_trait]
impl WorkerProvider for DockerProvider {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        let config = CreateContainerOptions {
            name: format!("hodei-worker-{}", spec.id),
            env: self.build_environment(spec),
            labels: spec.labels.clone(),
            ..Default::default()
        };

        let container = self.client
            .create_container(Some(config), self.build_container_config(spec))
            .await?;

        self.client.start_container(&container.id, None).await?;

        Ok(WorkerHandle {
            worker_id: WorkerId::new(),
            provider_type: ProviderType::Docker,
            provider_execution_id: container.id,
            connection_id: None,
        })
    }

    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        self.client
            .remove_container(
                &handle.provider_execution_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        Ok(())
    }

    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        self.client.ping().await?;
        Ok(HealthStatus::Healthy)
    }
}
```

---

#### Kubernetes Provider

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: proveer workers usando Kubernetes pods.
- **OCP**: Abierto para extender (nuevos recursos K8s) sin modificar lógica principal.
- **LSP**: `KubernetesProvider` es substituible por cualquier implementación de `WorkerProvider`.
- **ISP**: Solo implementa métodos relevantes del trait `WorkerProvider`.

**🏗️ Ámbito en DDD**: **Infrastructure Layer - Provider Adapter**
- Es un **adapter** que implementa el puerto `WorkerProvider` definido en dominio
- Conecta el dominio con Kubernetes API
- No contiene lógica de negocio, solo orquestación de K8s API

**🔗 Dependencias**:
- **Domain**: Implementa `WorkerProvider` trait
- **Domain Types**: `WorkerSpec`, `WorkerHandle`, `ProviderId`, `ProviderType`
- **External**: `kube::Client` (Kubernetes Rust client)
- **Configuration**: `KubernetesConfig` (namespace, image pull secrets, etc.)

**💡 Motivación**:
Se creó para:
1. **Production Grade**: K8s es estándar para orquestación de containers
2. **Auto-scaling**: K8s tiene autoscaling nativo (HPA)
3. **Cloud Native**: Facil integración con cloud providers (GKE, EKS, AKS)
4. **Multi-tenant**: Namespaces para isolation entre tenants

**🎨 Patrones Aplicados**:
- **Adapter Pattern**: Adapta K8s API a interface de dominio
- **Builder Pattern**: (implícito) Construye pods specs desde worker specs
- **Resource Management**: Maneja lifecycle de pods (create, delete, get status)
- **State Mapping**: Mapea pod phases a WorkerState

**⚠️ Consideraciones de Diseño**:
- **Pod Naming**: Usa naming convention consistente
- **Resource Requests/Limits**: Debe respetar specs de recursos
- **Namespace**: Operar en namespace aislado para workers
- **Image Pull Secrets**: Maneja private registries
- **Probes**: Configurar liveness/readiness probes
- **Cleanup**: Debe remover pods siempre, incluso en errores

**🔌 Kubernetes Integration Flow**:
```mermaid
graph LR
    A[Application Layer] -->|WorkerProvider trait| B[KubernetesProvider]
    B -->|K8s API| C[Kubernetes Cluster]
    C -->|Creates| D[Pod]
    D -->|Returns| B
    B -->|WorkerHandle| A
```

```rust
pub struct KubernetesProvider {
    client: Client,
    config: KubernetesConfig,
    provider_id: ProviderId,
}

#[async_trait]
impl WorkerProvider for KubernetesProvider {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        let pod = self.build_pod(spec);

        let created_pod: Pod = self.client
            .create(&self.config.namespace, &PostParams::default(), &pod)
            .await?;

        Ok(WorkerHandle {
            worker_id: WorkerId::new(),
            provider_type: ProviderType::Kubernetes,
            provider_execution_id: created_pod.metadata.name.unwrap(),
            connection_id: None,
        })
    }

    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        self.client
            .delete(
                &self.config.namespace,
                &handle.provider_execution_id,
                &DeleteParams::default(),
            )
            .await?;

        Ok(())
    }

    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError> {
        let pod: Pod = self.client
            .get(&self.config.namespace, &handle.provider_execution_id)
            .await?;

        let phase = pod.status.unwrap().phase.unwrap();
        
        Ok(match phase.as_str() {
            "Pending" => WorkerState::Creating,
            "Running" => WorkerState::Ready,
            "Succeeded" => WorkerState::Terminated,
            "Failed" => WorkerState::Terminated,
            _ => WorkerState::Creating,
        })
    }
}
```

#### Kubernetes Provider

```rust
pub struct KubernetesProvider {
    client: Client,
    config: KubernetesConfig,
    provider_id: ProviderId,
}

#[async_trait]
impl WorkerProvider for KubernetesProvider {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        let pod = self.build_pod(spec);

        let created_pod: Pod = self.client
            .create(&self.config.namespace, &PostParams::default(), &pod)
            .await?;

        Ok(WorkerHandle {
            worker_id: WorkerId::new(),
            provider_type: ProviderType::Kubernetes,
            provider_execution_id: created_pod.metadata.name.unwrap(),
            connection_id: None,
        })
    }

    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        self.client
            .delete(
                &self.config.namespace,
                &handle.provider_execution_id,
                &DeleteParams::default(),
            )
            .await?;

        Ok(())
    }

    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError> {
        let pod: Pod = self.client
            .get(&self.config.namespace, &handle.provider_execution_id)
            .await?;

        let phase = pod.status.unwrap().phase.unwrap();
        
        Ok(match phase.as_str() {
            "Pending" => WorkerState::Creating,
            "Running" => WorkerState::Ready,
            "Succeeded" => WorkerState::Terminated,
            "Failed" => WorkerState::Terminated,
            _ => WorkerState::Creating,
        })
    }
}
```

---

## Interface Layer (Server)

### Purpose

The interface layer provides **APIs for external clients** (gRPC, REST, WebSocket).

### gRPC Services (`interface/src/grpc/`)

**🏗️ Ámbito en DDD**: **Interface Layer - Adapters**
- Son **adapters** que exponen la API del sistema al mundo exterior
- Traducen entre protocolos externos (gRPC) y tipos de dominio
- No contienen lógica de negocio, solo orquestación y mapeo

**📋 Responsabilidad General (SOLID)**:
- **SRP**: Cada servicio es responsable de un bounded context (Jobs, Workers, etc.)
- **OCP**: Abierto para agregar nuevos endpoints sin modificar existentes
- **DIP**: Depende de use cases y abstracciones, no de implementaciones concretas

#### JobExecutionService

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: exponer API gRPC para operaciones de jobs.
- **OCP**: Abierto para agregar nuevos métodos de API sin modificar existentes.
- **LSP**: Cualquier implementación de `JobExecutionService` es substituible.
- **ISP**: Clientes dependen solo de los métodos que usan.

**🏗️ Ámbito en DDD**: **Interface Layer - Adapter**
- Es un **adapter** que implementa el contrato gRPC `JobExecutionService`
- Traduce entre protocol buffers (protobuf) y tipos de dominio
- Delega lógica de negocio a use cases

**🔗 Dependencias**:
- **Application**: `CreateJobUseCase`, `CancelJobUseCase`, `GetJobStatusUseCase`
- **Domain (Event Bus)**: `EventBus`
- **External**: `tonic::Request`, `tonic::Response`, `tonic::Status`
- **Concurrency**: `Arc<T>` para compartir estado entre threads

**💡 Motivación**:
Se creó para:
1. **Exponer API gRPC**: Protocolo eficiente para comunicación cliente-servidor
2. **Traducir tipos**: Convertir entre protobuf y tipos de dominio
3. **Delegar lógica**: No contiene lógica de negocio, solo orquestación
4. **Error Handling**: Traducir errores de dominio a gRPC status codes

**🎨 Patrones Aplicados**:
- **Adapter Pattern**: Adapta API gRPC a use cases de aplicación
- **Mapper Pattern**: Convierte entre tipos de dominio y protobuf
- **Dependency Injection**: Recibe use cases por constructor
- **Error Translation**: Convierte `Result<T>` a gRPC `Status`

**⚠️ Consideraciones de Diseño**:
- **Validation**: Valida requests en la capa de interface (no en dominio)
- **Error Codes**: Mapea errores de dominio a gRPC status codes apropiados
- **Idempotency**: APIs deberían ser idempotentes cuando sea posible
- **Backpressure**: Debe manejar muchas solicitudes concurrentes
- **Context Propagation**: Debe propagar trace ID entre llamadas

**🔌 gRPC Integration Flow**:
```mermaid
graph LR
    A[gRPC Client] -->|protobuf| B[JobExecutionServiceImpl]
    B -->|CreateJobCommand| C[CreateJobUseCase]
    C -->|Job| D[Domain Layer]
    C -->|JobCreatedEvent| E[Event Bus]
    B -->|protobuf Response| A
```

```rust
pub struct JobExecutionServiceImpl {
    create_job_uc: Arc<CreateJobUseCase>,
    cancel_job_uc: Arc<CancelJobUseCase>,
    get_job_status_uc: Arc<GetJobStatusUseCase>,
    event_bus: Arc<dyn EventBus>,
}

#[async_trait]
impl JobExecutionService for JobExecutionServiceImpl {
    async fn queue_job(
        &self,
        request: Request<QueueJobRequest>,
    ) -> Result<Response<QueueJobResponse>, Status> {
        let req = request.into_inner();
        
        let cmd = CreateJobCommand::from(req);
        let job = self.create_job_uc.execute(cmd).await
            .map_err(|e| Status::internal(e.to_string()))?;
        
        Ok(Response::new(QueueJobResponse {
            job_id: Some(job.id.into()),
            status: job.state as i32,
            created_at: Some(job.created_at.into()),
        }))
    }

    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> Result<Response<GetJobStatusResponse>, Status> {
        let req = request.into_inner();
        let job_id = JobId::try_from(req.job_id.ok_or_else(|| Status::invalid_argument("job_id required"))?)?;
        
        let job = self.get_job_status_uc.execute(job_id).await
            .map_err(|e| Status::not_found(e.to_string()))?;
        
        Ok(Response::new(GetJobStatusResponse {
            job_id: Some(job.id.into()),
            status: job.state as i32,
            spec: Some(job.spec.into()),
            execution_context: Some(job.execution_context.into()),
            created_at: Some(job.created_at.into()),
            updated_at: Some(job.updated_at.into()),
        }))
    }
}
```

---

#### WorkerAgentService

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: exponer API gRPC para comunicación worker-servidor.
- **OCP**: Abierto para agregar nuevos métodos de worker sin modificar existentes.
- **LSP**: Cualquier implementación de `WorkerAgentService` es substituible.
- **ISP**: Métodos específicos para worker registration y streaming.

**🏗️ Ámbito en DDD**: **Interface Layer - Adapter**
- Es un **adapter** que implementa el contrato gRPC `WorkerAgentService`
- Maneja registro de workers con OTP authentication
- Administra streaming bidireccional para comunicación continua

**🔗 Dependencias**:
- **Domain (Workers)**: `WorkerRegistry`
- **Domain (IAM)**: `OtpTokenStore`
- **Domain (Event Bus)**: `EventBus`
- **Infrastructure**: `GrpcWorkerCommandSender`
- **External**: `tonic::Request`, `tonic::Response`, `tokio::sync::mpsc`

**💡 Motivación**:
Se creó para:
1. **Exponer API workers**: Protocolo seguro para worker registration
2. **OTP Authentication**: Implementar autenticación one-time password
3. **Bidirectional Streaming**: Permitir comunicación continua server↔worker
4. **Backpressure**: Manejar flujos de alta velocidad con canales limitados

**🎨 Patrones Aplicados**:
- **Adapter Pattern**: Adapta API gRPC a use cases de workers
- **Authentication Pattern**: OTP para secure worker registration
- **Streaming Pattern**: Bidirectional stream para comunicación continua
- **Backpressure Handling**: Canales `mpsc` con capacidad limitada
- **Event Publishing**: Publica eventos de worker registration

**⚠️ Consideraciones de Diseño**:
- **OTP Security**: Tokens de un solo uso, expiran rápidamente
- **Stream Management**: Debe manejar desconexiones y reconexiones
- **Backpressure**: Debe aplicar backpressure cuando canal está lleno
- **Error Recovery**: Debe recuperar gracefully de errores de streaming
- **Heartbeat**: Implementar heartbeat para detectar workers muertos

**🔌 Worker Integration Flow**:
```mermaid
graph TB
    subgraph "Worker Agent"
        W[Worker Agent]
        OTP[OTP Token]
        W -->|Register with OTP| WS[WorkerAgentService]
    end
    
    subgraph "Server"
        WS -->|Validate OTP| OTS[OtpTokenStore]
        WS -->|Register| WR[WorkerRegistry]
        WS -->|Publish Event| EB[Event Bus]
    end
    
    subgraph "Streaming"
        WS <-->|Bidirectional Stream| W
        WS -->|Send Commands| W
        W -->|Send Heartbeats, Logs| WS
    end
```

```rust
pub struct WorkerAgentServiceImpl {
    worker_registry: Arc<dyn WorkerRegistry>,
    otp_store: Arc<dyn OtpTokenStore>,
    event_bus: Arc<dyn EventBus>,
    command_sender: Arc<GrpcWorkerCommandSender>,
}

#[async_trait]
impl WorkerAgentService for WorkerAgentServiceImpl {
    async fn register(
        &self,
        request: Request<RegisterWorkerRequest>,
    ) -> Result<Response<RegisterWorkerResponse>, Status> {
        let req = request.into_inner();
        
        // Validate OTP
        let registration = self.otp_store.validate(&OtpToken(req.auth_token)).await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        
        // Register worker
        let worker = self.worker_registry.register(
            WorkerHandle::from(req.worker_handle.ok_or_else(|| Status::invalid_argument("worker_handle required"))?),
            WorkerSpec::from(req.worker_spec.ok_or_else(|| Status::invalid_argument("worker_spec required"))?),
            registration.job_id,
        ).await
        .map_err(|e| Status::internal(e.to_string()))?;
        
        // Publish event
        self.event_bus.publish(WorkerRegisteredEvent {
            worker_id: worker.handle().worker_id.clone(),
            provider_id: worker.spec().provider_id.clone(),
        }).await;
        
        // Consume OTP
        self.otp_store.consume(&OtpToken(req.auth_token)).await;
        
        Ok(Response::new(RegisterWorkerResponse {
            session_id: Some(worker.handle().worker_id.into()),
            server_config: Some(ServerConfig::default()),
        }))
    }

    type WorkerStreamStream = ServerStreaming<ServerMessage>;
    
    async fn worker_stream(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::WorkerStreamStream>, Status> {
        // Implementación de streaming bidireccional
    }
}
```

#### WorkerAgentService

```rust
pub struct WorkerAgentServiceImpl {
    worker_registry: Arc<dyn WorkerRegistry>,
    otp_store: Arc<dyn OtpTokenStore>,
    event_bus: Arc<dyn EventBus>,
    command_sender: Arc<GrpcWorkerCommandSender>,
}

#[async_trait]
impl WorkerAgentService for WorkerAgentServiceImpl {
    async fn register(
        &self,
        request: Request<RegisterWorkerRequest>,
    ) -> Result<Response<RegisterWorkerResponse>, Status> {
        let req = request.into_inner();

        // Validate OTP
        let registration = self.otp_store.validate(&OtpToken(req.auth_token)).await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;

        // Register worker
        let worker = self.worker_registry.register(
            WorkerHandle::from(req.worker_handle.ok_or_else(|| Status::invalid_argument("worker_handle required"))?),
            WorkerSpec::from(req.worker_spec.ok_or_else(|| Status::invalid_argument("worker_spec required"))?),
            registration.job_id,
        ).await
        .map_err(|e| Status::internal(e.to_string()))?;

        // Publish event
        self.event_bus.publish(WorkerRegisteredEvent {
            worker_id: worker.handle().worker_id.clone(),
            provider_id: worker.spec().provider_id.clone(),
        }).await;

        // Consume OTP
        self.otp_store.consume(&OtpToken(req.auth_token)).await;

        Ok(Response::new(RegisterWorkerResponse {
            session_id: Some(worker.handle().worker_id.into()),
            server_config: Some(ServerConfig::default()),
        }))
    }

    type WorkerStreamStream = ServerStreaming<ServerMessage>;

    async fn worker_stream(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::WorkerStreamStream>, Status> {
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(100);

        // Handle inbound messages
        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                match msg {
                    Ok(msg) => self.handle_worker_message(msg, tx.clone()).await,
                    Err(e) => error!("Worker message error: {}", e),
                }
            }
        });

        Ok(Response::new(ServerStreaming::new(rx)))
    }
}
```

### Log Streaming

```rust
pub struct LogStreamService {
    log_storage: Arc<dyn LogStorage>,
}

impl LogStreamService {
    pub async fn subscribe_logs(&self, job_id: JobId) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(1000);

        let storage = self.log_storage.clone();
        tokio::spawn(async move {
            while let Some(log) = storage.next_log(job_id).await {
                if tx.send(log).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }
}
```

---

## Worker Architecture

### Purpose

The worker agent executes jobs on behalf of the control plane and reports results.

### Components

```mermaid
graph TB
    subgraph "Worker Agent"
        MAIN[Main Loop]
        CLIENT[gRPC Client]
        REG[Registration]
        STREAM[Bidirectional Stream]
        EXEC[Job Executor]
        LOG[Log Batcher]
        METRICS[Metrics Collector]
        CERT[Certificate Manager]
    end

    subgraph "Execution"
        CMD[Command Runner]
        SCRIPT[Script Executor]
        SECRET[Secret Injector]
    end

    MAIN --> CLIENT
    CLIENT --> REG
    REG --> STREAM
    STREAM <--> EXEC
    EXEC --> CMD
    EXEC --> SCRIPT
    EXEC --> SECRET
    EXEC --> LOG
    EXEC --> METRICS
    CLIENT --> CERT
```

### Worker Client

```rust
pub struct WorkerClient {
    client: WorkerAgentServiceClient<Channel>,
    session_id: String,
    stream_tx: mpsc::Sender<WorkerMessage>,
    log_batcher: Arc<LogBatcher>,
}

impl WorkerClient {
    pub async fn register(&self, otp: OtpToken, worker_info: WorkerInfo) -> Result<Session> {
        let request = RegisterWorkerRequest {
            auth_token: otp.0,
            worker_handle: Some(WorkerHandle::default()),
            worker_spec: Some(WorkerSpec::default()),
        };

        let response = self.client.register(request).await?;
        let session = Session {
            id: response.into_inner().session_id.unwrap().value,
        };

        Ok(session)
    }

    pub async fn start_stream(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel(100);
        self.stream_tx = tx;

        let mut client_stream = self.client.worker_stream(Request::new(tokio_stream::wrappers::ReceiverStream::new(rx))).await?.into_inner();

        // Handle server messages
        tokio::spawn(async move {
            while let Some(msg) = client_stream.next().await {
                match msg {
                    Ok(server_msg) => self.handle_server_message(server_msg).await,
                    Err(e) => error!("Server message error: {}", e),
                }
            }
        });

        Ok(())
    }

    async fn send_heartbeat(&self, metrics: ResourceUsage) {
        let msg = WorkerMessage {
            message: Some(worker_message::Message::Heartbeat(WorkerHeartbeat {
                metrics: Some(metrics.into()),
                timestamp: Some(Utc::now().into()),
            })),
        };

        let _ = self.stream_tx.try_send(msg);
    }
}
```

### Log Batcher (Optimization v8.0)

```rust
pub struct LogBatcher {
    tx: mpsc::Sender<WorkerMessage>,
    buffer: Vec<LogEntry>,
    capacity: usize,
    flush_interval: Duration,
    last_flush: Instant,
}

impl LogBatcher {
    pub async fn add(&mut self, entry: LogEntry) -> Result<()> {
        self.buffer.push(entry);

        // Flush if buffer full
        if self.buffer.len() >= self.capacity {
            self.flush().await?;
        }

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let batch = LogBatch {
            entries: self.buffer.clone(),
            timestamp: Some(Utc::now().into()),
        };

        let msg = WorkerMessage {
            message: Some(worker_message::Message::LogBatch(batch)),
        };

        // Non-blocking send with backpressure
        let _ = self.tx.try_send(msg);

        self.buffer.clear();
        self.last_flush = Instant::now();

        Ok(())
    }

    pub async fn start_flush_timer(&mut self) {
        let tx = self.tx.clone();
        let mut interval = tokio::time::interval(self.flush_interval);

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                // Timer-based flush
            }
        });
    }
}
```

### Job Executor

```rust
pub struct JobExecutor {
    script_dir: PathBuf,
    log_batcher: Arc<LogBatcher>,
    metrics_collector: Arc<MetricsCollector>,
}

impl JobExecutor {
    pub async fn execute_job(&self, command: RunJobCommand) -> Result<JobResult> {
        match command.command_type {
            CommandType::Shell { cmd, args } => self.execute_shell(cmd, args, command).await,
            CommandType::Script { interpreter, content } => self.execute_script(interpreter, content, command).await,
        }
    }

    async fn execute_script(
        &self,
        interpreter: String,
        content: String,
        command: RunJobCommand,
    ) -> Result<JobResult> {
        // 1. Create temporary script file
        let script_path = self.create_temp_script(&interpreter, &content).await?;

        // 2. Inject secrets via stdin
        if let Some(secrets) = &command.secrets {
            self.inject_secrets(secrets).await?;
        }

        // 3. Execute script
        let mut child = Command::new(&interpreter)
            .arg(&script_path)
            .envs(&command.environment)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // 4. Stream logs in real-time
        self.stream_logs(&mut child, &command.job_id).await?;

        // 5. Wait for completion
        let status = child.wait().await?;

        // 6. Clean up temp file asynchronously
        let path_clone = script_path.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = tokio::fs::remove_file(path_clone).await;
        });

        Ok(JobResult {
            success: status.success(),
            exit_code: status.code().unwrap_or(-1),
            error_message: None,
        })
    }

    async fn inject_secrets(&self, secrets: &HashMap<String, String>) -> Result<()> {
        let json = serde_json::to_string(secrets)?;
        // Write to stdin (implementation depends on context)
        Ok(())
    }
}
```

### Metrics Collector (Optimization v8.0)

```rust
pub struct MetricsCollector {
    cache: Arc<Mutex<CachedResourceUsage>>,
    cache_ttl: Duration,
}

struct CachedResourceUsage {
    usage: ResourceUsage,
    timestamp: Instant,
}

impl MetricsCollector {
    pub async fn get_usage(&self) -> Result<ResourceUsage> {
        let mut cache = self.cache.lock().await;

        // Check cache validity
        if cache.timestamp.elapsed() < self.cache_ttl {
            return Ok(cache.usage.clone());
        }

        // Collect fresh metrics in blocking task
        let usage = tokio::task::spawn_blocking({
            let provider = self.provider.clone();
            move || {
                // Read from cgroups
                provider.collect_metrics()
            }
        }).await??;

        // Update cache
        cache.usage = usage.clone();
        cache.timestamp = Instant::now();

        Ok(usage)
    }
}
```

---

## Bounded Contexts

### Overview

The platform is organized into **9 bounded contexts**, each with its own domain logic, aggregates, and events:

| Context | Responsibility | Key Aggregates | Key Events |
|---------|----------------|----------------|------------|
| **Jobs** | Job lifecycle | Job, JobSpec, JobQueue | JobCreated, JobCompleted, JobFailed |
| **Workers** | Worker lifecycle | Worker, WorkerSpec, WorkerHandle | WorkerRegistered, WorkerTerminated |
| **Providers** | Provider management | ProviderConfig | ProviderRegistered, ProviderHealthChanged |
| **Scheduling** | Job-to-worker assignment | Scheduler | SchedulingDecision |
| **Templates** | Job templates | JobTemplate | TemplateCreated, TemplateUpdated |
| **IAM** | Authentication & authorization | BootstrapToken | TokenGenerated, TokenConsumed |
| **Audit** | Audit logging | AuditLog | AuditEventLogged |
| **Credentials** | Secret management | Secret | SecretCreated, SecretAccessed |
| **Saga** | Distributed transactions | Saga | SagaStarted, SagaCompleted, SagaCompensated |

### Context Maps

```mermaid
graph LR
    JOBS[Jobs Context]
    WORKERS[Workers Context]
    PROVIDERS[Providers Context]
    SCHEDULING[Scheduling Context]
    TEMPLATES[Templates Context]
    IAM[IAM Context]
    AUDIT[Audit Context]
    CREDENTIALS[Credentials Context]
    SAGA[Saga Context]

    JOBS -.-> WORKERS
    JOBS -.-> SCHEDULING
    JOBS -.-> TEMPLATES
    JOBS -.-> SAGA

    WORKERS -.-> PROVIDERS
    WORKERS -.-> IAM
    WORKERS -.-> SAGA

    SCHEDULING -.-> WORKERS
    SCHEDULING -.-> PROVIDERS

    TEMPLATES -.-> JOBS
    CREDENTIALS -.-> JOBS

    JOBS -.-> AUDIT
    WORKERS -.-> AUDIT
    PROVIDERS -.-> AUDIT
```

---

## Domain Events Architecture

### Problem

The original `events.rs` contained a **monolithic enum** with 71+ variants mixing 5 bounded contexts:

- **Job**: ~13 variants
- **Worker**: ~19 variants
- **Provider**: ~6 variants
- **Scheduling**: ~5 variants
- **Template**: ~9 variants

### Impact

- **Violates SRP**: One file handles multiple domains
- **Coupling**: Changes in one context affect the entire file
- **Maintenance Difficulty**: ~1870 lines in one file
- **Compile Times**: Small changes recompile everything

### Proposed Structure

```
domain/src/
├── events/
│   ├── mod.rs              # Re-exports and backward compatibility
│   ├── job.rs              # JobCreated, JobStatusChanged, etc.
│   ├── worker.rs           # WorkerRegistered, WorkerStatusChanged, etc.
│   ├── provider.rs         # ProviderRegistered, ProviderHealthChanged, etc.
│   ├── scheduling.rs       # Scheduling events
│   └── template.rs         # Template events
```

### Migration Strategy

**Phase 1: Aliasing Strategy**
```rust
// In jobs/events/mod.rs
pub use crate::events::JobCreated as JobCreatedEvent;
pub use crate::events::JobStatusChanged as JobStatusChangedEvent;
```

**Phase 2: Refactoring Gradual**
1. Create separate structs in own modules
2. Keep `DomainEvent` enum with variants using new structs
3. Update imports progressively
4. Compatibility tests at each step

**Status**: Documented, separation pending

---

## Job Execution Flow

### Complete Flow

```mermaid
sequenceDiagram
    participant C as Client
    participant G as gRPC Server
    participant A as Application Layer
    participant D as Domain Layer
    participant I as Infrastructure
    participant P as Provider
    participant W as Worker Agent

    Note over C,W: 1. Job Submission
    C->>G: QueueJob(JobSpec)
    G->>A: CreateJobUseCase.execute()
    A->>D: Job::new(spec)
    D-->>A: Job aggregate
    A->>I: JobRepository.save()
    A->>I: JobQueue.enqueue()
    A-->>G: Job created
    G-->>C: QueueJobResponse(job_id)

    Note over C,W: 2. Worker Provisioning
    A->>D: Check available workers
    D-->>A: No workers available
    A->>A: WorkerProvisioningService.provision()
    A->>D: OtpTokenStore.generate()
    D-->>A: BootstrapToken
    A->>P: create_worker(spec, otp)
    P->>P: Start container/pod
    P-->>A: WorkerHandle

    Note over C,W: 3. Worker Registration (OTP)
    W->>G: Register(otp, worker_info)
    G->>A: ValidateOTP
    A->>D: OtpTokenStore.validate()
    D-->>A: WorkerRegistration
    A->>I: WorkerRegistry.register()
    A->>D: Publish WorkerRegisteredEvent
    A-->>W: RegisterResponse(session_id)

    Note over C,W: 4. Job Assignment
    A->>D: Find available worker
    D-->>A: Worker
    A->>D: AssignJobToWorker
    D-->>A: Job state → Running
    A->>I: JobRepository.save()

    Note over C,W: 5. Bidirectional Stream
    W->>G: WorkerStream(bidirectional)
    loop
        W->>G: WorkerHeartbeat(metrics)
        G-->>W: ACK
        G->>W: RunJobCommand(job_spec)
        W->>W: JobExecutor.execute()
        W->>G: LogBatch(batched logs)
        W->>G: JobResult(result)
    end

    Note over C,W: 6. Job Completion
    W->>G: JobResult(success=true)
    G->>A: JobCompletionHandler.handle()
    A->>D: Job.complete()
    D-->>A: Job state → Succeeded
    A->>I: JobRepository.save()
    A->>D: Publish JobCompletedEvent
    A-->>W: Ack

    Note over C,W: 7. Worker Termination
    A->>D: Check if worker can terminate
    D-->>A: Terminate(idle)
    A->>P: destroy_worker(handle)
    P->>P: Stop container/pod
    A->>D: Publish WorkerTerminatedEvent
```

### State Transitions

**Job State Machine**:
```
Pending → Scheduled → Queued → Running → Succeeded
                                 ↓
                            Failed/Cancelled/Timeout
```

**Worker State Machine**:
```
Creating → Connecting → Ready → Busy → Ready → Draining → Terminating → Terminated
```

---

## Worker Lifecycle Management

### Actor Model (EPIC-42)

```mermaid
graph TB
    subgraph "WorkerSupervisor"
        SUPER[Supervisor]
        ACTORS[WorkerActors]
    end

    subgraph "WorkerActor"
        STATE[State]
        HEARTBEAT[Heartbeat Tracker]
        TIMEOUT[Timeout Monitor]
    end

    SUPER -->|creates| ACTORS
    ACTORS --> STATE
    ACTORS --> HEARTBEAT
    ACTORS --> TIMEOUT

    HEARTBEAT -->|updates| STATE
    TIMEOUT -->|detects| STATE
    STATE -->|triggers| TIMEOUT
```

### WorkerActor Implementation

```rust
pub struct WorkerActor {
    worker_id: WorkerId,
    state: WorkerState,
    heartbeat_timeout: Duration,
    last_heartbeat: Arc<Mutex<Instant>>,
}

impl WorkerActor {
    pub async fn handle_heartbeat(&self, heartbeat: WorkerHeartbeat) {
        *self.last_heartbeat.lock().await = Instant::now();
    }

    pub async fn is_timeout(&self) -> bool {
        let last = *self.last_heartbeat.lock().await;
        last.elapsed() > self.heartbeat_timeout
    }

    pub async fn terminate(&self) {
        // Trigger termination logic
    }
}
```

### AutoScaling Logic

```rust
impl AutoScalingService {
    async fn evaluate_scaling(&self) -> Result<()> {
        let queue_len = self.job_queue.len().await?;
        let available = self.worker_registry.find_available().await?;

        let desired = self.calculate_desired(queue_len, available.len());

        if desired > available.len() {
            self.scale_up(desired - available.len()).await?;
        } else if desired < available.len() {
            self.scale_down(available.len() - desired).await?;
        }

        Ok(())
    }

    fn calculate_desired(&self, queue_len: usize, available: usize) -> usize {
        let min = self.config.min_workers;
        let max = self.config.max_workers;

        let target = (queue_len as f32 * self.config.scale_factor).ceil() as usize;
        target.clamp(min, max)
    }
}
```

---

## Worker Providers

### Comparison

| Provider | Isolation | Startup | GPU | Requirements | Use Case |
|----------|----------|---------|-----|--------------|----------|
| **Docker** | Container | ~1s | Yes | Docker daemon | Development, CI/CD |
| **Kubernetes** | Container (Pod) | ~5-15s | Yes | K8s cluster | Production, cloud-native |
| **Firecracker** | Hardware (KVM) | ~125ms | No | Linux + KVM | Multi-tenant, security-critical |

### Provider Configuration

**Docker**:
```bash
HODEI_DOCKER_ENABLED=1
HODEI_WORKER_IMAGE=hodei-worker:latest
HODEI_DOCKER_NETWORK=bridge
HODEI_DOCKER_CPUS=2.0
HODEI_DOCKER_MEMORY=4096
```

**Kubernetes**:
```bash
HODEI_K8S_ENABLED=1
HODEI_K8S_NAMESPACE=hodei-workers
HODEI_K8S_KUBECONFIG=/path/to/kubeconfig
HODEI_K8S_IMAGE_PULL_SECRET=registry-secret
HODEI_K8S_CPU_REQUEST=2
HODEI_K8S_MEMORY_REQUEST=4Gi
```

**Firecracker**:
```bash
HODEI_FC_ENABLED=1
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4
HODEI_FC_USE_JAILER=true
HODEI_FC_CPU_COUNT=2
HODEI_FC_MEMORY_SIZE=4096
```

---

## gRPC Services

### Service List

| Service | Description | RPCs |
|---------|-------------|------|
| **JobExecutionService** | Job lifecycle | QueueJob, GetJobStatus, CancelJob, ListJobs |
| **WorkerAgentService** | Worker registration & communication | Register, WorkerStream, UnregisterWorker |
| **SchedulerService** | Scheduling decisions | ScheduleJob, GetAvailableWorkers, ConfigureScheduler |
| **ProviderManagementService** | Provider management | RegisterProvider, ListProviders, GetProviderHealth |
| **MetricsService** | Metrics collection | StreamMetrics, GetAggregatedMetrics |
| **LogStreamService** | Log streaming | SubscribeLogs, GetLogs |
| **TemplateService** | Job templates | CreateTemplate, GetTemplate, ListTemplates |

### Message Examples

**QueueJob**:
```protobuf
message QueueJobRequest {
  JobDefinition job_definition = 1;
  string queued_by = 2;
}

message JobDefinition {
  oneof job_id {
    string value = 1;  // Auto-generate if empty
  }
  string name = 2;
  CommandType command = 3;
  JobResources resources = 4;
  JobPreferences preferences = 5;
}
```

**WorkerStream** (Bidirectional):
```protobuf
message WorkerMessage {
  oneof message {
    WorkerHeartbeat heartbeat = 1;
    LogEntry log_entry = 2;
    LogBatch log_batch = 3;  // Optimization v8.0
    JobResultMessage job_result = 4;
    WorkerStatsMessage stats = 5;
  }
}

message ServerMessage {
  oneof message {
    RunJobCommand run_job = 1;
    CancelJobCommand cancel_job = 2;
    AckMessage ack = 3;
    KeepAliveMessage keep_alive = 4;
  }
}
```

---

## Messaging & Event Bus

### Architecture

```mermaid
graph LR
    subgraph "Domain Layer"
        D[Domain Events]
    end

    subgraph "Application Layer"
        UC[Use Cases]
    end

    subgraph "Infrastructure Layer"
        OB[Outbox Pattern]
        PG[(PostgreSQL)]
        POLLER[Outbox Poller]
        NATS[NATS Broker]
    end

    subgraph "Event Handlers"
        H1[Handler 1]
        H2[Handler 2]
        H3[Handler 3]
    end

    D --> UC
    UC --> OB
    OB --> PG
    POLLER --> PG
    POLLER --> NATS
    NATS --> H1
    NATS --> H2
    NATS --> H3
```

### Event Types

**Job Events**:
- `JobCreated`
- `JobStatusChanged`
- `JobCompleted`
- `JobFailed`
- `JobCancelled`
- `JobTimeout`

**Worker Events**:
- `WorkerRegistered`
- `WorkerStatusChanged`
- `WorkerHeartbeat`
- `WorkerTerminated`
- `WorkerUnhealthy`

**Provider Events**:
- `ProviderRegistered`
- `ProviderHealthChanged`
- `ProviderDisabled`

### Event Consumers

```rust
// Orphan Worker Detector
pub struct OrphanWorkerDetectorConsumer {
    worker_registry: Arc<dyn WorkerRegistry>,
    worker_provisioning: Arc<WorkerProvisioningService>,
}

#[async_trait]
impl EventHandler<WorkerTerminatedEvent> for OrphanWorkerDetectorConsumer {
    async fn handle(&self, event: &WorkerTerminatedEvent) -> Result<()> {
        // Check if job was running
        let worker = self.worker_registry.get(&event.worker_id).await?;
        if let Some(job_id) = worker.current_job_id() {
            // Re-queue job
            self.job_queue.enqueue(job).await?;
        }
        Ok(())
    }
}

// Execution Saga Consumer
pub struct ExecutionSagaConsumer {
    saga_orchestrator: Arc<SagaOrchestrator>,
}

#[async_trait]
impl EventHandler<JobCreatedEvent> for ExecutionSagaConsumer {
    async fn handle(&self, event: &JobCreatedEvent) -> Result<()> {
        // Start execution saga
        self.saga_orchestrator.start_provisioning_saga(event.job_id).await
    }
}
```

---

## Persistence

### Database Schema

**Tables**:
- `jobs` - Job aggregates
- `job_queue` - Job queue (with SKIP LOCKED for concurrency)
- `workers` - Worker aggregates
- `providers` - Provider configurations
- `bootstrap_tokens` - OTP tokens
- `outbox` - Event outbox
- `audit_logs` - Audit trail
- `sagas` - Saga state
- `job_templates` - Job templates

### Migrations

Located in: `crates/server/infrastructure/migrations/`

```sql
-- Jobs table
CREATE TABLE jobs (
    id UUID PRIMARY KEY,
    spec JSONB NOT NULL,
    state INTEGER NOT NULL,
    selected_provider UUID REFERENCES providers(id),
    execution_context JSONB,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Job queue
CREATE TABLE job_queue (
    job_id UUID PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_job_queue_priority ON job_queue (priority DESC, created_at ASC);

-- Workers table
CREATE TABLE workers (
    worker_id UUID PRIMARY KEY,
    handle JSONB NOT NULL,
    spec JSONB NOT NULL,
    state INTEGER NOT NULL,
    current_job_id UUID REFERENCES jobs(id),
    resources JSONB,
    heartbeat TIMESTAMP,
    ttl_config JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Outbox pattern
CREATE TABLE outbox (
    event_id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    published_at TIMESTAMP
);

CREATE INDEX idx_outbox_published ON outbox (published, created_at);
```

---

## Security (Zero Trust)

### mTLS Infrastructure

```mermaid
graph TB
    CA[CA Root Certificate]
    SERVER[Server Certificate]
    CLIENT[Client Certificate]

    CA --> SERVER
    CA --> CLIENT

    SERVER -->|TLS Handshake| CLIENT
    CLIENT -->|Verify Certificate| SERVER
```

### Certificate Management

**Paths**:
- CA: `/etc/hodei/pki/ca.crt`
- Server cert: `/etc/hodei/pki/server.crt`
- Server key: `/etc/hodei/pki/server.key`
- Client cert: `/etc/hodei/pki/client.crt`
- Client key: `/etc/hodei/pki/client.key`

**Generation Script**: `scripts/generate-certificates.sh`

### OTP Authentication

```rust
pub struct BootstrapToken {
    token: OtpToken,
    worker_id: WorkerId,
    provider_id: ProviderId,
    expires_at: DateTime<Utc>,
}

impl OtpTokenStore {
    pub async fn generate(&self, worker_id: WorkerId, provider_id: ProviderId, ttl: Duration) -> Result<BootstrapToken> {
        let token = OtpToken::generate();
        let expires_at = Utc::now() + ttl;

        let bootstrap = BootstrapToken {
            token: token.clone(),
            worker_id,
            provider_id,
            expires_at,
        };

        self.store.save(&bootstrap).await?;
        Ok(bootstrap)
    }

    pub async fn validate(&self, token: &OtpToken) -> Result<WorkerRegistration> {
        let bootstrap = self.store.find_by_token(token).await?
            .ok_or(DomainError::InvalidToken)?;

        if bootstrap.expires_at < Utc::now() {
            return Err(DomainError::TokenExpired);
        }

        Ok(WorkerRegistration {
            worker_id: bootstrap.worker_id,
            provider_id: bootstrap.provider_id,
        })
    }
}
```

### Secret Injection

```rust
impl JobExecutor {
    async fn inject_secrets(&self, secrets: &HashMap<String, String>) -> Result<()> {
        let json = serde_json::to_string(secrets)?;

        // Write to stdin (closed after injection)
        let mut stdin = self.child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;

        stdin.write_all(json.as_bytes()).await?;
        stdin.shutdown().await?;

        Ok(())
    }
}
```

**Security Features**:
- Secrets never appear in logs (automatic redaction)
- Secrets transmitted via stdin with immediate closure
- JSON serialization for multiple secrets
- Audit logging of secret access

---

## Performance Optimizations (v8.0)

### 1. LogBatching (90-99% gRPC reduction)

```rust
pub struct LogBatcher {
    buffer: Vec<LogEntry>,
    capacity: usize,  // Default: 100
    flush_interval: Duration,  // Default: 100ms
    last_flush: Instant,
}

// Benefits:
// - 90-99% reduction in gRPC calls
// - Batch transmission (1 batch vs 100+ individual calls)
// - Automatic flush on capacity or timeout
```

### 2. Zero-Copy I/O

```rust
// FramedRead + BytesCodec for zero-copy
let mut framed = FramedRead::new(source, BytesCodec::new());
while let Some(chunk) = framed.next().await {
    let bytes = chunk?;  // Direct Bytes slice - no copy
    // Process directly from Bytes
}

// Benefits:
// - Zero-copy of log data
// - BytesCodec for efficient decoding
// - ~40% memory allocation reduction
```

### 3. Backpressure Handling

```rust
// Non-blocking async operations with try_send()
let _ = self.tx.try_send(message);

// Benefits:
// - Prevents blocking of async runtime
// - Drops messages when channel full
// - 100% async runtime stability
```

### 4. Cached Metrics (35s TTL)

```rust
pub struct CachedResourceUsage {
    usage: ResourceUsage,
    timestamp: Instant,
}

const METRICS_CACHE_TTL_SECS: u64 = 35;

// Benefits:
// - ~60% metrics collection overhead reduction
// - spawn_blocking for intensive tasks
// - Non-blocking metrics collection
```

### 5. Write-Execute Pattern

```rust
// Robust script execution with safety headers
const SAFETY_HEADERS: &str = r#"
set -e        # Exit on error
set -u        # Error on undefined variables
set -o pipefail  # Exit on pipe failure
"#;

// Benefits:
// - Automatic injection of safety headers
// - Secure temporary file management
// - Async cleanup (non-blocking)
// - Robust error handling
```

### Performance Metrics

| Optimization | Improvement | Metric |
|--------------|-------------|--------|
| **LogBatching** | 90-99% | gRPC calls reduction |
| **Zero-Copy I/O** | ~40% | Memory allocation reduction |
| **Cached Metrics** | ~60% | Metrics collection overhead |
| **Backpressure Handling** | 100% | Async runtime stability |
| **Write-Execute Pattern** | N/A | Script execution robustness |

---

## Testing Strategy

### Test Levels

1. **Unit Tests**: Domain logic, pure functions
2. **Integration Tests**: Use cases, repositories
3. **E2E Tests**: Full job execution with real providers
4. **Contract Tests**: gRPC service contracts

### Test Organization

```
crates/
├── server/
│   ├── domain/src/**/*.rs          # Unit tests in mod.rs
│   ├── application/tests/          # Integration tests
│   ├── infrastructure/tests/        # Infrastructure tests
│   └── tests/                      # E2E tests
│       ├── common/                 # Test fixtures
│       ├── docker_integration.rs    # Docker provider tests
│       ├── k8s_integration.rs      # K8s provider tests
│       └── firecracker_integration.rs  # Firecracker tests
└── worker/
    ├── bin/tests/                  # Worker E2E tests
    └── infrastructure/tests/        # Worker infrastructure tests
```

### Testing Infrastructure

**Test Utilities** (`infrastructure/src/test_infrastructure.rs`):
- In-memory repositories
- Mock event bus
- Test factories

**Fixtures** (`server/tests/common/`):
- Job fixtures
- Worker fixtures
- Provider fixtures

### Running Tests

```bash
# Unit tests
cargo test --workspace

# Integration tests
cargo test --test docker_integration -- --ignored

# E2E tests
cargo test --test e2e_docker_provider -- --ignored --nocapture

# All tests with coverage
cargo test --workspace && cargo tarpaulin --out Html
```

---

## Configuration Management

### Environment Variables

**Database**:
```bash
HODEI_DATABASE_URL=postgres://user:pass@localhost:5432/hodei
HODEI_DB_MAX_CONNECTIONS=10
HODEI_DB_CONNECTION_TIMEOUT_SECS=30
```

**gRPC Server**:
```bash
HODEI_SERVER_HOST=0.0.0.0
HODEI_GRPC_PORT=50051
HODEI_DEV_MODE=1
```

**Providers**:
```bash
HODEI_DOCKER_ENABLED=1
HODEI_K8S_ENABLED=1
HODEI_FC_ENABLED=0
```

**Worker**:
```bash
HODEI_SERVER=http://localhost:50051
HODEI_WORKER_HEARTBEAT_INTERVAL=30s
HODEI_WORKER_OTP_TOKEN_FILE=/var/run/hodei/otp.txt
```

### Configuration Files

**Server**: `config/server.toml`
```toml
[server]
host = "0.0.0.0"
grpc_port = 50051
rest_port = 8080

[database]
url = "postgres://localhost:5432/hodei"
max_connections = 10

[providers.docker]
enabled = true
image = "hodei-worker:latest"

[auto_scaling]
min_workers = 2
max_workers = 10
scale_factor = 0.5
```

**Worker**: `config/worker.toml`
```toml
[worker]
server_url = "http://localhost:50051"
heartbeat_interval = "30s"

[executor]
script_dir = "/tmp/hodei"
cleanup_timeout = "10s"

[metrics]
cache_ttl_secs = 35
```

---

## Deployment Architecture

### Docker Deployment

```yaml
# docker-compose.dev.yml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: hodei
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
    ports:
      - "5432:5432"

  server:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "50051:50051"
      - "8080:8080"
    environment:
      HODEI_DATABASE_URL: postgres://postgres:postgres@postgres:5432/hodei
      HODEI_DOCKER_ENABLED: 1
      HODEI_DEV_MODE: 1
    depends_on:
      - postgres
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock

  worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
    environment:
      HODEI_SERVER: http://server:50051
    depends_on:
      - server
```

### Kubernetes Deployment

```yaml
# deploy/kubernetes/server.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hodei-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: hodei-server
  template:
    metadata:
      labels:
        app: hodei-server
    spec:
      containers:
      - name: server
        image: hodei-server:latest
        ports:
        - containerPort: 50051
        env:
        - name: HODEI_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: hodei-secrets
              key: database-url
        - name: HODEI_K8S_ENABLED
          value: "1"
```

### Helm Chart

Located in: `helm/hodei-jobs/`

```bash
# Install
helm install hodei-jobs ./helm/hodei-jobs

# Upgrade
helm upgrade hodei-jobs ./helm/hodei-jobs

# Uninstall
helm uninstall hodei-jobs
```

---

## Detailed Use Cases with Examples

### Use Case 1: Submit and Execute a Simple Job

**Scenario**: Submit a shell command job and monitor its execution

```bash
# 1. Submit job via gRPC
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "my-job-001"},
    "name": "Hello World",
    "command": {
      "shell": {
        "cmd": "echo",
        "args": ["Hello from Hodei!"]
      }
    },
    "resources": {
      "cpu_cores": 1.0,
      "memory_mb": 512,
      "storage_mb": 1024
    },
    "preferences": {
      "allow_retry": true,
      "priority": 0
    }
  },
  "queued_by": "dev-team"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Response
{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"},
  "status": 2,  // Queued
  "created_at": "2026-01-09T10:00:00Z"
}

# 2. Monitor job status
grpcurl -plaintext -d '{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"}
}' localhost:50051 hodei.JobExecutionService/GetJobStatus

# 3. Stream logs
grpcurl -plaintext -d '{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"}
}' localhost:50051 hodei.LogStreamService/SubscribeLogs
```

**Internal Flow**:

```rust
// 1. Client sends QueueJobRequest
let request = QueueJobRequest { job_definition, queued_by };

// 2. gRPC server calls use case
let job = create_job_use_case.execute(request).await?;

// 3. Domain creates Job aggregate
let job = Job::new(job_spec, JobPreferences::default())?;

// 4. Persist to database
job_repository.save(&job).await?;
job_queue.enqueue(job.clone()).await?;

// 5. Publish domain event
event_bus.publish(JobCreatedEvent {
    job_id: job.id.clone(),
    spec: job.spec.clone(),
    queued_by: queued_by.clone(),
}).await?;

// 6. Event handler triggers provisioning
provisioning_handler.handle(JobCreatedEvent).await?;

// 7. Worker is provisioned and executes job
// 8. JobCompletionEvent published
// 9. Job status updated to Succeeded
```

### Use Case 2: Execute Job with Secrets

**Scenario**: Execute a job that requires secrets (API keys, tokens)

```bash
# 1. Create secret (via REST API or gRPC)
grpcurl -plaintext -d '{
  "secret": {
    "name": "api-key-prod",
    "value": {"secret_string": "sk-prod-1234567890"},
    "secret_type": "STRING"
  },
  "created_by": "dev-team"
}' localhost:50051 hodei.CredentialService.CreateSecret

# 2. Submit job with secret reference
grpcurl -plaintext -d '{
  "job_definition": {
    "name": "API Job",
    "command": {
      "shell": {
        "cmd": "python",
        "args": ["-c", "import json; import sys; data = json.load(sys.stdin); print(f\"API Key: {data['api-key-prod']}\")"]
      }
    },
    "secrets": ["api-key-prod"]
  },
  "queued_by": "dev-team"
}' localhost:50051 hodei.JobExecutionService/QueueJob
```

**Secret Injection Flow**:

```rust
// Worker side - JobExecutor
impl JobExecutor {
    async fn execute_with_secrets(&self, job: RunJobCommand) -> Result<JobResult> {
        // 1. Fetch secrets (if any)
        let secrets = if let Some(secret_names) = &job.secrets {
            self.fetch_secrets(secret_names).await?
        } else {
            HashMap::new()
        };

        // 2. Create temporary script
        let script_path = self.create_temp_script(&job.command_type).await?;

        // 3. Inject secrets via stdin
        let mut child = Command::new(&job.interpreter)
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write secrets to stdin and close immediately
        if !secrets.is_empty() {
            let json = serde_json::to_string(&secrets)?;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(json.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        // 4. Stream logs (secrets are automatically redacted)
        self.stream_logs(&mut child, &job.job_id).await?;

        // 5. Wait for completion
        let status = child.wait().await?;

        Ok(JobResult {
            success: status.success(),
            exit_code: status.code().unwrap_or(-1),
            error_message: None,
        })
    }
}
```

### Use Case 3: Scheduled Job (Cron-like)

**Scenario**: Schedule a job to run every hour

```bash
# 1. Create scheduled job
grpcurl -plaintext -d '{
  "scheduled_job": {
    "name": "Hourly Backup",
    "cron_expression": "0 * * * *",
    "job_definition": {
      "name": "Backup",
      "command": {
        "shell": {
          "cmd": "backup-script.sh",
          "args": []
        }
      }
    },
    "enabled": true
  },
  "created_by": "ops-team"
}' localhost:50051 hodei.SchedulerService.CreateScheduledJob

# 2. List scheduled jobs
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.ListScheduledJobs
```

### Use Case 4: Worker Auto-scaling

**Scenario**: Automatically scale workers based on queue length

```bash
# Configure auto-scaling
grpcurl -plaintext -d '{
  "config": {
    "min_workers": 2,
    "max_workers": 10,
    "scale_up_threshold": 5,
    "scale_down_threshold": 2,
    "scale_up_cooldown": 300,
    "scale_down_cooldown": 600
  }
}' localhost:50051 hodei.ProviderManagementService.ConfigureAutoScaling
```

---

## Design Patterns Implementation

### 1. Repository Pattern

**Purpose**: Abstract data access logic

```rust
// Domain trait
#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>>;
    async fn find_by_state(&self, state: &JobState) -> Result<Vec<Job>>;
}

// Infrastructure implementation
pub struct PostgresJobRepository {
    pool: PgPool,
}

#[async_trait]
impl JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        sqlx::query!(
            "INSERT INTO jobs (id, spec, state, ...) VALUES ($1, $2, $3, ...)",
            job.id.as_uuid(),
            serde_json::to_value(&job.spec)?,
            job.state as i32,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

### 2. Strategy Pattern

**Purpose**: Encapsulate scheduling algorithms

```rust
pub trait SchedulingStrategy: Send + Sync {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64;
}

pub struct LeastLoadedStrategy;

impl SchedulingStrategy for LeastLoadedStrategy {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64 {
        let cpu_score = 1.0 - (worker.resources.cpu_usage_percent / 100.0);
        let memory_score = 1.0 - (worker.resources.memory_usage_percent / 100.0);
        (cpu_score + memory_score) / 2.0
    }
}
```

### 3. Observer Pattern (Domain Events)

**Purpose**: React to domain changes

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCompletedEvent {
    pub job_id: JobId,
    pub result: JobExecutionResult,
    pub completed_at: DateTime<Utc>,
}

pub struct JobCompletionNotificationHandler {
    notification_service: Arc<NotificationService>,
}

#[async_trait]
impl EventHandler<JobCompletedEvent> for JobCompletionNotificationHandler {
    async fn handle(&self, event: &JobCompletedEvent) -> Result<()> {
        let job = self.job_repo.find_by_id(&event.job_id).await?;
        if job.spec.preferences.notify_on_completion {
            self.notification_service.send(&job.queued_by, &format!("Job {} completed", job.name)).await?;
        }
        Ok(())
    }
}
```

### 4. Saga Pattern

**Purpose**: Coordinate distributed transactions

```rust
pub struct ProvisioningSaga {
    steps: Vec<Box<dyn SagaStep>>,
    current_step: usize,
    compensation_data: HashMap<String, serde_json::Value>,
}

impl ProvisioningSaga {
    pub async fn execute(&mut self) -> Result<()> {
        for step in &self.steps {
            if let Err(e) = step.execute(self.get_input(step)?).await {
                self.compensate(self.current_step).await?;
                return Err(e);
            }
            self.current_step += 1;
        }
        Ok(())
    }

    async fn compensate(&mut self, from_step: usize) -> Result<()> {
        for step in (0..from_step).rev() {
            let input = self.get_compensating_input(step)?;
            self.steps[step].compensate(input).await?;
        }
        Ok(())
    }
}
```

---

## Monitoring & Observability

### Metrics Collection

```rust
use prometheus::{Counter, Histogram, Gauge};

pub struct Metrics {
    jobs_submitted: Counter,
    jobs_completed: Counter,
    jobs_failed: Counter,
    job_duration: Histogram,
    workers_active: Gauge,
}

impl Metrics {
    pub fn record_job_completed(&self, duration: Duration) {
        self.jobs_completed.inc();
        self.job_duration.observe(duration.as_secs_f64());
    }

    pub fn update_workers_active(&self, count: usize) {
        self.workers_active.set(count as f64);
    }
}
```

### Distributed Tracing

```rust
use opentelemetry::trace::{Tracer, TraceError};
use tracing_opentelemetry::OpenTelemetryLayer;

fn init_tracing() -> Result<TraceError> {
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("hodei-server")
        .install_simple()?;

    let telemetry = OpenTelemetryLayer::new(tracer);
    tracing_subscriber::registry().with(telemetry).try_init()
}
```

---

## Troubleshooting Guide

### Common Issues

#### 1. Worker Not Registering

**Diagnosis**:
```bash
grep "GenerateOTP" /var/log/hodei-server.log
grep "Register" /var/log/hodei-worker.log
docker exec <worker_container> cat /var/run/hodei/otp.txt
```

**Solutions**:
- Increase OTP token TTL: `HODEI_OTP_TTL_SECS=600`
- Verify network connectivity
- Check mTLS certificate validity

#### 2. Jobs Stuck in "Queued" State

**Diagnosis**:
```bash
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.GetQueueStatus
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.GetAvailableWorkers
```

**Solutions**:
- Verify auto-scaling is enabled
- Check provider health
- Ensure worker images are available

#### 3. High Memory Usage in Worker

**Diagnosis**:
```bash
grpcurl -plaintext -d '{}' localhost:50051 hodei.MetricsService.GetWorkerMetrics
docker stats
```

**Solutions**:
- Reduce job batch size
- Implement log truncation
- Add memory limits to container

---

## Best Practices

### 1. Job Design

**DO**:
- Keep jobs stateless and idempotent
- Use appropriate resource limits
- Implement graceful shutdown
- Set reasonable timeouts

**DON'T**:
- Store state in job environment
- Assume external dependencies are available
- Ignore error conditions
- Create infinite loops

### 2. Error Handling

**DO**:
- Use Result types explicitly
- Provide context with errors
- Log errors with appropriate level
- Implement retry logic for transient errors

**DON'T**:
- Use unwrap() in production code
- Ignore errors
- Panic on recoverable errors
- Log passwords or secrets

### 3. Performance Optimization

**DO**:
- Use async/await for I/O operations
- Batch database operations
- Use connection pooling
- Implement caching where appropriate

**DON'T**:
- Block async runtime
- N+1 query patterns
- Allocate unnecessarily
- Disable optimizations prematurely

---

## Extension Guide

### Adding a New Worker Provider

**Step 1**: Implement WorkerProvider trait
**Step 2**: Add configuration variant
**Step 3**: Register provider in factory
**Step 4**: Add comprehensive tests

### Adding a New Domain Event

**Step 1**: Define event struct
**Step 2**: Implement DomainEvent trait
**Step 3**: Publish from use cases
**Step 4**: Create event handlers
**Step 5**: Register handlers in event bus

---

## Migration Guide

### Migrating from v7.x to v8.0

**Breaking Changes**:

1. Log batching enabled by default
2. mTLS required for worker communication
3. Zero-copy I/O enabled

**Migration Steps**:

1. Backup database
2. Run migrations
3. Generate certificates
4. Update configuration
5. Rebuild workers
6. Verify deployment

---

## Glossary

### Domain Terms

| Term | Definition |
|------|------------|
| **Aggregate** | Cluster of domain objects treated as single unit |
| **Bounded Context** | Specific part of domain logic acting independently |
| **Domain Event** | Something important in domain to other parts of system |
| **Value Object** | Immutable object defined by its attributes |
| **Repository** | Collection-like interface for accessing domain objects |
| **Saga** | Sequence of local transactions for distributed operations |
| **CQRS** | Command Query Responsibility Segregation pattern |
| **Outbox Pattern** | Pattern to guarantee event publishing in transactions |

### Technical Terms

| Term | Definition |
|------|------------|
| **mTLS** | Mutual TLS authentication |
| **OTP** | One-Time Password |
| **gRPC** | Remote Procedure Call framework |
| **Zero-Copy I/O** | I/O without data copying |
| **Backpressure** | Mechanism to handle flow control |
| **DDD** | Domain-Driven Design |
| **SOLID** | Five design principles |

---

## Conclusion

This architecture document provides a comprehensive overview of the Hodei Job Platform's design, covering:

1. **Domain-Driven Design**: 9 bounded contexts with clear responsibilities
2. **Hexagonal Architecture**: Clean separation of concerns
3. **Event-Driven Design**: Reactive system with domain events
4. **Zero Trust Security**: mTLS + OTP authentication
5. **Performance Optimizations**: LogBatching, Zero-Copy, Cached Metrics
6. **Multi-Provider Support**: Docker, Kubernetes, Firecracker
7. **Production Ready**: Comprehensive testing, monitoring, deployment

The platform is designed for **scalability, reliability, and security**, enabling teams to execute workloads across any infrastructure with consistent operational experience.

---

**Document Version**: 1.0  
**Last Updated**: 2026-01-09  
**Maintained By**: Architecture Team  
**Review Cycle**: Quarterly  

# DDD Definitions - Layer and Category Reference

This document provides precise definitions for DDD layers and categories used in code catalogation.

---

## DDD Layers

### 1. Domain Layer (Dominio)

**Purpose:** Contains enterprise-wide business rules and entities.

**Characteristics:**
- Pure business logic, no infrastructure concerns
- No dependencies on application or infrastructure layers
- Defines entities, value objects, aggregates, domain events
- Implements domain services for business operations

**Types of Classes:**
- Entities (aggregate roots, child entities)
- Value Objects
- Domain Events
- Domain Services
- Repository Interfaces (contracts)
- Factory interfaces
- Domain Exceptions

**Example Files:**
- `crates/server/domain/src/jobs/aggregate.rs`
- `crates/server/domain/src/workers/aggregate.rs`
- `crates/server/domain/src/scheduling/strategies.rs`

---

### 2. Application Layer (Aplicación)

**Purpose:** Orchestrates domain objects to perform use cases.

**Characteristics:**
- Defines use cases (application services)
- Coordinates multiple domain objects
- Manages transactions
- Contains no business rules (delegates to domain)
- Depends only on domain layer

**Types of Classes:**
- Use Cases / Application Services
- Event Handlers / Subscribers
- Query Services (read side)
- DTOs for application boundaries
- Workflow orchestrators

**Example Files:**
- `crates/server/application/src/jobs/create.rs`
- `crates/server/application/src/scheduling/smart_scheduler.rs`
- `crates/server/application/src/workers/lifecycle.rs`

---

### 3. Infrastructure Layer (Infraestructura)

**Purpose:** Implements technical capabilities needed by other layers.

**Characteristics:**
- Implements interfaces defined in domain
- Contains adapters for external systems
- Database access, network communication
- File systems, message queues
- Depends on domain layer (DIP)

**Types of Classes:**
- Repository Implementations
- External Service Clients
- Message Publishers/Subscribers
- Database Mappers (ORM)
- Infrastructure Utilities
- Configuration Providers

**Example Files:**
- `crates/server/infrastructure/src/persistence/postgres/job_repository.rs`
- `crates/server/infrastructure/src/providers/kubernetes.rs`
- `crates/server/infrastructure/src/messaging/postgres.rs`

---

### 4. Interface Layer (Interfaz de Usuario)

**Purpose:** Handles interaction with external systems.

**Characteristics:**
- Entry point for external requests
- Converts external formats to internal
- API controllers, gateways
- gRPC service implementations
- Web handlers

**Types of Classes:**
- gRPC Service Implementations
- REST Controllers
- API Gateways
- View Models / Presenters
- Interface Adapters

**Example Files:**
- `crates/server/interface/src/grpc/`
- `crates/server/interface/src/rest/`

---

## DDD Categories

### Domain Layer Categories

| Category | Description | Indicators |
|----------|-------------|------------|
| **Aggregate** | Entity cluster with invariant enforcement | State machine, transition methods, `transition_to()` |
| **Entity** | Object with identity and lifecycle | Has unique ID, mutable state |
| **Value Object** | Immutable object without identity | No ID, all fields immutable, equality by value |
| **Domain Service** | Business logic not fitting in entity | Stateless, business calculations, policies |
| **Domain Event** | Something that happened in domain | Past tense naming, event data |
| **Repository Interface** | Contract for persistence | `trait XRepository`, async methods |
| **Factory Interface** | Contract for creation | `trait XFactory`, creation methods |
| **Domain Exception** | Business rule violation | Named Error, Error enum variants |

### Application Layer Categories

| Category | Description | Indicators |
|----------|-------------|------------|
| **Use Case** | Single business action orchestration | `CreateXUseCase`, `ProcessXCommand`, `ExecuteX` |
| **Event Handler** | Reacts to domain events | `on_event()`, `handle_event()` |
| **Query Service** | Read operations | `get_x()`, `find_x()`, `query_x()` |
| **Application Service** | Multi-step application logic | Service with use case coordination |
| **Workflow Orchestrator** | Complex multi-step process | `orchestrate()`, `execute_workflow()` |

### Infrastructure Layer Categories

| Category | Description | Indicators |
|----------|-------------|------------|
| **Repository Implementation** | Persistence logic | `PostgresXRepository`, `InMemoryXRepository` |
| **External Client** | External system adapter | `DockerClient`, `KubernetesClient`, API calls |
| **Message Publisher** | Event publishing | `publish()`, `send_to_kafka()` |
| **Message Subscriber** | Event consumption | `subscribe()`, `consume()` |
| **Database Mapper** | ORM or data mapping | `XMapper`, `EntityFrameworkMapper` |
| **Infrastructure Utility** | Technical helpers | `EncryptionService`, `HttpClient` |

### Interface Layer Categories

| Category | Description | Indicators |
|----------|-------------|------------|
| **Controller** | Request handling | `handle_request()`, `endpoint()`, `route()` |
| **gRPC Service** | gRPC endpoint implementation | `impl XService for XServiceImpl` |
| **DTO (Interface)** | Data transfer object | Request/Response structs |
| **View Model** | UI presentation data | `XViewModel`, `XPresentationModel` |
| **API Gateway** | External API facade | `gateway()`, `api_client()` |

---

## Mapping Patterns to Categories

### Common Patterns and Their Categories

| Pattern | Layer | Category |
|---------|-------|----------|
| `struct X` with `id: XId` and `state: XState` | Domain | Entity |
| `struct X` with only data fields, no ID | Domain | Value Object |
| `trait XRepository` | Domain | Repository Interface |
| `struct PostgresXRepository` | Infrastructure | Repository Implementation |
| `enum XEvent { ... }` | Domain | Domain Event |
| `struct CreateXCommand` | Application | DTO/Command |
| `impl CreateXUseCase` | Application | Use Case |
| `async fn handle_event(&self, event: XEvent)` | Application | Event Handler |
| `impl XService for XServiceImpl` (gRPC) | Interface | gRPC Service |

---

## Examples of Category Classification

### Domain Layer Examples

```rust
// Entity - Aggregate Root with state machine
pub struct Job {
    id: JobId,
    state: JobState,
    // ... methods with state transitions
}

// Value Object - Immutable, no identity
pub struct JobSpec {
    image: String,
    resources: ResourceRequirements,
    // ... all fields validated in constructor
}

// Domain Service - Business logic not fitting in entity
pub struct PricingCalculator;

// Domain Event
pub struct JobCreated {
    job_id: JobId,
    created_at: DateTime<Utc>,
}

// Repository Interface (contract)
#[async_trait]
pub trait JobRepository {
    async fn save(&self, job: &Job) -> Result<()>;
}
```

### Application Layer Examples

```rust
// Use Case
pub struct CreateJobUseCase;

// Event Handler
#[async_trait]
impl EventHandler for JobEventSubscriber {
    async fn handle(&self, event: &JobCompletedEvent) -> Result<()>;
}

// Workflow Orchestrator
pub struct JobOrchestrator;
```

### Infrastructure Layer Examples

```rust
// Repository Implementation
pub struct PostgresJobRepository;

// External Client
pub struct DockerProvider;

// Message Publisher
pub struct EventPublisher;
```

### Interface Layer Examples

```rust
// gRPC Service
#[tonic::grpc]
impl JobExecutionService for JobExecutionServiceImpl;

// DTO
pub struct CreateJobRequest;
pub struct JobResponse;
```

---

## Bounded Contexts in Hodei Jobs

Based on the codebase analysis, the following bounded contexts exist:

| Bounded Context | Domain Layer Module | Purpose |
|-----------------|---------------------|---------|
| **Jobs** | `domain/src/jobs/` | Job lifecycle management |
| **Workers** | `domain/src/workers/` | Worker registration and management |
| **Providers** | `domain/src/providers/` | Provider configuration |
| **Scheduling** | `domain/src/scheduling/` | Job scheduling strategies |
| **Audit** | `domain/src/audit/` | Audit logging |
| **IAM** | `domain/src/iam/` | Authentication tokens |
| **Outbox** | `domain/src/outbox/` | Transactional outbox pattern |
| **Credentials** | `domain/src/credentials/` | Secret management |

---

## Document Version
- **Version:** 1.0
- **Created:** 2025-12-30
- **Last Updated:** 2025-12-30

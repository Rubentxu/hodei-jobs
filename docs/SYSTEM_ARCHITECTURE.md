# Hodei Job Platform - V2 System Architecture

**Version**: 2.0 (Consolidated)
**Date**: 2026-01-10
**Status**: Definitive Reference

---

## 1. System Overview

Hodei Jobs Platform is a **distributed, event-driven job execution system** designed for High Performance Computing (HPC) workloads. It abstracts infrastructure providers (Docker, Kubernetes, Firecracker) to provide a uniform execution layer.

### 1.1 Architectural Style
*   **Hexagonal Architecture**: Core domain logic is isolated from infrastructure (Ports & Adapters).
*   **Hybrid Orchestration & Choreography**:
    *   **Orchestration (Sagas)** for complex, compensatable workflows (Job Execution).
    *   **Choreography (Events)** for reactive, fire-and-forget actions (Worker Registration).
*   **Zero Trust Security**: Workers are ephemeral and authenticated via OTP.

### 1.2 Technology Stack
*   **Core**: Rust 2024 (Tokio, Tonic, Axum)
*   **Messaging**: NATS JetStream (Durable Streams)
*   **Persistence**: PostgreSQL (SQLx)
*   **Consistency**: Transactional Outbox Pattern

---

## 2. Hybrid Event-Driven Architecture

The system uses a **Command-Event Pipeline** to maintain strict consistency while enabling loose coupling.

```mermaid
graph TD
    subgraph "Orchestration Layer (Strict Control)"
        SAGA[Saga Coordinator]
        CMD[Command Bus]
    end

    subgraph "Domain Core (Business Logic)"
        HANDLER[Command Handlers]
        AGG[Aggregates (Job/Worker)]
        REPO[Repositories]
    end

    subgraph "Choreography Layer (Reactive)"
        OUTBOX[Outbox Table]
        RELAY[Outbox Relay]
        NATS[NATS JetStream]
        CONSUMER[Event Consumers]
    end

    SAGA -->|Dispatches| CMD
    CMD -->|Routes to| HANDLER
    HANDLER -->|Updates| AGG
    AGG -->|Emits| OUTBOX
    OUTBOX -->|1. Polled by| RELAY
    RELAY -->|2. Publishes to| NATS
    NATS -->|3. Triggers| CONSUMER
    CONSUMER -->|4. Feeds back to| SAGA
```

---

## 3. Saga Catalogue (Visual Graphs)

### 3.1 Provisioning Saga
**Goal**: Create infrastructure for a worker when no capacity exists.
**Trigger**: `trigger_provisioning` (via `JobQueued`)

```mermaid
flowchart TB
    %% Nodes
    START((Start))
    
    subgraph Steps
        S1[ValidateProviderCapacityStep]
        S2[CreateInfrastructureStep]
        S3[PublishProvisionedEventStep]
    end

    subgraph Commands
        C1[ValidateProviderCommand]
        C2[CreateWorkerCommand]
        C3[PublishProvisionedCommand]
    end

    subgraph Handlers
        H1[ValidateProviderHandler]
        H2[CreateWorkerHandler]
        H3[PublishProvisionedHandler]
    end

    EVENT[WorkerProvisioned Event]
    
    %% Edges
    START --> S1
    S1 -->|Dispatch| C1
    C1 --> H1
    H1 -->|Success| S2
    H1 -->|Fail| X[Terminate Saga]

    S2 -->|Dispatch| C2
    C2 --> H2
    H2 -->|Success| S3
    H2 -->|Fail| COMP[Compensate: DestroyWorker]

    S3 -->|Dispatch| C3
    C3 --> H3
    H3 -->|Emits| EVENT
    
    %% Styling
    style S1 fill:#e1f5fe,stroke:#01579b
    style S2 fill:#e1f5fe,stroke:#01579b
    style S3 fill:#e1f5fe,stroke:#01579b
    style C1 fill:#fff9c4,stroke:#fbc02d
    style C2 fill:#fff9c4,stroke:#fbc02d
    style C3 fill:#fff9c4,stroke:#fbc02d
```

### 3.2 Execution Saga
**Goal**: Execute a job on a ready worker.
**Trigger**: `WorkerReady` (matched with `JobQueued`)

```mermaid
flowchart TB
    START((Start))

    subgraph Steps
        S1[ValidateJobStep]
        S2[AssignWorkerStep]
        S3[ExecuteJobStep]
        S4[CompleteJobStep]
    end

    subgraph Commands
        C1[ValidateJobCommand]
        C2[AssignWorkerCommand]
        C3[ExecuteJobCommand]
        C4[CompleteJobCommand]
    end

    subgraph Handlers
        H1[ValidateJobHandler]
        H2[AssignWorkerHandler]
        H3[ExecuteJobHandler]
        H4[CompleteJobHandler]
    end

    %% Flow
    START --> S1
    S1 -->|Dispatch| C1 --> H1
    
    H1 -->|Valid| S2
    S2 -->|Dispatch| C2 --> H2
    
    H2 -->|Assigned| S3
    S3 -->|Dispatch| C3 --> H3
    
    H3 -->|Executed| S4
    S4 -->|Dispatch| C4 --> H4
    
    %% Compensations
    H2 -.->|Fail| COMP1[ReleaseWorker]
    H3 -.->|Fail| COMP2[ReleaseWorker + Retry]
    
    %% Styling
    style S1 fill:#e8f5e9,stroke:#2e7d32
    style S2 fill:#e8f5e9,stroke:#2e7d32
    style S3 fill:#e8f5e9,stroke:#2e7d32
    style S4 fill:#e8f5e9,stroke:#2e7d32
```

### 3.3 Cancellation Saga
**Goal**: Safely abort a running or queued job.
**Trigger**: User API Call (`CancelJob`)

```mermaid
flowchart TB
    START((Start))

    subgraph Steps
        S1[NotifyWorkerStep]
        S2[UpdateJobStateStep]
        S3[ReleaseWorkerStep]
    end

    subgraph Commands
        C1[NotifyWorkerCommand]
        C2[UpdateJobStateCommand]
        C3[ReleaseWorkerCommand]
    end

    START --> S1
    S1 -->|Dispatch| C1
    C1 -->|gRPC Cancel| WORKER[Worker Agent]
    
    S1 --> S2
    S2 -->|Dispatch| C2
    C2 -->|DB Update| STATE[Job Cancelled]
    
    S2 --> S3
    S3 -->|Dispatch| C3
    C3 -->|Registry Update| REG[Worker Ready]

    style S1 fill:#ffebee,stroke:#c62828
    style S2 fill:#ffebee,stroke:#c62828
    style S3 fill:#ffebee,stroke:#c62828
```

---

## 4. Lifecycle State Machines

### 4.1 Job Lifecycle
Tracks the progress of a computational task.

```mermaid
stateDiagram-v2
    [*] --> CREATED
    CREATED --> QUEUED: queue_job()
    
    QUEUED --> PENDING: Provisioning Started
    note right of PENDING: Waiting for infrastructure
    
    PENDING --> ASSIGNED: Worker Ready
    QUEUED --> ASSIGNED: Worker Ready (Hot)
    
    ASSIGNED --> RUNNING: Job Dispatch
    
    RUNNING --> SUCCEEDED: Exit Code 0
    RUNNING --> FAILED: Exit Code != 0 / Error
    
    RUNNING --> CANCELLED: User Cancel
    QUEUED --> CANCELLED: User Cancel
    PENDING --> CANCELLED: User Cancel
    
    FAILED --> PENDING: Retry (attempts < max)
    FAILED --> [*]: Max Retries
    SUCCEEDED --> [*]
    CANCELLED --> [*]
```

### 4.2 Worker Lifecycle
Tracks the ephemeral infrastructure life.

```mermaid
stateDiagram-v2
    [*] --> PROVISIONING: Create Call
    PROVISIONING --> STARTING: Container Created
    
    STARTING --> READY: OTP Registration
    note right of READY: Available for Jobs
    
    READY --> BUSY: Job Assigned
    BUSY --> READY: Job Completed
    
    READY --> TERMINATED: Idle TTL
    BUSY --> DISCONNECTED: Heartbeat Missed
    DISCONNECTED --> READY: Reconnected
    DISCONNECTED --> ORPHANED: Cleanup Failed
    
    TERMINATED --> [*]
```

---

## 5. Component Architecture (Bounded Contexts)

### 5.1 Jobs Context (`crates/server/domain/src/jobs`)
*   **Aggregate**: `Job`
*   **Responsibilities**: Job Specifications, State Transitions, Retry Logic.
*   **Events**: `JobQueued`, `JobStarted`, `JobCompleted`.

### 5.2 Workers Context (`crates/server/domain/src/workers`)
*   **Aggregate**: `Worker`
*   **Responsibilities**: Registry, Heartbeats, Resource Usage, OTP Validation.
*   **Events**: `WorkerReady`, `WorkerTerminated`.

### 5.3 Infrastructure Context (`crates/server/infrastructure`)
*   **Providers**: `DockerProvider`, `K8sProvider`.
*   **Messaging**: `NatsEventBus` (Durable, At-least-once).
*   **Persistence**: `PostgresRepository`.

---

## 6. Critical Flows

### 6.1 Worker Self-Registration (OTP Flow)
This flow ensures Zero Trust security by requiring workers to authenticate with a one-time token injected during provisioning.

```mermaid
sequenceDiagram
    participant P as Provider (K8s/Docker)
    participant S as Server (Hodei)
    participant W as Worker Agent
    
    S->>S: Generate OTP + JobID
    S->>P: Create Pod (Env: HODEI_OTP=xyz)
    P->>W: Start Container
    W->>W: Read HODEI_OTP
    W->>S: gRPC Register(OTP)
    S->>S: Validate OTP
    alt Valid
        S-->>W: OK (Certificates/Config)
        S->>NATS: Publish WorkerReady
    else Invalid
        S-->>W: Unauthenticated
        S->>P: Destroy Pod
    end
```

### 6.2 Job Execution Flow
End-to-end flow from user submission to results.

```mermaid
sequenceDiagram
    actor U as User
    participant API as gRPC API
    participant DB as Postgres
    participant SAGA as ExecutionSaga
    participant W as Worker
    
    U->>API: Submit Job
    API->>DB: Insert Job (CREATED)
    API->>NATS: Pub JobCreated
    
    NATS->>Coordinator: JobCreated
    Coordinator->>DB: job.queue() (QUEUED)
    
    Note over Coordinator, W: (Provisioning happens here if needed)
    
    Coordinator->>SAGA: Match (Job <-> Worker)
    SAGA->>SAGA: ValidateJob
    SAGA->>SAGA: AssignWorker
    SAGA->>W: gRPC RunJob(Spec)
    W-->>SAGA: ACK
    
    W->>W: Execute...
    W->>SAGA: gRPC Complete(Result)
    
    SAGA->>DB: Update Job (SUCCEEDED)
    SAGA->>DB: Release Worker
    SAGA->>NATS: Pub JobCompleted
```

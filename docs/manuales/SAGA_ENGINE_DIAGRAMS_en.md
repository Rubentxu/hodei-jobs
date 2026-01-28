# Saga Engine V4 - Architecture Diagrams Manual

## Complete Visual Guide: Understanding Architecture Through Diagrams

---

> **What is this document?**
> This manual uses Mermaid diagrams to visualize Saga Engine V4's key concepts. Each diagram includes a detailed explanation of what it represents, why it matters, and how components relate to each other.
>
> **How to use this manual**:
> 1. Read each diagram's description
> 2. Observe how components connect
> 3. Check the referenced source code for implementation details

---

## Table of Contents

1. [General System Context](#1-general-system-context)
2. [Hexagonal Architecture (Ports & Adapters)](#2-hexagonal-architecture-ports--adapters)
3. [Directory Structure](#3-directory-structure)
4. [Workflow Execution Flow](#4-workflow-execution-flow)
5. [Event Model](#5-event-model)
6. [Compensation (Rollback) Pattern](#6-compensation-rollback-pattern)
7. [Workflow State Diagram](#7-workflow-state-diagram)
8. [Concurrency: Optimistic Locking](#8-concurrency-optimistic-locking)
9. [Snapshot Strategy](#9-snapshot-strategy)
10. [Timer Architecture](#10-timer-architecture)
11. [Task Queue with NATS JetStream](#11-task-queue-with-nats-jetstream)
12. [Activity Registry](#12-activity-registry)
13. [Error Hierarchy](#13-error-hierarchy)
14. [Codec Comparison](#14-codec-comparison)
15. [Watchdog System](#15-watchdog-system)
16. [How to Read and Create Mermaid Diagrams](#16-how-to-read-and-create-mermaid-diagrams)

---

## 1. General System Context

### 1.1 Panoramic View

This diagram shows how Saga Engine V4 fits into the Hodei Jobs platform and its external dependencies.

```mermaid
graph TB
    subgraph "Hodei Jobs Platform"
        SE[Saga Engine V4]
    end
    
    subgraph "Clients / Producers"
        API[REST/gRPC API]
        Scheduler[Job Scheduler]
        Webhook[Webhook Handler]
        CLI[CLI Tool]
    end
    
    subgraph "Saga Engine - Core"
        DE[Durable Execution]
        ES[Event Sourcing]
        CP[Compensation Pattern]
        TE[Temporal Events]
    end
    
    subgraph "Infrastructure Adapters"
        PG[(PostgreSQL)]
        NS[NATS JetStream]
        SK[(SQLite)]
        IN[(In-Memory)]
    end
    
    subgraph "Monitoring"
        MT[Metrics<br/>Prometheus]
        TR[Tracing<br/>Jaeger]
        LG[Logs]
    end
    
    API --> SE
    Scheduler --> SE
    Webhook --> SE
    CLI --> SE
    
    SE --> DE
    SE --> ES
    SE --> CP
    SE --> TE
    
    DE --> PG
    ES --> PG
    CP --> PG
    TE --> PG
    
    DE --> NS
    ES --> NS
    TE --> NS
    
    DE --> SK
    ES --> SK
    
    DE --> IN
    ES --> IN
    
    SE --> MT
    SE --> TR
    SE --> LG
```

**Detailed explanation**:

| Component | Description | Role |
|-----------|-------------|------|
| **API/Scheduler/Webhook/CLI** | Workflow producers | Initiate new workflows |
| **Durable Execution** | Execution engine | Ensure workflows complete |
| **Event Sourcing** | Event persistence | Record every change |
| **Compensation Pattern** | Failure handling | Rollback on error |
| **Temporal Events** | Timers and delays | Schedule future actions |
| **PostgreSQL** | Primary persistence | Store events and state |
| **NATS JetStream** | Message queues | Transmit tasks |
| **SQLite/In-Memory** | Testing adapters | Enable testing |

### 1.2 Main Data Flow

```
Producers → Saga Engine → Infrastructure → Consumers

  API           Durable              PostgreSQL        Metrics
  Scheduler  →  Execution      →      NATS        →   Tracing
  Webhook        Event                 SQLite          Logs
  CLI            Sourcing
                 Compensation
                 Timers
```

---

## 2. Hexagonal Architecture (Ports & Adapters)

### 2.1 The Ports and Adapters Pattern

This diagram shows how Saga Engine separates business logic from infrastructure.

```mermaid
graph TB
    subgraph "Application Layer"
        ENG[SagaEngine]
    end
    
    subgraph "Domain Core - Pure Rust"
        D1[Workflow Definition]
        D2[Activity]
        D3[Domain Events]
        D4[Compensation]
        D5[Saga Context]
    end
    
    subgraph "Inbound Ports"
        IP1[EventStore<br/>Trait]
        IP2[TaskQueue<br/>Trait]
        IP3[TimerStore<br/>Trait]
        IP4[SignalDispatcher<br/>Trait]
        IP5[HistoryReplayer<br/>Trait]
    end
    
    subgraph "Outbound Adapters"
        subgraph "PostgreSQL"
            AD1[PostgresEventStore]
            AD2[PostgresTimerStore]
            AD3[PostgresReplayer]
        end
        
        subgraph "NATS"
            AD4[NatsTaskQueue]
            AD5[NatsEventBus]
            AD6[NatsSignalDispatcher]
        end
        
        subgraph "Testing"
            AD7[InMemoryEventStore]
            AD8[InMemoryTaskQueue]
        end
    end
    
    ENG --> D1
    ENG --> D2
    ENG --> D3
    ENG --> D4
    ENG --> D5
    
    ENG --> IP1
    ENG --> IP2
    ENG --> IP3
    ENG --> IP4
    ENG --> IP5
    
    IP1 <--> AD1
    IP1 <--> AD7
    IP2 <--> AD4
    IP2 <--> AD8
    IP3 <--> AD2
    IP4 <--> AD6
    IP5 <--> AD3
```

**Key concepts**:

- **Domain Core (Green)**: Pure business logic, no external dependencies
- **Inbound Ports (Yellow)**: Contracts (traits) defining what the domain needs
- **Outbound Adapters (Blue/Orange)**: Concrete implementations for each technology

### 2.2 Benefits of This Architecture

```mermaid
graph LR
    subgraph "Without Hexagonal"
        A[App] --> B[MySQL<br/>Coupled]
        A --> C[Redis<br/>Coupled]
        A --> D[NATS<br/>Coupled]
    end
    
    subgraph "With Hexagonal"
        E[Domain] --> P1[Port]
        P1 <--> A1[MySQL]
        P1 <--> A2[Postgres]
        P1 <--> A3[SQLite]
    end
    
    style E fill:#c8e6c9
    style P1 fill:#bbdefb
    style A1 fill:#ffecb3
    style A2 fill:#ffecb3
    style A3 fill:#ffecb3
```

**Decoupling**: You can change the database without modifying the domain.

---

## 3. Directory Structure

### 3.1 Code Organization

```mermaid
graph TD
    root[crates/saga-engine/]
    
    root --> core[core/]
    root --> pg[pg/]
    root --> nats[nats/]
    root --> sqlite[sqlite/]
    root --> local[local/]
    root --> testing[testing/]
    
    core --> core_src[src/]
    core_src --> event[event/]
    core_src --> workflow[workflow/]
    core_src --> activity[activity_registry/]
    core_src --> compensation[compensation/]
    core_src --> port[port/]
    core_src --> codec[codec/]
    core_src --> snapshot[snapshot/]
    
    port --> es[event_store.rs]
    port --> tq[task_queue.rs]
    port --> ts[timer_store.rs]
    port --> sd[signal_dispatcher.rs]
    port --> hr[history_replayer.rs]
    
    pg --> pg_src[src/]
    pg_src --> es_pg[event_store.rs]
    pg_src --> ts_pg[timer_store.rs]
    pg_src --> rp[replayer.rs]
    pg_src --> eng[engine.rs]
    
    nats --> nats_src[src/]
    nats_src --> tq_nats[task_queue.rs]
    nats_src --> eb[event_bus.rs]
    nats_src --> sd_nats[signal_dispatcher.rs]
    
    style root fill:#e3f2fd
    style core fill:#c8e6c9
    style pg fill:#ffecb3
    style nats fill:#bbdefb
```

**Color legend**:

| Color | Meaning | Content |
|-------|---------|---------|
| **Light blue** | Root | Entry point |
| **Green** | Core domain | Pure logic |
| **Yellow** | PostgreSQL | Persistent implementation |
| **Blue** | NATS | Messaging implementation |

### 3.2 Layer Correspondence

```
┌─────────────────────────────────────────────┐
│           Application Layer                  │
│           (SagaEngine facade)                │
├─────────────────────────────────────────────┤
│              Domain Core                     │
│  ┌──────────┬──────────┬──────────┐        │
│  │ workflow │ activity │event     │        │
│  └──────────┴──────────┴──────────┘        │
├─────────────────────────────────────────────┤
│              Ports (Traits)                  │
│  ┌──────────┬──────────┬──────────┐        │
│  │EventStore│TaskQueue │TimerStore│        │
│  └──────────┴──────────┴──────────┘        │
├─────────────────────────────────────────────┤
│          Adapters (Infrastructure)           │
│  ┌──────────┬──────────┬──────────┐        │
│  │Postgres  │  NATS    │ InMemory │        │
│  └──────────┴──────────┴──────────┘        │
└─────────────────────────────────────────────┘
```

---

## 4. Workflow Execution Flow

### 4.1 Complete Sequence

This step-by-step sequence diagram shows how a workflow executes.

```mermaid
sequenceDiagram
    autonumber
    participant C as Client
    participant SE as SagaEngine
    participant ES as EventStore
    participant TQ as TaskQueue
    participant W as Worker
    participant A as Activity
    
    C->>SE: start_workflow(input)
    SE->>ES: append_event(WorkflowExecutionStarted)
    ES-->>SE: event_id = 0
    
    SE->>TQ: publish(WorkflowTask)
    TQ-->>SE: task_id
    
    Note over SE: Workflow scheduled
    
    loop Worker Pool
        W->>TQ: fetch()
        TQ-->>W: WorkflowTask
        W->>SE: resume_workflow()
        
        SE->>ES: get_history()
        ES-->>SE: events[]
        
        SE->>SE: replay_events()
        SE->>SE: run_workflow()
        
        Note over SE: execute_activity() called
        
        SE->>ES: append_event(ActivityTaskScheduled)
        SE->>TQ: publish(ActivityTask)
        SE-->>W: Paused
        
        W->>TQ: fetch()
        TQ-->>W: ActivityTask
        W->>A: execute(input)
        A-->>W: output
        W->>TQ: ack()
        
        W->>SE: complete_activity_task()
        SE->>ES: append_event(ActivityTaskCompleted)
        SE->>TQ: publish(WorkflowTask)
    end
    
    SE->>ES: append_event(WorkflowExecutionCompleted)
    SE-->>C: output
```

### 4.2 Step by Step

| Step | Description | What Happens |
|------|-------------|--------------|
| 1 | **Start Workflow** | Client sends input to engine |
| 2 | **Record Initial Event** | `WorkflowExecutionStarted` is persisted |
| 3 | **Publish Task** | Task goes to NATS queue |
| 4 | **Worker Fetches** | A worker picks up the task |
| 5 | **Replay** | State is reconstructed from events |
| 6 | **Execute Workflow** | Workflow logic runs |
| 7 | **Schedule Activity** | An activity is scheduled |
| 8 | **Worker Processes** | Activity executes |
| 9 | **Activity Completed** | Result is recorded |
| 10 | **Workflow Continues** | Workflow resumes |
| 11 | **Workflow Complete** | Final event is recorded |

### 4.3 Durability Guaranteed

```mermaid
flowchart LR
    subgraph "Without Durable Execution"
        A[Task] --> B[Worker]
        B --> C[Restart]
        C --> D[Lost!]
    end
    
    subgraph "With Durable Execution"
        E[Task] --> F[Worker]
        F --> G[Event Store]
        G --> H[Restart]
        H --> I[Recovered<br/>from events]
    end
    
    style D fill:#ffcdd2
    style I fill:#c8e6c9
```

---

## 5. Event Model

### 5.1 Event Taxonomy

```mermaid
graph TD
    HE[HistoryEvent<br/>SAGA History] --> ET[EventType<br/>Event Type]
    HE --> EC[EventCategory<br/>Category]
    HE --> ATTR[Attributes<br/>JSON Payload]
    
    ET --> WE[Workflow Events<br/>7 types]
    ET --> AE[Activity Events<br/>6 types]
    ET --> TE[Timer Events<br/>3 types]
    ET --> SE[Signal Events<br/>1 type]
    ET --> ME[Marker Events<br/>1 type]
    ET --> SNE[Snapshot Events<br/>1 type]
    ET --> CE[Command Events<br/>3 types]
    ET --> CWE[Child Workflow Events<br/>9 types]
    ET --> LAE[Local Activity Events<br/>6 types]
    ET --> SEE[Side Effect Events<br/>1 type]
    ET --> UE[Update Events<br/>5 types]
    ET --> SAE[Search Attribute Events<br/>1 type]
    ET --> NE[Nexus Events<br/>7 types]
    
    EC --> WC[Workflow]
    EC --> AC[Activity]
    EC --> TC[Timer]
    EC --> SC[Signal]
    EC --> MC[Marker]
    EC --> SnC[Snapshot]
    EC --> CC[Command]
    EC --> CwC[ChildWorkflow]
    EC --> LAC[LocalActivity]
    EC --> SeC[SideEffect]
    EC --> UC[Update]
    EC --> SaC[SearchAttribute]
    EC --> NC[Nexus]
    
    style HE fill:#fff9c4
    style ET fill:#c8e6c9
    style EC fill:#bbdefb
```

### 5.2 Category Details

| Category | Main Events | Purpose |
|----------|-------------|---------|
| **Workflow** | Started, Completed, Failed, TimedOut, Canceled | Workflow lifecycle |
| **Activity** | Scheduled, Started, Completed, Failed, TimedOut, Canceled | Activity execution |
| **Timer** | Created, Fired, Canceled | Timers |
| **Signal** | SignalReceived | External signals |
| **Marker** | MarkerRecorded | Special marks |
| **Snapshot** | SnapshotCreated | State snapshots |

### 5.3 JSON Event Example

```json
{
  "event_id": 15,
  "saga_id": "saga-123e4567-e89b-12d3-a456-426614174000",
  "event_type": "ActivityTaskCompleted",
  "category": "Activity",
  "timestamp": "2024-01-15T10:30:00.000Z",
  "attributes": {
    "activity_type": "ProcessPayment",
    "input": { "order_id": "order-456", "amount": 99.99 },
    "output": { "transaction_id": "txn-abc123" }
  },
  "event_version": 1,
  "is_reset_point": false,
  "is_retry": false,
  "trace_id": "span-abc123def456"
}
```

---

## 6. Compensation (Rollback) Pattern

### 6.1 Normal vs. Compensation Flow

```mermaid
flowchart LR
    subgraph "Normal Flow"
        S1[Step 1<br/>Reserve Inventory] -->|OK| S2[Step 2<br/>Charge Payment]
        S2 -->|OK| S3[Step 3<br/>Ship Order]
        S3 -->|OK| DONE[Done<br/>Order Complete]
    end
    
    subgraph "Compensation Flow"
        S1 -->|OK| S2
        S2 -->|OK| S3
        S3 -->|FAIL| ROLL[Rollback]
        
        ROLL --> C3[Comp 3<br/>Cancel Shipment]
        C3 -->|OK| C2[Comp 2<br/>Refund Payment]
        C2 -->|OK| C1[Comp 1<br/>Release Inventory]
        
        C1 --> FAILED[Failed<br/>Saga Terminated]
    end
    
    style ROLL fill:#ffcdd2
    style C3 fill:#ffcdd2
    style C2 fill:#ffcdd2
    style C1 fill:#ffcdd2
    style FAILED fill:#ffcdd2
```

### 6.2 Compensation Order (LIFO)

```mermaid
flowchart TD
    A[Execution order] --> B[1. Reserve Inventory]
    B --> C[2. Charge Payment]
    C --> D[3. Ship Order]
    D -->|FAILS| E[Fault detected]
    
    E --> F[Compensation order]
    F --> G[1. Cancel Shipment<br/>(compensation of step 3)]
    G --> H[2. Refund Payment<br/>(compensation of step 2)]
    H --> I[3. Release Inventory<br/>(compensation of step 1)]
    
    style E fill:#ffcdd2
    style F fill:#ffecb3
    style G fill:#ffcdd2
    style H fill:#ffcdd2
    style I fill:#ffcdd2
```

**Important**: Compensations execute in **reverse order** (LIFO - Last In, First Out).

### 6.3 Compensation Data Model

```mermaid
classDiagram
    class CompensationTracker {
        <<service>>
        +steps: Vec~CompletedStep~
        +auto_compensate: bool
        +track_step()
        +get_compensation_actions()
    }
    
    class CompletedStep {
        <<entity>>
        +step_id: String
        +activity_type: String
        +compensation_activity_type: String
        +input: Value
        +output: Value
        +step_order: u32
        +completed_at: DateTime
    }
    
    class CompensationAction {
        <<value object>>
        +step_id: String
        +activity_type: String
        +compensation_type: String
        +input: Value
        +retry_count: u32
        +max_retries: u32
    }
    
    CompensationTracker "1" --> "*" CompletedStep : tracks
    CompletedStep "1" --> "*" CompensationAction : generates
```

---

## 7. Workflow State Diagram

### 7.1 State Machine

```mermaid
stateDiagram-v2
    [*] --> Running : start_workflow()
    
    Running --> Paused : execute_activity()
    Running --> Completed : All steps done
    Running --> Failed : Unhandled error
    Running --> Cancelled : cancel_workflow()
    
    Paused --> Running : Activity completed
    Paused --> Failed : Activity failed
    Paused --> Cancelled : cancel_workflow()
    
    Completed --> [*]
    Failed --> Compensating : Has compensation actions
    Failed --> [*] : No compensation needed
    
    Compensating --> Compensating : Execute next compensation
    Compensating --> Cancelled : All compensations done
    Compensating --> Failed : Compensation failed
    
    Cancelled --> [*]
    
    note right of Running : Replaying events from history on every state transition
```

### 7.2 State Transitions

| From | To | Trigger |
|------|-----|---------|
| Initial | Running | `start_workflow()` |
| Running | Paused | `execute_activity()` |
| Running | Completed | All steps done |
| Running | Failed | Unhandled error |
| Running | Cancelled | `cancel_workflow()` |
| Paused | Running | Activity completed |
| Paused | Failed | Activity failed |
| Failed | Compensating | Has compensations |
| Failed | Terminal | No compensations |
| Compensating | Cancelled | All done |
| Compensating | Failed | Compensation failed |

### 7.3 Replay on Every Transition

```mermaid
flowchart TD
    A[Current state] --> B[New transition]
    B --> C[get_history() from DB]
    C --> D[Reconstruct state]
    D --> E[Apply transition]
    E --> F[New state]
    
    style C fill:#bbdefb
    style D fill:#c8e6c9
```

---

## 8. Concurrency: Optimistic Locking

### 8.1 How It Works

```mermaid
sequenceDiagram
    participant W1 as Worker 1
    participant ES as EventStore
    participant W2 as Worker 2
    
    Note over W1: Saga 123 at event_id=5
    
    W1->>ES: append_event(saga_id=123, expected=5, event_A)
    ES->>ES: SELECT event_id FROM saga_events WHERE saga_id=123
    ES-->>ES: current_event_id = 5
    ES->>ES: 5 == 5 ✓
    ES->>ES: INSERT INTO saga_events...
    ES-->>W1: OK (event_id=5)
    
    Note over W2: Saga 123 at event_id=5 (stale)
    
    W2->>ES: append_event(saga_id=123, expected=5, event_B)
    ES->>ES: SELECT event_id FROM saga_events WHERE saga_id=123
    ES-->>ES: current_event_id = 6
    ES->>ES: 5 != 6 ✗
    ES-->>W2: ConflictError!
    
    W2->>ES: get_history(saga_id=123)
    ES-->>W2: events[0..6]
    
    W2->>W2: Replay to reconstruct state
    W2->>ES: append_event(saga_id=123, expected=6, event_B)
    ES-->>W2: OK (event_id=6)
```

### 8.2 Why Optimistic Over Pessimistic

```mermaid
graph TD
    subgraph "Optimistic Locking"
        O1[Read<br/>No lock] --> O2[Check version]
        O2 --> O3[Write if match]
        O4[Read<br/>No lock] --> O5[Check version]
        O5 --> O6[Write if match]
    end
    
    subgraph "Pessimistic Locking"
        P1[Read<br/>Acquire lock] --> P2[Write<br/>Hold lock]
        P3[Read<br/>Wait...] --> P4[Wait for lock]
        P4 --> P5[Write<br/>Release lock]
    end
    
    style P3 fill:#ffcdd2
    style P4 fill:#ffcdd2
```

| Aspect | Optimistic | Pessimistic |
|--------|------------|-------------|
| **Throughput** | High | Low (contention) |
| **Complexity** | Handle conflicts | Simpler code |
| **When to use** | Low contention | High contention |
| **Saga Engine** | ✅ Chosen | ❌ |

---

## 9. Snapshot Strategy

### 9.1 The Full Replay Problem

```mermaid
flowchart TB
    subgraph "Without Snapshots"
        A[Event 0] --> B[Event 1]
        B --> C[...]
        C --> D[Event 999]
        D --> E[Event 1000]
        E --> F[Full replay<br/>1000 events]
    end
    
    subgraph "With Snapshots every 100"
        G[Event 0] --> H[...]
        H --> I[Event 100<br/>⭐ SNAPSHOT]
        I --> J[...]
        J --> K[Event 200<br/>⭐ SNAPSHOT]
        K --> L[...]
        L --> M[Replay only<br/>200 events]
    end
```

### 9.2 Snapshot Flow

```mermaid
flowchart TB
    subgraph "State Reconstruction"
        A[Start replay] --> B{Snapshot exists?}
        B -->|Yes| C[Get snapshot<br/>last_event_id=50]
        B -->|No| D[Empty initial state]
        
        C --> E[get_history_from(event_id=50)]
        D --> F[get_history()]
        
        E --> G[Events 51..100]
        F --> H[Events 0..100]
        
        G --> I[replayer.replay(snapshot, events)]
        H --> I
        
        I --> J[Reconstructed state<br/>at event 100]
    end
    
    subgraph "Snapshot Creation"
        K[New event appended] --> L[Counter++]
        L --> M{Counter >= interval?}
        M -->|Yes| N[Create snapshot]
        M -->|No| O[Continue]
        N --> P[Save to EventStore]
        P --> Q[Reset counter]
    end
    
    style C fill:#c8e6c9
    style N fill:#bbdefb
```

### 9.3 Snapshot Configuration

```rust
pub struct SnapshotConfig {
    /// Events between snapshots (default: 100)
    pub interval: u64,
    
    /// Maximum snapshots per saga (default: 5)
    pub max_snapshots: u32,
    
    /// Enable SHA-256 checksums (default: true)
    pub enable_checksum: bool,
}
```

---

## 10. Timer Architecture

### 10.1 Timer System Components

```mermaid
flowchart TD
    subgraph "Timer Scheduler"
        TS[Timer Scheduler<br/>Polling Loop]
    end
    
    subgraph "Timer Store"
        TS --> TSQ[Timer Store Query<br/>SELECT WHERE fire_at <= NOW()]
        TSQ --> CLAIM[claim_timers()<br/>UPDATE status = Processing]
        CLAIM --> TIMERS[Expired Timers List]
    end
    
    subgraph "Event Processing"
        TIMERS --> LOOP[For each timer]
        LOOP --> E1[append_event(TimerFired)]
        E1 --> E2[SignalDispatcher.notify()]
        E2 --> E3[Workflow resumes]
    end
    
    subgraph "Timer Types"
        TT1[WorkflowTimeout<br/>SAGA level]
        TT2[ActivityTimeout<br/>ACTIVITY level]
        TT3[Sleep<br/>User delay]
        TT4[RetryBackoff<br/>Exponential backoff]
        TT5[Scheduled<br/>Cron-style]
    end
    
    TT1 --> TS
    TT2 --> TS
    TT3 --> TS
    TT4 --> TS
    TT5 --> TS
```

### 10.2 Timer Types

| Type | Description | Use Case |
|------|-------------|----------|
| **WorkflowTimeout** | Saga-level timeout | Cancel abandoned sagas |
| **ActivityTimeout** | Per-activity timeout | Fail-fast on slow activities |
| **Sleep** | User-defined delay | Wait before continuing |
| **RetryBackoff** | Exponential backoff | Retry with delay |
| **Scheduled** | Cron-style | Scheduled jobs |

### 10.3 Timer Flow

```mermaid
sequenceDiagram
    participant W as Workflow
    participant TS as TimerScheduler
    participant DB as TimerStore
    participant ES as EventStore
    
    W->>TS: create_timer("delay", 1h)
    TS->>DB: INSERT timer (fire_at = now + 1h)
    
    Note over TS: Polling every 10 seconds
    
    loop Polling
        TS->>DB: SELECT WHERE fire_at <= now
        DB-->>TS: [timer-123]
        TS->>DB: UPDATE status = Processing
    end
    
    TS->>ES: append_event(TimerFired)
    ES-->>TS: OK
    
    TS->>W: notify(timer fired)
    W->>W: Continue execution
```

---

## 11. Task Queue with NATS JetStream

### 11.1 Queue Architecture

```mermaid
flowchart TB
    subgraph "Publish Path"
        P[Publisher] --> EN[Encode Task<br/>JSON/Bincode]
        EN --> PUB[Publish to Subject<br/>saga.tasks.workflow-id]
        PUB --> JS[NATS JetStream]
        JS --> STREAM[Stream: SAGA_TASKS]
    end
    
    subgraph "Consumer Path"
        WORKER[Worker] --> FETCH[pull().max_messages(N)]
        FETCH --> SUB[Subscribe Consumer<br/>Durable: saga-workers]
        SUB --> JS
        JS --> MSGS[Batch Messages]
        MSGS --> WORKER
        WORKER --> ACK[ack() or nak(delay)]
        ACK --> JS
    end
    
    subgraph "Message Lifecycle"
        NEW[New Message] --> PROCESS[Processing]
        PROCESS --> ACKED[Acked<br/>Removed from queue]
        PROCESS --> NAKED[Nak'd<br/>Redeliver after delay]
        PROCESS --> TERM[Terminated<br/>To DLQ]
    end
    
    style STREAM fill:#bbdefb
    style SUB fill:#c8e6c9
```

### 11.2 NATS JetStream Concepts

| Concept | Description |
|---------|-------------|
| **Stream** | Message persistence (like a Kafka topic) |
| **Consumer** | Subscription with state (tracked offset) |
| **Durable Consumer** | Consumer that persists its position |
| **Ack/NAK** | Confirm or reject message |
| **Pull Consumer** | Worker explicitly requests messages |

### 11.3 Delivery Guarantees

```mermaid
flowchart LR
    subgraph "At-Least-Once Delivery"
        A[Publisher] -->|1| N[NATS]
        N -->|2| W1[Worker 1]
        N -->|3| W2[Worker 2]
        W1 -->|4| ACK[NATS]
        W2 -->|5| NAK[NATS]
        ACK -->|6| Remove
        NAK -->|7| Redeliver to W2
    end
```

---

## 12. Activity Registry

### 12.1 Registry Class Diagram

```mermaid
classDiagram
    class ActivityRegistry {
        <<service>>
        +activities: DashMap~String, Arc~dyn DynActivity~~
        +default_timeout: Duration
        +register_activity~A~()
        +has_activity~str~ bool
        +get_activity~str~ Option~Arc~dyn DynActivity~~
    }
    
    class ActivityTypeId {
        <<value object>>
        +String id
        +new~String~ ActivityTypeId
    }
    
    class DynActivity {
        <<interface>>
        +execute_dyn~Value~ Result~Value, ActivityError~
    }
    
    class Activity~T~ {
        <<interface>>
        +TYPE_ID: &'static str
        +Input
        +Output
        +Error
        +execute~Input~ Result~Output, Error~
    }
    
    class PaymentActivity {
        +TYPE_ID: "process-payment"
    }
    
    class InventoryActivity {
        +TYPE_ID: "reserve-inventory"
    }
    
    ActivityRegistry --> ActivityTypeId
    ActivityRegistry --> DynActivity
    DynActivity <|.. Activity~T~
    Activity~T~ <|-- PaymentActivity
    Activity~T~ <|-- InventoryActivity
```

### 12.2 Activity Registration

```rust
// ⭐ Register an activity
registry.register_activity(PaymentActivity);
registry.register_activity(ReserveInventoryActivity);
registry.register_activity(ShipOrderActivity);

// ⭐ Use the activity
let activity = registry.get_activity("process-payment");
let result = activity.execute(input).await;
```

---

## 13. Error Hierarchy

### 13.1 Error Structure

```mermaid
graph TD
    E[Error<br/>Central Error Type] --> EK[ErrorKind<br/>Enum ~17 variants~]
    E --> EM[message: String]
    E --> EC[context: HashMap~String, String~]
    E --> ES[source: Option~Box~Error~~]
    E --> ET[timestamp: SystemTime]
    
    EK --> EK1[EventStore]
    EK --> EK2[Codec]
    EK --> EK3[WorkflowExecution]
    EK --> EK4[StepExecution]
    EK --> EK5[ActivityExecution]
    EK --> EK6[TimerStore]
    EK --> EK7[SignalDispatcher]
    EK --> EK8[TaskQueue]
    EK --> EK9[Snapshot]
    EK --> EK10[Replay]
    EK --> EK11[Configuration]
    EK --> EK12[Validation]
    EK --> EK13[Timeout]
    EK --> EK14[Cancelled]
    EK --> EK15[Concurrency]
    EK --> EK16[Compensation]
    EK --> EK17[Unknown]
    
    style E fill:#fff9c4
    style EK fill:#c8e6c9
```

### 13.2 Error Types and Causes

| Error Kind | Typical Cause | Strategy |
|------------|---------------|----------|
| **EventStore** | DB temporarily unavailable | Retry with backoff |
| **WorkflowExecution** | Bug in workflow logic | Fix code |
| **ActivityExecution** | Bug in activity or external service | Fix or retry |
| **Timeout** | Activity too slow | Increase timeout |
| **Concurrency** | Two workers same saga | Automatic replay |
| **Compensation** | Compensation failed | Manual intervention |
| **Validation** | Invalid input | Validate before |

---

## 14. Codec Comparison

### 14.1 Serialization Performance

```mermaid
graph LR
    subgraph "Encode Performance"
        E1[JSON] -->|~1MB/s| E2[Bincode]
        E2 -->|~10MB/s| E3[Postcard]
    end
    
    subgraph "Decode Performance"
        D1[JSON] -->|~1MB/s| D2[Bincode]
        D2 -->|~10MB/s| D3[Postcard]
    end
    
    subgraph "Size"
        S1[JSON<br/>~2x original] --> S2[Bincode<br/>~1x original]
        S2 --> S3[Postcard<br/>~0.9x original]
    end
    
    subgraph "Human Readable"
        H1[JSON ✓] --> H2[Bincode ✗]
        H2 --> H3[Postcard ✗]
    end
    
    style E3 fill:#c8e6c9
    style D3 fill:#c8e6c9
    style S3 fill:#c8e6c9
```

### 14.2 Recommendations

| Scenario | Recommended Codec | Reason |
|----------|-------------------|--------|
| Development | JSON | Readable, easy debugging |
| Production | Bincode | Performance/size balance |
| High throughput | Postcard | Maximum speed |
| Limited storage | Postcard | Smallest size |

---

## 15. Watchdog System

### 15.1 Watchdog Architecture

```mermaid
flowchart TB
    subgraph "Watchdog Components"
        WD[Watchdog<br/>Orchestrator]
    end
    
    subgraph "Detection"
        SD[Stall Detector<br/>No progress > timeout]
        DD[Deadlock Detector<br/>Circular wait detected]
        HD[Health Detector<br/>Component unhealthy]
        CD[Capacity Detector<br/>Queue backlog]
    end
    
    subgraph "Actions"
        FA[Force Activity Timeout]
        FC[Force Cancellation]
        FR[Force Restart]
        AA[Alert + Auto-recovery]
    end
    
    subgraph "Health Checks"
        HC1[EventStore Ping]
        HC2[TaskQueue Connected]
        HC3[TimerStore Responsive]
    end
    
    SD --> WD
    DD --> WD
    HD --> WD
    CD --> WD
    
    WD --> FA
    WD --> FC
    WD --> FR
    WD --> AA
    
    FA --> HC1
    FC --> HC1
    FR --> HC1
    AA --> HC1
    
    style WD fill:#fff9c4
```

### 15.2 Detectors and Responses

| Detector | What It Detects | Response |
|----------|-----------------|----------|
| **Stall Detector** | Saga no progress for time | Alert + force timeout |
| **Deadlock Detector** | Circular waits | Terminate + compensate |
| **Health Detector** | Component unresponsive | Circuit breaker |
| **Capacity Detector** | Queue saturated | Scale workers |

---

## 16. How to Read and Create Mermaid Diagrams

### 16.1 Mermaid Quick Guide

```mermaid
flowchart TD
    A[Start] --> B{Decision?}
    B -->|Yes| C[Do something]
    B -->|No| D[Do other thing]
    C --> E[End]
    D --> E
```

**Basic syntax**:

| Type | Syntax | Description |
|------|--------|-------------|
| Node | `A[Name]` | Node with text |
| Arrow | `A --> B` | Connection |
| Decision | `B{?}` | Decision diamond |
| Subgraph | `subgraph X ... end` | Grouping |
| Style | `style A fill:#color` | Color |

### 16.2 Semantic Colors

| Color | Meaning | Usage |
|-------|---------|-------|
| **Green** | Success, happy path | Normal flow |
| **Red** | Error, failure | Failures, rollbacks |
| **Yellow** | Wait, pause | Intermediate states |
| **Blue** | Process, data | Processes, databases |
| **Orange** | Decision | Decision points |

### 16.3 Sequence Diagram Syntax

```mermaid
sequenceDiagram
    participant A as Actor
    participant S as System
    
    A->>S: Request
    S-->>A: Response
```

**Sequence syntax**:

| Element | Syntax |
|---------|--------|
| Participant | `participant X as Alias` |
| Sync message | `A->>B: message` |
| Async message | `A-->>B: message` |
| Note | `Note over A,B: text` |
| Loop | `loop Label ... end` |
| Alt | `alt Condition ... else ... end` |

---

## Appendix: Complete Architecture in One View

```mermaid
graph TB
    subgraph "Client Layer"
        API[REST/gRPC API]
        CLI[CLI]
    end
    
    subgraph "Application Layer"
        SE[SagaEngine]
    end
    
    subgraph "Domain Layer"
        WF[Workflows]
        AC[Activities]
        EV[Events]
        CP[Compensation]
    end
    
    subgraph "Ports Layer"
        ES[EventStore Port]
        TQ[TaskQueue Port]
        TS[TimerStore Port]
    end
    
    subgraph "Adapters Layer"
        PGE[Postgres Adapter]
        NAT[NATS Adapter]
        INM[InMemory Adapter]
    end
    
    API --> SE
    CLI --> SE
    SE --> WF
    SE --> AC
    WF --> EV
    AC --> EV
    WF --> CP
    
    WF --> ES
    AC --> TQ
    CP --> TS
    
    ES <--> PGE
    TQ <--> NAT
    TS <--> INM
```

---

*Document Version: 2.0.0*
*Format: Mermaid.js*
*Generated: 2026-01-28*
*Language: English*

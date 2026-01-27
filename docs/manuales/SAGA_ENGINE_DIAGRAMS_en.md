# Saga Engine V4 - Architecture Diagrams

This document contains Saga Engine architecture diagrams in Mermaid format.

---

## 1. General Context Diagram

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

---

## 2. Hexagonal Architecture (Ports & Adapters)

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

---

## 3. Directory Structure

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
    
    style root fill:#e3f2fd
    style core fill:#c8e6c9
    style pg fill:#ffecb3
    style nats fill:#bbdefb
```

---

## 4. Execution Flow Diagram

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
    TQ-->>
    
    Note over SE: Workflow scheduledSE: task_id
    
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

---

## 5. Event Model

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

---

## 6. Compensation (Rollback Pattern)

```mermaid
flowchart LR
    subgraph "Normal Flow"
        S1[Step 1<br/>Reserve Inventory] -->|OK| S2[Step 2<br/>Charge Payment]
        S2 -->|OK| S3[Step 3<br/>Ship Order]
        S3 -->|OK| DONE[Done<br/>Order Complete]
    end
    
    subgraph "Fallback Flow"
        S1 -->|OK| S2
        S2 -->|OK| S3
        S3 -->|FAIL| ROLL[Rollback]
        
        ROLL --> C3[Comp 3<br/>Cancel Shipment]
        C3 -->|OK| C2[Comp 2<br/>Refund Payment]
        C2 -->|OK| C1[Comp 1<br/>Release Inventory]
        
        C1 --> FAILED[Failed<br/>Saga Terminated]
    end
    
    style ROLL fill:#ffcdd2
    style C1 fill:#ffcdd2
    style C2 fill:#ffcdd2
    style C3 fill:#ffcdd2
    style FAILED fill:#ffcdd2
```

---

## 7. Workflow State Diagram

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
    
    note right of Running
        Replaying events from history
        on every state transition
    end note
```

---

## 8. Concurrency: Optimistic Locking

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

---

## 9. Snapshot Strategy

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
        L --> M{counter >= interval?}
        M -->|Yes| N[Create snapshot]
        M -->|No| O[Continue]
        N --> P[Save to EventStore]
        P --> Q[Reset counter]
    end
    
    style C fill:#c8e6c9
    style N fill:#bbdefb
```

---

## 10. Timer Architecture

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

---

## 11. Task Queue (NATS JetStream)

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

---

## 12. Activity Registry

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

---

## 13. Error Hierarchy

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

---

## 14. Codec Performance Comparison

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

---

## 15. Watchdog System

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

---

*Document Version: 1.0.0*
*Format: Mermaid.js*
*Generated: 2026-01-27*

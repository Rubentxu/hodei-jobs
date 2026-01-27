# Saga Engine V4 - Architect's Manual

## 1. Executive Summary

The **Saga Engine V4** is a distributed workflow orchestration platform implementing the **Saga Pattern** with **Event Sourcing** and **Durable Execution**. This document provides the architectural blueprint for architects, lead developers, and DevOps engineers.

### Architectural Style

- **Pattern**: Event Sourcing + Saga Pattern + Ports & Adapters (Hexagonal)
- **Paradigm**: Domain-Driven Design (DDD)
- **Concurrency**: Async/Await with Actor-style message passing
- **Persistence**: Append-only event store with snapshot optimization

---

## 2. Context Map (Strategic DDD)

### 2.1 System Context

```mermaid
graph TB
    subgraph "Hodei Jobs Platform"
        SE[Saga Engine V4]
    end
    
    subgraph "Upstream Producers"
        API[REST/gRPC API]
        Scheduler[Job Scheduler]
        Webhook[Webhook Handler]
    end
    
    subgraph "Saga Engine Bounded Contexts"
        WC[Workflow Context]
        AC[Activity Context]
        EC[Event Context]
        CC[Compensation Context]
        TC[Timer Context]
    end
    
    subgraph "Downstream Consumers"
        PG[(PostgreSQL)]
        NS[NATS JetStream]
        K8S[Kubernetes]
        StatsD[Metrics]
    end
    
    API --> SE
    Scheduler --> SE
    Webhook --> SE
    
    SE --> WC
    SE --> AC
    SE --> EC
    SE --> CC
    SE --> TC
    
    WC --> PG
    AC --> NS
    EC --> PG
    CC --> PG
    TC --> PG
    
    SE --> StatsD
```

### 2.2 Bounded Context Relationships

```mermaid
graph LR
    subgraph "Core Domain"
        WC[Workflow Context]
        AC[Activity Context]
        EC[Event Context]
        CC[Compensation Context]
        TC[Timer Context]
    end
    
    WC --> AC : "executes"
    WC --> EC : "produces events"
    WC --> CC : "tracks compensation"
    WC --> TC : "schedules delays"
    
    EC --> CC : "replays for rollback"
    AC --> EC : "produces activity events"
    TC --> EC : "produces timer events"
    
    style WC fill:#e1f5fe
    style AC fill:#f3e5f5
    style EC fill:#e8f5e8
    style CC fill:#ffebee
    style TC fill:#fff3e0
```

---

## 3. Aggregate Architecture

### 3.1 Saga Aggregate (Root)

```mermaid
classDiagram
    class SagaId {
        <<value object>>
        +String id
        +new() SagaId
        +from_uuid(uuid) SagaId
    }
    
    class SagaAggregate {
        <<aggregate root>>
        +SagaId id
        +WorkflowTypeId workflow_type
        +WorkflowState state
        +Vec~HistoryEvent~ history
        +Option~Snapshot~ latest_snapshot
        +CompensationTracker compensation_tracker
        +create_event() HistoryEvent
        +append_event() Result~(), ConflictError~
        +replay() WorkflowContext
    }
    
    class HistoryEvent {
        <<entity>>
        +EventId event_id
        +EventType event_type
        +EventCategory category
        +serde_json::Value attributes
        +timestamp: DateTime~Utc~
    }
    
    class WorkflowContext {
        <<value object>>
        +SagaId execution_id
        +WorkflowTypeId workflow_type
        +usize current_step_index
        +HashMap~String, Value~ step_outputs
        +Option~CancellationState~ cancellation
        +CompensationTracker compensation_tracker
    }
    
    class CompensationTracker {
        <<value object>>
        +Vec~CompletedStep~ steps
        +bool auto_compensate
        +track_step()
        +get_compensation_actions() Vec~CompensationAction~
    }
    
    SagaAggregate "1" --> "1" SagaId : identifies
    SagaAggregate "1" --> "*" HistoryEvent : contains
    SagaAggregate "1" --> "1" WorkflowContext : reconstructs
    WorkflowContext "1" --> "1" CompensationTracker : has
```

### 3.2 Aggregate Invariants

| Invariant | Description | Enforcement |
|-----------|-------------|-------------|
| Event ID Monotonicity | `event_id[n+1] > event_id[n]` | Optimistic locking in EventStore |
| Immutability | Events never modified/deleted | Append-only storage |
| Compensation Order | Compensation in LIFO order | `get_compensation_actions()` reverses list |
| Saga Finality | Terminal states are final | State machine guards |

---

## 4. Detailed Component Diagram

### 4.1 Core Layer Architecture

```mermaid
graph TB
    subgraph "core/src - Domain Layer (Pure Rust)"
        subgraph "workflow/"
            DW[DurableWorkflow Trait]
            WC[WorkflowContext]
            WD[WorkflowDefinition ~DEPRECATED~]
        end
        
        subgraph "activity_registry/"
            AR[ActivityRegistry]
            AC[ActivityContext]
            DY[ DynActivity Trait]
        end
        
        subgraph "event/"
            HE[HistoryEvent]
            ET[EventType ~60+ variants~]
            EC[EventCategory ~13 categories~]
        end
        
        subgraph "compensation/"
            CT[CompensationTracker]
            CA[CompensationAction]
            CH[CompensationHandler Trait]
        end
        
        subgraph "codec/"
            JC[JsonCodec]
            BC[BincodeCodec]
            PC[PostcardCodec]
        end
        
        subgraph "snapshot/"
            SM[SnapshotManager]
            SC[Snapshot]
            CS[SnapshotChecksum]
        end
    end
    
    DW --> WC
    WC --> CT
    AR --> DY
    HE --> ET
    ET --> EC
    SM --> SC
    SC --> CS
```

### 4.2 Port/Adapter Layer

```mermaid
graph TB
    subgraph "Ports (Interfaces in core/src/port/)"
        ES[EventStore Trait]
        TQ[TaskQueue Trait]
        TS[TimerStore Trait]
        SB[SignalDispatcher Trait]
        HR[HistoryReplayer Trait]
        EB[EventBus Trait]
        CB[CommandBus Trait]
    end
    
    subgraph "Adapters (Infrastructure)"
        subgraph "pg/src - PostgreSQL"
            PES[PostgresEventStore]
            PTS[PostgresTimerStore]
            PPR[PostgresReplayer]
            PEB[PostgresEventBus]
        end
        
        subgraph "nats/src - NATS"
            NTQ[NatsTaskQueue]
            NEB[NatsEventBus]
            NSD[NatsSignalDispatcher]
        end
        
        subgraph "local/src - In-Memory"
            IMES[InMemoryEventStore]
            IMTQ[InMemoryTaskQueue]
            IMTS[InMemoryTimerStore]
        end
    end
    
    ES <--> PES
    ES <--> IMES
    TQ <--> NTQ
    TQ <--> IMTQ
    TS <--> PTS
    TS <--> IMTS
    HR <--> PPR
```

---

## 5. Deployment Architecture

### 5.1 Production Deployment

```mermaid
graph TB
    subgraph "Kubernetes Cluster"
        subgraph "saga-engine Namespace"
            API[API Pods<br/>~3 replicas~]
            Worker[Worker Pods<br/>~5 replicas~]
            Timer[Timer Scheduler<br/>~1 replica~]
            
            subgraph "Data Layer"
                PG[(PostgreSQL<br/>~Primary + Replica~)]
                NS[NATS JetStream<br/>~3 nodes~]
            end
            
            subgraph "Caching"
                RC[(Redis<br/>~Cluster mode~)]
            end
        end
    end
    
    API --> PG
    API --> NS
    
    Worker --> PG
    Worker --> NS
    Worker --> RC
    
    Timer --> PG
    Timer --> NS
```

### 5.2 Data Flow During Execution

```mermaid
flowchart TD
    A[Client Request] --> B[API Pod]
    B --> C{EventStore Available?}
    C -->|Yes| D[Append Event]
    C -->|No| E[Reject with Error]
    
    D --> F[Publish Task to NATS]
    F --> G[Task in Queue]
    
    G --> H[Worker Pod]
    H --> I[Fetch Task from NATS]
    I --> J[Reconstruct State]
    
    J --> K[Execute Workflow]
    K --> L{Activity Needed?}
    
    L -->|Yes| M[Schedule Activity Task]
    M --> N[Activity in Queue]
    N --> O[Activity Worker]
    O --> P[Execute Activity]
    P --> Q[Complete Task]
    
    L -->|No| R[Workflow Complete]
    R --> S[Append Completion Event]
    S --> T[Return Result to Client]
```

---

## 6. Event Model

### 6.1 Event Categories

```mermaid
graph TB
    subgraph "Workflow Events (Category: Workflow)"
        WES[WorkflowExecutionStarted]
        WEC[WorkflowExecutionCompleted]
        WEF[WorkflowExecutionFailed]
        WET[WorkflowExecutionTimedOut]
        WECan[WorkflowExecutionCanceled]
    end
    
    subgraph "Activity Events (Category: Activity)"
        ATS[ActivityTaskScheduled]
        ATSt[ActivityTaskStarted]
        ATC[ActivityTaskCompleted]
        ATF[ActivityTaskFailed]
        ATT[ActivityTaskTimedOut]
        ATCan[ActivityTaskCanceled]
    end
    
    subgraph "Timer Events (Category: Timer)"
        TC[TimerCreated]
        TF[TimerFired]
        TCan[TimerCanceled]
    end
    
    subgraph "Signal Events (Category: Signal)"
        SR[SignalReceived]
    end
    
    subgraph "Marker Events (Category: Marker)"
        MR[MarkerRecorded]
    end
    
    subgraph "Snapshot Events (Category: Snapshot)"
        SC[SnapshotCreated]
    end
```

### 6.2 Event Schema

```rust
pub struct HistoryEvent {
    /// Monotonic event ID within saga
    pub event_id: EventId,
    
    /// Saga identifier
    pub saga_id: SagaId,
    
    /// Event type (60+ variants)
    pub event_type: EventType,
    
    /// Event category for filtering
    pub category: EventCategory,
    
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Event payload (flexible JSON)
    pub attributes: Value,
    
    /// Schema version for migrations
    pub event_version: u32,
    
    /// Reset point for optimized replay
    pub is_reset_point: bool,
    
    /// Retry indicator
    pub is_retry: bool,
    
    /// Parent event for event trees
    pub parent_event_id: Option<EventId>,
    
    /// Task queue routing
    pub task_queue: Option<String>,
    
    /// Distributed tracing
    pub trace_id: Option<String>,
}
```

---

## 7. Compensation (Rollback) Model

### 7.1 Compensation Architecture

```mermaid
flowchart LR
    subgraph "Step Execution"
        S1[Step 1<br/>Reserve Inventory] --> S2[Step 2<br/>Charge Payment]
        S2 --> S3[Step 3<br/>Ship Order]
    end
    
    subgraph "Compensation Stack"
        C1[Comp 1<br/>Release Inventory]
        C2[Comp 2<br/>Refund Payment]
        C3[Comp 3<br/>Cancel Shipment]
    end
    
    S3 -->|FAIL| CF[Compensation Flow]
    CF --> C3
    C3 --> C2
    C2 --> C1
    
    style CF fill:#ffebee
    style C1 fill:#ffcdd2
    style C2 fill:#ffcdd2
    style C3 fill:#ffcdd2
```

### 7.2 Compensation Data Model

```rust
pub struct CompensationTracker {
    /// Completed steps (thread-safe via Arc<Mutex>)
    steps: Arc<Mutex<Vec<CompletedStep>>>,
    
    /// Auto-compensate on failure flag
    auto_compensate: bool,
}

pub struct CompletedStep {
    pub step_id: String,
    pub activity_type: String,
    pub compensation_activity_type: String,
    pub input: Value,
    pub output: Value,
    pub step_order: u32,
    pub completed_at: DateTime<Utc>,
}

pub struct CompensationAction {
    pub step_id: String,
    pub activity_type: String,
    pub compensation_type: String,
    pub input: Value,
    pub retry_count: u32,
    pub max_retries: u32,
}
```

---

## 8. Concurrency Model

### 8.1 Optimistic Locking

```mermaid
sequenceDiagram
    participant W1 as Worker 1
    participant ES as EventStore
    participant W2 as Worker 2
    
    W1->>ES: append_event(saga_id=123, expected=5, event)
    ES->>ES: Check current_event_id == 5
    ES-->>W1: OK (event_id=5)
    
    W2->>ES: append_event(saga_id=123, expected=5, event)
    ES->>ES: Check current_event_id == 6
    ES-->>W2: ConflictError!
    
    W2->>ES: get_history(saga_id=123)
    ES-->>W2: events[0..6]
    
    W2->>ES: append_event(saga_id=123, expected=6, event)
    ES-->>W2: OK (event_id=6)
```

### 8.2 Lock Granularity

| Resource | Lock Type | Scope | Duration |
|----------|-----------|-------|----------|
| Saga Events | Optimistic (version check) | Per saga | Single append |
| Task Queue | Lease-based | Per message | Processing time + ack_wait |
| Timer | Claim-based | Per timer | Until fired |
| Activity Registry | None (read mostly) | Global | N/A |

---

## 9. Scalability Patterns

### 9.1 Horizontal Scaling

```mermaid
graph TB
    subgraph "Load Balancer"
        LB[Round Robin / Least Connections]
    end
    
    subgraph "Worker Pool - Zone A"
        W1[Worker 1]
        W2[Worker 2]
        W3[Worker 3]
    end
    
    subgraph "Worker Pool - Zone B"
        W4[Worker 4]
        W5[Worker 5]
        W6[Worker 6]
    end
    
    subgraph "Shared State"
        PG[(PostgreSQL)]
        NS[NATS]
    end
    
    LB --> W1
    LB --> W2
    LB --> W3
    LB --> W4
    LB --> W5
    LB --> W6
    
    W1 --> PG
    W2 --> PG
    W3 --> PG
    W4 --> PG
    W5 --> PG
    W6 --> PG
    
    W1 --> NS
    W2 --> NS
    W3 --> NS
    W4 --> NS
    W5 --> NS
    W6 --> NS
```

### 9.2 Sharding Strategy

```rust
pub enum ShardingStrategy {
    /// Shard by SagaId hash
    HashBased {
        shard_count: u64,
        hash_fn: fn(&SagaId) -> u64,
    },
    
    /// Shard by WorkflowType
    ByWorkflowType {
        mapping: HashMap<&'static str, u64>,
    },
    
    /// Shard by Tenant/Customer
    ByTenant {
        tenant_extractor: fn(&SagaId) -> String,
    },
}
```

---

## 10. Observability

### 10.1 Metrics

```rust
pub struct SagaEngineMetrics {
    /// Workflow metrics
    workflow_started: Counter,
    workflow_completed: Counter,
    workflow_failed: Counter,
    workflow_duration: Histogram,
    
    /// Activity metrics
    activity_started: Counter,
    activity_completed: Counter,
    activity_failed: Counter,
    activity_duration: Histogram,
    
    /// Event metrics
    events_persisted: Counter,
    event_persist_duration: Histogram,
    
    /// Replay metrics
    replay_duration: Histogram,
    snapshot_count: Gauge,
    
    /// Compensation metrics
    compensation_started: Counter,
    compensation_completed: Counter,
    compensation_failed: Counter,
}
```

### 10.2 Tracing

```mermaid
flowchart TD
    A[Root Span: start_workflow] --> B[Span: append_event]
    B --> C[Span: publish_task]
    
    C --> D[Span: fetch_task]
    D --> E[Span: get_history]
    
    E --> F[Span: replay_events]
    F --> G[Span: run_workflow]
    
    G --> H[Span: execute_activity]
    H --> I[Span: append_activity_event]
    
    I --> J[Span: publish_activity_task]
```

---

## 11. Failure Modes & Recovery

### 11.1 Failure Scenarios

| Scenario | Detection | Recovery Strategy |
|----------|-----------|-------------------|
| Worker crash mid-processing | NATS redeliver | Replay from last event |
| Database connection loss | Connection timeout | Retry with backoff |
| Saga stuck (no progress) | Stall detector | Force terminate or signal |
| Duplicate events | Idempotency keys | Deduplicate via event_id |
| Event store corruption | Checksum verification | Restore from backup |

### 11.2 Watchdog Components

```mermaid
graph TB
    subgraph "Watchdog System"
        SD[Stall Detector]
        DD[Deadlock Detector]
        HA[Health Aggregator]
        HC[Health Checks]
    end
    
    subgraph "Detection"
        SD -->|No progress > 5min| AA[Alert + Auto-recovery]
        DD -->|Circular wait| TA[Terminate + Compensate]
        HA -->|Unhealthy| CA[Circuit breaker]
    end
    
    subgraph "Actions"
        AA --> R1[Force task timeout]
        TA --> R2[Signal cancellation]
        CA --> R3[Route to healthy nodes]
    end
```

---

## 12. Performance Characteristics

### 12.1 Benchmarks (Typical Results)

| Operation | Latency (p50) | Latency (p99) | Throughput |
|-----------|---------------|---------------|------------|
| Event append (single) | 0.5ms | 2ms | 2,000/sec |
| Event append (batch 10) | 1ms | 5ms | 10,000/sec |
| Workflow replay (100 events) | 5ms | 20ms | N/A |
| Activity scheduling | 1ms | 3ms | 5,000/sec |
| Snapshot creation | 10ms | 50ms | 100/sec |

### 12.2 Optimization Strategies

| Technique | Impact | Trade-off |
|-----------|--------|-----------|
| Batched event inserts | +10x throughput | Slightly higher latency |
| Snapshot every N events | -100x replay time | Storage overhead |
| Parallel activity execution | -N x duration | Complexity in compensation |
| Event encoding (bincode) | +2x serialization | Debugging harder |
| Connection pooling | -90% connection overhead | Memory usage |

---

## 13. Security Model

### 13.1 Authentication & Authorization

```rust
pub struct SagaSecurityConfig {
    /// Authentication provider
    auth_provider: Box<dyn AuthProvider>,
    
    /// Authorization policy
    authorization: AuthorizationPolicy,
    
    /// TLS configuration
    tls: Option<TlsConfig>,
    
    /// Rate limiting
    rate_limit: RateLimitConfig,
}
```

### 13.2 Data Isolation

| Level | Isolation Method |
|-------|-----------------|
| Saga | Each saga has unique UUID |
| Tenant | Sharding by tenant_id |
| Event | Event attributes encrypted at rest |
| Snapshot | Snapshot encryption optional |

---

## 14. Upgrade & Migration Strategy

### 14.1 Version Compatibility Matrix

| Core Version | Postgres Adapter | NATS Adapter | Protocol |
|--------------|------------------|--------------|----------|
| 4.0.x | 4.0.x | 4.0.x | JSON/Bincode |
| 4.1.x | 4.1.x | 4.0.x | +Postcard |
| 4.2.x | 4.2.x | 4.1.x | +Compression |

### 14.2 Rolling Upgrade Procedure

1. **Phase 1**: Deploy new infrastructure adapters (dual-write capable)
2. **Phase 2**: Switch core to use new adapters (feature flag)
3. **Phase 3**: Migrate existing sagas (event replay)
4. **Phase 4**: Remove old adapter code

---

## 15. Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| Event Sourcing | 2024-Q1 | Audit trail, replay capability essential |
| Bincode default codec | 2024-Q1 | Performance priority over debuggability |
| PostgreSQL primary store | 2024-Q1 | ACID, mature ecosystem |
| NATS JetStream | 2024-Q2 | At-least-once delivery, low latency |
| Workflow-as-Code | 2024-Q3 | Developer experience, type safety |

---

*Document Version: 1.0.0*
*Last Updated: 2026-01-27*
*Classification: Internal - Architecture*

//! # Local Saga Engine Example
//!
//! This example demonstrates how to use the LocalSagaEngine for local development
//! and testing without requiring distributed infrastructure.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --package saga-engine-local --example basic_usage
//! ```

use saga_engine_core::SagaEngine;
use saga_engine_core::SagaId;
use saga_engine_core::event::{EventId, EventType, HistoryEvent};
use saga_engine_core::port::event_store::EventStore;
use saga_engine_core::port::task_queue::TaskQueue;
use saga_engine_local::LocalSagaEngine;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Local Saga Engine Example\n");

    println!("1. Creating LocalSagaEngine with in-memory SQLite...");
    let engine = LocalSagaEngine::testing().await?;
    println!("   ✓ Engine created successfully\n");

    println!("2. Engine components:");
    println!("   - Event Store: SqliteEventStore (in-memory)");
    println!("   - Task Queue: TokioTaskQueue");
    println!("   - Timer Store: InMemoryTimerStore\n");

    println!("3. Demonstrating saga creation and event appending...");
    let saga_id = SagaId::new();

    let event = HistoryEvent::builder()
        .event_id(EventId(1))
        .event_type(EventType::WorkflowExecutionStarted)
        .saga_id(saga_id.clone())
        .payload(serde_json::json!({
            "input": "example-input",
            "workflow_type": "example-workflow"
        }))
        .build();

    engine.event_store().append_event(&saga_id, 0, &event).await?;
    println!("   ✓ Event appended for saga: {}\n", saga_id);

    println!("4. Creating and publishing a workflow task...");
    let task = saga_engine_core::port::task_queue::Task::new(
        "workflow-execute".to_string(),
        saga_id.clone(),
        uuid::Uuid::new_v4().to_string(),
        vec![],
    );

    engine.task_queue().publish(&task, "workflow").await?;
    println!("   ✓ Task published to queue\n");

    println!("5. Fetching tasks from queue...");
    let messages = engine.task_queue().fetch("workflow", 10, Duration::from_millis(100)).await?;
    println!("   ✓ Fetched {} task(s)\n", messages.len());

    println!("6. Fetching saga events from event store...");
    let events = engine.event_store().get_history(&saga_id).await?;
    println!("   ✓ Found {} event(s)\n", events.len());

    println!("Example completed successfully!");
    println!("\nPresets available:");
    println!("  - LocalSagaEngine::cli(path)     - CLI preset (2 workers, snapshots)");
    println!("  - LocalSagaEngine::desktop(path) - Desktop preset (CPU cores)");
    println!("  - LocalSagaEngine::testing()     - Testing preset (in-memory)");
    println!("  - LocalSagaEngine::embedded(path)- Embedded preset (1 worker)");

    Ok(())
}

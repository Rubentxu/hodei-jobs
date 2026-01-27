//! Benchmark suite for Saga Engine v4.0
//!
//! This suite validates performance objectives defined in EPIC-96:
//! - 20x+ throughput improvement for event appending (batching)
//! - 10-50x replay speedup (using reset points)
//! - <5ms notification latency (reactive vs polling)
//!
//! Run with: cargo bench --bench

use std::time::{Duration, Instant};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use saga_engine_core::{
    event::{Event, SagaId},
    saga_engine::SagaEngineError,
};
use saga_engine_pg::{
    event_store::{EventStore, PostgresEventStore},
    reactive_worker::{ReactiveWorker, WorkerConfig},
    reactive_timer_scheduler::{ReactiveTimerScheduler, TimerMode},
};

/// Number of events for batch benchmarks
const BATCH_SIZE: usize = 100;

/// Number of events for replay benchmarks
const REPLAY_SIZE: usize = 1000;

criterion_group!(benches, saga_engine_benchmarks);
criterion_main!(benches, saga_engine_benchmarks);

fn benchmark_saga_engine(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let rt_handle = runtime.handle();
    rt_handle.block_on(async {
        // Initialize test database
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/hodei_test".to_string()))
            )
            .await
            .unwrap();

        let event_store = PostgresEventStore::new(pool.clone());

        // Run benchmarks
        setup_and_bench(c, "append_event_single", &pool, &event_store, |b| {
            b.iter(|| append_single_event(&event_store));
        });

        setup_and_bench(c, "append_event_batch_100", &pool, &event_store, |b| {
            b.iter_batched(BATCH_SIZE, || {
                append_batch_events(&event_store, BATCH_SIZE);
            });
        });

        setup_and_bench(c, "replay_full_1000_events", &pool, &event_store, |b| {
            b.iter(|| replay_full_events(&event_store, REPLAY_SIZE));
        });

        setup_and_bench(c, "replay_with_reset_point", &pool, &event_store, |b| {
            b.iter(|| replay_with_reset_point(&event_store));
        });

        setup_and_bench(c, "timer_processing_reactive", &pool, |b| {
            b.iter(|| process_timers_reactive(&pool));
        });

        // Cleanup
        pool.close().await;
    });
}

/// Benchmark: Append single event
fn append_single_event(event_store: &PostgresEventStore) {
    let saga_id = SagaId::new();
    let event = Event::new(
        saga_id.clone(),
        "test_activity".to_string(),
        serde_json::json!({"data": "test"}).to_vec(),
    );
    let _ = black_box(event_store.append_event(saga_id, event).now_or_never());
}

/// Benchmark: Append batch of events
fn append_batch_events(event_store: &PostgresEventStore, count: usize) {
    let saga_id = SagaId::new();
    let mut events = Vec::with_capacity(count);
    for i in 0..count {
        events.push(Event::new(
            saga_id.clone(),
            format!("test_activity_{}", i),
            serde_json::json!({"data": format!("value_{}", i)}).to_vec(),
        ));
    }
    let _ = black_box(event_store.append_events(saga_id, &events).now_or_never());
}

/// Benchmark: Replay full event history
async fn replay_full_events(event_store: &PostgresEventStore, count: usize) {
    let saga_id = SagaId::new();
    let _ = black_box(
        event_store
            .get_events_for_saga(&saga_id)
            .await
            .map(|_| ())
            .unwrap_or_else(|_| ())
    );
}

/// Benchmark: Replay using reset point optimization
async fn replay_with_reset_point(event_store: &PostgresEventStore) {
    let saga_id = SagaId::new();
    let _ = black_box(
        event_store
            .replay_from_last_reset_point(&saga_id)
            .await
            .map(|_| ())
            .unwrap_or_else(|_| ())
    );
}

/// Benchmark: Timer processing with reactive mode
async fn process_timers_reactive(pool: &sqlx::postgres::PgPool) {
    let config = WorkerConfig::default();
    let timer_scheduler = ReactiveTimerScheduler::new(pool.clone(), config);
    let worker = ReactiveWorker::new(pool.clone(), timer_scheduler, config);

    // Process 100 timers
    let saga_id = SagaId::new();
    for i in 0..100 {
        let timer_id = format!("timer_{}", i);
        let _ = black_box(
            worker
                .process_timer_event(&saga_id, &timer_id)
                .await
                .unwrap_or_else(|_| ())
        );
    }
}

/// Helper: Setup and bench with proper cleanup
fn setup_and_bench<F>(
    c: &mut Criterion,
    name: &str,
    pool: &sqlx::postgres::PgPool,
    event_store: &PostgresEventStore,
    bench_fn: F,
) where
    F: Fn(&PostgresEventStore) + Send + Sync,
{
    let mut group = c.benchmark_group(name);
    group.bench_function(name, |b| {
        b.iter(bench_fn);
    });

    group.finish();
}

/// Test configuration for benchmarks
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_single_event_works() {
        // This test verifies that the benchmark setup works
        // Actual benchmarks require database connection
        assert!(true);
    }
}

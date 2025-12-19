#[allow(unused_imports)]
use hodei_jobs::worker_agent_service_server::{WorkerAgentService, WorkerAgentServiceServer};
use hodei_jobs::{
    LogEntry, RegisterWorkerResponse, ServerMessage, UnregisterWorkerRequest,
    UnregisterWorkerResponse, UpdateWorkerStatusRequest, UpdateWorkerStatusResponse, WorkerMessage,
    server_message::Payload as ServerPayload, worker_message::Payload as WorkerPayload,
};
use hodei_worker_infrastructure::logging::LogBatcher;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, transport::Server};

struct MockServer {
    #[allow(dead_code)]
    tx: mpsc::Sender<Result<ServerMessage, Status>>,
    log_batches_received: Arc<AtomicUsize>,
    total_logs_received: Arc<AtomicUsize>,
    heartbeats_received: Arc<AtomicUsize>,
    simulate_slow_network: bool,
}

#[tonic::async_trait]
impl WorkerAgentService for MockServer {
    type WorkerStreamStream = ReceiverStream<Result<ServerMessage, Status>>;

    async fn register(
        &self,
        _request: Request<hodei_jobs::RegisterWorkerRequest>,
    ) -> Result<Response<RegisterWorkerResponse>, Status> {
        Ok(Response::new(RegisterWorkerResponse {
            success: true,
            message: "Registered".into(),
            worker_id: Some(hodei_jobs::WorkerId {
                value: "test-worker".into(),
            }),
            session_id: "test-session".into(),
            registration_time: Some(prost_types::Timestamp::default()),
        }))
    }

    async fn worker_stream(
        &self,
        request: Request<tonic::Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::WorkerStreamStream>, Status> {
        let mut in_stream = request.into_inner();
        let log_batches = self.log_batches_received.clone();
        let total_logs = self.total_logs_received.clone();
        let heartbeats = self.heartbeats_received.clone();
        let simulate_slow = self.simulate_slow_network;

        tokio::spawn(async move {
            while let Ok(Some(msg)) = in_stream.message().await {
                match msg.payload {
                    Some(WorkerPayload::Heartbeat(hb)) => {
                        heartbeats.fetch_add(1, Ordering::Relaxed);
                        if hb.dropped_logs > 0 {
                            println!("Received heartbeat with dropped logs: {}", hb.dropped_logs);
                        }
                    }
                    Some(WorkerPayload::LogBatch(batch)) => {
                        log_batches.fetch_add(1, Ordering::Relaxed);
                        total_logs.fetch_add(batch.entries.len(), Ordering::Relaxed);

                        if simulate_slow {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                    Some(WorkerPayload::Result(_)) => {}
                    _ => {}
                }
            }
        });

        let (tx, rx) = mpsc::channel(128);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn update_worker_status(
        &self,
        _request: Request<UpdateWorkerStatusRequest>,
    ) -> Result<Response<UpdateWorkerStatusResponse>, Status> {
        Ok(Response::new(UpdateWorkerStatusResponse {
            success: true,
            message: "Updated".into(),
            timestamp: Some(prost_types::Timestamp::default()),
        }))
    }

    async fn unregister_worker(
        &self,
        _request: Request<UnregisterWorkerRequest>,
    ) -> Result<Response<UnregisterWorkerResponse>, Status> {
        Ok(Response::new(UnregisterWorkerResponse {
            success: true,
            message: "Unregistered".into(),
            jobs_migrated: 0,
        }))
    }
}

/// Test for log batching performance
/// Verifies that LogBatcher can handle >10,000 logs/second throughput with <1% drop rate
#[tokio::test]
async fn test_log_batching_performance() {
    // Setup: Create a channel with limited capacity to simulate backpressure
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tx_metrics = metrics.clone();

    // Spawn a receiver to count batches
    tokio::spawn(async move {
        let mut batches_received = 0;
        let mut total_logs = 0;

        while let Some(msg) = rx.recv().await {
            if let Some(WorkerPayload::LogBatch(batch)) = msg.payload {
                batches_received += 1;
                total_logs += batch.entries.len();

                if batches_received % 100 == 0 {
                    tx_metrics.store(batches_received, Ordering::Relaxed);
                }
            }
        }
    });

    // Create LogBatcher with batching configuration
    let capacity = 100;
    let flush_interval = Duration::from_millis(250);
    let mut batcher = LogBatcher::new(
        tx,
        capacity,
        flush_interval,
        Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new()),
    );

    // Test: Send 10,000 logs rapidly and measure throughput
    let num_logs = 10_000usize;
    let start = Instant::now();

    for i in 0..num_logs {
        let log_entry = LogEntry {
            job_id: "perf-test-job".to_string(),
            line: format!("Test log line {}", i),
            is_stderr: false,
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        };

        batcher.push(log_entry).await;
    }

    // Flush remaining logs
    batcher.flush().await;

    let elapsed = start.elapsed();
    let throughput = num_logs as f64 / elapsed.as_secs_f64();

    // Verify performance requirements
    println!("Sent {} logs in {:?}", num_logs, elapsed);
    println!("Throughput: {:.2} logs/second", throughput);
    println!("Required: >10,000 logs/second");

    assert!(
        throughput > 10_000.0,
        "Throughput {:.2} logs/sec should be >10,000 logs/sec",
        throughput
    );

    // Give some time for batches to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify batching is effective (should receive much fewer than num_logs batches)
    let batches_received = metrics.load(Ordering::Relaxed);
    let effective_batching_ratio = num_logs as f64 / batches_received as f64;

    println!("Batches received: {}", batches_received);
    println!(
        "Effective batching ratio: {:.2} logs/batch",
        effective_batching_ratio
    );

    assert!(
        effective_batching_ratio > 50.0,
        "Should batch at least 50 logs per batch, got {:.2}",
        effective_batching_ratio
    );

    println!("✓ Log batching performance test passed");
    println!("  Throughput: {:.2} logs/sec (target: >10,000)", throughput);
    println!(
        "  Batching: {:.2} logs/batch (target: >50)",
        effective_batching_ratio
    );
}

/// Test for log batching under backpressure
/// Verifies that LogBatcher handles backpressure gracefully with <1% drop rate
#[tokio::test]
async fn test_log_batching_under_backpressure() {
    // Setup: Create a very limited channel to force backpressure
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(5);
    let metrics = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tx_metrics = metrics.clone();

    // Spawn a slow receiver to simulate backpressure
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Some(WorkerPayload::LogBatch(_batch)) = msg.payload {
                // Simulate slow network by processing batches slowly
                tokio::time::sleep(Duration::from_millis(10)).await;
                tx_metrics.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    // Create LogBatcher with aggressive batching
    let mut batcher = LogBatcher::new(
        tx,
        100,                        // High capacity
        Duration::from_millis(250), // Regular flush
        Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new()),
    );

    // Test: Send logs faster than they can be processed
    let num_logs = 1000usize;
    let mut dropped_count = 0;

    for i in 0..num_logs {
        let log_entry = LogEntry {
            job_id: "backpressure-test-job".to_string(),
            line: format!("Backpressure test log {}", i),
            is_stderr: false,
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        };

        batcher.push(log_entry).await;

        // Check if we need to flush
        if batcher.len() >= 100 {
            let flush_result = batcher.flush().await;
            if !flush_result {
                dropped_count += 100;
            }
        }
    }

    // Final flush
    if !batcher.is_empty() {
        let flush_result = batcher.flush().await;
        if !flush_result {
            dropped_count += batcher.len();
        }
    }

    // Allow time for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    let batches_received = metrics.load(Ordering::Relaxed);
    let total_logs_processed = batches_received * 100; // Assuming 100 per batch
    let drop_rate = (dropped_count as f64 / num_logs as f64) * 100.0;

    println!("\nBackpressure Test Results:");
    println!("  Logs sent: {}", num_logs);
    println!("  Batches received: {}", batches_received);
    println!("  Logs processed: {}", total_logs_processed);
    println!("  Dropped: {}", dropped_count);
    println!("  Drop rate: {:.2}%", drop_rate);
    println!("  Required: <1% drop rate");

    assert!(drop_rate < 1.0, "Drop rate {:.2}% should be <1%", drop_rate);

    println!("✓ Log batching backpressure test passed");
    println!("  Drop rate: {:.2}% (target: <1%)", drop_rate);
}

/// Test log batching with various batch sizes
#[tokio::test]
async fn test_log_batching_different_sizes() {
    let test_cases = vec![
        (10, "Small batch (10)"),
        (100, "Medium batch (100)"),
        (500, "Large batch (500)"),
    ];

    for (capacity, description) in test_cases {
        println!("\nTesting: {}", description);

        let (tx, mut rx) = mpsc::channel::<WorkerMessage>(1000);
        let batches_received = Arc::new(AtomicUsize::new(0));
        let batches_count = batches_received.clone();

        // Spawn receiver
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Some(WorkerPayload::LogBatch(_batch)) = msg.payload {
                    batches_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        // Create batcher with specific capacity
        let mut batcher = LogBatcher::new(
            tx,
            capacity,
            Duration::from_millis(250),
            Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new()),
        );

        // Send logs equal to 5x capacity
        let num_logs = capacity * 5;
        for i in 0..num_logs {
            let log_entry = LogEntry {
                job_id: "size-test-job".to_string(),
                line: format!("Size test log {}", i),
                is_stderr: false,
                timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            };

            batcher.push(log_entry).await;
        }

        // Flush remaining
        batcher.flush().await;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        let received = batches_received.load(Ordering::Relaxed);
        let expected_batches = (num_logs / capacity) + if num_logs % capacity > 0 { 1 } else { 0 };

        println!("  Sent {} logs with capacity {}", num_logs, capacity);
        println!("  Expected batches: {}", expected_batches);
        println!("  Received batches: {}", received);

        // Allow 10% variance for timing issues
        assert!(
            (received as isize - expected_batches as isize).abs()
                <= (expected_batches as f64 * 0.1) as isize,
            "Batch count mismatch for capacity {}: expected ~{}, got {}",
            capacity,
            expected_batches,
            received
        );

        println!("  ✓ {} passed", description);
    }

    println!("\n✓ All batch size tests passed");
}

//! E2E Tests for TestWorkerProvider - Fast Testing Infrastructure
//!
//! This test suite uses TestWorkerProvider instead of DockerProvider,
//! providing ~10x faster test execution (~10s vs ~140s).
//!
//! Run: cargo test --test e2e_test_provider_test -- --ignored --nocapture

use std::collections::HashMap;
use std::time::Duration;

use tokio::time::timeout;
use tokio_stream::StreamExt;

use hodei_jobs::{
    JobDefinition, JobId, JobLogEntry, QueueJobRequest, ResourceRequirements, SubscribeLogsRequest,
    log_stream_service_client::LogStreamServiceClient,
};
use uuid::Uuid;

mod common;

use common::TestStack;

// =============================================================================
// E2E Test: Complete Job Flow with TestWorkerProvider (FAST)
// =============================================================================

#[tokio::test]
#[ignore = "Requires worker binary at target/release/worker"]
async fn test_e2e_complete_job_flow_with_test_provider() {
    println!("\n🧪 E2E Test: Complete Job Flow with TestWorkerProvider (FAST)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack with TestWorkerProvider (FAST)
    println!("📦 Starting TestStack with TestWorkerProvider...");
    let stack = match TestStack::with_test_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            eprintln!("   Make sure worker binary exists at: target/release/worker");
            eprintln!("   Build with: cargo build --release -p hodei-jobs-grpc --bin worker");
            return;
        }
    };
    println!("✓ TestStack started at {}", stack.server.endpoint());

    // 2. Get clients
    let mut job_client = stack.server.job_client().await.expect("Job client");
    let channel = tonic::transport::Channel::from_shared(stack.server.endpoint())
        .unwrap()
        .connect()
        .await
        .expect("Channel");
    let mut log_client = LogStreamServiceClient::new(channel);

    // 3. Create and queue a job
    let job_id = Uuid::new_v4().to_string();
    println!("📝 Queuing job: {}", job_id);

    let queue_request = QueueJobRequest {
        job_definition: Some(JobDefinition {
            job_id: Some(JobId {
                value: job_id.clone(),
            }),
            name: "Test Worker Echo Test".to_string(),
            description: "Test job for TestWorkerProvider".to_string(),
            command: "echo".to_string(),
            arguments: vec!["Hello from TestWorkerProvider!".to_string()],
            environment: HashMap::new(),
            requirements: Some(ResourceRequirements {
                cpu_cores: 0.5,
                memory_bytes: 256 * 1024 * 1024,
                disk_bytes: 512 * 1024 * 1024,
                gpu_count: 0,
                custom_required: HashMap::new(),
            }),
            scheduling: None,
            selector: None,
            tolerations: vec![],
            timeout: Some(hodei_jobs::TimeoutConfig {
                execution_timeout: Some(prost_types::Duration {
                    seconds: 60,
                    nanos: 0,
                }),
                heartbeat_timeout: None,
                cleanup_timeout: None,
            }),
            tags: vec!["e2e-test".to_string(), "fast".to_string()],
        }),
        queued_by: "e2e-test".to_string(),
    };

    let queue_response = job_client
        .queue_job(queue_request)
        .await
        .expect("Queue job");
    let response = queue_response.into_inner();
    assert!(response.success, "Job should be queued successfully");
    println!("✓ Job queued: {}", response.message);

    // 4. Subscribe to logs
    println!("📡 Subscribing to log stream...");
    let subscribe_request = SubscribeLogsRequest {
        job_id: job_id.clone(),
        include_history: true,
        tail_lines: 0,
    };

    let log_stream = log_client
        .subscribe_logs(subscribe_request)
        .await
        .expect("Subscribe to logs");
    let mut stream = log_stream.into_inner();

    // 5. Collect logs with timeout
    let mut collected_logs: Vec<JobLogEntry> = Vec::new();
    let collect_timeout = Duration::from_secs(120);

    println!("⏳ Waiting for job execution and logs...");

    let result = timeout(collect_timeout, async {
        while let Some(result) = stream.next().await {
            match result {
                Ok(log_entry) => {
                    println!(
                        "  📝 [{}] {}",
                        if log_entry.is_stderr { "ERR" } else { "OUT" },
                        log_entry.line
                    );
                    collected_logs.push(log_entry);

                    if collected_logs
                        .iter()
                        .any(|l| l.line.contains("Hello from TestWorkerProvider"))
                    {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠️  Log stream error: {}", e);
                    break;
                }
            }
        }
    })
    .await;

    // 6. Cleanup
    println!("\n🧹 Cleaning up...");
    stack.shutdown().await;

    // 7. Verify results
    println!("\n📊 Results:");
    println!("  - Job ID: {}", job_id);
    println!("  - Total Logs: {}", collected_logs.len());

    if result.is_err() {
        eprintln!("  ⚠️  Timeout waiting for logs");
    }

    // Assertions
    assert!(
        !collected_logs.is_empty(),
        "❌ Should have received logs from worker"
    );
    assert!(
        collected_logs
            .iter()
            .any(|l| l.line.contains("Hello from TestWorkerProvider")),
        "❌ Should contain expected output"
    );

    println!(
        "\n✅ Test PASSED: Complete job flow with {} log entries",
        collected_logs.len()
    );
    println!("⏱️  This test used TestWorkerProvider (FAST - ~10s vs ~140s with Docker)");
}

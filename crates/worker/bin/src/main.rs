use std::time::Duration;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{Level, error, info, warn};
use tracing_subscriber::FmtSubscriber;

use hodei_jobs::{
    JobResultMessage, RegisterWorkerRequest, ResourceCapacity, ServerMessage, WorkerHeartbeat,
    WorkerId, WorkerInfo, WorkerMessage, WorkerStatus, server_message::Payload as ServerPayload,
    worker_agent_service_client::WorkerAgentServiceClient,
    worker_message::Payload as WorkerPayload,
};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

/// Pending job result to be delivered to server
#[derive(Debug, Clone)]
struct PendingJobResult {
    job_id: String,
    exit_code: i32,
    success: bool,
    error_message: String,
}

use hodei_worker_infrastructure::{
    CertificatePaths, InjectionStrategy, JobExecutor,
    metrics::{CachedResourceUsage, MetricsCollector, WorkerMetrics},
};

use crate::config::WorkerConfig;

mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("Starting Hodei Jobs Worker Agent...");

    // Setup Shutdown Signal
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);
    let shutdown_tx = Arc::new(shutdown_tx);

    // Spawn signal handler
    let shutdown_sender = shutdown_tx.clone();
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("🛑 Shutdown signal received, initiating graceful shutdown...");
                let _ = shutdown_sender.send(());
            }
            Err(err) => {
                error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    });

    // Load configuration
    let mut config = WorkerConfig::default();

    // T4.3: Configure TLS if enabled
    let tls_config = if config.is_mtls_enabled() {
        info!("🔐 mTLS verification enabled");
        let cert_paths = CertificatePaths {
            client_cert_path: config.client_cert_path.clone().unwrap(),
            client_key_path: config.client_key_path.clone().unwrap(),
            ca_cert_path: config.ca_cert_path.clone().unwrap(),
        };

        match cert_paths.load_certificates().await {
            Ok((client_cert, client_key, ca_cert)) => {
                let identity = Identity::from_pem(client_cert, client_key);
                let ca = Certificate::from_pem(ca_cert);

                Some(
                    ClientTlsConfig::new()
                        .domain_name("hodei-server")
                        .identity(identity)
                        .ca_certificate(ca),
                )
            }
            Err(e) => {
                error!("Failed to load certificates: {}", e);
                return Err(e);
            }
        }
    } else {
        warn!("⚠ Running WITHOUT mTLS (Development Mode)");
        None
    };

    // Connect to server
    info!("Connecting to server at {}...", config.server_addr);
    let channel: Channel = if let Some(tls) = tls_config.clone() {
        Channel::from_shared(config.server_addr.clone())?
            .tls_config(tls)?
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(10))
            .connect()
            .await?
    } else {
        Channel::from_shared(config.server_addr.clone())?
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(10))
            .connect()
            .await?
    };

    let mut client = WorkerAgentServiceClient::new(channel);
    info!("✓ Connected to server");

    // Initialize components
    let metrics = Arc::new(WorkerMetrics::new());
    let log_dir = config.log_dir.clone();
    let executor = Arc::new(JobExecutor::new(
        config.log_batch_size,
        config.log_flush_interval_ms,
        log_dir,
        metrics.clone(),
    ));
    let running_jobs = Arc::new(Mutex::new(
        HashMap::<String, tokio::task::JoinHandle<()>>::new(),
    ));
    let mut metrics_collector = MetricsCollector::new();

    // CRITICAL: Persistent result channel that survives reconnections
    // This ensures job results are never lost even if connection drops during execution
    let (result_tx, mut result_rx) = mpsc::channel::<PendingJobResult>(100);
    let pending_results: Arc<Mutex<Vec<PendingJobResult>>> = Arc::new(Mutex::new(Vec::new()));

    // 5. Main Reconnection Loop
    info!("Starting main worker loop...");
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);
    let mut current_session_id: Option<String> = None;

    let mut shutdown_triggered = false;

    loop {
        if shutdown_triggered {
            break;
        }

        // Register worker (or re-register on reconnect)
        info!("Registering worker...");
        match register_worker(&mut client, &config, &mut shutdown_rx, &current_session_id).await {
            Ok((sid, assigned_id)) => {
                current_session_id = Some(sid.clone());

                // Update config with assigned worker_id from server
                if let Some(worker_id) = assigned_id {
                    config.worker_id = worker_id.value;
                    info!("🔑 Received assigned worker_id: {}", config.worker_id);
                }

                info!("✅ Worker registered successfully with session: {}", sid);
            }
            Err(e) => {
                if e.to_string().contains("Shutdown triggered") {
                    break;
                }

                // If registration fails and we had a session, it might be invalid
                // Clear the session so we can re-register with a new OTP token
                if current_session_id.is_some() {
                    warn!(
                        "Registration failed with existing session. Clearing session to re-register with new OTP token"
                    );
                    current_session_id = None;
                }

                error!("Registration failed: {}. Retrying in {:?}...", e, backoff);
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {
                         backoff = std::cmp::min(backoff * 2, max_backoff);
                         continue;
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(100);
        let rx_stream = ReceiverStream::new(rx);

        info!("Establishing bidirectional stream...");

        let stream_result = client.worker_stream(rx_stream).await;

        match stream_result {
            Ok(response) => {
                let mut server_stream = response.into_inner();
                info!("✓ Bidirectional stream established");
                backoff = Duration::from_secs(1); // Reset backoff on success

                // CRITICAL: Deliver any pending job results from previous connection
                {
                    let mut pending = pending_results.lock().await;
                    if !pending.is_empty() {
                        info!(
                            "📤 Delivering {} pending job result(s) from previous connection",
                            pending.len()
                        );
                        for result in pending.drain(..) {
                            let result_msg = WorkerMessage {
                                payload: Some(WorkerPayload::Result(JobResultMessage {
                                    job_id: result.job_id.clone(),
                                    exit_code: result.exit_code,
                                    success: result.success,
                                    error_message: result.error_message.clone(),
                                    completed_at: Some(prost_types::Timestamp::from(
                                        std::time::SystemTime::now(),
                                    )),
                                })),
                            };
                            if let Err(e) = tx.send(result_msg).await {
                                error!(
                                    "Failed to deliver pending result for job {}: {}",
                                    result.job_id, e
                                );
                                // Re-queue for next attempt (shouldn't happen since we just connected)
                            } else {
                                info!(
                                    "✅ Delivered pending result for job {} (exit code: {})",
                                    result.job_id, result.exit_code
                                );
                            }
                        }
                    }
                }

                let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(15));
                let mut cached_metrics: Option<CachedResourceUsage> = None;

                // Inner communication loop
                loop {
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            info!("Graceful shutdown triggered in inner loop");
                            shutdown_triggered = true;
                            break;
                        }

                        // Outgoing: Heartbeats
                        _ = heartbeat_interval.tick() => {
                            let resource_usage = if let Some(cache) = &cached_metrics {
                                 if cache.is_fresh() {
                                     cache.get_usage().clone()
                                 } else {
                                      let usage = metrics_collector.collect();
                                      cached_metrics = Some(CachedResourceUsage::new(usage.clone()));
                                      usage
                                  }
                             } else {
                                  let usage = metrics_collector.collect();
                                  cached_metrics = Some(CachedResourceUsage::new(usage.clone()));
                                  usage
                             };

                            let active_jobs_count = running_jobs.lock().await.len() as i32;
                            let running_ids = running_jobs.lock().await.keys().cloned().collect();

                            let heartbeat_req = WorkerHeartbeat {
                                 worker_id: Some(WorkerId { value: config.worker_id.clone() }),
                                 usage: Some(resource_usage),
                                 status: WorkerStatus::Available as i32,
                                 active_jobs: active_jobs_count,
                                 running_job_ids: running_ids,
                                 timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                                 dropped_logs: metrics.dropped_logs.load(std::sync::atomic::Ordering::Relaxed),
                            };

                            if let Err(e) = tx.send(WorkerMessage {
                                payload: Some(WorkerPayload::Heartbeat(heartbeat_req))
                            }).await {
                                error!("Failed to send heartbeat: {}", e);
                                break;
                            }
                        }

                        // Forward pending job results to server
                        pending_result = result_rx.recv() => {
                            if let Some(result) = pending_result {
                                info!("📤 Forwarding job result for {} to server", result.job_id);
                                let result_msg = WorkerMessage {
                                    payload: Some(WorkerPayload::Result(JobResultMessage {
                                        job_id: result.job_id.clone(),
                                        exit_code: result.exit_code,
                                        success: result.success,
                                        error_message: result.error_message.clone(),
                                        completed_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                                    }))
                                };
                                if let Err(e) = tx.send(result_msg).await {
                                    // Connection failed - store result for retry on reconnection
                                    error!("Failed to send result for job {}, storing for retry: {}", result.job_id, e);
                                    pending_results.lock().await.push(result);
                                    break; // Exit inner loop to trigger reconnection
                                } else {
                                    info!("✅ Job {} result delivered to server (exit code: {})", result.job_id, result.exit_code);
                                }
                            }
                        }

                        // Incoming: Server Messages
                        msg = server_stream.message() => {
                            match msg {
                                Ok(Some(ServerMessage { payload: Some(payload) })) => {
                                    match payload {
                                        ServerPayload::RunJob(run_job) => {
                                            info!("🚀 Received job: {}", run_job.job_id);
                                            let exec = executor.clone();
                                            let log_tx = tx.clone(); // For streaming logs only
                                            let persistent_result_tx = result_tx.clone(); // Persistent channel for results
                                            let job_id = run_job.job_id.clone();
                                            let timeout_ms = run_job.timeout_ms as u64;
                                            let jobs_registry = running_jobs.clone();

                                            // Send acknowledgment to server immediately
                                            let ack_msg = WorkerMessage {
                                                payload: Some(WorkerPayload::Ack(hodei_jobs::AckMessage {
                                                    message_id: format!("job-{}", run_job.job_id),
                                                    success: true,
                                                    worker_id: config.worker_id.clone(),
                                                }))
                                            };
                                            if let Err(e) = tx.send(ack_msg).await {
                                                error!("Failed to send acknowledgment: {}", e);
                                            } else {
                                                info!("✅ Sent acknowledgment for job {}", run_job.job_id);
                                            }

                                            let handle = tokio::spawn(async move {
                                                #[allow(deprecated)]
                                                // Execute job - logs are streamed via the worker_stream channel
                                                // but results go through the PERSISTENT result channel
                                                let result = exec.execute_from_command(
                                                    &job_id,
                                                    run_job.command,
                                                    run_job.env,
                                                    Some(run_job.working_dir),
                                                    log_tx, // Logs go through ephemeral stream (best effort)
                                                    Some(timeout_ms / 1000),
                                                    run_job.stdin,
                                                    run_job.secrets_json,
                                                    InjectionStrategy::TmpfsFile,
                                                ).await;

                                                // Remove from registry when done
                                                jobs_registry.lock().await.remove(&job_id);

                                                let (exit_code, success, error_message) = match result {
                                                    Ok((code, _, _)) => (code, code == 0, String::new()),
                                                    Err(e) => (-1, false, e),
                                                };

                                                // CRITICAL: Send result via PERSISTENT channel
                                                // This channel survives reconnections, ensuring results are NEVER lost
                                                let pending_result = PendingJobResult {
                                                    job_id: job_id.clone(),
                                                    exit_code,
                                                    success,
                                                    error_message,
                                                };

                                                if let Err(e) = persistent_result_tx.send(pending_result).await {
                                                    // This should never happen unless worker is shutting down
                                                    error!("CRITICAL: Failed to queue job result (internal channel closed): {}", e);
                                                    error!("Job ID: {}, Exit Code: {} - Result may be lost!", job_id, exit_code);
                                                } else {
                                                    info!("📋 Job {} execution complete, result queued for delivery", job_id);
                                                }
                                            });

                                            running_jobs.lock().await.insert(run_job.job_id, handle);
                                        }
                                        ServerPayload::Cancel(cancel) => {
                                            info!("⏹ Received cancel for job: {}", cancel.job_id);
                                            if let Some(handle) = running_jobs.lock().await.remove(&cancel.job_id) {
                                                handle.abort();
                                                info!("✓ Job {} aborted", cancel.job_id);
                                            } else {
                                                warn!("Job {} not found in registry", cancel.job_id);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(None) => {
                                    warn!("Server closed the connection");
                                    break;
                                }
                                Err(e) => {
                                    error!("Stream error: {}", e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Check if shutdown occurred during connection attempt
                if let Ok(_) = shutdown_rx.try_recv() {
                    break;
                }

                error!(
                    "Failed to establish stream: {}. Retrying in {:?}...",
                    e, backoff
                );

                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {
                         backoff = std::cmp::min(backoff * 2, max_backoff);
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        }
    }

    // Graceful Shutdown Logic
    info!("Performing graceful shutdown...");

    let mut jobs = running_jobs.lock().await;
    if !jobs.is_empty() {
        info!(
            "Waiting for {} running jobs to finish (timeout: 10s)...",
            jobs.len()
        );

        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();

        // Polling wait
        loop {
            // Remove finished jobs
            jobs.retain(|_, handle| !handle.is_finished());

            if jobs.is_empty() {
                info!("All jobs finished successfully.");
                break;
            }

            if start.elapsed() > timeout {
                warn!("Timeout reached! Aborting {} remaining jobs...", jobs.len());
                for (id, handle) in jobs.iter() {
                    info!("Aborting job {}", id);
                    handle.abort();
                }
                break;
            }

            // Release lock and sleep briefly
            drop(jobs);
            tokio::time::sleep(Duration::from_millis(500)).await;
            jobs = running_jobs.lock().await;
        }
    } else {
        info!("No running jobs. Exiting.");
    }

    // Unregister worker
    info!("Unregistering worker...");
    let unregister_req = hodei_jobs::UnregisterWorkerRequest {
        worker_id: Some(hodei_jobs::WorkerId {
            value: config.worker_id.clone(),
        }),
        reason: "Graceful Shutdown".to_string(),
        force: false,
    };

    match client.unregister_worker(unregister_req).await {
        Ok(_) => info!("✓ Worker unregistered successfully"),
        Err(e) => error!("Failed to unregister worker: {}", e),
    }

    info!("👋 Worker Agent shutdown complete.");
    Ok(())
}

async fn register_worker(
    client: &mut WorkerAgentServiceClient<Channel>,
    config: &WorkerConfig,
    shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    session_id: &Option<String>,
) -> Result<(String, Option<WorkerId>), Box<dyn std::error::Error>> {
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);

    loop {
        // Check for shutdown before retry
        if let Ok(_) = shutdown_rx.try_recv() {
            info!("Shutdown signal received during registration. Exiting.");
            return Err("Shutdown triggered".into());
        }

        let request = RegisterWorkerRequest {
            auth_token: config.auth_token.clone(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId {
                    value: config.worker_id.clone(),
                }),
                name: config.worker_name.clone(),
                version: "0.1.0".to_string(),
                hostname: config.worker_name.clone(),
                ip_address: "127.0.0.1".to_string(),
                os_info: std::env::consts::OS.to_string(),
                architecture: std::env::consts::ARCH.to_string(),
                capacity: Some(ResourceCapacity {
                    cpu_cores: config.cpu_cores,
                    memory_bytes: config.memory_bytes,
                    disk_bytes: config.disk_bytes,
                    gpu_count: 0,
                    custom_resources: std::collections::HashMap::new(),
                }),
                capabilities: config.capabilities.clone(),
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            }),
            session_id: session_id.clone().unwrap_or_default(),
        };

        match client.register(request).await {
            Ok(response) => {
                let resp = response.into_inner();
                let assigned_worker_id = resp.worker_id;
                info!(
                    "✓ {} (ID: {}, Session: {})",
                    resp.message,
                    assigned_worker_id
                        .as_ref()
                        .map(|id| &id.value)
                        .unwrap_or(&config.worker_id),
                    resp.session_id
                );
                return Ok((resp.session_id, assigned_worker_id));
            }
            Err(e) => {
                warn!(
                    "Failed to register worker: {}. Retrying in {:?}...",
                    e, backoff
                );

                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {
                         backoff = std::cmp::min(backoff * 2, max_backoff);
                    },
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown signal received during retry wait. Exiting.");
                        return Err("Shutdown triggered".into());
                    }
                }
            }
        }
    }
}

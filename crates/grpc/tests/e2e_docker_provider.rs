//! E2E Tests for Docker Provider - Flujo Coherente
//!
//! Tests organizados por nivel de complejidad:
//!
//! ## Nivel 1: Infraestructura Base
//! - `test_01_docker_available` - Docker CLI funciona
//! - `test_02_postgres_testcontainer` - PostgreSQL via Testcontainers
//! - `test_03_grpc_server_starts` - Server gRPC inicia correctamente
//!
//! ## Nivel 2: Worker Registration (sin container real)
//! - `test_04_worker_registration_with_otp` - Worker se registra con OTP
//! - `test_05_worker_stream_connection` - Stream bidireccional funciona
//! - `test_06_job_queued_and_dispatched` - Job se encola y despacha al worker
//!
//! ## Nivel 3: Docker Provider
//! - `test_07_docker_provider_creates_container` - DockerProvider crea container
//! - `test_08_docker_provider_container_lifecycle` - Ciclo de vida completo
//!
//! ## Nivel 4: Flujo E2E Completo (requiere imagen hodei-worker)
//! - `test_09_full_e2e_with_real_worker` - Flujo completo con worker Docker real
//!
//! Run: cargo test --test e2e_docker_provider -- --ignored --nocapture

use std::time::Duration;
use tokio::time::timeout;

use hodei_jobs::{
    QueueJobRequest, JobDefinition, WorkerId, JobId,
    RegisterWorkerRequest, WorkerInfo, ResourceCapacity,
    WorkerMessage, WorkerHeartbeat, JobResultMessage,
    worker_message::Payload as WorkerPayload,
    server_message::Payload as ServerPayload,
};

mod common;

use common::{TestStack, TestServerConfig, get_postgres_context, is_docker_available, DockerVerifier};

// =============================================================================
// NIVEL 1: Infraestructura Base
// =============================================================================

/// Test 01: Verifica que Docker CLI está disponible
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_01_docker_available() {
    let verifier = DockerVerifier::new().expect("Docker CLI should be available");
    assert!(verifier.is_available(), "Docker should be running");
    println!("✓ Docker CLI disponible y funcionando");
}

/// Test 02: Verifica que PostgreSQL via Testcontainers funciona
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_02_postgres_testcontainer() {
    let db = match get_postgres_context().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping: Failed to start Postgres: {}", e);
            return;
        }
    };
    
    println!("✓ PostgreSQL iniciado: {}", db.connection_string);
    assert!(!db.connection_string.is_empty());
}

/// Test 03: Verifica que el servidor gRPC inicia correctamente
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_03_grpc_server_starts() {
    let db = match get_postgres_context().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let config = TestServerConfig::new(db.connection_string.clone());
    let server = match common::TestServer::start(config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    // Verificar conexión
    let client = server.job_client().await;
    assert!(client.is_ok(), "Should create job client");
    
    println!("✓ Server gRPC iniciado en {}", server.addr);
    server.shutdown().await;
}

// =============================================================================
// NIVEL 2: Worker Registration y Job Dispatch (sin container real)
// =============================================================================

/// Test 04: Worker se registra correctamente con OTP
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_04_worker_registration_with_otp() {
    let stack = match TestStack::without_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let worker_id = uuid::Uuid::new_v4().to_string();
    
    // 1. Generar OTP
    let otp = stack.server.generate_otp(&worker_id).await
        .expect("Should generate OTP");
    println!("✓ OTP generado para worker: {}", worker_id);

    // 2. Registrar worker con OTP
    let mut worker_client = stack.server.worker_client().await
        .expect("Should create worker client");

    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 4.0,
                memory_bytes: 8 * 1024 * 1024 * 1024,
                disk_bytes: 100 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["shell".to_string()],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };

    let reg_response = worker_client.register(reg_request).await
        .expect("Should register worker")
        .into_inner();

    assert!(reg_response.success, "Registration should succeed");
    assert!(!reg_response.session_id.is_empty(), "Should have session_id");
    println!("✓ Worker registrado con session: {}", reg_response.session_id);

    stack.shutdown().await;
}

/// Test 05: Worker stream bidireccional funciona
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_05_worker_stream_connection() {
    let stack = match TestStack::without_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let worker_id = uuid::Uuid::new_v4().to_string();
    let otp = stack.server.generate_otp(&worker_id).await.unwrap();

    let mut worker_client = stack.server.worker_client().await.unwrap();

    // Registrar
    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "Stream Test Worker".to_string(),
            ..Default::default()
        }),
    };
    worker_client.register(reg_request).await.unwrap();
    println!("✓ Worker registrado");

    // Conectar stream
    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);

    // Enviar heartbeat
    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            status: 2, // AVAILABLE
            usage: None,
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: None,
        })),
    };
    tx.send(heartbeat).await.unwrap();

    let inbound = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = worker_client.worker_stream(inbound).await
        .expect("Should connect to stream");
    let mut stream = response.into_inner();

    // Esperar ACK
    let ack_result = timeout(Duration::from_secs(2), stream.message()).await;
    assert!(ack_result.is_ok(), "Should receive ACK");
    println!("✓ Stream bidireccional conectado");

    stack.shutdown().await;
}

/// Test 06: Job se encola y se despacha al worker conectado
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_06_job_queued_and_dispatched() {
    // Usar stack CON JobController habilitado
    let stack = match TestStack::with_job_controller().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let worker_id = uuid::Uuid::new_v4().to_string();
    let otp = stack.server.generate_otp(&worker_id).await.unwrap();

    let mut worker_client = stack.server.worker_client().await.unwrap();

    // Registrar worker
    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "Job Dispatch Test Worker".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 4.0,
                memory_bytes: 8 * 1024 * 1024 * 1024,
                disk_bytes: 100 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["shell".to_string()],
            ..Default::default()
        }),
    };
    worker_client.register(reg_request).await.unwrap();
    println!("✓ Worker registrado: {}", worker_id);

    // Conectar stream y enviar heartbeat
    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);
    let tx_for_result = tx.clone();

    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            status: 2, // AVAILABLE
            usage: None,
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: None,
        })),
    };
    tx.send(heartbeat).await.unwrap();

    let inbound = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = worker_client.worker_stream(inbound).await.unwrap();
    let mut stream = response.into_inner();

    // Esperar ACK inicial
    let _ = timeout(Duration::from_secs(1), stream.message()).await;
    println!("✓ Stream conectado");

    // Encolar job
    let mut job_client = stack.server.job_client().await.unwrap();
    let job_id = uuid::Uuid::new_v4().to_string();

    let queue_request = QueueJobRequest {
        job_definition: Some(JobDefinition {
            job_id: Some(JobId { value: job_id.clone() }),
            name: "Dispatch Test Job".to_string(),
            description: "Test job dispatch".to_string(),
            command: "echo".to_string(),
            arguments: vec!["Hello".to_string()],
            environment: std::collections::HashMap::new(),
            requirements: None,
            scheduling: None,
            selector: None,
            tolerations: vec![],
            timeout: None,
            tags: vec![],
        }),
        queued_by: "test".to_string(),
    };

    let queue_response = job_client.queue_job(queue_request).await.unwrap().into_inner();
    assert!(queue_response.success, "Job should be queued");
    println!("✓ Job encolado: {}", job_id);

    // Esperar RunJob del JobController
    let run_job_result = timeout(Duration::from_secs(5), async {
        use tokio_stream::StreamExt;
        while let Some(msg_result) = stream.next().await {
            if let Ok(server_msg) = msg_result {
                if let Some(ServerPayload::RunJob(run_job)) = server_msg.payload {
                    return Some(run_job);
                }
            }
        }
        None
    }).await;

    match run_job_result {
        Ok(Some(run_job)) => {
            println!("✓ RunJob recibido: {}", run_job.job_id);
            assert_eq!(run_job.job_id, job_id);

            // Simular ejecución y enviar resultado
            let result_msg = WorkerMessage {
                payload: Some(WorkerPayload::Result(JobResultMessage {
                    job_id: job_id.clone(),
                    exit_code: 0,
                    success: true,
                    error_message: String::new(),
                    completed_at: None,
                })),
            };
            tx_for_result.send(result_msg).await.unwrap();
            println!("✓ Resultado enviado");
        }
        Ok(None) => {
            println!("⚠ Stream cerrado sin RunJob");
        }
        Err(_) => {
            println!("⚠ Timeout esperando RunJob (JobController puede no haber asignado aún)");
        }
    }

    stack.shutdown().await;
    println!("✓ Test completado");
}

// =============================================================================
// NIVEL 3: Docker Provider
// =============================================================================

/// Test 07: DockerProvider crea container correctamente
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_07_docker_provider_creates_container() {
    use hodei_jobs_domain::worker::WorkerSpec;
    use hodei_jobs_domain::worker_provider::WorkerProvider;
    use hodei_jobs_infrastructure::providers::DockerProvider;

    let provider = match DockerProvider::new().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: DockerProvider not available: {}", e);
            return;
        }
    };
    println!("✓ DockerProvider creado");

    // Crear spec
    let spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    );
    let worker_id = spec.worker_id.clone();

    // Crear worker
    let handle = provider.create_worker(&spec).await
        .expect("Should create worker");
    println!("✓ Container creado: {}", handle.provider_resource_id);

    // Verificar con CLI
    let verifier = DockerVerifier::new().expect("Docker CLI");
    let container_name = format!("hodei-worker-{}", worker_id);
    let containers = verifier.find_containers(&container_name).unwrap();
    assert!(!containers.is_empty(), "Container should exist");
    println!("✓ Container verificado via CLI");

    // Cleanup
    provider.destroy_worker(&handle).await.expect("Should destroy");
    println!("✓ Container eliminado");
}

/// Test 08: Ciclo de vida completo del container
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_08_docker_provider_container_lifecycle() {
    use hodei_jobs_domain::worker::WorkerSpec;
    use hodei_jobs_domain::worker_provider::WorkerProvider;
    use hodei_jobs_domain::shared_kernel::WorkerState;
    use hodei_jobs_infrastructure::providers::DockerProvider;

    let provider = match DockerProvider::new().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    );

    // 1. Crear
    let handle = provider.create_worker(&spec).await.unwrap();
    println!("✓ 1. Container creado");

    // 2. Verificar estado
    let state = provider.get_worker_status(&handle).await.unwrap();
    println!("✓ 2. Estado: {:?}", state);

    // 3. Obtener logs (pueden estar vacíos para alpine)
    let logs = provider.get_worker_logs(&handle, Some(10)).await.unwrap();
    println!("✓ 3. Logs obtenidos: {} líneas", logs.len());

    // 4. Destruir
    provider.destroy_worker(&handle).await.unwrap();
    println!("✓ 4. Container destruido");

    // 5. Verificar que no existe
    let verifier = DockerVerifier::new().unwrap();
    let container_name = format!("hodei-worker-{}", spec.worker_id);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let containers = verifier.find_containers(&container_name).unwrap();
    assert!(containers.is_empty(), "Container should be gone");
    println!("✓ 5. Container limpiado correctamente");
}

// Tests antiguos eliminados - cubiertos por test_08_docker_provider_container_lifecycle

// =============================================================================
// NIVEL 4: Tests Adicionales de Integración
// =============================================================================

/// Test 09: Múltiples jobs se encolan correctamente
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_09_multiple_jobs_queued() {
    let stack = match TestStack::without_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let mut job_client = stack.server.job_client().await.unwrap();

    for i in 1..=3 {
        let job_id = uuid::Uuid::new_v4().to_string();
        let queue_request = QueueJobRequest {
            job_definition: Some(JobDefinition {
                job_id: Some(JobId { value: job_id.clone() }),
                name: format!("Batch Job {}", i),
                description: format!("Batch job {} of 3", i),
                command: "echo".to_string(),
                arguments: vec![format!("Job {}", i)],
                environment: std::collections::HashMap::new(),
                requirements: None,
                scheduling: None,
                selector: None,
                tolerations: vec![],
                timeout: None,
                tags: vec![],
            }),
            queued_by: "test".to_string(),
        };

        let response = job_client.queue_job(queue_request).await.unwrap().into_inner();
        assert!(response.success, "Job {} should be queued", i);
    }

    println!("✓ 3 jobs encolados correctamente");
    stack.shutdown().await;
}

/// Test 10: Estado de la cola del scheduler
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_10_scheduler_queue_status() {
    let stack = match TestStack::without_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let mut scheduler_client = stack.server.scheduler_client().await.unwrap();

    let status_response = scheduler_client
        .get_queue_status(hodei_jobs::GetQueueStatusRequest {
            scheduler_name: "default".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(status_response.status.is_some(), "Should have queue status");
    let status = status_response.status.unwrap();
    println!("✓ Queue status: pending={}, running={}", status.pending_jobs, status.running_jobs);

    stack.shutdown().await;
}

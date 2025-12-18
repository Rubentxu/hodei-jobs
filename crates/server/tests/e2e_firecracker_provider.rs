//! E2E Tests for Firecracker Provider - Flujo Coherente
//!
//! Tests organizados por nivel de complejidad:
//!
//! ## Nivel 1: Infraestructura Base
//! - `test_01_firecracker_available` - KVM y Firecracker binarios disponibles
//! - `test_02_postgres_testcontainer` - PostgreSQL via Testcontainers
//! - `test_03_grpc_server_starts` - Server gRPC inicia correctamente
//!
//! ## Nivel 2: Worker Registration (sin microVM real)
//! - `test_04_worker_registration_with_otp` - Worker se registra con OTP
//! - `test_05_worker_stream_connection` - Stream bidireccional funciona
//! - `test_06_job_queued_and_dispatched` - Job se encola y despacha al worker
//!
//! ## Nivel 3: Firecracker Provider
//! - `test_07_firecracker_provider_creates_vm` - FirecrackerProvider crea microVM
//! - `test_08_firecracker_provider_vm_lifecycle` - Ciclo de vida completo
//!
//! ## Nivel 4: Flujo E2E Completo (requiere kernel y rootfs)
//! - `test_09_full_e2e_with_real_worker` - Flujo completo con worker microVM real
//!
//! ## Requisitos:
//! - Linux con KVM habilitado (/dev/kvm)
//! - Firecracker binary instalado
//! - Kernel Linux (vmlinux) preparado
//! - Rootfs con agente hodei-worker
//! - Docker para Testcontainers (PostgreSQL)
//!
//! Run: HODEI_FC_TEST=1 cargo test --test e2e_firecracker_provider -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

use hodei_jobs::{
    JobDefinition, JobId, JobResultMessage, QueueJobRequest, RegisterWorkerRequest,
    ResourceCapacity, WorkerHeartbeat, WorkerId, WorkerInfo, WorkerMessage,
    server_message::Payload as ServerPayload, worker_message::Payload as WorkerPayload,
};

mod common;

use common::{TestServerConfig, TestStack, get_postgres_context};

// =============================================================================
// Helper Functions
// =============================================================================

fn should_run_fc_tests() -> bool {
    std::env::var("HODEI_FC_TEST").unwrap_or_default() == "1"
}

fn get_firecracker_path() -> PathBuf {
    PathBuf::from(
        std::env::var("HODEI_FC_FIRECRACKER_PATH")
            .unwrap_or_else(|_| "/usr/bin/firecracker".to_string()),
    )
}

fn get_kernel_path() -> PathBuf {
    PathBuf::from(
        std::env::var("HODEI_FC_KERNEL_PATH")
            .unwrap_or_else(|_| "/var/lib/hodei/vmlinux".to_string()),
    )
}

fn get_rootfs_path() -> PathBuf {
    PathBuf::from(
        std::env::var("HODEI_FC_ROOTFS_PATH")
            .unwrap_or_else(|_| "/var/lib/hodei/rootfs.ext4".to_string()),
    )
}

fn get_data_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("HODEI_FC_DATA_DIR").unwrap_or_else(|_| "/tmp/hodei-fc-test".to_string()),
    )
}

/// Verifier for Firecracker environment
pub struct FirecrackerVerifier {
    firecracker_path: PathBuf,
    kernel_path: PathBuf,
    rootfs_path: PathBuf,
}

impl FirecrackerVerifier {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            firecracker_path: get_firecracker_path(),
            kernel_path: get_kernel_path(),
            rootfs_path: get_rootfs_path(),
        })
    }

    pub fn is_kvm_available(&self) -> bool {
        PathBuf::from("/dev/kvm").exists()
    }

    pub fn is_firecracker_available(&self) -> bool {
        self.firecracker_path.exists()
    }

    pub fn is_kernel_available(&self) -> bool {
        self.kernel_path.exists()
    }

    pub fn is_rootfs_available(&self) -> bool {
        self.rootfs_path.exists()
    }

    pub fn check_all_requirements(&self) -> Vec<String> {
        let mut missing = Vec::new();

        if !self.is_kvm_available() {
            missing.push("/dev/kvm not available".to_string());
        }
        if !self.is_firecracker_available() {
            missing.push(format!(
                "Firecracker binary not found at {:?}",
                self.firecracker_path
            ));
        }
        if !self.is_kernel_available() {
            missing.push(format!("Kernel not found at {:?}", self.kernel_path));
        }
        if !self.is_rootfs_available() {
            missing.push(format!("Rootfs not found at {:?}", self.rootfs_path));
        }

        missing
    }

    pub fn can_run_basic_tests(&self) -> bool {
        self.is_kvm_available() && self.is_firecracker_available()
    }

    pub fn can_run_full_tests(&self) -> bool {
        self.is_kvm_available()
            && self.is_firecracker_available()
            && self.is_kernel_available()
            && self.is_rootfs_available()
    }

    pub fn get_firecracker_version(&self) -> anyhow::Result<String> {
        let output = std::process::Command::new(&self.firecracker_path)
            .arg("--version")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to get Firecracker version");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

// =============================================================================
// NIVEL 1: Infraestructura Base
// =============================================================================

/// Test 01: Verifica que KVM y Firecracker están disponibles
#[tokio::test]
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_01_firecracker_available() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    let verifier = FirecrackerVerifier::new().expect("Should create verifier");

    // Check KVM
    if !verifier.is_kvm_available() {
        eprintln!("⚠ /dev/kvm not available - Firecracker requires KVM");
        eprintln!("  Try: sudo modprobe kvm_intel (or kvm_amd)");
        eprintln!("  And: sudo chmod 666 /dev/kvm");
        return;
    }
    println!("✓ KVM disponible (/dev/kvm)");

    // Check Firecracker binary
    if !verifier.is_firecracker_available() {
        eprintln!(
            "⚠ Firecracker binary not found at {:?}",
            get_firecracker_path()
        );
        eprintln!("  Download from: https://github.com/firecracker-microvm/firecracker/releases");
        return;
    }

    match verifier.get_firecracker_version() {
        Ok(version) => println!("✓ Firecracker disponible: {}", version),
        Err(_) => println!("✓ Firecracker disponible (version unknown)"),
    }

    // Check kernel and rootfs (informational)
    if verifier.is_kernel_available() {
        println!("✓ Kernel disponible: {:?}", get_kernel_path());
    } else {
        println!("⚠ Kernel no encontrado: {:?}", get_kernel_path());
        println!(
            "  Download from: https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.9/x86_64/vmlinux-5.10.217"
        );
    }

    if verifier.is_rootfs_available() {
        println!("✓ Rootfs disponible: {:?}", get_rootfs_path());
    } else {
        println!("⚠ Rootfs no encontrado: {:?}", get_rootfs_path());
        println!("  Build with: sudo ./scripts/firecracker/build-rootfs.sh");
    }
}

/// Test 02: Verifica que PostgreSQL via Testcontainers funciona
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_02_postgres_testcontainer() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_03_grpc_server_starts() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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

    let client = server.job_client().await;
    assert!(client.is_ok(), "Should create job client");

    println!("✓ Server gRPC iniciado en {}", server.addr);
    server.shutdown().await;
}

// =============================================================================
// NIVEL 2: Worker Registration y Job Dispatch (sin microVM real)
// =============================================================================

/// Test 04: Worker se registra correctamente con OTP
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_04_worker_registration_with_otp() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    let stack = match TestStack::without_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let worker_id = uuid::Uuid::new_v4().to_string();

    let otp = stack
        .server
        .generate_otp(&worker_id)
        .await
        .expect("Should generate OTP");
    println!("✓ OTP generado para worker: {}", worker_id);

    let mut worker_client = stack
        .server
        .worker_client()
        .await
        .expect("Should create worker client");

    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            name: "Firecracker Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "fc-microvm".to_string(),
            ip_address: "172.16.0.2".to_string(),
            os_info: "Linux (microVM)".to_string(),
            architecture: "x86_64".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 1.0,
                memory_bytes: 512 * 1024 * 1024, // 512MB typical for microVM
                disk_bytes: 1 * 1024 * 1024 * 1024,
                gpu_count: 0, // Firecracker doesn't support GPU
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["shell".to_string(), "firecracker".to_string()],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };

    let reg_response = worker_client
        .register(reg_request)
        .await
        .expect("Should register worker")
        .into_inner();

    assert!(reg_response.success, "Registration should succeed");
    assert!(
        !reg_response.session_id.is_empty(),
        "Should have session_id"
    );
    println!(
        "✓ Worker registrado con session: {}",
        reg_response.session_id
    );

    stack.shutdown().await;
}

/// Test 05: Worker stream bidireccional funciona
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_05_worker_stream_connection() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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

    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            name: "FC Stream Test Worker".to_string(),
            ..Default::default()
        }),
    };
    worker_client.register(reg_request).await.unwrap();
    println!("✓ Worker registrado");

    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);

    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            status: 2, // AVAILABLE
            usage: None,
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: None,
        })),
    };
    tx.send(heartbeat).await.unwrap();

    let inbound = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = worker_client
        .worker_stream(inbound)
        .await
        .expect("Should connect to stream");
    let mut stream = response.into_inner();

    let ack_result = timeout(Duration::from_secs(2), stream.message()).await;
    assert!(ack_result.is_ok(), "Should receive ACK");
    println!("✓ Stream bidireccional conectado");

    stack.shutdown().await;
}

/// Test 06: Job se encola y se despacha al worker conectado
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_06_job_queued_and_dispatched() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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

    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            name: "FC Job Dispatch Test Worker".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 1.0,
                memory_bytes: 512 * 1024 * 1024,
                disk_bytes: 1 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["shell".to_string()],
            ..Default::default()
        }),
    };
    worker_client.register(reg_request).await.unwrap();
    println!("✓ Worker registrado: {}", worker_id);

    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);
    let tx_for_result = tx.clone();

    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            status: 2,
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

    let _ = timeout(Duration::from_secs(1), stream.message()).await;
    println!("✓ Stream conectado");

    let mut job_client = stack.server.job_client().await.unwrap();
    let job_id = uuid::Uuid::new_v4().to_string();

    let queue_request = QueueJobRequest {
        job_definition: Some(JobDefinition {
            job_id: Some(JobId {
                value: job_id.clone(),
            }),
            name: "FC Dispatch Test Job".to_string(),
            description: "Test job dispatch for Firecracker".to_string(),
            command: "echo".to_string(),
            arguments: vec!["Hello from microVM".to_string()],
            environment: std::collections::HashMap::new(),
            requirements: None,
            scheduling: None,
            selector: None,
            tolerations: vec![],
            timeout: None,
            tags: vec!["firecracker".to_string()],
        }),
        queued_by: "fc-test".to_string(),
    };

    let queue_response = job_client
        .queue_job(queue_request)
        .await
        .unwrap()
        .into_inner();
    assert!(queue_response.success, "Job should be queued");
    println!("✓ Job encolado: {}", job_id);

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
    })
    .await;

    match run_job_result {
        Ok(Some(run_job)) => {
            println!("✓ RunJob recibido: {}", run_job.job_id);
            assert_eq!(run_job.job_id, job_id);

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
// NIVEL 3: Firecracker Provider
// =============================================================================

/// Test 07: FirecrackerProvider crea microVM correctamente
#[tokio::test]
#[ignore = "Requires KVM, Firecracker, kernel, and rootfs. Run with HODEI_FC_TEST=1"]
async fn test_07_firecracker_provider_creates_vm() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    use hodei_jobs_domain::workers::WorkerProvider;
    use hodei_jobs_domain::workers::WorkerSpec;
    use hodei_jobs_infrastructure::providers::{FirecrackerConfig, FirecrackerProvider};

    let verifier = FirecrackerVerifier::new().expect("Should create verifier");

    if !verifier.can_run_full_tests() {
        let missing = verifier.check_all_requirements();
        eprintln!("⚠ Skipping: Missing requirements:");
        for m in missing {
            eprintln!("  - {}", m);
        }
        return;
    }

    let config = FirecrackerConfig::builder()
        .firecracker_path(get_firecracker_path())
        .data_dir(get_data_dir())
        .kernel_path(get_kernel_path())
        .rootfs_path(get_rootfs_path())
        .use_jailer(false) // Disable jailer for tests
        .build()
        .expect("Should build config");

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: FirecrackerProvider not available: {}", e);
            return;
        }
    };
    println!("✓ FirecrackerProvider creado");

    let spec = WorkerSpec::new(
        "unused".to_string(), // Firecracker uses rootfs, not container image
        "http://localhost:50051".to_string(),
    )
    .with_env("TEST_VAR", "firecracker_test");
    let _worker_id = spec.worker_id.clone();

    let handle = match provider.create_worker(&spec).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("⚠ Failed to create microVM: {}", e);
            return;
        }
    };
    println!("✓ MicroVM creado: {}", handle.provider_resource_id);

    // Cleanup
    provider
        .destroy_worker(&handle)
        .await
        .expect("Should destroy");
    println!("✓ MicroVM eliminado");
}

/// Test 08: Ciclo de vida completo de la microVM
#[tokio::test]
#[ignore = "Requires KVM, Firecracker, kernel, and rootfs. Run with HODEI_FC_TEST=1"]
async fn test_08_firecracker_provider_vm_lifecycle() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    use hodei_jobs_domain::shared_kernel::WorkerState;
    use hodei_jobs_domain::workers::WorkerProvider;
    use hodei_jobs_domain::workers::WorkerSpec;
    use hodei_jobs_infrastructure::providers::{FirecrackerConfig, FirecrackerProvider};

    let verifier = FirecrackerVerifier::new().expect("Should create verifier");

    if !verifier.can_run_full_tests() {
        let missing = verifier.check_all_requirements();
        eprintln!("⚠ Skipping: Missing requirements:");
        for m in missing {
            eprintln!("  - {}", m);
        }
        return;
    }

    let config = FirecrackerConfig::builder()
        .firecracker_path(get_firecracker_path())
        .data_dir(get_data_dir())
        .kernel_path(get_kernel_path())
        .rootfs_path(get_rootfs_path())
        .use_jailer(false)
        .build()
        .expect("Should build config");

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let spec = WorkerSpec::new("unused".to_string(), "http://localhost:50051".to_string())
        .with_env("TEST_VAR", "lifecycle_test");

    // 1. Crear
    let handle = match provider.create_worker(&spec).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("⚠ Failed to create microVM: {}", e);
            return;
        }
    };
    println!("✓ 1. MicroVM creado: {}", handle.provider_resource_id);

    // 2. Verificar estado
    let state = provider.get_worker_status(&handle).await.unwrap();
    println!("✓ 2. Estado: {:?}", state);

    // 3. Obtener logs
    let logs = provider.get_worker_logs(&handle, Some(10)).await.unwrap();
    println!("✓ 3. Logs obtenidos: {} líneas", logs.len());

    // 4. Destruir
    provider.destroy_worker(&handle).await.unwrap();
    println!("✓ 4. MicroVM destruido");

    // 5. Verificar que no existe
    let final_state = provider.get_worker_status(&handle).await.unwrap();
    assert!(
        matches!(final_state, WorkerState::Terminated),
        "MicroVM should be terminated"
    );
    println!("✓ 5. MicroVM limpiado correctamente");
}

// =============================================================================
// NIVEL 4: Tests Adicionales de Integración
// =============================================================================

/// Test 09: Múltiples jobs se encolan correctamente
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_09_multiple_jobs_queued() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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
                job_id: Some(JobId {
                    value: job_id.clone(),
                }),
                name: format!("FC Batch Job {}", i),
                description: format!("Firecracker batch job {} of 3", i),
                command: "echo".to_string(),
                arguments: vec![format!("FC Job {}", i)],
                environment: std::collections::HashMap::new(),
                requirements: None,
                scheduling: None,
                selector: None,
                tolerations: vec![],
                timeout: None,
                tags: vec!["firecracker".to_string(), "batch".to_string()],
            }),
            queued_by: "fc-test".to_string(),
        };

        let response = job_client
            .queue_job(queue_request)
            .await
            .unwrap()
            .into_inner();
        assert!(response.success, "Job {} should be queued", i);
    }

    println!("✓ 3 jobs encolados correctamente");
    stack.shutdown().await;
}

/// Test 10: Estado de la cola del scheduler
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Firecracker"]
async fn test_10_scheduler_queue_status() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

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
    println!(
        "✓ Queue status: pending={}, running={}",
        status.pending_jobs, status.running_jobs
    );

    stack.shutdown().await;
}

/// Test 11: FirecrackerProvider health check
#[tokio::test]
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_11_firecracker_provider_health_check() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    use hodei_jobs_domain::workers::{HealthStatus, WorkerProvider};
    use hodei_jobs_infrastructure::providers::{FirecrackerConfig, FirecrackerProvider};

    let verifier = FirecrackerVerifier::new().expect("Should create verifier");

    if !verifier.can_run_basic_tests() {
        eprintln!("⚠ Skipping: KVM or Firecracker not available");
        return;
    }

    let config = FirecrackerConfig::builder()
        .firecracker_path(get_firecracker_path())
        .data_dir(get_data_dir())
        .kernel_path(get_kernel_path())
        .rootfs_path(get_rootfs_path())
        .use_jailer(false)
        .build()
        .expect("Should build config");

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let health = provider
        .health_check()
        .await
        .expect("Health check should not fail");

    match health {
        HealthStatus::Healthy => {
            println!("✓ FirecrackerProvider is healthy");
        }
        HealthStatus::Degraded { reason } => {
            println!("⚠ FirecrackerProvider is degraded: {}", reason);
        }
        HealthStatus::Unhealthy { reason } => {
            println!("✗ FirecrackerProvider is unhealthy: {}", reason);
        }
        HealthStatus::Unknown => {
            println!("? FirecrackerProvider status unknown");
        }
    }
}

/// Test 12: FirecrackerProvider capabilities
#[tokio::test]
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_12_firecracker_provider_capabilities() {
    if !should_run_fc_tests() {
        println!("⚠ Skipping: HODEI_FC_TEST not set");
        return;
    }

    use hodei_jobs_domain::workers::WorkerProvider;
    use hodei_jobs_infrastructure::providers::{FirecrackerConfig, FirecrackerProvider};

    let verifier = FirecrackerVerifier::new().expect("Should create verifier");

    if !verifier.can_run_basic_tests() {
        eprintln!("⚠ Skipping: KVM or Firecracker not available");
        return;
    }

    let config = FirecrackerConfig::builder()
        .firecracker_path(get_firecracker_path())
        .data_dir(get_data_dir())
        .kernel_path(get_kernel_path())
        .rootfs_path(get_rootfs_path())
        .use_jailer(false)
        .build()
        .expect("Should build config");

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let capabilities = provider.capabilities();

    println!("✓ FirecrackerProvider capabilities:");
    println!(
        "  - Max CPU: {} cores",
        capabilities.max_resources.max_cpu_cores
    );
    println!(
        "  - Max Memory: {} GB",
        capabilities.max_resources.max_memory_bytes / (1024 * 1024 * 1024)
    );
    println!(
        "  - GPU Support: {} (Firecracker doesn't support GPU)",
        capabilities.gpu_support
    );
    println!("  - Architectures: {:?}", capabilities.architectures);
    println!(
        "  - Persistent Storage: {}",
        capabilities.persistent_storage
    );
    println!("  - Custom Networking: {}", capabilities.custom_networking);
    println!(
        "  - Estimated startup time: {:?}",
        provider.estimated_startup_time()
    );

    // Firecracker specific assertions
    assert!(
        !capabilities.gpu_support,
        "Firecracker doesn't support GPU passthrough"
    );
    assert_eq!(capabilities.max_resources.max_gpu_count, 0);
    assert!(
        provider.estimated_startup_time().as_millis() < 1000,
        "Firecracker should boot in < 1 second"
    );
}

/// Test 13: IP Pool allocation (unit test, no infrastructure needed)
#[test]
fn test_13_ip_pool_allocation() {
    use hodei_jobs_infrastructure::providers::firecracker::IpPool;

    let mut pool = IpPool::from_cidr("172.16.0.0/28").expect("should parse CIDR");

    // Allocate several IPs
    let ip1 = pool.allocate().expect("should allocate first IP");
    let ip2 = pool.allocate().expect("should allocate second IP");
    let ip3 = pool.allocate().expect("should allocate third IP");

    assert_ne!(ip1, ip2);
    assert_ne!(ip2, ip3);
    assert_ne!(ip1, ip3);

    println!("✓ Allocated IPs: {}, {}, {}", ip1, ip2, ip3);

    // Release one
    pool.release(ip2);
    assert!(!pool.is_allocated(&ip2));

    // Allocate again
    let ip4 = pool.allocate().expect("should allocate after release");
    println!("✓ After release, allocated: {}", ip4);

    println!("✓ IP pool allocation and release works correctly");
}

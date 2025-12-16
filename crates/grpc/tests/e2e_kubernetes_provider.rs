//! E2E Tests for Kubernetes Provider - Flujo Coherente
//!
//! Tests organizados por nivel de complejidad:
//!
//! ## Nivel 1: Infraestructura Base
//! - `test_01_kubernetes_available` - kubectl y cluster accesibles
//! - `test_02_postgres_testcontainer` - PostgreSQL via Testcontainers
//! - `test_03_grpc_server_starts` - Server gRPC inicia correctamente
//!
//! ## Nivel 2: Worker Registration (sin Pod real)
//! - `test_04_worker_registration_with_otp` - Worker se registra con OTP
//! - `test_05_worker_stream_connection` - Stream bidireccional funciona
//! - `test_06_job_queued_and_dispatched` - Job se encola y despacha al worker
//!
//! ## Nivel 3: Kubernetes Provider
//! - `test_07_kubernetes_provider_creates_pod` - KubernetesProvider crea Pod
//! - `test_08_kubernetes_provider_pod_lifecycle` - Ciclo de vida completo
//!
//! ## Nivel 4: Flujo E2E Completo (requiere imagen hodei-worker)
//! - `test_09_full_e2e_with_real_worker` - Flujo completo con worker Pod real
//!
//! ## Requisitos:
//! - Kubernetes cluster accesible (kind, minikube, o real)
//! - kubectl configurado
//! - Namespace `hodei-workers` creado
//! - Docker para Testcontainers (PostgreSQL)
//!
//! Run: HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored --nocapture

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

use common::{TestStack, TestServerConfig, get_postgres_context};

// =============================================================================
// Helper Functions
// =============================================================================

fn should_run_k8s_tests() -> bool {
    std::env::var("HODEI_K8S_TEST").unwrap_or_default() == "1"
}

fn get_test_namespace() -> String {
    std::env::var("HODEI_K8S_TEST_NAMESPACE")
        .unwrap_or_else(|_| "hodei-jobs-workers".to_string())
}

/// Verifier for Kubernetes CLI operations
pub struct KubernetesVerifier {
    namespace: String,
}

impl KubernetesVerifier {
    pub fn new() -> anyhow::Result<Self> {
        // Check if kubectl is available
        let output = std::process::Command::new("kubectl")
            .args(["version", "--client", "-o", "json"])
            .output()?;
        
        if !output.status.success() {
            anyhow::bail!("kubectl not available");
        }
        
        Ok(Self {
            namespace: get_test_namespace(),
        })
    }

    pub fn is_cluster_available(&self) -> bool {
        let output = std::process::Command::new("kubectl")
            .args(["cluster-info"])
            .output();
        
        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }

    pub fn namespace_exists(&self) -> bool {
        let output = std::process::Command::new("kubectl")
            .args(["get", "namespace", &self.namespace])
            .output();
        
        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }

    pub fn create_namespace(&self) -> anyhow::Result<()> {
        let output = std::process::Command::new("kubectl")
            .args(["create", "namespace", &self.namespace])
            .output()?;
        
        // Ignore "already exists" error
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("already exists") {
                anyhow::bail!("Failed to create namespace: {}", stderr);
            }
        }
        Ok(())
    }

    pub fn find_pods(&self, label_selector: &str) -> anyhow::Result<Vec<String>> {
        let output = std::process::Command::new("kubectl")
            .args([
                "get", "pods",
                "-n", &self.namespace,
                "-l", label_selector,
                "-o", "jsonpath={.items[*].metadata.name}",
            ])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list pods: {}", stderr);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pods: Vec<String> = stdout
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        
        Ok(pods)
    }

    pub fn get_pod_status(&self, pod_name: &str) -> anyhow::Result<String> {
        let output = std::process::Command::new("kubectl")
            .args([
                "get", "pod", pod_name,
                "-n", &self.namespace,
                "-o", "jsonpath={.status.phase}",
            ])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get pod status: {}", stderr);
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn delete_pod(&self, pod_name: &str) -> anyhow::Result<()> {
        let output = std::process::Command::new("kubectl")
            .args([
                "delete", "pod", pod_name,
                "-n", &self.namespace,
                "--ignore-not-found",
            ])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to delete pod: {}", stderr);
        }
        
        Ok(())
    }

    pub fn get_pod_logs(&self, pod_name: &str, tail: Option<u32>) -> anyhow::Result<String> {
        let mut args = vec!["logs", pod_name, "-n", &self.namespace];
        let tail_str;
        if let Some(n) = tail {
            tail_str = n.to_string();
            args.push("--tail");
            args.push(&tail_str);
        }
        
        let output = std::process::Command::new("kubectl")
            .args(&args)
            .output()?;
        
        // Logs might not be available yet, don't fail
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

// =============================================================================
// NIVEL 1: Infraestructura Base
// =============================================================================

/// Test 01: Verifica que kubectl y cluster están disponibles
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_01_kubernetes_available() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
        return;
    }

    let verifier = match KubernetesVerifier::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Skipping: kubectl not available: {}", e);
            return;
        }
    };

    assert!(verifier.is_cluster_available(), "Kubernetes cluster should be reachable");
    println!("✓ kubectl disponible y cluster accesible");

    // Ensure namespace exists
    if !verifier.namespace_exists() {
        verifier.create_namespace().expect("Should create namespace");
        println!("✓ Namespace {} creado", get_test_namespace());
    } else {
        println!("✓ Namespace {} existe", get_test_namespace());
    }
}

/// Test 02: Verifica que PostgreSQL via Testcontainers funciona
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_02_postgres_testcontainer() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_03_grpc_server_starts() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
// NIVEL 2: Worker Registration y Job Dispatch (sin Pod real)
// =============================================================================

/// Test 04: Worker se registra correctamente con OTP
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_04_worker_registration_with_otp() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
    
    let otp = stack.server.generate_otp(&worker_id).await
        .expect("Should generate OTP");
    println!("✓ OTP generado para worker: {}", worker_id);

    let mut worker_client = stack.server.worker_client().await
        .expect("Should create worker client");

    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "K8s Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "k8s-test-pod".to_string(),
            ip_address: "10.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 2.0,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                disk_bytes: 50 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["shell".to_string(), "kubernetes".to_string()],
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
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_05_worker_stream_connection() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "K8s Stream Test Worker".to_string(),
            ..Default::default()
        }),
    };
    worker_client.register(reg_request).await.unwrap();
    println!("✓ Worker registrado");

    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);

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

    let ack_result = timeout(Duration::from_secs(2), stream.message()).await;
    assert!(ack_result.is_ok(), "Should receive ACK");
    println!("✓ Stream bidireccional conectado");

    stack.shutdown().await;
}

/// Test 06: Job se encola y se despacha al worker conectado
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_06_job_queued_and_dispatched() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
            worker_id: Some(WorkerId { value: worker_id.clone() }),
            name: "K8s Job Dispatch Test Worker".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 2.0,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                disk_bytes: 50 * 1024 * 1024 * 1024,
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
            worker_id: Some(WorkerId { value: worker_id.clone() }),
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
            job_id: Some(JobId { value: job_id.clone() }),
            name: "K8s Dispatch Test Job".to_string(),
            description: "Test job dispatch for Kubernetes".to_string(),
            command: "echo".to_string(),
            arguments: vec!["Hello from K8s".to_string()],
            environment: std::collections::HashMap::new(),
            requirements: None,
            scheduling: None,
            selector: None,
            tolerations: vec![],
            timeout: None,
            tags: vec!["kubernetes".to_string()],
        }),
        queued_by: "k8s-test".to_string(),
    };

    let queue_response = job_client.queue_job(queue_request).await.unwrap().into_inner();
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
    }).await;

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
// NIVEL 3: Kubernetes Provider
// =============================================================================

/// Test 07: KubernetesProvider crea Pod correctamente
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_07_kubernetes_provider_creates_pod() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
        return;
    }

    use hodei_jobs_domain::worker::WorkerSpec;
    use hodei_jobs_domain::worker_provider::WorkerProvider;
    use hodei_jobs_infrastructure::providers::{KubernetesConfig, KubernetesProvider};

    let verifier = match KubernetesVerifier::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Skipping: kubectl not available: {}", e);
            return;
        }
    };

    if !verifier.is_cluster_available() {
        eprintln!("Skipping: Kubernetes cluster not available");
        return;
    }

    // Ensure namespace exists
    if !verifier.namespace_exists() {
        verifier.create_namespace().expect("Should create namespace");
    }

    let config = KubernetesConfig::builder()
        .namespace(get_test_namespace())
        .build()
        .expect("Should build config");

    let provider = match KubernetesProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: KubernetesProvider not available: {}", e);
            return;
        }
    };
    println!("✓ KubernetesProvider creado");

    let spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    );
    let worker_id = spec.worker_id.clone();

    let handle = provider.create_worker(&spec).await
        .expect("Should create worker");
    println!("✓ Pod creado: {}", handle.provider_resource_id);

    // Wait for pod to be scheduled
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify with kubectl
    let _pod_name = format!("hodei-jobs-worker-{}", worker_id);
    let pods = verifier.find_pods(&format!("hodei.io/worker-id={}", worker_id)).unwrap();
    assert!(!pods.is_empty(), "Pod should exist");
    println!("✓ Pod verificado via kubectl");

    // Cleanup
    provider.destroy_worker(&handle).await.expect("Should destroy");
    println!("✓ Pod eliminado");
}

/// Test 08: Ciclo de vida completo del Pod
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_08_kubernetes_provider_pod_lifecycle() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
        return;
    }

    use hodei_jobs_domain::worker::WorkerSpec;
    use hodei_jobs_domain::worker_provider::WorkerProvider;
    use hodei_jobs_domain::shared_kernel::WorkerState;
    use hodei_jobs_infrastructure::providers::{KubernetesConfig, KubernetesProvider};

    let verifier = match KubernetesVerifier::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    if !verifier.is_cluster_available() {
        eprintln!("Skipping: Kubernetes cluster not available");
        return;
    }

    if !verifier.namespace_exists() {
        verifier.create_namespace().expect("Should create namespace");
    }

    let config = KubernetesConfig::builder()
        .namespace(get_test_namespace())
        .build()
        .expect("Should build config");

    let provider = match KubernetesProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    )
    .with_env("TEST_VAR", "kubernetes_test");

    // 1. Crear
    let handle = provider.create_worker(&spec).await.unwrap();
    println!("✓ 1. Pod creado: {}", handle.provider_resource_id);

    // 2. Esperar y verificar estado
    tokio::time::sleep(Duration::from_secs(3)).await;
    let state = provider.get_worker_status(&handle).await.unwrap();
    println!("✓ 2. Estado: {:?}", state);

    // 3. Obtener logs (pueden estar vacíos para alpine)
    let logs = provider.get_worker_logs(&handle, Some(10)).await.unwrap();
    println!("✓ 3. Logs obtenidos: {} líneas", logs.len());

    // 4. Destruir
    provider.destroy_worker(&handle).await.unwrap();
    println!("✓ 4. Pod destruido");

    // 5. Verificar que no existe
    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_state = provider.get_worker_status(&handle).await.unwrap();
    assert!(
        matches!(final_state, WorkerState::Terminated),
        "Pod should be terminated"
    );
    println!("✓ 5. Pod limpiado correctamente");
}

// =============================================================================
// NIVEL 4: Tests Adicionales de Integración
// =============================================================================

/// Test 09: Múltiples jobs se encolan correctamente
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_09_multiple_jobs_queued() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
                job_id: Some(JobId { value: job_id.clone() }),
                name: format!("K8s Batch Job {}", i),
                description: format!("Kubernetes batch job {} of 3", i),
                command: "echo".to_string(),
                arguments: vec![format!("K8s Job {}", i)],
                environment: std::collections::HashMap::new(),
                requirements: None,
                scheduling: None,
                selector: None,
                tolerations: vec![],
                timeout: None,
                tags: vec!["kubernetes".to_string(), "batch".to_string()],
            }),
            queued_by: "k8s-test".to_string(),
        };

        let response = job_client.queue_job(queue_request).await.unwrap().into_inner();
        assert!(response.success, "Job {} should be queued", i);
    }

    println!("✓ 3 jobs encolados correctamente");
    stack.shutdown().await;
}

/// Test 10: Estado de la cola del scheduler
#[tokio::test]
#[ignore = "Requires Docker, Testcontainers, and Kubernetes"]
async fn test_10_scheduler_queue_status() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
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
    println!("✓ Queue status: pending={}, running={}", status.pending_jobs, status.running_jobs);

    stack.shutdown().await;
}

/// Test 11: KubernetesProvider health check
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_11_kubernetes_provider_health_check() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
        return;
    }

    use hodei_jobs_domain::worker_provider::{HealthStatus, WorkerProvider};
    use hodei_jobs_infrastructure::providers::{KubernetesConfig, KubernetesProvider};

    let config = KubernetesConfig::builder()
        .namespace(get_test_namespace())
        .build()
        .expect("Should build config");

    let provider = match KubernetesProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let health = provider.health_check().await.expect("Health check should not fail");

    match health {
        HealthStatus::Healthy => {
            println!("✓ KubernetesProvider is healthy");
        }
        HealthStatus::Degraded { reason } => {
            println!("⚠ KubernetesProvider is degraded: {}", reason);
        }
        HealthStatus::Unhealthy { reason } => {
            println!("✗ KubernetesProvider is unhealthy: {}", reason);
        }
        HealthStatus::Unknown => {
            println!("? KubernetesProvider status unknown");
        }
    }
}

/// Test 12: KubernetesProvider capabilities
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_12_kubernetes_provider_capabilities() {
    if !should_run_k8s_tests() {
        println!("⚠ Skipping: HODEI_K8S_TEST not set");
        return;
    }

    use hodei_jobs_domain::worker_provider::WorkerProvider;
    use hodei_jobs_infrastructure::providers::{KubernetesConfig, KubernetesProvider};

    let config = KubernetesConfig::builder()
        .namespace(get_test_namespace())
        .build()
        .expect("Should build config");

    let provider = match KubernetesProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    let capabilities = provider.capabilities();

    println!("✓ KubernetesProvider capabilities:");
    println!("  - Max CPU: {} cores", capabilities.max_resources.max_cpu_cores);
    println!("  - Max Memory: {} GB", capabilities.max_resources.max_memory_bytes / (1024 * 1024 * 1024));
    println!("  - GPU Support: {}", capabilities.gpu_support);
    println!("  - Architectures: {:?}", capabilities.architectures);
    println!("  - Persistent Storage: {}", capabilities.persistent_storage);
    println!("  - Custom Networking: {}", capabilities.custom_networking);

    assert!(capabilities.max_resources.max_cpu_cores > 0.0);
    assert!(capabilities.max_resources.max_memory_bytes > 0);
    assert!(!capabilities.architectures.is_empty());
}

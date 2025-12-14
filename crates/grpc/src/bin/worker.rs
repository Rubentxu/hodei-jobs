//! Hodei Jobs Worker Agent
//!
//! Real worker that connects to the gRPC server and executes jobs using Docker.

use hodei_jobs::{
    worker_agent_service_client::WorkerAgentServiceClient,
    RegisterWorkerRequest, WorkerInfo, WorkerId, ResourceCapacity,
    UnregisterWorkerRequest, ResourceUsage,
    WorkerMessage, WorkerHeartbeat, LogEntry, JobResultMessage,
    worker_message::Payload as WorkerPayload,
    server_message::Payload as ServerPayload,
    command_spec::CommandType as ProtoCommandType,
};
use std::collections::HashMap;
use std::process::Command as StdCommand;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::{info, warn, error, debug, Level};
use tracing_subscriber::FmtSubscriber;
use prost_types::Timestamp;
use std::time::SystemTime;

#[derive(Clone)]
struct WorkerConfig {
    worker_id: String,
    worker_name: String,
    server_addr: String,
    capabilities: Vec<String>,
    cpu_cores: f64,
    memory_bytes: i64,
    disk_bytes: i64,
    auth_token: String,  // OTP token from environment
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        
        Self {
            worker_id: uuid::Uuid::new_v4().to_string(),
            worker_name: format!("Docker Worker on {}", hostname),
            server_addr: "http://localhost:50051".to_string(),
            capabilities: vec!["docker".to_string(), "shell".to_string()],
            cpu_cores: num_cpus::get() as f64,
            memory_bytes: get_system_memory(),
            disk_bytes: get_disk_space(),
            auth_token: String::new(),  // Must be provided by HODEI_TOKEN env
        }
    }
}

fn get_system_memory() -> i64 {
    // Try to get actual system memory, fallback to 8GB
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb) = line.split_whitespace().nth(1) {
                        if let Ok(kb_val) = kb.parse::<i64>() {
                            return kb_val * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
    }
    8 * 1024 * 1024 * 1024 // 8GB default
}

fn get_disk_space() -> i64 {
    // Default to 100GB
    100 * 1024 * 1024 * 1024
}

/// Job executor for Docker and shell commands
struct JobExecutor {
    docker_available: bool,
}

impl JobExecutor {
    fn new() -> Self {
        let docker_available = StdCommand::new("docker")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Self { docker_available }
    }

    fn is_docker_available(&self) -> bool {
        self.docker_available
    }

    /// Execute a job from RunJobCommand
    async fn execute_from_command(
        &self,
        job_id: &str,
        command_spec: Option<hodei_jobs::CommandSpec>,
        env_vars: HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
    ) -> Result<(i32, String, String), String> {
        let spec = command_spec.ok_or("No command spec provided")?;
        
        match spec.command_type {
            Some(ProtoCommandType::Shell(shell)) => {
                self.execute_shell(job_id, &shell.cmd, &shell.args, &env_vars, working_dir, log_sender).await
            }
            Some(ProtoCommandType::Script(script)) => {
                self.execute_script(job_id, &script.interpreter, &script.content, &env_vars, working_dir, log_sender).await
            }
            None => Err("Empty command spec".to_string()),
        }
    }

    async fn execute_shell(
        &self,
        job_id: &str,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
    ) -> Result<(i32, String, String), String> {
        info!("Executing shell job {}: {} {:?}", job_id, command, args);
        
        // Send log entry
        let _ = log_sender.send(create_log_message(job_id, &format!("Starting: {} {:?}", command, args), false)).await;
        
        let mut cmd = StdCommand::new(command);
        cmd.args(args);
        
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()
            .map_err(|e| format!("Failed to execute command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        // Send stdout logs
        for line in stdout.lines() {
            let _ = log_sender.send(create_log_message(job_id, line, false)).await;
        }
        
        // Send stderr logs
        for line in stderr.lines() {
            let _ = log_sender.send(create_log_message(job_id, line, true)).await;
        }

        Ok((exit_code, stdout, stderr))
    }

    async fn execute_script(
        &self,
        job_id: &str,
        interpreter: &str,
        content: &str,
        env_vars: &HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
    ) -> Result<(i32, String, String), String> {
        info!("Executing script job {} with interpreter {}", job_id, interpreter);
        
        let _ = log_sender.send(create_log_message(job_id, &format!("Starting script with {}", interpreter), false)).await;
        
        let mut cmd = StdCommand::new(interpreter);
        cmd.arg("-c").arg(content);
        
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()
            .map_err(|e| format!("Failed to execute script: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        for line in stdout.lines() {
            let _ = log_sender.send(create_log_message(job_id, line, false)).await;
        }
        for line in stderr.lines() {
            let _ = log_sender.send(create_log_message(job_id, line, true)).await;
        }

        Ok((exit_code, stdout, stderr))
    }

    #[allow(dead_code)]
    async fn execute_docker_job(
        &self,
        job_id: &str,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        image: Option<&str>,
    ) -> Result<(i32, String, String), String> {
        let image = image.unwrap_or("alpine:latest");
        
        info!("Executing job {} with Docker image {}", job_id, image);
        
        // Build docker command
        let mut docker_args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            format!("hodei-job-{}", job_id),
        ];

        // Add environment variables
        for (key, value) in env_vars {
            docker_args.push("-e".to_string());
            docker_args.push(format!("{}={}", key, value));
        }

        // Add image and command
        docker_args.push(image.to_string());
        docker_args.push(command.to_string());
        docker_args.extend(args.iter().cloned());

        info!("Running: docker {}", docker_args.join(" "));

        let output = StdCommand::new("docker")
            .args(&docker_args)
            .output()
            .map_err(|e| format!("Failed to execute docker: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        if output.status.success() {
            info!("Job {} completed successfully", job_id);
        } else {
            warn!("Job {} failed with exit code {}", job_id, exit_code);
        }

        Ok((exit_code, stdout, stderr))
    }
}

/// Helper to create log message for stream
fn create_log_message(job_id: &str, line: &str, is_stderr: bool) -> WorkerMessage {
    WorkerMessage {
        payload: Some(WorkerPayload::Log(LogEntry {
            job_id: job_id.to_string(),
            line: line.to_string(),
            is_stderr,
            timestamp: Some(current_timestamp()),
        })),
    }
}

/// Helper to create job result message
fn create_result_message(job_id: &str, exit_code: i32, success: bool, error_msg: Option<String>) -> WorkerMessage {
    WorkerMessage {
        payload: Some(WorkerPayload::Result(JobResultMessage {
            job_id: job_id.to_string(),
            exit_code,
            success,
            error_message: error_msg.unwrap_or_default(),
            completed_at: Some(current_timestamp()),
        })),
    }
}

/// Helper to create heartbeat message
fn create_heartbeat_message(worker_id: &str, status: i32, usage: ResourceUsage) -> WorkerMessage {
    WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            status,
            usage: Some(usage),
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: Some(current_timestamp()),
        })),
    }
}

fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[derive(Clone)]
struct Worker {
    config: WorkerConfig,
    executor: Arc<JobExecutor>,
    active_jobs: Arc<tokio::sync::RwLock<Vec<String>>>,
}

impl Worker {
    fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            executor: Arc::new(JobExecutor::new()),
            active_jobs: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    async fn connect(&self) -> Result<Channel, Box<dyn std::error::Error>> {
        info!("Connecting to server at {}", self.config.server_addr);
        let channel = Channel::from_shared(self.config.server_addr.clone())?
            .connect()
            .await?;
        info!("Connected successfully");
        Ok(channel)
    }

    async fn register(&self, channel: Channel) -> Result<String, Box<dyn std::error::Error>> {
        let mut client = WorkerAgentServiceClient::new(channel);
        
        // PRD v6.0: Register with OTP token
        let request = RegisterWorkerRequest {
            auth_token: self.config.auth_token.clone(),
            session_id: String::new(),  // New registration
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: self.config.worker_id.clone() }),
                name: self.config.worker_name.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                hostname: hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "unknown".to_string()),
                ip_address: get_local_ip(),
                os_info: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
                architecture: std::env::consts::ARCH.to_string(),
                capacity: Some(ResourceCapacity {
                    cpu_cores: self.config.cpu_cores,
                    memory_bytes: self.config.memory_bytes,
                    disk_bytes: self.config.disk_bytes,
                    gpu_count: 0,
                    custom_resources: std::collections::HashMap::new(),
                }),
                capabilities: self.config.capabilities.clone(),
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        };

        let response = client.register(request).await?;
        let resp = response.into_inner();
        
        if resp.success {
            info!("Worker registered successfully: {} (session: {})", 
                self.config.worker_id, resp.session_id);
            Ok(resp.session_id)
        } else {
            error!("Failed to register worker: {}", resp.message);
            Err(resp.message.into())
        }
    }

    /// Start the bidirectional job stream (PRD v6.0)
    /// This connects to WorkerStream and handles incoming commands
    async fn start_job_stream(&self, channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = WorkerAgentServiceClient::new(channel);
        let config = self.config.clone();
        let executor = self.executor.clone();
        let active_jobs = self.active_jobs.clone();
        
        // Channel for outbound messages (heartbeats, logs, results)
        let (tx, rx) = mpsc::channel::<WorkerMessage>(100);
        let tx_for_heartbeat = tx.clone();
        let tx_for_jobs = tx.clone();
        
        // Spawn heartbeat task
        let heartbeat_config = config.clone();
        let heartbeat_jobs = active_jobs.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                
                let jobs = heartbeat_jobs.read().await;
                let usage = ResourceUsage {
                    cpu_cores: get_cpu_usage() as f64 / 100.0 * heartbeat_config.cpu_cores,
                    memory_bytes: (get_memory_usage() as f64 / 100.0 * heartbeat_config.memory_bytes as f64) as i64,
                    disk_bytes: (0.3 * heartbeat_config.disk_bytes as f64) as i64,
                    gpu_count: 0,
                    custom_usage: HashMap::new(),
                };
                let status = if jobs.is_empty() { 2 } else { 3 }; // 2=AVAILABLE, 3=BUSY
                drop(jobs);
                
                let msg = create_heartbeat_message(&heartbeat_config.worker_id, status, usage);
                if tx_for_heartbeat.send(msg).await.is_err() {
                    warn!("Heartbeat channel closed");
                    break;
                }
                debug!("Heartbeat sent");
            }
        });
        
        // Create outbound stream
        let outbound = ReceiverStream::new(rx);
        
        // Connect to server stream
        info!("Connecting to job stream...");
        let response = client.worker_stream(outbound).await?;
        let mut inbound = response.into_inner();
        
        info!("✓ Connected to job stream. Waiting for commands...");
        
        // Process incoming server messages
        while let Some(msg_result) = inbound.next().await {
            match msg_result {
                Ok(server_msg) => {
                    if let Some(payload) = server_msg.payload {
                        match payload {
                            ServerPayload::RunJob(run_job) => {
                                info!("📥 Received job: {}", run_job.job_id);
                                
                                // Track active job
                                active_jobs.write().await.push(run_job.job_id.clone());
                                
                                // Execute job in background
                                let job_id = run_job.job_id.clone();
                                let exec = executor.clone();
                                let tx_job = tx_for_jobs.clone();
                                let jobs_ref = active_jobs.clone();
                                
                                tokio::spawn(async move {
                                    let working_dir = if run_job.working_dir.is_empty() {
                                        None
                                    } else {
                                        Some(run_job.working_dir.clone())
                                    };
                                    
                                    let result = exec.execute_from_command(
                                        &job_id,
                                        run_job.command,
                                        run_job.env,
                                        working_dir,
                                        tx_job.clone(),
                                    ).await;
                                    
                                    // Send result
                                    let result_msg = match result {
                                        Ok((exit_code, _, _)) => {
                                            info!("✅ Job {} completed with exit code {}", job_id, exit_code);
                                            create_result_message(&job_id, exit_code, exit_code == 0, None)
                                        }
                                        Err(e) => {
                                            error!("❌ Job {} failed: {}", job_id, e);
                                            create_result_message(&job_id, -1, false, Some(e))
                                        }
                                    };
                                    
                                    let _ = tx_job.send(result_msg).await;
                                    
                                    // Remove from active jobs
                                    jobs_ref.write().await.retain(|j| j != &job_id);
                                });
                            }
                            ServerPayload::Cancel(cancel) => {
                                warn!("📛 Cancel request for job: {}", cancel.job_id);
                                // TODO: Implement job cancellation
                            }
                            ServerPayload::Ack(ack) => {
                                debug!("ACK received: {}", ack.message_id);
                            }
                            ServerPayload::KeepAlive(_) => {
                                debug!("KeepAlive received");
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    break;
                }
            }
        }
        
        warn!("Job stream disconnected");
        Ok(())
    }

    async fn unregister(&self, channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = WorkerAgentServiceClient::new(channel);
        
        let request = UnregisterWorkerRequest {
            worker_id: Some(WorkerId { value: self.config.worker_id.clone() }),
            reason: "Shutdown".to_string(),
        };

        let response = client.unregister_worker(request).await?;
        let resp = response.into_inner();
        
        if resp.success {
            info!("Worker unregistered successfully");
        }

        Ok(())
    }

}

fn get_local_ip() -> String {
    // Simple way to get local IP
    "127.0.0.1".to_string()
}

fn get_cpu_usage() -> f32 {
    // Simplified - return a mock value
    // In production, use sysinfo crate
    15.0
}

fn get_memory_usage() -> f32 {
    // Simplified - return a mock value
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let mut total: f64 = 0.0;
            let mut available: f64 = 0.0;
            
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb) = line.split_whitespace().nth(1) {
                        total = kb.parse().unwrap_or(0.0);
                    }
                } else if line.starts_with("MemAvailable:") {
                    if let Some(kb) = line.split_whitespace().nth(1) {
                        available = kb.parse().unwrap_or(0.0);
                    }
                }
            }
            
            if total > 0.0 {
                return ((total - available) / total * 100.0) as f32;
            }
        }
    }
    45.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse config from environment
    let auth_token = env::var("HODEI_TOKEN").unwrap_or_default();
    if auth_token.is_empty() {
        warn!("HODEI_TOKEN not set - registration may fail without OTP");
    }
    
    let config = WorkerConfig {
        server_addr: env::var("HODEI_SERVER").unwrap_or_else(|_| "http://localhost:50051".to_string()),
        worker_id: env::var("WORKER_ID").unwrap_or_else(|_| WorkerConfig::default().worker_id),
        worker_name: env::var("WORKER_NAME").unwrap_or_else(|_| WorkerConfig::default().worker_name),
        auth_token,
        ..Default::default()
    };

    info!("╔═══════════════════════════════════════════════════════════════╗");
    info!("║           Hodei Jobs Worker Agent                             ║");
    info!("╚═══════════════════════════════════════════════════════════════╝");
    info!("Worker ID: {}", config.worker_id);
    info!("Server: {}", config.server_addr);
    info!("Capabilities: {:?}", config.capabilities);
    info!("CPU Cores: {}", config.cpu_cores);
    info!("Memory: {} GB", config.memory_bytes / 1024 / 1024 / 1024);

    let worker = Worker::new(config);

    // Check Docker availability
    if worker.executor.is_docker_available() {
        info!("✓ Docker is available");
    } else {
        warn!("✗ Docker is NOT available - will run in shell-only mode");
    }

    // Connect to server
    let channel = match worker.connect().await {
        Ok(ch) => ch,
        Err(e) => {
            error!("Failed to connect to server: {}", e);
            error!("Make sure the server is running: cargo run --bin server -p hodei-jobs-grpc");
            return Err(e);
        }
    };

    // Register worker (PRD v6.0: with OTP)
    let _session_id = worker.register(channel.clone()).await?;

    info!("Worker is running. Waiting for jobs via stream...");
    
    // Start job stream with graceful shutdown
    let stream_channel = channel.clone();
    let worker_clone = worker.clone();
    
    tokio::select! {
        result = worker_clone.start_job_stream(stream_channel) => {
            if let Err(e) = result {
                error!("Job stream error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }
    
    info!("Shutting down...");
    worker.unregister(channel).await?;

    Ok(())
}

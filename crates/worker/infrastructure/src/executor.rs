use hodei_jobs::{
    CommandSpec, LogEntry, WorkerMessage, command_spec::CommandType as ProtoCommandType,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use crate::metrics::WorkerMetrics;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;
use tracing::{debug, error, info, warn};

use secrecy::ExposeSecret;

use crate::logging::{FileLogger, LogBatcher};
use crate::secret_injector::{
    InjectionConfig, InjectionStrategy, PreparedExecution, SecretInjector, SecretString,
};

/// Errors that can occur during path validation
#[derive(Debug, thiserror::Error)]
pub enum PathSecurityError {
    #[error("Path '{requested}' escapes sandbox to '{resolved}'")]
    PathEscapeAttempt { requested: String, resolved: String },
    #[error("Base path does not exist: {base_path}")]
    InvalidBasePath { base_path: String },
    #[error("Requested path does not exist: {path}")]
    PathDoesNotExist { path: String },
}

/// Validates that a path is within the allowed sandbox
///
/// # Example
///
/// ```
/// let sandbox = PathSandbox::new("/var/lib/hodei/jobs");
/// let safe_path = sandbox.validate("/etc/passwd")?; // Returns error
/// let safe_path = sandbox.validate("job-123")?; // Returns sandbox/job-123
/// ```
#[derive(Debug, Clone)]
pub struct PathSandbox {
    base_dir: PathBuf,
}

impl PathSandbox {
    /// Creates a new sandbox with the specified base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, PathSecurityError> {
        let base_dir = base_dir.into();
        // Ensure base directory exists
        if !base_dir.exists() {
            return Err(PathSecurityError::InvalidBasePath {
                base_path: base_dir.to_string_lossy().to_string(),
            });
        }
        // Canonicalize to resolve any .. or . components
        let canonical_base =
            base_dir
                .canonicalize()
                .map_err(|_| PathSecurityError::InvalidBasePath {
                    base_path: base_dir.to_string_lossy().to_string(),
                })?;
        Ok(Self {
            base_dir: canonical_base,
        })
    }

    /// Returns the base directory of this sandbox
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Validates a requested path and returns the sandboxed path
    ///
    /// # Arguments
    ///
    /// * `requested_path` - The path requested by the job (relative or absolute)
    ///
    /// # Returns
    ///
    /// The validated path within the sandbox, or an error if the path escapes
    pub fn validate(&self, requested_path: &str) -> Result<PathBuf, PathSecurityError> {
        let requested = PathBuf::from(requested_path);

        // If it's an absolute path, validate it directly
        let resolved = if requested.is_absolute() {
            // Check if the absolute path exists first
            if !requested.exists() {
                return Err(PathSecurityError::PathDoesNotExist {
                    path: requested.to_string_lossy().to_string(),
                });
            }
            requested
                .canonicalize()
                .map_err(|_| PathSecurityError::PathDoesNotExist {
                    path: requested.to_string_lossy().to_string(),
                })?
        } else {
            // Relative path: join with base and resolve
            self.base_dir.join(&requested)
        };

        // Verify the resolved path is within the sandbox
        if !resolved.starts_with(&self.base_dir) {
            return Err(PathSecurityError::PathEscapeAttempt {
                requested: requested_path.to_string(),
                resolved: resolved.to_string_lossy().to_string(),
            });
        }

        Ok(resolved)
    }
}

fn current_timestamp() -> prost_types::Timestamp {
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0));
    prost_types::Timestamp {
        seconds: since_epoch.as_secs() as i64,
        nanos: since_epoch.subsec_nanos() as i32,
    }
}

/// Job executor for local system commands (Shell, Scripts)
pub struct JobExecutor {
    log_batch_size: usize,
    log_flush_interval_ms: u64,
    log_dir: std::path::PathBuf,
    metrics: Arc<WorkerMetrics>,
}

impl JobExecutor {
    pub fn new(
        log_batch_size: usize,
        log_flush_interval_ms: u64,
        log_dir: std::path::PathBuf,
        metrics: Arc<WorkerMetrics>,
    ) -> Self {
        Self {
            log_batch_size,
            log_flush_interval_ms,
            log_dir,
            metrics,
        }
    }

    /// Execute a job from RunJobCommand with timeout and cancellation support
    pub async fn execute_from_command(
        &self,
        job_id: &str,
        command_spec: Option<CommandSpec>,
        env_vars: HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: Option<u64>,
        _stdin_content: Option<String>,
        secrets_json: Option<String>,
        injection_strategy: InjectionStrategy,
    ) -> Result<(i32, String, String), String> {
        let timeout_secs = timeout_secs.unwrap_or(3600); // Default 1h

        // Parse secrets if provided
        let secrets = if let Some(secrets_json_str) = secrets_json {
            match serde_json::from_str::<HashMap<String, String>>(&secrets_json_str) {
                Ok(secrets) => {
                    info!(
                        secrets_count = secrets.len(),
                        strategy = ?injection_strategy,
                        "Parsed secrets for injection strategy"
                    );
                    Some(secrets)
                }
                Err(e) => {
                    warn!("Failed to parse secrets JSON: {}", e);
                    return Err(format!("Invalid secrets JSON: {}", e));
                }
            }
        } else {
            None
        };

        // Inject secrets using the specified strategy
        let config = InjectionConfig::with_strategy(injection_strategy);
        let injector = SecretInjector::new(config);
        let prepared_execution = if let Some(ref secrets) = secrets {
            injector
                .prepare_execution(job_id, secrets, &env_vars)
                .map_err(|e| {
                    format!("Failed to prepare execution with injection strategy: {}", e)
                })?
        } else {
            PreparedExecution {
                env_vars,
                stdin_content: None,
                secrets_dir: None,
                strategy: injection_strategy,
            }
        };

        // Extract secrets_dir for cleanup after execution
        let secrets_dir = prepared_execution.secrets_dir.clone();

        match command_spec.and_then(|cs| cs.command_type) {
            Some(ProtoCommandType::Shell(shell)) => {
                let result = self
                    .execute_shell_with_timeout(
                        job_id,
                        &shell.cmd,
                        &shell.args,
                        prepared_execution,
                        working_dir,
                        log_sender,
                        timeout_secs,
                    )
                    .await;

                // Cleanup tmpfs files if they were created
                if let Some(ref dir) = secrets_dir {
                    if let Err(e) = SecretInjector::cleanup_tmpfs_secrets(dir).await {
                        warn!("Failed to cleanup tmpfs secrets directory: {}", e);
                    }
                }

                result
            }
            Some(ProtoCommandType::Script(script)) => {
                let result = self
                    .execute_script_with_timeout(
                        job_id,
                        &script.interpreter,
                        &script.content,
                        prepared_execution,
                        working_dir,
                        log_sender,
                        timeout_secs,
                    )
                    .await;

                // Cleanup tmpfs files if they were created
                if let Some(ref dir) = secrets_dir {
                    if let Err(e) = SecretInjector::cleanup_tmpfs_secrets(dir).await {
                        warn!("Failed to cleanup tmpfs secrets directory: {}", e);
                    }
                }

                result
            }
            None => Err("Empty command spec".to_string()),
        }
    }

    async fn execute_shell_with_timeout(
        &self,
        job_id: &str,
        cmd: &str,
        args: &[String],
        prepared_execution: PreparedExecution,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: u64,
    ) -> Result<(i32, String, String), String> {
        info!(
            job_id = %job_id,
            cmd = %cmd,
            args = ?args,
            "Executing shell command"
        );

        // Execute the command directly with arguments
        let mut command = TokioCommand::new(cmd);

        // Add arguments
        for arg in args {
            command.arg(arg);
        }

        // Set environment variables
        command
            .envs(prepared_execution.env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        // Set working directory if provided, create if not exists
        if let Some(ref dir) = working_dir {
            if !dir.trim().is_empty() {
                let path = std::path::Path::new(dir);
                // Create working directory if it doesn't exist (like Jenkins workspace)
                if !path.exists() {
                    tokio::fs::create_dir_all(path).await.map_err(|e| {
                        format!("Failed to create working directory '{}': {}", dir, e)
                    })?;
                    info!("Created working directory: {}", dir);
                }
                command.current_dir(dir);
            }
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn command '{}': {}", cmd, e))?;

        let stdin_content = prepared_execution.stdin_content;

        let stream_result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.stream_command_output(&mut child, job_id, &log_sender, stdin_content),
        )
        .await;

        let result = match stream_result {
            Ok(res) => res, // Success or error from streaming
            Err(_) => {
                // Timeout
                warn!("Job {} timed out after {} seconds", job_id, timeout_secs);
                let _ = child.kill().await;
                Err(format!(
                    "TIMEOUT: Job timed out after {} seconds",
                    timeout_secs
                ))
            }
        };

        result
    }

    async fn execute_script_with_timeout(
        &self,
        job_id: &str,
        interpreter: &str,
        content: &str,
        prepared_execution: PreparedExecution,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: u64,
    ) -> Result<(i32, String, String), String> {
        // Create temp script file
        let script_dir = std::env::temp_dir().join(format!("hodei-job-{}", job_id));
        tokio::fs::create_dir_all(&script_dir)
            .await
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;

        let script_path = script_dir.join("script_file");
        tokio::fs::write(&script_path, content)
            .await
            .map_err(|e| format!("Failed to write script file: {}", e))?;

        // Make executable (unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path)
                .await
                .map_err(|e| format!("Failed to get metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&script_path, perms)
                .await
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        let script_path_str = script_path.to_string_lossy().to_string();

        // Execute using interpreter
        let result = self
            .execute_shell_with_timeout(
                job_id,
                interpreter,
                &[script_path_str],
                prepared_execution,
                working_dir,
                log_sender,
                timeout_secs,
            )
            .await;

        // Cleanup
        let _ = tokio::fs::remove_dir_all(script_dir).await;

        result
    }

    async fn stream_command_output(
        &self,
        child: &mut tokio::process::Child,
        job_id: &str,
        log_sender: &mpsc::Sender<WorkerMessage>,
        stdin_content: Option<SecretString>,
    ) -> Result<(i32, String, String), String> {
        // Handle STDIN if provided - write synchronously before spawning
        if let Some(input) = stdin_content {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let input_str = input.expose_secret().to_string();
                // Write directly (not spawned) to avoid lifetime issues
                if let Err(e) = stdin.write_all(input_str.as_bytes()).await {
                    warn!("Failed to write to stdin: {}", e);
                }
            }
        }

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = FramedRead::new(stdout, tokio_util::codec::LinesCodec::new());
        let mut stderr_reader = FramedRead::new(stderr, tokio_util::codec::LinesCodec::new());

        let logger_instance = FileLogger::new(&self.log_dir); // Use configured log directory
        let logger = Arc::new(tokio::sync::Mutex::new(logger_instance));

        // Use updated LogBatcher::new signature
        let log_batcher = Arc::new(tokio::sync::Mutex::new(LogBatcher::new(
            log_sender.clone(),
            self.log_batch_size,
            Duration::from_millis(self.log_flush_interval_ms),
        )));

        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();
        let mut stdout_done = false;
        let mut stderr_done = false;

        // Auto-flush task
        let batcher_clone = log_batcher.clone();
        let flush_interval = self.log_flush_interval_ms;
        let flush_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(flush_interval));
            loop {
                interval.tick().await;
                let mut batcher = batcher_clone.lock().await;
                // Updated flush method returns bool, correct waiting logic
                if !batcher.flush().await {
                    warn!("Auto-flush returned false (failed or empty)");
                }
            }
        });

        loop {
            if stdout_done && stderr_done {
                break;
            }

            tokio::select! {
                line = stdout_reader.next(), if !stdout_done => {
                    match line {
                        Some(Ok(text)) => {
                            // Append to full buffer
                            stdout_buffer.push_str(&text);
                            stdout_buffer.push('\n');

                            // LogEntry construction
                            let entry = LogEntry {
                                job_id: job_id.to_string(),
                                timestamp: Some(current_timestamp()),
                                line: text.clone(),
                                is_stderr: false,
                            };

                            debug!(job_id = %job_id, line = %text, "Received stdout line");

                            // Log to file (may fail on read-only filesystems like K8s)
                            // Logs are still sent to server via batcher below
                            {
                                let file_logger = logger.lock().await;
                                if let Err(e) = file_logger.log(&entry).await {
                                    // Only log as debug for read-only fs (expected in K8s)
                                    let is_readonly = e.to_string().contains("read-only") || e.to_string().contains("Read-only");
                                    if is_readonly {
                                        debug!("Local log file unavailable (read-only filesystem - expected in K8s)");
                                    } else {
                                        warn!("Failed to write stdout to local file: {}", e);
                                    }
                                }
                            }

                            // Log to server batcher
                            {
                                let mut batcher = log_batcher.lock().await;
                                batcher.push(entry).await;
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error reading stdout: {}", e);
                            stdout_done = true;
                        }
                        None => {
                            stdout_done = true;
                        }
                    }
                }
                line = stderr_reader.next(), if !stderr_done => {
                    match line {
                        Some(Ok(text)) => {
                            // Append to full buffer
                            stderr_buffer.push_str(&text);
                            stderr_buffer.push('\n');

                            // LogEntry construction
                            let entry = LogEntry {
                                job_id: job_id.to_string(),
                                timestamp: Some(current_timestamp()),
                                line: text,
                                is_stderr: true,
                            };

                            // Log to file (may fail on read-only filesystems like K8s)
                            // Logs are still sent to server via batcher below
                            {
                                let file_logger = logger.lock().await;
                                if let Err(e) = file_logger.log(&entry).await {
                                    // Only log as debug for read-only fs (expected in K8s)
                                    let is_readonly = e.to_string().contains("read-only") || e.to_string().contains("Read-only");
                                    if is_readonly {
                                        debug!("Local log file unavailable (read-only filesystem - expected in K8s)");
                                    } else {
                                        warn!("Failed to write stderr to local file: {}", e);
                                    }
                                }
                            }

                            // Log to server batcher
                            {
                                let mut batcher = log_batcher.lock().await;
                                batcher.push(entry).await;
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error reading stderr: {}", e);
                            stderr_done = true;
                        }
                        None => {
                            stderr_done = true;
                        }
                    }
                }
            }
        }

        flush_handle.abort();

        // Flush remaining logs
        let mut batcher = log_batcher.lock().await;
        if !batcher.flush().await {
            warn!("Failed to flush remaining logs for job {}", job_id);
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for process: {}", e))?;

        let exit_code = status.code().unwrap_or(-1);

        info!(
            job_id = %job_id,
            exit_code = exit_code,
            stdout_bytes = stdout_buffer.len(),
            stderr_bytes = stderr_buffer.len(),
            "Command completed"
        );

        Ok((exit_code, stdout_buffer, stderr_buffer))
    }
}

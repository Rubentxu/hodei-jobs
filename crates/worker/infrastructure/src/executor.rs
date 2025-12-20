use hodei_jobs::{
    CommandSpec, LogEntry, WorkerMessage, command_spec::CommandType as ProtoCommandType,
};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::{error, info, warn};

use crate::logging::{FileLogger, LogBatcher};
use crate::metrics::WorkerMetrics;
use crate::secret_injector::{
    InjectionConfig, InjectionStrategy, PreparedExecution, SecretInjector,
};

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
    metrics: Arc<WorkerMetrics>,
}

impl JobExecutor {
    pub fn new(
        log_batch_size: usize,
        log_flush_interval_ms: u64,
        metrics: Arc<WorkerMetrics>,
    ) -> Self {
        Self {
            log_batch_size,
            log_flush_interval_ms,
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
                        "Parsed {} secrets for injection strategy: {:?}",
                        secrets.len(),
                        injection_strategy
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
        info!("Executing shell command: {} {:?}", cmd, args);

        let mut command = TokioCommand::new(cmd);
        command
            .args(args)
            .envs(prepared_execution.env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }

        let child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn command: {}", e))?;

        // 4. Stream output with timeout and stdin
        let stdin_content = prepared_execution.stdin_content;
        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.stream_command_output(child, job_id, &log_sender, stdin_content),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(format!("Command timed out after {} seconds", timeout_secs)),
        }
    }

    async fn execute_script_with_timeout(
        &self,
        job_id: &str,
        interpreter: &str,
        content: &str,
        mut prepared_execution: PreparedExecution,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: u64,
    ) -> Result<(i32, String, String), String> {
        info!("Executing script with interpreter: {}", interpreter);

        // Jenkins-compatible approach: Write script to file and execute directly
        // This respects the shebang and is 100% compatible with Jenkins-style execution
        let full_content = content.to_string();

        // 2. Create unique temporary directory for the job
        let job_tmp_dir = std::env::temp_dir().join(format!("hodei-job-{}", job_id));
        tokio::fs::create_dir_all(&job_tmp_dir)
            .await
            .map_err(|e| format!("Failed to create job temp dir: {}", e))?;

        let script_path = job_tmp_dir.join("script_file");
        if let Err(e) = tokio::fs::write(&script_path, full_content).await {
            let _ = tokio::fs::remove_dir_all(&job_tmp_dir).await;
            return Err(format!("Failed to write script file: {}", e));
        }

        // 3. Ensure executable permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = tokio::fs::metadata(&script_path).await {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = tokio::fs::set_permissions(&script_path, perms).await;
            }
        }

        // 4. Prepare environment with HODEI_JOB_ID
        prepared_execution
            .env_vars
            .insert("HODEI_JOB_ID".to_string(), job_id.to_string());

        // 5. Execute with explicit interpreter (generic Linux compatibility)
        // Works on any Linux image without relying on shebang resolution
        // This is the universal approach that works everywhere
        let result = self
            .execute_shell_with_timeout(
                job_id,
                interpreter,
                &[script_path.to_string_lossy().to_string()],
                prepared_execution,
                working_dir,
                log_sender,
                timeout_secs,
            )
            .await;

        // 6. Cleanup
        let _ = tokio::fs::remove_dir_all(&job_tmp_dir).await;

        result
    }

    pub async fn stream_command_output(
        &self,
        mut child: tokio::process::Child,
        job_id: &str,
        log_sender: &mpsc::Sender<WorkerMessage>,
        stdin_content: Option<String>,
    ) -> Result<(i32, String, String), String> {
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // Handle stdin
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(content) = stdin_content {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(content.as_bytes()).await;
                let _ = stdin.shutdown().await;
            }
        }

        let stdout_stream = FramedRead::new(stdout, BytesCodec::new());
        let stderr_stream = FramedRead::new(stderr, BytesCodec::new());

        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();

        let log_batcher = Arc::new(tokio::sync::Mutex::new(LogBatcher::new(
            log_sender.clone(),
            self.log_batch_size,
            Duration::from_millis(self.log_flush_interval_ms),
            self.metrics.clone(),
        )));

        let file_logger = FileLogger::new("/tmp/hodei-logs");

        let log_batcher_for_flush = log_batcher.clone();
        let flush_interval = Duration::from_millis(self.log_flush_interval_ms);
        let flush_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(flush_interval);
            loop {
                interval.tick().await;
                let mut batcher = log_batcher_for_flush.lock().await;
                if !batcher.flush_if_needed().await {
                    warn!("Periodic log flush failed");
                }
            }
        });

        let mut stdout_stream = Box::pin(stdout_stream);
        let mut stderr_stream = Box::pin(stderr_stream);

        loop {
            tokio::select! {
                biased;
                chunk = stdout_stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            let line = String::from_utf8_lossy(&bytes);
                            stdout_buffer.push_str(&line);
                            stdout_buffer.push('\n');

                            let log_entry = LogEntry {
                                job_id: job_id.to_string(),
                                line: line.to_string(),
                                is_stderr: false,
                                timestamp: Some(current_timestamp()),
                            };

                            // Persist locally
                            let _ = file_logger.log(&log_entry).await;

                            let mut batcher = log_batcher.lock().await;
                            batcher.push(log_entry).await;
                        }
                        Some(Err(e)) => warn!("Stdout read error: {}", e),
                        None => break,
                    }
                }
                chunk = stderr_stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            let line = String::from_utf8_lossy(&bytes);
                            stderr_buffer.push_str(&line);
                            stderr_buffer.push('\n');

                            let log_entry = LogEntry {
                                job_id: job_id.to_string(),
                                line: line.to_string(),
                                is_stderr: true,
                                timestamp: Some(current_timestamp()),
                            };

                            // Persist locally
                            let _ = file_logger.log(&log_entry).await;

                            let mut batcher = log_batcher.lock().await;
                            batcher.push(log_entry).await;
                        }
                        Some(Err(e)) => warn!("Stderr read error: {}", e),
                        None => break,
                    }
                }
            }
        }

        flush_handle.abort();

        let mut batcher = log_batcher.lock().await;
        let _ = batcher.flush().await;

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for process: {}", e))?;

        let exit_code = status.code().unwrap_or(-1);

        info!(
            "Command completed - stdout: {} bytes, stderr: {} bytes, exit_code: {}",
            stdout_buffer.len(),
            stderr_buffer.len(),
            exit_code
        );

        Ok((exit_code, stdout_buffer, stderr_buffer))
    }
}

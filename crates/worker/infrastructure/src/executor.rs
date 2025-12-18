use hodei_jobs::{
    LogEntry, WorkerMessage,
    command_spec::CommandType as ProtoCommandType,
};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::{info, warn};

use crate::logging::{LogBatcher, LOG_BATCHER_CAPACITY, LOG_FLUSH_INTERVAL_MS};

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
pub struct JobExecutor;

impl JobExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Execute a job from RunJobCommand with timeout and cancellation support
    pub async fn execute_from_command(
        &self,
        job_id: &str,
        command_spec: Option<hodei_jobs::CommandSpec>,
        env_vars: HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: Option<u64>,
        _abort_recv: Option<mpsc::Receiver<()>>,
    ) -> Result<(i32, String, String), String> {
        let spec = command_spec.ok_or("No command spec provided")?;
        let timeout_secs = timeout_secs.unwrap_or(3600);

        match spec.command_type {
            Some(ProtoCommandType::Shell(shell)) => {
                self.execute_shell_with_timeout(
                    job_id,
                    &shell.cmd,
                    &shell.args,
                    &env_vars,
                    working_dir,
                    log_sender,
                    timeout_secs,
                )
                .await
            }
            Some(ProtoCommandType::Script(script)) => {
                self.execute_script_with_timeout(
                    job_id,
                    &script.interpreter,
                    &script.content,
                    &env_vars,
                    working_dir,
                    log_sender,
                    timeout_secs,
                )
                .await
            }
            None => Err("Empty command spec".to_string()),
        }
    }

    async fn execute_shell_with_timeout(
        &self,
        job_id: &str,
        cmd: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: u64,
    ) -> Result<(i32, String, String), String> {
        info!("Executing shell command: {} {:?}", cmd, args);

        let mut command = TokioCommand::new(cmd);
        command
            .args(args)
            .envs(env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }

        let child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn command: {}", e))?;

        let execute_future = self.stream_command_output(child, job_id, &log_sender);

        match tokio::time::timeout(Duration::from_secs(timeout_secs), execute_future).await {
            Ok(result) => result,
            Err(_) => Err(format!("Execution timed out after {} seconds", timeout_secs)),
        }
    }

    async fn execute_script_with_timeout(
        &self,
        job_id: &str,
        interpreter: &str,
        content: &str,
        env_vars: &HashMap<String, String>,
        working_dir: Option<String>,
        log_sender: mpsc::Sender<WorkerMessage>,
        timeout_secs: u64,
    ) -> Result<(i32, String, String), String> {
        info!("Executing script with interpreter: {}", interpreter);

        // TODO: Create temporary file better (infra concern)
        let script_path = format!("/tmp/job_{}.script", job_id);
        if let Err(e) = tokio::fs::write(&script_path, content).await {
            return Err(format!("Failed to write script file: {}", e));
        }

        // Ensure executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path).await.map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            let _ = tokio::fs::set_permissions(&script_path, perms).await;
        }

        let result = self.execute_shell_with_timeout(
            job_id,
            interpreter,
            &[script_path.clone()],
            env_vars,
            working_dir,
            log_sender,
            timeout_secs,
        ).await;

        let _ = tokio::fs::remove_file(script_path).await;
        result
    }

    pub async fn stream_command_output(
        &self,
        mut child: tokio::process::Child,
        job_id: &str,
        log_sender: &mpsc::Sender<WorkerMessage>,
    ) -> Result<(i32, String, String), String> {
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let stdout_stream = FramedRead::new(stdout, BytesCodec::new());
        let stderr_stream = FramedRead::new(stderr, BytesCodec::new());

        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();

        let log_batcher = Arc::new(tokio::sync::Mutex::new(LogBatcher::new(
            log_sender.clone(),
            LOG_BATCHER_CAPACITY,
            Duration::from_millis(LOG_FLUSH_INTERVAL_MS),
        )));

        let log_batcher_for_flush = log_batcher.clone();
        let flush_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(LOG_FLUSH_INTERVAL_MS));
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

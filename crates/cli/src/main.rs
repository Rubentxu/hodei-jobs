//! Hodei Jobs CLI - Complete CLI for Hodei Job Platform
//!
//! Covers client-facing gRPC services:
//! - JobExecutionService: Queue, Get, List, Cancel jobs
//! - LogStreamService: Subscribe to logs, get historical logs
//! - SchedulerService: Schedule jobs, get queue status

use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::io::Write;
use tonic::transport::Channel;
use tracing::info;

use hodei_jobs::{
    ExecutionId, GetJobRequest, GetLogsRequest, GetQueueStatusRequest, JobDefinition, JobId,
    ListJobsRequest, QueueJobRequest, ScheduleJobRequest, SchedulingInfo, SubscribeLogsRequest,
    job_execution_service_client::JobExecutionServiceClient,
    log_stream_service_client::LogStreamServiceClient,
    scheduler_service_client::SchedulerServiceClient,
};

#[derive(Parser)]
#[command(name = "hodei-jobs-cli")]
#[command(about = "CLI for Hodei Job Platform", long_about = None)]
#[command(version)]
struct Cli {
    /// gRPC server address
    #[arg(short, long, default_value = "http://localhost:50051", global = true)]
    server: String,

    /// Output format (text, json)
    #[arg(short, long, default_value = "text", global = true)]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Job management commands
    Job {
        #[command(subcommand)]
        action: JobAction,
    },
    /// Scheduler commands
    Scheduler {
        #[command(subcommand)]
        action: SchedulerAction,
    },
    /// Log streaming commands
    Logs {
        #[command(subcommand)]
        action: LogsAction,
    },
}

#[derive(Subcommand, Clone)]
enum JobAction {
    /// Queue a new job (without log streaming)
    Queue {
        #[arg(short, long)]
        name: String,
        /// Command to execute (mutually exclusive with --script)
        #[arg(short, long, conflicts_with = "script")]
        command: Option<String>,
        /// Path to script file to execute (mutually exclusive with --command)
        #[arg(long, conflicts_with = "command")]
        script: Option<String>,
        /// Preferred provider (docker, kubernetes, k8s, kube)
        #[arg(short, long)]
        provider: Option<String>,
        #[arg(long, default_value = "1.0")]
        cpu: f64,
        #[arg(long, default_value = "1073741824")]
        memory: i64,
        #[arg(short, long, default_value = "600")]
        timeout: i64,
        /// Required labels for worker (key=value pairs, repeatable)
        #[arg(short = 'l', long = "label", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        required_labels: Vec<String>,
        /// Required annotations for worker (key=value pairs, repeatable)
        #[arg(short = 'a', long = "annotation", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        required_annotations: Vec<String>,
        /// Preferred region for worker
        #[arg(long = "region")]
        preferred_region: Option<String>,
    },
    /// Run a job and stream logs in real-time (queue + subscribe)
    Run {
        #[arg(short, long)]
        name: String,
        /// Command to execute (mutually exclusive with --script)
        #[arg(short, long, conflicts_with = "script")]
        command: Option<String>,
        /// Path to script file to execute (mutually exclusive with --command)
        #[arg(long, conflicts_with = "command")]
        script: Option<String>,
        /// Preferred provider (docker, kubernetes, k8s, kube)
        #[arg(short, long)]
        provider: Option<String>,
        #[arg(long, default_value = "1.0")]
        cpu: f64,
        #[arg(long, default_value = "1073741824")]
        memory: i64,
        #[arg(short, long, default_value = "600")]
        timeout: i64,
        /// Required labels for worker (key=value pairs, repeatable)
        #[arg(short = 'l', long = "label", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        required_labels: Vec<String>,
        /// Required annotations for worker (key=value pairs, repeatable)
        #[arg(short = 'a', long = "annotation", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        required_annotations: Vec<String>,
        /// Preferred region for worker
        #[arg(long = "region")]
        preferred_region: Option<String>,
    },
    /// Get job details
    Get {
        #[arg(short, long)]
        id: String,
    },
    /// List jobs
    List {
        #[arg(short, long)]
        limit: Option<i32>,
    },
    /// Cancel a job
    Cancel {
        #[arg(short, long)]
        job_id: String,
        #[arg(short, long)]
        execution_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum SchedulerAction {
    /// Get queue status
    QueueStatus,
    /// Schedule a job
    Schedule {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        command: String,
    },
    /// Get available workers
    Workers,
}

#[derive(Subcommand)]
enum LogsAction {
    /// Subscribe to real-time logs for a job
    Follow {
        #[arg(short, long)]
        job_id: String,
        /// Include historical logs
        #[arg(short = 'H', long, default_value = "true")]
        history: bool,
        /// Only get last N lines of history
        #[arg(short, long, default_value = "0")]
        tail: i32,
    },
    /// Get historical logs (non-streaming)
    Get {
        #[arg(short, long)]
        job_id: String,
        #[arg(short, long, default_value = "100")]
        limit: i32,
    },
}

/// Parse a key=value string into a tuple
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.split('=').collect();
    if parts.len() == 2 {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        Err("Formato esperado: key=value".to_string())
    }
}

/// Convert vector of key=value strings to HashMap
fn parse_key_value_map(pairs: &[String]) -> HashMap<String, String> {
    pairs
        .iter()
        .filter_map(|kv| parse_key_value(kv).ok())
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only init tracing if RUST_LOG is set
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt::init();
    }

    let cli = Cli::parse();
    info!("Connecting to: {}", cli.server);

    let channel = Channel::from_shared(cli.server.clone())?.connect().await?;

    match cli.command {
        Commands::Job { action } => handle_job(channel, action).await?,
        Commands::Scheduler { action } => handle_scheduler(channel, action).await?,
        Commands::Logs { action } => handle_logs(channel, action).await?,
    }

    Ok(())
}

async fn handle_job(channel: Channel, action: JobAction) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = JobExecutionServiceClient::new(channel.clone());

    match action {
        JobAction::Queue {
            name,
            command,
            script,
            provider,
            cpu,
            memory,
            timeout,
            required_labels,
            required_annotations,
            preferred_region,
        } => {
            let cmd = resolve_command(command, script)?;
            let job_definition = create_job_definition(
                name,
                cmd,
                cpu,
                memory,
                timeout,
                provider,
                &required_labels,
                &required_annotations,
                preferred_region.as_deref(),
            );
            let request = QueueJobRequest {
                job_definition: Some(job_definition),
                queued_by: "cli".to_string(),
            };
            let response = client.queue_job(request).await?;
            let result = response.into_inner();

            if result.success {
                if let Some(job_id) = &result.job_id {
                    println!("✅ Job queued successfully!");
                    println!("   Job ID: {}", job_id.value);
                    println!("   Message: {}", result.message);
                } else {
                    println!("✅ Job queued: {}", result.message);
                }
            } else {
                eprintln!("❌ Failed to queue job: {}", result.message);
                std::process::exit(1);
            }
        }
        JobAction::Run {
            name,
            command,
            script,
            provider,
            cpu,
            memory,
            timeout,
            required_labels,
            required_annotations,
            preferred_region,
        } => {
            let cmd = resolve_command(command, script.clone())?;
            let job_definition = create_job_definition(
                name.clone(),
                cmd.clone(),
                cpu,
                memory,
                timeout,
                provider,
                &required_labels,
                &required_annotations,
                preferred_region.as_deref(),
            );
            let request = QueueJobRequest {
                job_definition: Some(job_definition),
                queued_by: "cli".to_string(),
            };

            println!("╔════════════════════════════════════════════════════════════════╗");
            println!("║              HODEI JOB RUNNER WITH LIVE LOGS                   ║");
            println!("╚════════════════════════════════════════════════════════════════╝");
            println!();
            println!("📤 Submitting job: {}", name);
            if let Some(script_path) = &script {
                println!("   Script: {}", script_path);
            } else {
                println!("   Command: {}", cmd);
            }
            println!("   CPU: {} cores, Memory: {} bytes", cpu, memory);

            // Print scheduling info if provided
            if !required_labels.is_empty() {
                println!("   Required labels: {:?}", required_labels);
            }
            if !required_annotations.is_empty() {
                println!("   Required annotations: {:?}", required_annotations);
            }
            if let Some(ref region) = preferred_region {
                println!("   Preferred region: {}", region);
            }
            println!();

            let response = client.queue_job(request).await?;
            let result = response.into_inner();

            if !result.success {
                eprintln!("❌ Failed to queue job: {}", result.message);
                std::process::exit(1);
            }

            let job_id = result
                .job_id
                .map(|id| id.value)
                .ok_or("Server did not return job_id")?;

            println!("✅ Job queued successfully!");
            println!("   Job ID: {}", job_id);
            println!();

            // Now subscribe to logs
            println!("📡 Subscribing to log stream...");
            println!("{:-<60}", "");
            println!();

            let mut log_client = LogStreamServiceClient::new(channel);
            let subscribe_request = SubscribeLogsRequest {
                job_id: job_id.clone(),
                include_history: true,
                tail_lines: 0,
            };

            let mut stream = log_client
                .subscribe_logs(subscribe_request)
                .await?
                .into_inner();

            println!("🚀 Streaming logs in real-time...");
            println!("   Press Ctrl+C to stop watching");
            println!();

            let mut log_count = 0;
            let start_time = std::time::Instant::now();

            loop {
                match stream.message().await {
                    Ok(Some(log_entry)) => {
                        log_count += 1;

                        let timestamp = log_entry
                            .timestamp
                            .as_ref()
                            .and_then(|ts| {
                                chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                            })
                            .unwrap_or_else(chrono::Utc::now);

                        let ts_str = timestamp.format("%H:%M:%S.%3f").to_string();
                        let prefix = if log_entry.is_stderr {
                            "[ERR]"
                        } else {
                            "[OUT]"
                        };

                        println!("{} {} {}", ts_str, prefix, log_entry.line);
                        std::io::stdout().flush()?;
                    }
                    Ok(None) => {
                        // Stream ended
                        break;
                    }
                    Err(e) => {
                        eprintln!("\n❌ Stream error: {}", e);
                        break;
                    }
                }
            }

            let duration = start_time.elapsed();

            println!();
            println!("{:-<60}", "");
            println!();
            println!("╔════════════════════════════════════════════════════════════════╗");
            println!("║                    STREAM FINISHED                             ║");
            println!("╚════════════════════════════════════════════════════════════════╝");
            println!();
            println!("📊 Summary:");
            println!("   Job ID: {}", job_id);
            println!("   Logs Received: {}", log_count);
            println!("   Duration: {:?}", duration);
        }
        JobAction::Get { id } => {
            let request = GetJobRequest {
                job_id: Some(JobId { value: id.clone() }),
            };
            let response = client.get_job(request).await?;
            let result = response.into_inner();
            println!("Job {}:", id);
            if let Some(job) = result.job {
                println!("  Name: {}", job.name);
                println!("  Command: {}", job.command);
                println!("  Status: {:?}", result.status);
            } else {
                println!("  Not found");
            }
        }
        JobAction::List { limit } => {
            let request = ListJobsRequest {
                limit: limit.unwrap_or(20),
                offset: 0,
                status: None,
                search_term: None,
            };
            let response = client.list_jobs(request).await?;
            let result = response.into_inner();
            println!("Jobs ({} total):", result.total_count);
            for job in result.jobs {
                let job_id = job
                    .job_id
                    .as_ref()
                    .map(|id| id.value.as_str())
                    .unwrap_or_default();
                println!("  - {} ({}) - {:?}", job.name, job_id, job.status());
            }
        }
        JobAction::Cancel {
            job_id,
            execution_id,
        } => {
            let request = hodei_jobs::CancelJobRequest {
                execution_id: execution_id.map(|id| ExecutionId { value: id }),
                job_id: Some(JobId {
                    value: job_id.clone(),
                }),
                reason: "Cancelled by CLI".to_string(),
            };
            let response = client.cancel_job(request).await?;
            let result = response.into_inner();
            if result.success {
                println!("✅ Job {} cancelled", job_id);
            } else {
                eprintln!("❌ Failed to cancel job: {}", result.message);
            }
        }
    }
    Ok(())
}

async fn handle_scheduler(
    channel: Channel,
    action: SchedulerAction,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = SchedulerServiceClient::new(channel);

    match action {
        SchedulerAction::QueueStatus => {
            let request = GetQueueStatusRequest {
                scheduler_name: "default".to_string(),
            };
            let response = client.get_queue_status(request).await?;
            let result = response.into_inner();
            println!("Queue Status:");
            if let Some(status) = result.status {
                println!("  Pending: {}", status.pending_jobs);
                println!("  Running: {}", status.running_jobs);
                println!("  Completed: {}", status.completed_jobs);
                println!("  Failed: {}", status.failed_jobs);
            }
        }
        SchedulerAction::Schedule { name, command } => {
            let job_definition =
                create_job_definition(name, command, 1.0, 1073741824, 600, None, &[], &[], None);
            let request = ScheduleJobRequest {
                job_definition: Some(job_definition),
                requested_by: "cli".to_string(),
            };
            let response = client.schedule_job(request).await?;
            println!("Scheduled: {:?}", response.into_inner());
        }
        SchedulerAction::Workers => {
            let request = hodei_jobs::GetAvailableWorkersRequest {
                filter: None,
                scheduler_name: "default".to_string(),
            };
            let response = client.get_available_workers(request).await?;
            let result = response.into_inner();
            println!("Available Workers ({}):", result.workers.len());
            for worker in result.workers {
                let worker_id = worker
                    .worker_id
                    .as_ref()
                    .map(|id| id.value.as_str())
                    .unwrap_or_default();
                println!("  - {} (status: {:?})", worker_id, worker.status());
            }
        }
    }
    Ok(())
}

async fn handle_logs(
    channel: Channel,
    action: LogsAction,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = LogStreamServiceClient::new(channel);

    match action {
        LogsAction::Follow {
            job_id,
            history,
            tail,
        } => {
            let request = SubscribeLogsRequest {
                job_id: job_id.clone(),
                include_history: history,
                tail_lines: tail,
            };

            println!("📡 Subscribing to logs for job: {}", job_id);
            println!("   Press Ctrl+C to stop");
            println!();

            let mut stream = client.subscribe_logs(request).await?.into_inner();

            loop {
                match stream.message().await {
                    Ok(Some(log_entry)) => {
                        let timestamp = log_entry
                            .timestamp
                            .as_ref()
                            .and_then(|ts| {
                                chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                            })
                            .unwrap_or_else(chrono::Utc::now);

                        let ts_str = timestamp.format("%H:%M:%S.%3f").to_string();
                        let prefix = if log_entry.is_stderr {
                            "[ERR]"
                        } else {
                            "[OUT]"
                        };

                        println!("{} {} {}", ts_str, prefix, log_entry.line);
                        std::io::stdout().flush()?;
                    }
                    Ok(None) => {
                        println!("\n📋 Log stream ended");
                        break;
                    }
                    Err(e) => {
                        eprintln!("\n❌ Stream error: {}", e);
                        break;
                    }
                }
            }
        }
        LogsAction::Get { job_id, limit } => {
            let request = GetLogsRequest {
                job_id: job_id.clone(),
                limit,
                since_sequence: 0,
            };
            let response = client.get_logs(request).await?;
            let result = response.into_inner();

            println!(
                "Logs for job {} ({} entries):",
                job_id,
                result.entries.len()
            );
            for entry in result.entries {
                let prefix = if entry.is_stderr { "[ERR]" } else { "[OUT]" };
                println!("{} {}", prefix, entry.line);
            }
            if result.has_more {
                println!("... (more logs available)");
            }
        }
    }
    Ok(())
}

/// Resolve command from either --command or --script option
fn resolve_command(
    command: Option<String>,
    script: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    match (command, script) {
        (Some(cmd), None) => Ok(cmd),
        (None, Some(script_path)) => {
            // Read script file and wrap it for execution
            let script_content = std::fs::read_to_string(&script_path)
                .map_err(|e| format!("Failed to read script '{}': {}", script_path, e))?;
            // Execute script content via bash
            Ok(script_content)
        }
        (None, None) => Err("Either --command or --script must be provided".into()),
        (Some(_), Some(_)) => Err("Cannot specify both --command and --script".into()),
    }
}

/// Create a JobDefinition with scheduling preferences
///
/// ## US-27.2: CLI with --required-labels
/// ## US-27.3: CLI with --required-annotations
/// ## US-27.6: Region Affinity (MVP)
fn create_job_definition(
    name: String,
    command: String,
    cpu: f64,
    memory: i64,
    timeout_secs: i64,
    provider: Option<String>,
    required_labels: &[String],
    required_annotations: &[String],
    preferred_region: Option<&str>,
) -> JobDefinition {
    // Parse key=value pairs into HashMap
    let labels_map: HashMap<String, String> = parse_key_value_map(required_labels);
    let annotations_map: HashMap<String, String> = parse_key_value_map(required_annotations);

    let scheduling = Some(SchedulingInfo {
        priority: 0, // Default priority
        scheduler_name: String::new(),
        deadline: None,
        preemption_allowed: false,
        preferred_provider: provider.unwrap_or_default(),
        required_labels: labels_map,
        required_annotations: annotations_map,
        preferred_region: preferred_region.unwrap_or_default().to_string(),
    });

    JobDefinition {
        job_id: None,
        name,
        description: String::new(),
        command,
        arguments: vec![],
        environment: std::collections::HashMap::new(),
        requirements: Some(hodei_jobs::ResourceRequirements {
            cpu_cores: cpu,
            memory_bytes: memory,
            disk_bytes: 0,
            gpu_count: 0,
            gpu_types: vec![],
            custom_required: std::collections::HashMap::new(),
        }),
        scheduling,
        selector: None,
        tolerations: vec![],
        tags: vec![],
        timeout: Some(hodei_jobs::TimeoutConfig {
            execution_timeout: Some(prost_types::Duration {
                seconds: timeout_secs,
                nanos: 0,
            }),
            heartbeat_timeout: Some(prost_types::Duration {
                seconds: 30,
                nanos: 0,
            }),
            cleanup_timeout: Some(prost_types::Duration {
                seconds: 30,
                nanos: 0,
            }),
        }),
    }
}

/// Parse key=value string to tuple
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_value_valid() {
        let result = parse_key_value("key=value");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ("key".to_string(), "value".to_string()));
    }

    #[test]
    fn test_parse_key_value_invalid() {
        let result = parse_key_value("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_key_value_multiple_equals() {
        let result = parse_key_value("key=value=extra");
        // This should work, taking first = as delimiter
        assert!(result.is_ok());
        let (key, value) = result.unwrap();
        assert_eq!(key, "key");
        assert_eq!(value, "value=extra");
    }

    #[test]
    fn test_parse_key_value_map() {
        let pairs = vec!["env=production".to_string(), "team=platform".to_string()];
        let map = parse_key_value_map(&pairs);

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("env"), Some(&"production".to_string()));
        assert_eq!(map.get("team"), Some(&"platform".to_string()));
    }

    #[test]
    fn test_parse_key_value_map_ignores_invalid() {
        let pairs = vec![
            "valid=key".to_string(),
            "invalid".to_string(),
            "another=valid".to_string(),
        ];
        let map = parse_key_value_map(&pairs);

        assert_eq!(map.len(), 2);
        assert!(map.contains_key("valid"));
        assert!(map.contains_key("another"));
    }

    #[test]
    fn test_create_job_definition_with_labels_and_annotations() {
        let job_def = create_job_definition(
            "test-job".to_string(),
            "echo hello".to_string(),
            1.0,
            1024,
            600,
            Some("kubernetes".to_string()),
            &["environment=production".to_string()],
            &["team=ml".to_string()],
            Some("us-east-1"),
        );

        let scheduling = job_def.scheduling.unwrap();

        assert_eq!(scheduling.preferred_provider, "kubernetes");
        assert_eq!(
            scheduling.required_labels.get("environment"),
            Some(&"production".to_string())
        );
        assert_eq!(
            scheduling.required_annotations.get("team"),
            Some(&"ml".to_string())
        );
        assert_eq!(scheduling.preferred_region, "us-east-1");
    }

    #[test]
    fn test_create_job_definition_empty_scheduling() {
        let job_def = create_job_definition(
            "test-job".to_string(),
            "echo hello".to_string(),
            1.0,
            1024,
            600,
            None,
            &[],
            &[],
            None,
        );

        let scheduling = job_def.scheduling.unwrap();

        assert!(scheduling.preferred_provider.is_empty());
        assert!(scheduling.required_labels.is_empty());
        assert!(scheduling.required_annotations.is_empty());
        assert!(scheduling.preferred_region.is_empty());
    }
}

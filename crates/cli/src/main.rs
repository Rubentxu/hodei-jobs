//! Hodei Jobs CLI

use clap::{Parser, Subcommand};
use tonic::transport::Channel;
use tracing::info;

use hodei_jobs::{
    worker_agent_service_client::WorkerAgentServiceClient,
    job_execution_service_client::JobExecutionServiceClient,
    scheduler_service_client::SchedulerServiceClient,
    RegisterWorkerRequest, WorkerInfo, WorkerId,
    QueueJobRequest, JobDefinition, JobId, ExecutionId,
    ScheduleJobRequest, GetQueueStatusRequest,
};

#[derive(Parser)]
#[command(name = "hodei-jobs-cli")]
#[command(about = "CLI for Hodei Job Platform")]
#[command(version)]
struct Cli {
    #[arg(short, long, default_value = "http://localhost:50051")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Worker {
        #[command(subcommand)]
        action: WorkerAction,
    },
    Job {
        #[command(subcommand)]
        action: JobAction,
    },
    Scheduler {
        #[command(subcommand)]
        action: SchedulerAction,
    },
}

#[derive(Subcommand)]
enum WorkerAction {
    Register {
        #[arg(short, long)]
        name: String,
        #[arg(short = 'H', long)]
        hostname: String,
    },
    Unregister {
        #[arg(short, long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum JobAction {
    Queue {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        command: String,
    },
    Cancel {
        #[arg(short, long)]
        job_id: String,
        #[arg(short, long)]
        execution_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum SchedulerAction {
    QueueStatus,
    Schedule {
        #[arg(short, long)]
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    info!("Connecting to: {}", cli.server);
    
    let channel = Channel::from_shared(cli.server)?
        .connect()
        .await?;

    match cli.command {
        Commands::Worker { action } => handle_worker(channel, action).await?,
        Commands::Job { action } => handle_job(channel, action).await?,
        Commands::Scheduler { action } => handle_scheduler(channel, action).await?,
    }

    Ok(())
}

async fn handle_worker(channel: Channel, action: WorkerAction) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = WorkerAgentServiceClient::new(channel);

    match action {
        WorkerAction::Register { name, hostname } => {
            // PRD v6.0: Register requires auth_token (for CLI, use empty for testing)
            let request = RegisterWorkerRequest {
                auth_token: std::env::var("HODEI_OTP_TOKEN").unwrap_or_default(),
                session_id: String::new(),
                worker_info: Some(WorkerInfo {
                    worker_id: None,
                    name,
                    version: "1.0.0".to_string(),
                    hostname,
                    ip_address: String::new(),
                    os_info: String::new(),
                    architecture: String::new(),
                    capacity: None,
                    capabilities: vec![],
                    taints: vec![],
                    labels: std::collections::HashMap::new(),
                    tolerations: vec![],
                    affinity: None,
                    start_time: None,
                }),
            };
            let response = client.register(request).await?;
            println!("Worker registered: {:?}", response.into_inner());
        }
        WorkerAction::Unregister { id } => {
            let request = hodei_jobs::UnregisterWorkerRequest {
                worker_id: Some(WorkerId { value: id }),
                reason: "CLI unregister".to_string(),
            };
            let response = client.unregister_worker(request).await?;
            println!("Unregistered: {:?}", response.into_inner());
        }
    }
    Ok(())
}

async fn handle_job(channel: Channel, action: JobAction) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = JobExecutionServiceClient::new(channel);

    match action {
        JobAction::Queue { name, command } => {
            let request = QueueJobRequest {
                job_definition: Some(JobDefinition {
                    job_id: None,
                    name,
                    description: String::new(),
                    command,
                    arguments: vec![],
                    environment: std::collections::HashMap::new(),
                    requirements: None,
                    scheduling: None,
                    selector: None,
                    tolerations: vec![],
                    tags: vec![],
                    timeout: None,
                }),
                queued_by: "cli".to_string(),
            };
            let response = client.queue_job(request).await?;
            println!("Job queued: {:?}", response.into_inner());
        }
        JobAction::Cancel { job_id, execution_id } => {
            let request = hodei_jobs::CancelJobRequest {
                execution_id: execution_id.map(|id| ExecutionId { value: id }),
                job_id: Some(JobId { value: job_id.clone() }),
                reason: "Cancelled by CLI".to_string(),
            };
            let response = client.cancel_job(request).await?;
            println!("Job {} cancelled: {:?}", job_id, response.into_inner());
        }
    }
    Ok(())
}

async fn handle_scheduler(channel: Channel, action: SchedulerAction) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = SchedulerServiceClient::new(channel);

    match action {
        SchedulerAction::QueueStatus => {
            let request = GetQueueStatusRequest {
                scheduler_name: "default".to_string(),
            };
            let response = client.get_queue_status(request).await?;
            println!("Queue: {:?}", response.into_inner());
        }
        SchedulerAction::Schedule { name } => {
            let request = ScheduleJobRequest {
                job_definition: Some(JobDefinition {
                    job_id: None,
                    name,
                    description: String::new(),
                    command: "echo".to_string(),
                    arguments: vec!["hello".to_string()],
                    environment: std::collections::HashMap::new(),
                    requirements: None,
                    scheduling: None,
                    selector: None,
                    tolerations: vec![],
                    tags: vec![],
                    timeout: None,
                }),
                requested_by: "cli".to_string(),
            };
            let response = client.schedule_job(request).await?;
            println!("Scheduled: {:?}", response.into_inner());
        }
    }
    Ok(())
}

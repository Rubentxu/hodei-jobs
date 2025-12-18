use async_trait::async_trait;
use hodei_jobs::{
    ArtifactInput, ArtifactOutput, CommandSpec, RunJobCommand, ScriptCommand, ServerMessage,
    ShellCommand, command_spec::CommandType as ProtoCommandType,
    server_message::Payload as ServerPayload,
};
use hodei_server_application::workers::WorkerCommandSender;
use hodei_server_domain::jobs::{CommandType, Job};
use hodei_server_domain::shared_kernel::{Result, WorkerId};
use std::collections::HashMap;
use tonic::Status;

use crate::grpc::WorkerAgentServiceImpl;

#[derive(Clone)]
pub struct GrpcWorkerCommandSender {
    worker_service: WorkerAgentServiceImpl,
}

impl GrpcWorkerCommandSender {
    pub fn new(worker_service: WorkerAgentServiceImpl) -> Self {
        Self { worker_service }
    }

    fn clamp_timeout_ms(timeout_ms: u64) -> i64 {
        if timeout_ms > i64::MAX as u64 {
            i64::MAX
        } else {
            timeout_ms as i64
        }
    }

    fn build_run_job_command(job: &Job) -> RunJobCommand {
        let cmd = match &job.spec.command {
            CommandType::Shell { cmd, args } => CommandSpec {
                command_type: Some(ProtoCommandType::Shell(ShellCommand {
                    cmd: cmd.clone(),
                    args: args.clone(),
                })),
            },
            CommandType::Script {
                interpreter,
                content,
            } => CommandSpec {
                command_type: Some(ProtoCommandType::Script(ScriptCommand {
                    interpreter: interpreter.clone(),
                    content: content.clone(),
                })),
            },
        };

        let env: HashMap<String, String> = job.spec.env.clone();

        let inputs = job
            .spec
            .inputs
            .iter()
            .map(|i| ArtifactInput {
                url: i.url.clone(),
                dest_path: i.dest_path.clone(),
            })
            .collect();

        let outputs = job
            .spec
            .outputs
            .iter()
            .map(|o| ArtifactOutput {
                src_path: o.src_path.clone(),
                url: o.url.clone(),
            })
            .collect();

        RunJobCommand {
            job_id: job.id.to_string(),
            command: Some(cmd),
            env,
            inputs,
            outputs,
            timeout_ms: Self::clamp_timeout_ms(job.spec.timeout_ms),
            working_dir: job.spec.working_dir.clone().unwrap_or_default(),
        }
    }

    async fn send_to_worker(
        &self,
        worker_id: &WorkerId,
        cmd: RunJobCommand,
    ) -> std::result::Result<(), Status> {
        let msg = ServerMessage {
            payload: Some(ServerPayload::RunJob(cmd)),
        };

        self.worker_service
            .send_to_worker(&worker_id.to_string(), msg)
            .await
    }
}

#[async_trait]
impl WorkerCommandSender for GrpcWorkerCommandSender {
    async fn send_run_job(&self, worker_id: &WorkerId, job: &Job) -> Result<()> {
        let cmd = Self::build_run_job_command(job);
        self.send_to_worker(worker_id, cmd).await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!(
                    "Failed to send RunJobCommand to worker {}: {}",
                    worker_id, e
                ),
            }
        })?;

        Ok(())
    }
}

use async_trait::async_trait;
use hodei_server_domain::jobs::Job;
use hodei_server_domain::shared_kernel::{Result, WorkerId};

#[async_trait]
pub trait WorkerCommandSender: Send + Sync {
    async fn send_run_job(&self, worker_id: &WorkerId, job: &Job) -> Result<()>;
}

/// No-op implementation for cases where worker command sending is not needed
#[derive(Debug, Clone, Default)]
pub struct NoopWorkerCommandSender;

#[async_trait]
impl WorkerCommandSender for NoopWorkerCommandSender {
    async fn send_run_job(&self, _worker_id: &WorkerId, _job: &Job) -> Result<()> {
        Ok(())
    }
}

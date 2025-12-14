use async_trait::async_trait;
use hodei_jobs_domain::job_execution::Job;
use hodei_jobs_domain::shared_kernel::{Result, WorkerId};

#[async_trait]
pub trait WorkerCommandSender: Send + Sync {
    async fn send_run_job(&self, worker_id: &WorkerId, job: &Job) -> Result<()>;
}

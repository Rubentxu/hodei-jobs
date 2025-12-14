use hodei_jobs_domain::{
    job_execution::{Job, JobQueue, JobRepository, JobSpec},
    shared_kernel::JobId,
};
use hodei_jobs_infrastructure::persistence::{DatabaseConfig, PostgresJobQueue, PostgresJobRepository};

mod common;

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_job_queue_fifo() {
    let _db = common::get_postgres_context().await;

    let config = DatabaseConfig {
        url: _db.connection_string.clone(),
        max_connections: 5,
        connection_timeout: std::time::Duration::from_secs(30),
    };

    let repo = PostgresJobRepository::connect(&config)
        .await
        .expect("Failed to connect to Postgres");
    repo.run_migrations().await.expect("Failed to run migrations");

    let queue = PostgresJobQueue::connect(&config)
        .await
        .expect("Failed to connect to Postgres");
    queue.run_migrations().await.expect("Failed to run migrations");

    assert!(queue.is_empty().await.expect("is_empty failed"));

    let job1_id = JobId::new();
    let job1 = Job::new(job1_id.clone(), JobSpec::new(vec!["echo".to_string(), "one".to_string()]));
    repo.save(&job1).await.expect("save job1 failed");

    let job2_id = JobId::new();
    let job2 = Job::new(job2_id.clone(), JobSpec::new(vec!["echo".to_string(), "two".to_string()]));
    repo.save(&job2).await.expect("save job2 failed");

    queue.enqueue(job2.clone()).await.expect("enqueue job2 failed");
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    queue.enqueue(job1.clone()).await.expect("enqueue job1 failed");

    let peeked = queue.peek().await.expect("peek failed").expect("expected peeked job");
    assert_eq!(peeked.id, job2_id);

    let first = queue.dequeue().await.expect("dequeue failed").expect("expected job");
    let second = queue.dequeue().await.expect("dequeue failed").expect("expected job");

    assert_eq!(first.id, job2_id);
    assert_eq!(second.id, job1_id);

    assert!(queue.dequeue().await.expect("dequeue failed").is_none());
    assert!(queue.is_empty().await.expect("is_empty failed"));

    queue.clear().await.expect("clear failed");
    assert!(queue.is_empty().await.expect("is_empty failed"));
}

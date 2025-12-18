use hodei_jobs_domain::{
    jobs::{Job, JobRepository, JobSpec},
    shared_kernel::{JobId, JobState},
};
use hodei_jobs_infrastructure::persistence::{DatabaseConfig, PostgresJobRepository};

mod common;

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_job_lifecycle() {
    let _db = common::get_postgres_context().await;

    let config = DatabaseConfig {
        url: _db.connection_string.clone(),
        max_connections: 5,
        connection_timeout: std::time::Duration::from_secs(30),
    };

    let repo = PostgresJobRepository::connect(&config)
        .await
        .expect("Failed to connect to Postgres");

    repo.run_migrations()
        .await
        .expect("Failed to run migrations");

    // 1. Create and Save Job
    let job_id = JobId::new();
    let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);
    let job = Job::new(job_id.clone(), spec);

    repo.save(&job).await.expect("Failed to save job");

    // 2. Find by ID
    let found = repo.find_by_id(&job_id).await.expect("Failed to find job");
    assert!(found.is_some());
    let found_job = found.unwrap();
    assert_eq!(found_job.id, job_id);
    assert_eq!(found_job.state, JobState::Pending);

    // 3. Find pending
    let pending = repo.find_pending().await.expect("Failed to find pending");
    assert!(pending.iter().any(|j| j.id == job_id));

    // 4. Update
    let mut updated_job = found_job.clone();
    updated_job.state = JobState::Running;
    repo.update(&updated_job)
        .await
        .expect("Failed to update job");

    let found_updated = repo
        .find_by_id(&job_id)
        .await
        .expect("Failed to find updated job")
        .unwrap();
    assert_eq!(found_updated.state, JobState::Running);

    // 5. Delete
    repo.delete(&job_id).await.expect("Failed to delete job");
    let found_deleted = repo
        .find_by_id(&job_id)
        .await
        .expect("Failed to find deleted job");
    assert!(found_deleted.is_none());
}

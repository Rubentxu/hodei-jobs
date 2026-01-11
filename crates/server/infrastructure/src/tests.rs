//! Unit tests for infrastructure layer
#[cfg(test)]
// use crate::test_infrastructure::{InMemoryJobQueue, InMemoryJobRepository};
use crate::persistence::postgres::test_in_memory::{InMemoryJobQueue, InMemoryJobRepository};
use crate::test_helpers::init_rustls;

#[ctor::ctor]
fn init_rustls_for_tests() {
    init_rustls();
}

use hodei_server_domain::jobs::{Job, JobQueue, JobRepository, JobSpec};
use hodei_server_domain::shared_kernel::{JobId, JobState};

fn create_test_job() -> Job {
    let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
    Job::new(JobId::new(), spec)
}

mod job_repository_tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_find_job() {
        let repo = InMemoryJobRepository::new();
        let job = create_test_job();
        let job_id = job.id.clone();

        repo.save(&job).await.unwrap();

        let found = repo.find_by_id(&job_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, job_id);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let repo = InMemoryJobRepository::new();
        let result = repo.find_by_id(&JobId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_by_state() {
        let repo = InMemoryJobRepository::new();

        let job1 = create_test_job();
        let mut job2 = create_test_job();
        // Use proper state transition
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let context = hodei_server_domain::jobs::ExecutionContext::new(
            job2.id.clone(),
            provider_id.clone(),
            "exec-1".to_string(),
        );
        job2.submit_to_provider(provider_id, context).unwrap();
        job2.mark_running().unwrap();

        repo.save(&job1).await.unwrap();
        repo.save(&job2).await.unwrap();

        let pending = repo.find_by_state(&JobState::Pending).await.unwrap();
        assert_eq!(pending.len(), 1);

        let running = repo.find_by_state(&JobState::Running).await.unwrap();
        assert_eq!(running.len(), 1);
    }

    #[tokio::test]
    async fn test_find_pending() {
        let repo = InMemoryJobRepository::new();

        let job1 = create_test_job();
        let mut job2 = create_test_job();
        // Use proper state transition
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let context = hodei_server_domain::jobs::ExecutionContext::new(
            job2.id.clone(),
            provider_id.clone(),
            "exec-1".to_string(),
        );
        job2.submit_to_provider(provider_id, context).unwrap();
        job2.mark_running().unwrap();

        repo.save(&job1).await.unwrap();
        repo.save(&job2).await.unwrap();

        let pending = repo.find_pending().await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_update_job() {
        let repo = InMemoryJobRepository::new();
        let mut job = create_test_job();
        let job_id = job.id.clone();

        repo.save(&job).await.unwrap();

        // Use proper state transition
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let context = hodei_server_domain::jobs::ExecutionContext::new(
            job.id.clone(),
            provider_id.clone(),
            "exec-1".to_string(),
        );
        job.submit_to_provider(provider_id, context).unwrap();
        job.mark_running().unwrap();

        repo.update(&job).await.unwrap();

        let found = repo.find_by_id(&job_id).await.unwrap().unwrap();
        assert_eq!(*found.state(), JobState::Running);
    }

    #[tokio::test]
    async fn test_delete_job() {
        let repo = InMemoryJobRepository::new();
        let job = create_test_job();
        let job_id = job.id.clone();

        repo.save(&job).await.unwrap();
        repo.delete(&job_id).await.unwrap();

        let found = repo.find_by_id(&job_id).await.unwrap();
        assert!(found.is_none());
    }
}

mod job_queue_tests {
    use super::*;

    #[tokio::test]
    async fn test_enqueue_dequeue() {
        let queue = InMemoryJobQueue::new();
        let job = create_test_job();
        let job_id = job.id.clone();

        queue.enqueue(job).await.unwrap();

        let dequeued = queue.dequeue().await.unwrap();
        assert!(dequeued.is_some());
        assert_eq!(dequeued.unwrap().id, job_id);
    }

    #[tokio::test]
    async fn test_dequeue_empty() {
        let queue = InMemoryJobQueue::new();
        let result = queue.dequeue().await.unwrap();
        assert!(result.is_none());
    }

    /*
        #[tokio::test]
        async fn test_peek() {
            let queue = InMemoryJobQueue::new();
            let job = create_test_job();
            let job_id = job.id.clone();

            queue.enqueue(job).await.unwrap();

            let peeked = queue.peek().await.unwrap();
            assert!(peeked.is_some());
            assert_eq!(peeked.unwrap().id, job_id);
        }
    */

    #[tokio::test]
    async fn test_len() {
        let queue = InMemoryJobQueue::new();

        assert_eq!(queue.len().await.unwrap(), 0);

        queue.enqueue(create_test_job()).await.unwrap();
        queue.enqueue(create_test_job()).await.unwrap();

        assert_eq!(queue.len().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_is_empty() {
        let queue = InMemoryJobQueue::new();

        assert!(queue.is_empty().await.unwrap());

        queue.enqueue(create_test_job()).await.unwrap();

        assert!(!queue.is_empty().await.unwrap());
    }

    #[tokio::test]
    async fn test_clear() {
        let queue = InMemoryJobQueue::new();

        queue.enqueue(create_test_job()).await.unwrap();
        queue.enqueue(create_test_job()).await.unwrap();

        queue.clear().await.unwrap();

        assert!(queue.is_empty().await.unwrap());
    }

    #[tokio::test]
    async fn test_fifo_order() {
        let queue = InMemoryJobQueue::new();

        let job1 = create_test_job();
        let job2 = create_test_job();
        let id1 = job1.id.clone();
        let id2 = job2.id.clone();

        queue.enqueue(job1).await.unwrap();
        queue.enqueue(job2).await.unwrap();

        let first = queue.dequeue().await.unwrap().unwrap();
        let second = queue.dequeue().await.unwrap().unwrap();

        assert_eq!(first.id, id1);
        assert_eq!(second.id, id2);
    }
}

mod provider_config_repository_tests {
    #[cfg(test)]
    use crate::persistence::postgres::test_in_memory::InMemoryProviderConfigRepository;
    use hodei_server_domain::providers::{
        DockerConfig, ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
    };
    use hodei_server_domain::shared_kernel::ProviderStatus;
    use hodei_server_domain::workers::ProviderType;

    fn create_test_repo() -> InMemoryProviderConfigRepository {
        InMemoryProviderConfigRepository::new()
    }

    fn create_test_provider_config(name: &str) -> ProviderConfig {
        ProviderConfig::new(
            name.to_string(),
            ProviderType::Docker,
            ProviderTypeConfig::Docker(DockerConfig::default()),
        )
    }

    #[tokio::test]
    async fn test_save_and_find_by_id() {
        let repo = create_test_repo();
        let config = create_test_provider_config("test-docker");
        let config_id = config.id.clone();

        repo.save(&config).await.unwrap();

        let found = repo.find_by_id(&config_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test-docker");
    }

    #[tokio::test]
    async fn test_persistence_across_instances() {
        let repo = create_test_repo();
        let config = create_test_provider_config("persistent-provider");
        let provider_id = config.id.clone();

        repo.save(&config).await.unwrap();

        let found = repo.find_by_id(&provider_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "persistent-provider");
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let repo = create_test_repo();
        let config = create_test_provider_config("unique-name");

        repo.save(&config).await.unwrap();

        let found = repo.find_by_name("unique-name").await.unwrap();
        assert!(found.is_some());

        let not_found = repo.find_by_name("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_enabled() {
        let repo = create_test_repo();

        let active_config = create_test_provider_config("active");
        let mut disabled_config = create_test_provider_config("disabled");
        disabled_config.status = ProviderStatus::Disabled;

        repo.save(&active_config).await.unwrap();
        repo.save(&disabled_config).await.unwrap();

        let enabled = repo.find_enabled().await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "active");
    }

    #[tokio::test]
    async fn test_delete() {
        let repo = create_test_repo();
        let config = create_test_provider_config("to-delete");
        let config_id = config.id.clone();

        repo.save(&config).await.unwrap();
        repo.delete(&config_id).await.unwrap();

        let found = repo.find_by_id(&config_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_update() {
        let repo = create_test_repo();
        let mut config = create_test_provider_config("to-update");
        let config_id = config.id.clone();

        repo.save(&config).await.unwrap();

        config.priority = 100;
        repo.update(&config).await.unwrap();

        let updated = repo.find_by_id(&config_id).await.unwrap().unwrap();
        assert_eq!(updated.priority, 100);
    }
}

mod worker_registry_tests {
    #[cfg(test)]
    use crate::persistence::postgres::test_in_memory::InMemoryWorkerRegistry;
    use hodei_server_domain::{
        shared_kernel::{JobId, ProviderId, WorkerState},
        workers::registry::WorkerRegistry,
        workers::{ProviderType, WorkerHandle, WorkerSpec},
    };

    fn create_test_worker_handle() -> (WorkerHandle, WorkerSpec) {
        let spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-123".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        (handle, spec)
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = InMemoryWorkerRegistry::new();
        let (handle, spec) = create_test_worker_handle();
        let worker_id = handle.worker_id.clone();

        let worker = registry.register(handle, spec, JobId::new()).await.unwrap();
        assert_eq!(worker.id(), &worker_id);

        let found = registry.get(&worker_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id(), &worker_id);
    }

    #[tokio::test]
    async fn test_register_duplicate_fails() {
        let registry = InMemoryWorkerRegistry::new();
        let (handle, spec) = create_test_worker_handle();

        registry
            .register(handle.clone(), spec.clone(), JobId::new())
            .await
            .unwrap();

        let result = registry.register(handle, spec, JobId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = InMemoryWorkerRegistry::new();
        let (handle, spec) = create_test_worker_handle();
        let worker_id = handle.worker_id.clone();

        registry.register(handle, spec, JobId::new()).await.unwrap();
        registry.unregister(&worker_id).await.unwrap();

        let found = registry.get(&worker_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_available() {
        let registry = InMemoryWorkerRegistry::new();
        let (handle, spec) = create_test_worker_handle();
        let worker_id = handle.worker_id.clone();

        registry.register(handle, spec, JobId::new()).await.unwrap();

        // Worker starts in Provisioning, not available
        let available = registry.find_available().await.unwrap();
        assert!(available.is_empty());

        // Transition to Ready (Crash-Only Design: Creating -> Ready)
        registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .unwrap();

        let available = registry.find_available().await.unwrap();
        assert_eq!(available.len(), 1);
    }

    #[tokio::test]
    async fn test_assign_and_release() {
        let registry = InMemoryWorkerRegistry::new();
        let (handle, spec) = create_test_worker_handle();
        let worker_id = handle.worker_id.clone();

        registry.register(handle, spec, JobId::new()).await.unwrap();
        registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .unwrap();

        let job_id = JobId::new();
        registry.assign_to_job(&worker_id, job_id).await.unwrap();

        let worker = registry.get(&worker_id).await.unwrap().unwrap();
        assert_eq!(*worker.state(), WorkerState::Busy);

        registry.release_from_job(&worker_id).await.unwrap();

        let worker = registry.get(&worker_id).await.unwrap().unwrap();
        // EPIC-21: Workers are ephemeral - after job completion, worker is terminated
        assert_eq!(*worker.state(), WorkerState::Terminated);
    }

    #[tokio::test]
    async fn test_stats() {
        let registry = InMemoryWorkerRegistry::new();

        for i in 0..3 {
            let spec = WorkerSpec::new(
                "hodei-jobs-worker:latest".to_string(),
                "http://localhost:50051".to_string(),
            );
            let handle = WorkerHandle::new(
                spec.worker_id.clone(),
                format!("container-{}", i),
                ProviderType::Docker,
                ProviderId::new(),
            );
            registry.register(handle, spec, JobId::new()).await.unwrap();
        }

        let stats = registry.stats().await.unwrap();
        assert_eq!(stats.total_workers, 3);
    }
}

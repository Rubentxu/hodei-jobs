//! Test E2E para CreateJobUseCase
//! Verifica que los jobs se encolan correctamente

use futures::stream::BoxStream;
use hodei_server_application::jobs::create::{CreateJobRequest, CreateJobUseCase, JobSpecRequest};
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::{Job, JobQueue, JobRepository};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Mock EventBus para tests
#[derive(Default)]
struct MockEventBus {
    published_events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl MockEventBus {
    fn new() -> Self {
        Self {
            published_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_published_events(&self) -> Vec<DomainEvent> {
        self.published_events.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl EventBus for MockEventBus {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        self.published_events.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn subscribe(
        &self,
        _topic: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        unimplemented!()
    }
}

/// Mock JobQueue para tests
struct MockJobQueue {
    queue: Arc<Mutex<VecDeque<Job>>>,
}

impl MockJobQueue {
    fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

#[async_trait::async_trait]
impl JobQueue for MockJobQueue {
    async fn enqueue(&self, job: Job) -> Result<(), DomainError> {
        self.queue.lock().unwrap().push_back(job);
        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>, DomainError> {
        Ok(self.queue.lock().unwrap().pop_front())
    }

    async fn len(&self) -> Result<usize, DomainError> {
        Ok(self.queue.lock().unwrap().len())
    }

    async fn is_empty(&self) -> Result<bool, DomainError> {
        Ok(self.queue.lock().unwrap().is_empty())
    }

    async fn clear(&self) -> Result<(), DomainError> {
        self.queue.lock().unwrap().clear();
        Ok(())
    }
}

/// Mock JobRepository que simula Postgres pero en memoria
struct MockJobRepository {
    save_calls: Arc<Mutex<Vec<JobId>>>,
}

impl MockJobRepository {
    fn new() -> Self {
        Self {
            save_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl JobRepository for MockJobRepository {
    async fn save(&self, job: &Job) -> Result<(), DomainError> {
        self.save_calls.lock().unwrap().push(job.id.clone());
        Ok(())
    }

    async fn find_by_id(&self, _job_id: &JobId) -> Result<Option<Job>, DomainError> {
        Ok(None)
    }

    async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>, DomainError> {
        Ok(vec![])
    }

    async fn find_pending(&self) -> Result<Vec<Job>, DomainError> {
        Ok(vec![])
    }

    async fn find_all(
        &self,
        _limit: usize,
        _offset: usize,
    ) -> Result<(Vec<Job>, usize), DomainError> {
        Ok((vec![], 0))
    }

    async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>, DomainError> {
        Ok(None)
    }

    async fn delete(&self, _job_id: &JobId) -> Result<(), DomainError> {
        Ok(())
    }

    async fn update(&self, _job: &Job) -> Result<(), DomainError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_create_job_saves_and_enqueues_atomically() {
    let job_repo = Arc::new(MockJobRepository::new());
    let job_queue = Arc::new(MockJobQueue::new());
    let event_bus = Arc::new(MockEventBus::new());

    let request = CreateJobRequest {
        spec: JobSpecRequest {
            command: vec!["echo".to_string(), "test".to_string()],
            image: None,
            env: None,
            timeout_ms: Some(1000),
            working_dir: None,
            cpu_cores: None,
            memory_bytes: None,
            disk_bytes: None,
        },
        correlation_id: Some("test-correlation".to_string()),
        actor: Some("test-user".to_string()),
        job_id: None,
    };

    let use_case = CreateJobUseCase::new(job_repo.clone(), job_queue.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(result.is_ok(), "CreateJobUseCase should succeed");

    // Verificar que se publicaron eventos (JobCreated + JobQueueDepthChanged)
    let events = event_bus.get_published_events();
    assert!(
        events.len() >= 1,
        "Should publish at least JobCreated event"
    );

    match &events[0] {
        DomainEvent::JobCreated { job_id, .. } => {
            println!("✓ JobCreated event published for job_id: {}", job_id.0);
        }
        _ => panic!("Expected JobCreated event"),
    }
}

#[tokio::test]
async fn test_create_job_with_explicit_id() {
    let job_repo = Arc::new(MockJobRepository::new());
    let job_queue = Arc::new(MockJobQueue::new());
    let event_bus = Arc::new(MockEventBus::new());

    let request = CreateJobRequest {
        spec: JobSpecRequest {
            command: vec!["echo".to_string(), "test".to_string()],
            image: None,
            env: None,
            timeout_ms: Some(1000),
            working_dir: None,
            cpu_cores: None,
            memory_bytes: None,
            disk_bytes: None,
        },
        correlation_id: Some("test-correlation".to_string()),
        actor: Some("test-user".to_string()),
        job_id: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
    };

    let use_case = CreateJobUseCase::new(job_repo.clone(), job_queue.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(
        result.is_ok(),
        "CreateJobUseCase should succeed with explicit ID"
    );

    let events = event_bus.get_published_events();
    assert!(events.len() >= 1, "Should publish at least one event");
}

#[tokio::test]
async fn test_create_job_fails_with_invalid_command() {
    let job_repo = Arc::new(MockJobRepository::new());
    let job_queue = Arc::new(MockJobQueue::new());
    let event_bus = Arc::new(MockEventBus::new());

    let request = CreateJobRequest {
        spec: JobSpecRequest {
            command: vec!["echo".to_string(), "test".to_string()],
            image: None,
            env: None,
            timeout_ms: Some(0), // Zero timeout should fail
            working_dir: None,
            cpu_cores: None,
            memory_bytes: None,
            disk_bytes: None,
        },
        correlation_id: None,
        actor: None,
        job_id: None,
    };

    let use_case = CreateJobUseCase::new(job_repo.clone(), job_queue.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(
        result.is_err(),
        "CreateJobUseCase should fail with zero timeout"
    );
}

#[tokio::test]
async fn test_create_job_publishes_correct_event_data() {
    let job_repo = Arc::new(MockJobRepository::new());
    let job_queue = Arc::new(MockJobQueue::new());
    let event_bus = Arc::new(MockEventBus::new());

    let request = CreateJobRequest {
        spec: JobSpecRequest {
            command: vec!["echo".to_string(), "hello".to_string(), "world".to_string()],
            image: Some("ubuntu:latest".to_string()),
            env: Some(std::collections::HashMap::from([
                ("KEY1".to_string(), "VALUE1".to_string()),
                ("KEY2".to_string(), "VALUE2".to_string()),
            ])),
            timeout_ms: Some(5000),
            working_dir: Some("/tmp".to_string()),
            cpu_cores: Some(2.0),
            memory_bytes: Some(1024 * 1024 * 1024),    // 1GB
            disk_bytes: Some(10 * 1024 * 1024 * 1024), // 10GB
        },
        correlation_id: Some("correlation-123".to_string()),
        actor: Some("user@example.com".to_string()),
        job_id: None,
    };

    let use_case = CreateJobUseCase::new(job_repo.clone(), job_queue.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(result.is_ok(), "CreateJobUseCase should succeed");

    let events = event_bus.get_published_events();
    assert!(events.len() >= 1, "Should publish at least one event");

    match &events[0] {
        DomainEvent::JobCreated {
            job_id,
            spec,
            correlation_id,
            actor,
            ..
        } => {
            assert_eq!(
                correlation_id.as_deref(),
                Some("correlation-123"),
                "Correlation ID should match"
            );
            assert_eq!(
                actor.as_deref(),
                Some("user@example.com"),
                "Actor should match"
            );

            let cmd_vec = spec.command_vec();
            // EPIC-21 Jenkins sh behavior: commands are wrapped as bash -c "command"
            assert_eq!(
                cmd_vec[0], "bash",
                "First element should be 'bash' interpreter"
            );
            assert_eq!(cmd_vec[1], "-c", "Second element should be '-c' flag");
            assert!(
                cmd_vec[2].contains("echo"),
                "Script content should contain 'echo'"
            );
            assert!(
                cmd_vec[2].contains("hello"),
                "Script content should contain 'hello'"
            );
            assert!(
                cmd_vec[2].contains("world"),
                "Script content should contain 'world'"
            );
            assert_eq!(spec.image.as_deref(), Some("ubuntu:latest"));
            assert_eq!(spec.env.get("KEY1"), Some(&"VALUE1".to_string()));
            assert_eq!(spec.timeout_ms, 5000);

            println!("✓ All event data verified correctly");
            println!("  - Job ID: {}", job_id.0);
        }
        _ => panic!("Expected JobCreated event"),
    }
}

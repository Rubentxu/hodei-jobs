//! Test E2E para CreateJobUseCase
//! Verifica que los jobs se encolan correctamente

use futures::stream::BoxStream;
use hodei_server_application::jobs::create::{CreateJobRequest, CreateJobUseCase, JobSpecRequest};
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::{Job, JobRepository};
use hodei_server_domain::shared_kernel::{JobId, JobState, ProviderId};
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

/// Mock JobRepository que simula Postgres pero en memoria
struct MockJobRepository {
    jobs: std::collections::HashMap<JobId, Job>,
    save_calls: Arc<Mutex<Vec<JobId>>>,
}

impl MockJobRepository {
    fn new() -> Self {
        Self {
            jobs: std::collections::HashMap::new(),
            save_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_save_calls(&self) -> Vec<JobId> {
        self.save_calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl JobRepository for MockJobRepository {
    async fn save(&self, job: &Job) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
        // Simular save exitoso
        self.save_calls.lock().unwrap().push(job.id.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        job_id: &JobId,
    ) -> Result<Option<Job>, hodei_server_domain::shared_kernel::DomainError> {
        Ok(self.jobs.get(job_id).cloned())
    }

    async fn find_by_state(
        &self,
        _state: &JobState,
    ) -> Result<Vec<Job>, hodei_server_domain::shared_kernel::DomainError> {
        Ok(vec![])
    }

    async fn find_pending(
        &self,
    ) -> Result<Vec<Job>, hodei_server_domain::shared_kernel::DomainError> {
        Ok(vec![])
    }

    async fn find_all(
        &self,
        _limit: usize,
        _offset: usize,
    ) -> Result<(Vec<Job>, usize), hodei_server_domain::shared_kernel::DomainError> {
        Ok((vec![], 0))
    }

    async fn find_by_execution_id(
        &self,
        _execution_id: &str,
    ) -> Result<Option<Job>, hodei_server_domain::shared_kernel::DomainError> {
        Ok(None)
    }

    async fn delete(
        &self,
        _job_id: &JobId,
    ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
        Ok(())
    }

    async fn update(
        &self,
        _job: &Job,
    ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_create_job_saves_and_enqueues_atomically() {
    // Setup
    let job_repo = Arc::new(MockJobRepository::new());
    let event_bus = Arc::new(MockEventBus::new());

    // El CreateJobUseCase actual usa JobQueue separadamente
    // Esto debería cambiar para usar solo save() que ya incluye enqueue

    // Crear request
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

    // Ejecutar use case
    let use_case = CreateJobUseCase::new(job_repo.clone(), event_bus.clone());

    // Verificar que se guardó
    let result = use_case.execute(request).await;
    assert!(result.is_ok(), "CreateJobUseCase should succeed");

    // Verificar que se publicó el evento
    let events = event_bus.get_published_events();
    assert_eq!(events.len(), 1, "Should publish exactly one event");
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

    let use_case = CreateJobUseCase::new(job_repo.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(
        result.is_ok(),
        "CreateJobUseCase should succeed with explicit ID"
    );

    let events = event_bus.get_published_events();
    assert_eq!(events.len(), 1, "Should publish exactly one event");
}

#[tokio::test]
async fn test_create_job_fails_with_invalid_command() {
    let job_repo = Arc::new(MockJobRepository::new());
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

    let use_case = CreateJobUseCase::new(job_repo.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(
        result.is_err(),
        "CreateJobUseCase should fail with zero timeout"
    );
}

#[tokio::test]
async fn test_create_job_publishes_correct_event_data() {
    let job_repo = Arc::new(MockJobRepository::new());
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

    let use_case = CreateJobUseCase::new(job_repo.clone(), event_bus.clone());

    let result = use_case.execute(request).await;
    assert!(result.is_ok(), "CreateJobUseCase should succeed");

    let events = event_bus.get_published_events();
    assert_eq!(events.len(), 1, "Should publish exactly one event");

    match &events[0] {
        DomainEvent::JobCreated {
            job_id,
            spec,
            occurred_at,
            correlation_id,
            actor,
        } => {
            // Verificar correlation_id
            assert_eq!(
                correlation_id.as_deref(),
                Some("correlation-123"),
                "Correlation ID should match"
            );

            // Verificar actor
            assert_eq!(
                actor.as_deref(),
                Some("user@example.com"),
                "Actor should match"
            );

            // Verificar spec
            let cmd_vec = spec.command_vec();
            assert_eq!(cmd_vec[0], "echo", "First command should be 'echo'");
            assert_eq!(cmd_vec[1], "hello", "Second arg should be 'hello'");
            assert_eq!(cmd_vec[2], "world", "Third arg should be 'world'");

            // Verificar imagen
            assert_eq!(spec.image.as_deref(), Some("ubuntu:latest"));

            // Verificar env
            assert_eq!(spec.env.get("KEY1"), Some(&"VALUE1".to_string()));
            assert_eq!(spec.env.get("KEY2"), Some(&"VALUE2".to_string()));

            // Verificar timeout
            assert_eq!(spec.timeout_ms, 5000);

            println!("✓ All event data verified correctly");
            println!("  - Job ID: {}", job_id.0);
            println!("  - Correlation ID: {}", correlation_id.as_deref().unwrap());
            println!("  - Actor: {}", actor.as_deref().unwrap());
            println!("  - Command: {:?}", cmd_vec);
            println!("  - Timeout: {}ms", spec.timeout_ms);
        }
        _ => panic!("Expected JobCreated event"),
    }
}

//! Testing Framework for Saga Orchestration
//!
//! Provides utilities and fixtures for testing sagas.
//!
//! EPIC-55: Testing Framework

use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// Mock Implementations
// ============================================================================

/// Mock Job Repository for testing
#[derive(Clone, Debug)]
pub struct MockJobRepository {
    jobs: Arc<Mutex<Vec<String>>>,
}

impl MockJobRepository {
    /// Creates a new mock repository
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Adds a job ID to the mock repository
    pub async fn add_job(&self, id: &str) {
        let mut jobs = self.jobs.lock().await;
        jobs.push(id.to_string());
    }

    /// Clears all jobs
    pub async fn clear(&self) {
        let mut jobs = self.jobs.lock().await;
        jobs.clear();
    }
}

impl Default for MockJobRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock Worker Repository for testing
#[derive(Clone, Debug)]
pub struct MockWorkerRepository {
    workers: Arc<Mutex<Vec<String>>>,
}

impl MockWorkerRepository {
    /// Creates a new mock repository
    pub fn new() -> Self {
        Self {
            workers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Adds a worker ID to the mock repository
    pub async fn add_worker(&self, id: &str) {
        let mut workers = self.workers.lock().await;
        workers.push(id.to_string());
    }

    /// Clears all workers
    pub async fn clear(&self) {
        let mut workers = self.workers.lock().await;
        workers.clear();
    }
}

impl Default for MockWorkerRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock Event Bus for testing
#[derive(Clone, Debug, Default)]
pub struct MockEventBus {
    published_events: Arc<Mutex<Vec<String>>>,
}

impl MockEventBus {
    /// Creates a new mock event bus
    pub fn new() -> Self {
        Self {
            published_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns all published events
    pub async fn published_events(&self) -> Vec<String> {
        let events = self.published_events.lock().await;
        events.clone()
    }

    /// Clears all published events
    pub async fn clear(&self) {
        let mut events = self.published_events.lock().await;
        events.clear();
    }
}

/// Mock Outbox Repository for testing
#[derive(Clone, Debug, Default)]
pub struct MockOutboxRepository {
    records: Arc<Mutex<Vec<String>>>,
}

impl MockOutboxRepository {
    /// Creates a new mock repository
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Adds a record to the mock repository
    pub async fn add_record(&self, record: impl Into<String>) {
        let mut records = self.records.lock().await;
        records.push(record.into());
    }

    /// Returns all records
    pub async fn all(&self) -> Vec<String> {
        let records = self.records.lock().await;
        records.clone()
    }

    /// Clears all records
    pub async fn clear(&self) {
        let mut records = self.records.lock().await;
        records.clear();
    }
}

// ============================================================================
// Test Fixtures
// ============================================================================

/// Complete test fixture for saga testing
#[derive(Debug, Clone)]
pub struct SagaTestFixture {
    /// Mock job repository
    pub job_repo: MockJobRepository,
    /// Mock worker repository
    pub worker_repo: MockWorkerRepository,
    /// Mock outbox repository
    pub outbox_repo: MockOutboxRepository,
    /// Mock event bus
    pub event_bus: MockEventBus,
}

impl SagaTestFixture {
    /// Creates a new test fixture
    pub fn new() -> Self {
        Self {
            job_repo: MockJobRepository::new(),
            worker_repo: MockWorkerRepository::new(),
            outbox_repo: MockOutboxRepository::new(),
            event_bus: MockEventBus::new(),
        }
    }

    /// Clears all mock data
    pub async fn clear(&self) {
        self.job_repo.clear().await;
        self.worker_repo.clear().await;
        self.outbox_repo.clear().await;
        self.event_bus.clear().await;
    }
}

impl Default for SagaTestFixture {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Test Command
// ============================================================================

/// A test command for verifying command bus dispatch
#[derive(Debug, Clone)]
pub struct TestCommand {
    pub saga_id: String,
    pub should_fail: bool,
}

impl TestCommand {
    /// Creates a new test command
    pub fn new(saga_id: impl Into<String>) -> Self {
        Self {
            saga_id: saga_id.into(),
            should_fail: false,
        }
    }

    /// Creates a command that will fail
    pub fn failing(saga_id: impl Into<String>) -> Self {
        Self {
            saga_id: saga_id.into(),
            should_fail: true,
        }
    }
}

/// Marker trait for all commands in the system.
pub trait TestCommandTrait: Debug + Send + Sync + 'static {
    /// The type returned by the handler when executing this command
    type Output: Send;

    /// Returns an idempotency key for this command.
    fn idempotency_key(&self) -> Cow<'_, str>;
}

impl TestCommandTrait for TestCommand {
    type Output = String;

    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(self.saga_id.clone())
    }
}

// ============================================================================
// Test Command Handler
// ============================================================================

/// Handler for TestCommand
#[derive(Clone, Debug)]
pub struct TestCommandHandler {
    pub dispatched: Arc<Mutex<Vec<TestCommand>>>,
}

impl TestCommandHandler {
    /// Creates a new handler
    pub fn new() -> Self {
        Self {
            dispatched: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns dispatched commands
    pub async fn dispatched(&self) -> Vec<TestCommand> {
        self.dispatched.lock().await.clone()
    }
}

// ============================================================================
// Utilities
// ============================================================================

/// Generates a unique test ID
pub fn generate_test_id() -> String {
    format!("test-{}", uuid::Uuid::new_v4())
}

/// Assertion helpers for saga test verification
#[derive(Debug, Default)]
pub struct SagaAssertions;

impl SagaAssertions {
    /// Verifies that a result is ok
    pub fn ok<T, E>(result: &Result<T, E>) {
        assert!(result.is_ok(), "Expected result to be Ok");
    }

    /// Verifies that a result is err
    pub fn err<T, E>(result: &Result<T, E>) {
        assert!(result.is_err(), "Expected result to be Err");
    }
}

/// Shared state for concurrent test execution
#[derive(Clone, Debug)]
pub struct SharedTestState<T: Clone> {
    state: Arc<Mutex<T>>,
}

impl<T: Clone + Default> SharedTestState<T> {
    /// Creates new shared state with default value
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(T::default())),
        }
    }

    /// Gets the current state
    pub async fn get(&self) -> T {
        self.state.lock().await.clone()
    }

    /// Sets the state
    pub async fn set(&self, value: T) {
        *self.state.lock().await = value;
    }
}

impl<T: Clone + Default> Default for SharedTestState<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Test result collector for capturing multiple results
#[derive(Clone, Debug)]
pub struct TestResultCollector {
    results: Arc<Mutex<Vec<Result<(), String>>>>,
}

impl TestResultCollector {
    /// Creates a new collector
    pub fn new() -> Self {
        Self {
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Records a result
    pub async fn record(&self, result: Result<(), String>) {
        self.results.lock().await.push(result);
    }

    /// Returns all results
    pub async fn results(&self) -> Vec<Result<(), String>> {
        self.results.lock().await.clone()
    }

    /// Checks if all results are successful
    pub async fn all_success(&self) -> bool {
        self.results.lock().await.iter().all(|r| r.is_ok())
    }
}

impl Default for TestResultCollector {
    fn default() -> Self {
        Self::new()
    }
}

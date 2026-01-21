use async_trait::async_trait;
use dashmap::DashMap;
use saga_engine_core::activity_registry::ActivityRegistry;
use saga_engine_core::event::{HistoryEvent, SagaId};
use saga_engine_core::port::{
    ConsumerConfig, DurableTimer, EventStore, EventStoreError, Task, TaskId, TaskMessage,
    TaskQueue, TaskQueueError, TimerStatus, TimerStore, TimerStoreError,
};
use saga_engine_core::saga_engine::{SagaEngine, SagaEngineConfig, SagaExecutionResult};
use saga_engine_core::worker::{Worker, WorkerConfig};
use saga_engine_core::workflow::registry::WorkflowRegistry;
use saga_engine_core::workflow::{DurableWorkflow, WorkflowContext};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// --- Mocks ---

#[derive(Debug, Clone, Default)]
struct MockEventStore {
    events: Arc<DashMap<SagaId, Vec<HistoryEvent>>>,
}

#[async_trait]
impl EventStore for MockEventStore {
    type Error = String;

    async fn append_event(
        &self,
        saga_id: &SagaId,
        _expected_next: u64,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<Self::Error>> {
        let mut events = self.events.entry(saga_id.clone()).or_insert_with(Vec::new);
        events.push(event.clone());
        Ok(events.len() as u64)
    }

    async fn append_events(
        &self,
        saga_id: &SagaId,
        _expected_next: u64,
        new_events: &[HistoryEvent],
    ) -> Result<u64, EventStoreError<Self::Error>> {
        let mut events = self.events.entry(saga_id.clone()).or_insert_with(Vec::new);
        for event in new_events {
            events.push(event.clone());
        }
        Ok(events.len() as u64)
    }

    async fn get_history(&self, saga_id: &SagaId) -> Result<Vec<HistoryEvent>, Self::Error> {
        Ok(self
            .events
            .get(saga_id)
            .map(|e| e.clone())
            .unwrap_or_default())
    }

    async fn get_history_from(
        &self,
        _saga_id: &SagaId,
        _from: u64,
    ) -> Result<Vec<HistoryEvent>, Self::Error> {
        Ok(vec![])
    }
    async fn save_snapshot(&self, _saga: &SagaId, _id: u64, _st: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn get_latest_snapshot(
        &self,
        _saga: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
        Ok(None)
    }
    async fn get_current_event_id(&self, _saga: &SagaId) -> Result<u64, Self::Error> {
        Ok(0)
    }
    async fn saga_exists(&self, _saga: &SagaId) -> Result<bool, Self::Error> {
        Ok(true)
    }
    async fn snapshot_count(&self, _saga: &SagaId) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

#[derive(Debug, Clone, Default)]
struct MockTaskQueue {
    tasks: Arc<Mutex<Vec<Task>>>,
}

#[async_trait]
impl TaskQueue for MockTaskQueue {
    type Error = String;

    async fn publish(
        &self,
        task: &Task,
        _subject: &str,
    ) -> Result<TaskId, TaskQueueError<Self::Error>> {
        self.tasks.lock().await.push(task.clone());
        Ok(task.task_id.clone())
    }

    async fn fetch(
        &self,
        _consumer: &str,
        _max: u64,
        _timeout: Duration,
    ) -> Result<Vec<TaskMessage>, TaskQueueError<Self::Error>> {
        let mut tasks = self.tasks.lock().await;
        // Simple FIFO fetch
        if !tasks.is_empty() {
            let task = tasks.remove(0);
            return Ok(vec![TaskMessage::new(
                uuid::Uuid::new_v4().to_string(),
                task,
                "subject".to_string(),
                false,
                1,
            )]);
        }
        Ok(vec![])
    }

    async fn ensure_consumer(
        &self,
        _name: &str,
        _cfg: &ConsumerConfig,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
    async fn ack(&self, _id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
    async fn nak(
        &self,
        _id: &str,
        _delay: Option<Duration>,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
    async fn terminate(&self, _id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
struct MockTimerStore;
#[async_trait]
impl TimerStore for MockTimerStore {
    type Error = String;
    async fn create_timer(
        &self,
        _timer: &DurableTimer,
    ) -> Result<(), TimerStoreError<Self::Error>> {
        Ok(())
    }
    async fn cancel_timer(&self, _id: &str) -> Result<(), TimerStoreError<Self::Error>> {
        Ok(())
    }
    async fn get_timer(
        &self,
        _id: &str,
    ) -> Result<Option<DurableTimer>, TimerStoreError<Self::Error>> {
        Ok(None)
    }
    async fn get_expired_timers(
        &self,
        _limit: u64,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        Ok(vec![])
    }
    async fn claim_timers(
        &self,
        _timer_ids: &[String],
        _scheduler_id: &str,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        Ok(vec![])
    }
    async fn update_timer_status(
        &self,
        _id: &str,
        _status: TimerStatus,
    ) -> Result<(), TimerStoreError<Self::Error>> {
        Ok(())
    }
    async fn get_timers_for_saga(
        &self,
        _saga_id: &SagaId,
        _include_fired: bool,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        Ok(vec![])
    }
}

// --- Workflow ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestInput {
    msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TestOutput {
    result: String,
}

#[derive(Debug)]
struct TestWorkflow;

#[async_trait]
impl DurableWorkflow for TestWorkflow {
    type Input = TestInput;
    type Output = TestOutput;
    type Error = std::io::Error;
    const TYPE_ID: &'static str = "test-workflow";

    async fn run(
        &self,
        ctx: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        let act_output: String = ctx
            .execute_activity(&TestActivity, input.msg.clone())
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        Ok(TestOutput {
            result: format!("Correct: {}", act_output),
        })
    }
}

// --- Activity ---

#[derive(Debug)]
struct TestActivity;

#[async_trait]
impl saga_engine_core::workflow::Activity for TestActivity {
    type Input = String;
    type Output = String;
    type Error = std::io::Error;
    const TYPE_ID: &'static str = "test-activity";

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        Ok(format!("Processed {}", input))
    }
}

// --- Test ---

#[tokio::test]
async fn test_execution_flow() {
    // 1. Setup
    let event_store = Arc::new(MockEventStore::default());
    let task_queue = Arc::new(MockTaskQueue::default());
    let timer_store = Arc::new(MockTimerStore::default());

    let activity_registry = Arc::new(ActivityRegistry::new());
    activity_registry.register_activity(TestActivity);

    let workflow_registry = Arc::new(WorkflowRegistry::new());
    workflow_registry.register_workflow(TestWorkflow);

    let engine = Arc::new(SagaEngine::new(
        SagaEngineConfig::default(),
        event_store.clone(),
        task_queue.clone(),
        timer_store.clone(),
    ));

    let worker = Worker::new(
        WorkerConfig::default(),
        engine.clone(),
        activity_registry.clone(),
        workflow_registry.clone(),
    );

    // 2. Start Workflow
    let saga_id = SagaId("saga-1".to_string());
    engine
        .start_workflow::<TestWorkflow>(
            saga_id.clone(),
            TestInput {
                msg: "Hello".to_string(),
            },
        )
        .await
        .unwrap();

    // 3. Worker should pick up "workflow-execute"
    let msgs = task_queue
        .fetch("worker", 1, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    let msg = &msgs[0];
    assert_eq!(msg.task.task_type, "workflow-execute");

    // Process Task (Resume 1)
    // Manually calling process_message or mocking worker loop part?
    // Let's call internal logic via `resume_workflow` indirectly?
    // The test logic calls `handle_workflow_task`.
    // Since `handle_workflow_task` is private in Worker, we can't call it directly unless we make it pub or use `process_message` which is private.
    // Worker has `start` loop. But we want manual control.
    // Actually `Worker::start` runs loop.
    // Let's spawn worker in background?

    // Instead of spawning worker (which loops), we can manually check queues and drive engine.
    // Step 3a: Resume Workflow (Engine)
    // Get payload
    let task = &msg.task;
    // We can't call worker private methods.
    // But we can call `engine.resume_workflow_dyn`?
    // We need to deserialize the task payload first to get `WorkflowTask` with `saga_id`.
    // Wait, the test has `saga_id`.

    // Resume Workflow MANUALLY via Engine (Simulating Worker)
    let result = engine
        .resume_workflow(&TestWorkflow, saga_id.clone())
        .await
        .unwrap();

    // Should be paused on activity
    match result {
        SagaExecutionResult::Running { state } => {
            assert_eq!(state["status"], "paused");
            assert_eq!(state["waiting_for"], "test-activity");
        }
        _ => panic!("Expected Running/Paused, got {:?}", result),
    }

    // 4. Activity Task should be in queue
    let msgs = task_queue
        .fetch("worker", 1, Duration::from_secs(1))
        .await
        .unwrap();
    // Wait, `fetch` removes from mock queue? My Mock implementation removes!
    // So previous fetch removed the workflow task.
    // Now we expect activity task.
    assert_eq!(msgs.len(), 1);
    let msg = &msgs[0];
    assert_eq!(msg.task.task_type, "test-activity");

    // 5. Execute Activity (Simulating Worker)
    let act_input: String = serde_json::from_slice(
        &serde_json::from_slice::<saga_engine_core::saga_engine::WorkflowTask>(&msg.task.payload)
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(act_input, "Hello"); // Input passed from workflow

    let act_output = format!("Processed {}", act_input); // Logic of TestActivity

    // 6. Complete Activity
    engine
        .complete_activity_task(
            saga_id.clone(),
            "test-workflow".to_string(),
            Ok(serde_json::to_value(act_output).unwrap()),
        )
        .await
        .unwrap();

    // 7. Workflow Task should be in queue (Resume 2)
    let msgs = task_queue
        .fetch("worker", 1, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    let msg = &msgs[0];
    assert_eq!(msg.task.task_type, "workflow-execute");

    // 8. Resume Workflow Final
    let result = engine
        .resume_workflow(&TestWorkflow, saga_id.clone())
        .await
        .unwrap();

    // Should be Completed
    match result {
        SagaExecutionResult::Completed { output, .. } => {
            let out: TestOutput = serde_json::from_value(output).unwrap();
            assert_eq!(out.result, "Correct: Processed Hello");
        }
        _ => panic!("Expected Completed, got {:?}", result),
    }
}

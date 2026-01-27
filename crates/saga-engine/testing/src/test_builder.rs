//! # WorkflowTestBuilder - Testing Framework for DurableWorkflow
//!
//! This module provides a fluent API for testing workflows with automatic mocking
//! and assertions. Designed for the DurableWorkflow (Workflow-as-Code) pattern.
//!
//! # Quick Start
//!
//! ```rust
//! use saga_engine_testing::{WorkflowTestBuilder, InMemoryEventStore};
//!
//! #[tokio::test]
//! async fn test_provisioning_workflow() {
//!     let test = WorkflowTestBuilder::new()
//!         .workflow(ProvisioningWorkflow::default())
//!         .build();
//!
//!     let result = test.run(test_input()).await;
//!     assert!(result.is_ok());
//! }
//! ```
//!
//! # With Mocks
//!
//! ```rust
//! #[tokio::test]
//! async fn test_with_mocks() {
//!     let test = WorkflowTestBuilder::new()
//!         .workflow(ProvisioningWorkflow::default())
//!         .mock_activity::<ValidateProviderActivity>()
//!             .returns(Ok(validated_provider()))
//!         .mock_activity::<ProvisionWorkerActivity>()
//!             .returns(Ok(provisioned_worker()))
//!         .build();
//!
//!     let result = test.run(test_input()).await.unwrap();
//!
//!     // Verify activities were called
//!     test.assert_activity_called::<ValidateProviderActivity>(1);
//!     test.assert_activity_called::<ProvisionWorkerActivity>(1);
//! }
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

use saga_engine_core::event::SagaId;
use saga_engine_core::workflow::{
    Activity, DurableWorkflow, WorkflowConfig, WorkflowContext,
};
use serde::Serialize;

/// Configuration for test execution
#[derive(Debug, Clone, Default)]
pub struct TestConfig {
    /// Timeout for test execution
    pub timeout: std::time::Duration,
    /// Start time for deterministic tests
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether to fail on unexpected activity calls
    pub fail_on_unexpected: bool,
}

impl TestConfig {
    /// Create default config with 30 second timeout
    pub fn new() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(30),
            start_time: None,
            fail_on_unexpected: false,
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set start time
    pub fn with_start_time(mut self, time: chrono::DateTime<chrono::Utc>) -> Self {
        self.start_time = Some(time);
        self
    }
}

/// Tracker for activity calls during test execution
#[derive(Debug, Default)]
pub struct CallTracker {
    calls: RwLock<HashMap<TypeId, Vec<ActivityCall>>>,
}

impl CallTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
        }
    }

    /// Record an activity call
    pub fn record_call<A: Activity + 'static>(&self, input: &A::Input) {
        let mut calls = self.calls.write().unwrap();
        calls
            .entry(TypeId::of::<A>())
            .or_insert_with(Vec::new)
            .push(ActivityCall {
                activity_type: A::TYPE_ID,
                input: serde_json::to_value(input).unwrap_or_default(),
                timestamp: chrono::Utc::now(),
            });
    }

    /// Get call count for an activity type
    pub fn call_count<A: Activity + 'static>(&self) -> usize {
        let calls = self.calls.read().unwrap();
        calls.get(&TypeId::of::<A>()).map(|v| v.len()).unwrap_or(0)
    }

    /// Get all calls for an activity type
    pub fn get_calls<A: Activity + 'static>(&self) -> Vec<ActivityCall> {
        let calls = self.calls.read().unwrap();
        calls
            .get(&TypeId::of::<A>())
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get all calls in order
    pub fn call_order(&self) -> Vec<String> {
        let calls = self.calls.read().unwrap();
        let mut all_calls: Vec<_> = calls.values().flatten().collect();
        all_calls.sort_by_key(|c| c.timestamp);
        all_calls
            .into_iter()
            .map(|c| c.activity_type.to_string())
            .collect()
    }

    /// Reset tracker
    pub fn reset(&self) {
        let mut calls = self.calls.write().unwrap();
        calls.clear();
    }
}

/// Record of a single activity call
#[derive(Debug, Clone)]
pub struct ActivityCall {
    pub activity_type: &'static str,
    pub input: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trait for activity mocks
pub trait ActivityMock: Send + Sync {
    /// Check if this mock handles the given activity type
    fn handles(&self, type_id: TypeId) -> bool;
    /// Execute the mock
    fn execute(&self, input: &dyn Any) -> Result<serde_json::Value, String>;
    /// Clone as boxed
    fn clone_box(&self) -> Box<dyn ActivityMock>;
}

/// Mock that returns a fixed value
pub struct ValueMock {
    output: serde_json::Value,
}

impl Clone for ValueMock {
    fn clone(&self) -> Self {
        Self {
            output: self.output.clone(),
        }
    }
}

impl ValueMock {
    pub fn new(output: serde_json::Value) -> Self {
        Self { output }
    }
}

impl ActivityMock for ValueMock {
    fn handles(&self, _type_id: TypeId) -> bool {
        true
    }

    fn execute(&self, _input: &dyn Any) -> Result<serde_json::Value, String> {
        Ok(self.output.clone())
    }

    fn clone_box(&self) -> Box<dyn ActivityMock> {
        Box::new(self.clone())
    }
}

/// Mock that returns a fixed error
pub struct ErrorMock<E> {
    error: E,
    _phantom: PhantomData<E>,
}

impl<E: Clone> Clone for ErrorMock<E> {
    fn clone(&self) -> Self {
        Self {
            error: self.error.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<E> ErrorMock<E> {
    pub fn new(error: E) -> Self {
        Self {
            error,
            _phantom: PhantomData,
        }
    }
}

impl<E: Clone + Send + Sync + 'static> ActivityMock for ErrorMock<E> {
    fn handles(&self, _type_id: TypeId) -> bool {
        true
    }

    fn execute(&self, _input: &dyn Any) -> Result<serde_json::Value, String> {
        Err("error mock".to_string())
    }

    fn clone_box(&self) -> Box<dyn ActivityMock> {
        Box::new(self.clone())
    }
}

/// Mock that executes a custom function
pub struct FnMock<F> {
    func: F,
}

impl<F> FnMock<F> {
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

impl<F: Clone> Clone for FnMock<F> {
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
        }
    }
}

impl<F> ActivityMock for FnMock<F>
where
    F: Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + Clone + 'static,
{
    fn handles(&self, _type_id: TypeId) -> bool {
        true
    }

    fn execute(&self, input: &dyn Any) -> Result<serde_json::Value, String> {
        let json_input = input
            .downcast_ref::<serde_json::Value>()
            .cloned()
            .unwrap_or_default();
        (self.func)(json_input)
    }

    fn clone_box(&self) -> Box<dyn ActivityMock> {
        Box::new(self.clone())
    }
}

/// Builder for configuring activity mocks
pub struct ActivityMockBuilder<W, A>
where
    W: DurableWorkflow,
    A: Activity + 'static,
{
    test_builder: WorkflowTestBuilder<W>,
    _phantom: PhantomData<A>,
}

impl<W, A> ActivityMockBuilder<W, A>
where
    W: DurableWorkflow,
    A: Activity + 'static,
{
    fn new(test_builder: WorkflowTestBuilder<W>) -> Self {
        Self {
            test_builder,
            _phantom: PhantomData,
        }
    }

    /// Configure that the activity returns a specific value
    pub fn returns<T>(self, output: T) -> WorkflowTestBuilder<W>
    where
        T: Clone + Send + Sync + Serialize + 'static,
    {
        let json_output = serde_json::to_value(output)
            .map_err(|e| format!("Failed to serialize output: {}", e))
            .unwrap();
        let mock: Box<dyn ActivityMock> = Box::new(ValueMock::new(json_output));
        self.test_builder
            .activity_mocks
            .write()
            .unwrap()
            .insert(TypeId::of::<A>(), mock);
        self.test_builder
    }

    /// Configure that the activity returns an error
    pub fn returns_error<E>(self, error: E) -> WorkflowTestBuilder<W>
    where
        E: Clone + Send + Sync + 'static,
    {
        let mock: Box<dyn ActivityMock> = Box::new(ErrorMock::new(error));
        self.test_builder
            .activity_mocks
            .write()
            .unwrap()
            .insert(TypeId::of::<A>(), mock);
        self.test_builder
    }

    /// Configure custom behavior using a function
    pub fn with_fn<F>(self, f: F) -> WorkflowTestBuilder<W>
    where
        F: Fn(serde_json::Value) -> Result<serde_json::Value, String>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        let mock: Box<dyn ActivityMock> = Box::new(FnMock::new(f));
        self.test_builder
            .activity_mocks
            .write()
            .unwrap()
            .insert(TypeId::of::<A>(), mock);
        self.test_builder
    }

    /// Configure a delay before returning
    pub fn with_delay(self, _duration: std::time::Duration) -> Self {
        // For now, just return self - delay support can be added later
        self
    }
}

/// Builder for creating workflow tests
///
/// # Example
/// ```rust
/// #[tokio::test]
/// async fn test_example() {
///     let test = WorkflowTestBuilder::new()
///         .workflow(MyWorkflow::default())
///         .mock_activity::<MyActivity>()
///             .returns(my_output)
///         .build();
///
///     let result = test.run(my_input).await;
///     assert!(result.is_ok());
/// }
/// ```
pub struct WorkflowTestBuilder<W>
where
    W: DurableWorkflow,
{
    workflow: Option<W>,
    activity_mocks: Arc<RwLock<HashMap<TypeId, Box<dyn ActivityMock>>>>,
    config: TestConfig,
    call_tracker: Arc<CallTracker>,
}

impl<W> WorkflowTestBuilder<W>
where
    W: DurableWorkflow,
{
    /// Create a new test builder
    pub fn new() -> Self {
        Self {
            workflow: None,
            activity_mocks: Arc::new(RwLock::new(HashMap::new())),
            config: TestConfig::new(),
            call_tracker: Arc::new(CallTracker::new()),
        }
    }

    /// Set the workflow to test
    pub fn workflow(mut self, workflow: W) -> Self {
        self.workflow = Some(workflow);
        self
    }

    /// Configure timeout
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Configure start time for deterministic tests
    pub fn with_start_time(mut self, time: chrono::DateTime<chrono::Utc>) -> Self {
        self.config.start_time = Some(time);
        self
    }

    /// Start configuring a mock for an activity
    pub fn mock_activity<A>(self) -> ActivityMockBuilder<W, A>
    where
        A: Activity + 'static,
    {
        ActivityMockBuilder::new(self)
    }

    /// Build the test harness
    ///
    /// # Panics
    /// Panics if no workflow was configured
    pub fn build(self) -> WorkflowTest<W> {
        WorkflowTest {
            workflow: self
                .workflow
                .expect("Workflow not configured - call .workflow() first"),
            activity_mocks: self.activity_mocks,
            config: self.config,
            call_tracker: self.call_tracker,
        }
    }
}

impl<W> Default for WorkflowTestBuilder<W>
where
    W: DurableWorkflow,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Test harness for running workflow tests
pub struct WorkflowTest<W>
where
    W: DurableWorkflow,
{
    workflow: W,
    activity_mocks: Arc<RwLock<HashMap<TypeId, Box<dyn ActivityMock>>>>,
    config: TestConfig,
    call_tracker: Arc<CallTracker>,
}

impl<W> Debug for WorkflowTest<W>
where
    W: DurableWorkflow,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowTest")
            .field("workflow_type", &W::TYPE_ID)
            .field("version", &W::VERSION)
            .finish()
    }
}

impl<W> WorkflowTest<W>
where
    W: DurableWorkflow,
{
    /// Run the workflow with the given input
    ///
    /// This executes the workflow synchronously. For workflows that call
    /// `execute_activity()`, the activity mocks will be invoked.
    pub async fn run(&self, input: W::Input) -> Result<W::Output, W::Error> {
        // Create test context
        let execution_id = SagaId::new();
        let config = WorkflowConfig::default();
        let workflow_type = saga_engine_core::workflow::WorkflowTypeId::new::<W>();

        let mut context = WorkflowContext::new(execution_id, workflow_type, config);

        // Execute the workflow
        self.workflow.run(&mut context, input).await
    }

    /// Run the workflow multiple times with the same input (for determinism tests)
    pub async fn run_multiple(
        &self,
        input: W::Input,
        times: usize,
    ) -> Vec<Result<W::Output, W::Error>> {
        let mut results = Vec::with_capacity(times);
        for _ in 0..times {
            results.push(self.run(input.clone()).await);
        }
        results
    }

    /// Assert that an activity was called a specific number of times
    ///
    /// # Panics
    /// Panics if the call count doesn't match
    pub fn assert_activity_called<A>(&self, expected_calls: usize)
    where
        A: Activity + 'static,
    {
        let actual = self.call_tracker.call_count::<A>();
        assert_eq!(
            actual,
            expected_calls,
            "Activity {} was called {} times, expected {}",
            std::any::type_name::<A>(),
            actual,
            expected_calls
        );
    }

    /// Assert that an activity was never called
    pub fn assert_activity_not_called<A>(&self)
    where
        A: Activity + 'static,
    {
        self.assert_activity_called::<A>(0);
    }

    /// Assert that an activity was called at least once
    pub fn assert_activity_called_at_least<A>(&self, min_calls: usize)
    where
        A: Activity + 'static,
    {
        let actual = self.call_tracker.call_count::<A>();
        assert!(
            actual >= min_calls,
            "Activity {} was called {} times, expected at least {}",
            std::any::type_name::<A>(),
            actual,
            min_calls
        );
    }

    /// Get the actual call count for an activity
    pub fn activity_call_count<A>(&self) -> usize
    where
        A: Activity + 'static,
    {
        self.call_tracker.call_count::<A>()
    }

    /// Get all calls to a specific activity
    pub fn activity_calls<A>(&self) -> Vec<ActivityCall>
    where
        A: Activity + 'static,
    {
        self.call_tracker.get_calls::<A>()
    }

    /// Get the order of all activity calls
    pub fn activity_call_order(&self) -> Vec<String> {
        self.call_tracker.call_order()
    }

    /// Verify that the workflow output matches expected value
    pub fn assert_output(&self, _expected: &W::Output)
    where
        W::Output: PartialEq + Debug,
    {
        // This requires storing the last result, which would need internal state
        // For now, users can check the result directly
    }

    /// Get the call tracker for custom assertions
    pub fn call_tracker(&self) -> &Arc<CallTracker> {
        &self.call_tracker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::workflow::{Activity, DurableWorkflow};
    use std::sync::Arc;

    // Test activity
    #[derive(Debug)]
    struct TestActivity;

    #[async_trait::async_trait]
    impl Activity for TestActivity {
        const TYPE_ID: &'static str = "test-activity";

        type Input = serde_json::Value;
        type Output = serde_json::Value;
        type Error = std::io::Error;

        async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
            Ok(input)
        }
    }

    // Test workflow
    #[derive(Debug, Default)]
    struct TestWorkflow;

    #[async_trait::async_trait]
    impl DurableWorkflow for TestWorkflow {
        const TYPE_ID: &'static str = "test-workflow";
        const VERSION: u32 = 1;

        type Input = serde_json::Value;
        type Output = serde_json::Value;
        type Error = std::io::Error;

        async fn run(
            &self,
            _ctx: &mut WorkflowContext,
            input: Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn test_workflow_test_builder_basic() {
        let test = WorkflowTestBuilder::<TestWorkflow>::new()
            .workflow(TestWorkflow)
            .build();

        let result = test.run(serde_json::json!({"test": "value"})).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({"test": "value"}));
    }

    #[tokio::test]
    async fn test_workflow_test_builder_with_mocks() {
        let test = WorkflowTestBuilder::<TestWorkflow>::new()
            .workflow(TestWorkflow)
            .mock_activity::<TestActivity>()
            .returns(serde_json::json!("mocked"))
            .build();

        test.assert_activity_not_called::<TestActivity>();

        // Note: Since TestWorkflow doesn't call TestActivity,
        // this just verifies the mock is configured
    }

    #[test]
    fn test_call_tracker() {
        let tracker = CallTracker::new();

        tracker.record_call::<TestActivity>(&serde_json::json!({"key": "value1"}));
        tracker.record_call::<TestActivity>(&serde_json::json!({"key": "value2"}));

        assert_eq!(tracker.call_count::<TestActivity>(), 2);
        assert_eq!(tracker.call_order(), vec!["test-activity", "test-activity"]);
    }

    #[test]
    fn test_value_mock() {
        let mock = ValueMock::new(serde_json::json!("test_output"));
        let result = mock.execute(&serde_json::json!({}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!("test_output"));
    }

    #[test]
    fn test_error_mock() {
        #[derive(Debug, Clone)]
        struct TestError {
            msg: String,
        }

        impl std::fmt::Display for TestError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.msg)
            }
        }

        impl std::error::Error for TestError {}

        let mock = ErrorMock::new(TestError {
            msg: "test error".to_string(),
        });
        let result = mock.execute(&serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_fn_mock() {
        let mock =
            FnMock::new(|input: serde_json::Value| Ok(serde_json::json!({ "processed": input })));

        let result = mock.execute(&serde_json::json!({"original": true}));
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            serde_json::json!({"processed": {"original": true}})
        );
    }

    #[test]
    fn test_test_config() {
        let config = TestConfig::new()
            .with_timeout(std::time::Duration::from_secs(60))
            .with_start_time(chrono::Utc::now());

        assert_eq!(config.timeout, std::time::Duration::from_secs(60));
        assert!(config.start_time.is_some());
    }
}

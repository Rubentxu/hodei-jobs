# Migration Guide: Hodei Jobs Saga Engine v3.0 to v4.0

This guide helps you migrate your workflows from the legacy `WorkflowDefinition` pattern to the modern `DurableWorkflow` pattern introduced in EPIC-96.

**Version:** v4.0.89  
**Release Date:** 2026-01-26

---

## Table of Contents

- [Overview](#overview)
- [Breaking Changes](#breaking-changes)
- [Migration Steps](#migration-steps)
- [Before/After Examples](#beforeafter-examples)
- [Frequently Asked Questions](#frequently-asked-questions)

---

## Overview

### What Changed?

The saga engine v4.0 introduced the **Workflow-as-Code** pattern using the `DurableWorkflow` trait, which provides:

- **Code-level workflow definitions** (no step lists or builders)
- **Type safety** with compile-time validation
- **Better error handling** with rich error context
- **Integrated compensation tracking** (automatic on workflow failure)
- **OpenTelemetry observability** out of the box

### Why Migrate?

The legacy `WorkflowDefinition` trait is **deprecated** and will be removed in v5.0:
- ❌ No automatic compensation (must be manual)
- ❌ Lacks structured error context
- ❌ Harder to test (requires mocking of step builders)
- ❌ Missing observability integration
- ✅ `DurableWorkflow` provides all these improvements

---

## Breaking Changes

### 1. Trait Signature Change

**Before (v3.0 - WorkflowDefinition):**
```rust
#[async_trait]
pub trait WorkflowDefinition {
    const TYPE_ID: &'static str;
    const CONFIGURATION: WorkflowConfiguration;
    
    type Input;
    type Output;
    type Error;
    
    async fn execute(
        &self,
        context: WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}
```

**After (v4.0 - DurableWorkflow):**
```rust
#[async_trait]
pub trait DurableWorkflow {
    const TYPE_ID: &'static str;
    const VERSION: u32;
    
    type Input;
    type Output;
    type Error;
    
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}
```

**Key Changes:**
- ✅ `execute()` → `run()` - simpler naming
- ✅ `WorkflowContext` is now **mutable** (allows state tracking)
- ✅ Added `VERSION` constant for workflow versioning
- ✅ Removed `CONFIGURATION` (configuration via `WorkflowContext` methods)
- ⚠️ **Context is now mutable** - you receive `&mut` instead of `&`

### 2. Automatic Compensation

**Before (v3.0):**
```rust
let workflow = MyWorkflow::new();
let output = workflow.execute(&context, input).await?;

// If workflow fails, you MUST manually compensate:
if let Err(e) = &output {
    context.compensate_all().await?;  // Manual!
}
```

**After (v4.0):**
```rust
let workflow = MyWorkflow::new();
let output = workflow.run(&mut context, input).await?;

// Automatic compensation on error!
// No need for manual compensation - it's handled internally
```

**Impact:**
- ✅ **Less boilerplate** - no more manual compensation handling
- ✅ **Correct order** - compensation always LIFO as designed
- ✅ **Always executed** - even if you forget to handle errors

### 3. Error Handling

**Before (v3.0):**
```rust
// Errors returned as boxed trait objects
pub type WorkflowResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Minimal error information
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Execution failed: {0}")]
    Failed(String),
}
```

**After (v4.0):**
```rust
// Rich error context with activity details
#[derive(Debug, thiserror::Error)]
pub enum ActivityExecutionError {
    #[error("Activity '{activity_type}' failed: {message}")]
    ExecutionFailed {
        activity_type: &'static str,
        activity_id: String,
        saga_id: SagaId,
        input: String,
        source_error: anyhow::Error,
    },
    
    #[error("Activity '{activity_type}' timed out after {timeout}ms")]
    Timeout {
        activity_type: &'static str,
        activity_id: String,
        timeout: Duration,
    },
    
    #[error("Activity '{activity_type}' was cancelled: {reason}")]
    Cancelled {
        activity_type: &'static str,
        activity_id: String,
        reason: String,
    },
}
```

**Impact:**
- ✅ **Better debugging** - activity_id, saga_id, input captured automatically
- ✅ **Easier error tracking** - timeouts vs failures clearly distinguished
- ✅ **Automatic compensation context** - errors contain all necessary information

### 4. Workflow Context Methods

**Before (v3.0):**
```rust
context.set_workflow_state("processing")
  .track_compensation::<MyActivity, MyCompensation>("cleanup-1", |ctx| {
      ctx.execute_activity(MyActivity::new(), input).await
  });
```

**After (v4.0):**
```rust
// Same methods, but integrated with compensation tracker
context.set_workflow_state("processing")
  .track_compensation::<MyActivity, MyCompensation>("cleanup-1", |ctx| {
      ctx.execute_activity(MyActivity::new(), input).await
  });

// Compensation now tracked automatically
// No need for manual compensation list management
```

### 5. Removed APIs

**These APIs are deprecated and will be removed in v5.0:**

- ❌ `WorkflowExecutor<W: WorkflowDefinition>` struct - use `DurableWorkflow` directly with `SagaEngine`
- ❌ `WorkflowDefinition::configuration()` method - configuration via `WorkflowContext::configure()`
- ❌ `WorkflowDefinition::steps()` method - steps are defined in code, not via trait

---

## Migration Steps

### Step 1: Update Trait Implementation

Change your trait from `WorkflowDefinition` to `DurableWorkflow`:

```rust
// Before
#[async_trait]
impl WorkflowDefinition for MyWorkflow {
    const TYPE_ID: &'static str = "my-workflow";
    const CONFIGURATION: WorkflowConfiguration = WorkflowConfiguration::default();
    type Input = MyInput;
    type Output = MyOutput;
    type Error = MyError;
    
    async fn execute(
        &self,
        context: WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Legacy workflow logic
    }
}

// After
#[async_trait]
impl DurableWorkflow for MyWorkflow {
    const TYPE_ID: &'static str = "my-workflow";
    const VERSION: u32 = 1;  // Version your workflow
    type Input = MyInput;
    type Output = MyOutput;
    type Error = MyError;
    
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // Modern workflow logic
    }
}
```

**Action:**
- Add `VERSION` constant
- Change `execute` → `run`
- Change `&self, context: WorkflowContext` → `&self, context: &mut WorkflowContext`
- Remove `CONFIGURATION` if present

---

### Step 2: Update Workflow Execution

**Before (v3.0 - using WorkflowExecutor):**
```rust
use saga_engine_core::workflow::{WorkflowExecutor, WorkflowDefinition};

let executor = WorkflowExecutor::new(workflow);
let result = executor.execute(&context, input).await?;
```

**After (v4.0 - using SagaEngine):**
```rust
use saga_engine_core::saga_engine::SagaEngine;

// Option 1: Execute as a one-shot workflow
let result = saga_engine
    .start_workflow::<MyWorkflow>(input)
    .await?;

// Option 2: Execute within saga context (recommended)
let saga_id = SagaId::new();
saga_engine
    .start_workflow::<MyWorkflow>(saga_id, input)
    .await?;
```

**Action:**
- Replace `WorkflowExecutor` with `SagaEngine`
- Use `start_workflow()` or `start_workflow_with_id()`
- Remove any executor wrapping code

---

### Step 3: Remove Manual Compensation

**Before (v3.0):**
```rust
#[async_trait]
impl WorkflowDefinition for OrderWorkflow {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: OrderInput,
    ) -> Result<OrderOutput, WorkflowError> {
        // Execute steps
        context.execute_activity(CreateOrderActivity::new(), input.create).await?;
        context.execute_activity(ValidatePaymentActivity::new(), input.payment).await?;
        context.execute_activity(ConfirmOrderActivity::new(), input.confirm).await?;
        
        // Manual compensation handling
        match &output {
            Ok(_) => {
                context.clear_compensations(); // Manual!
            },
            Err(e) => {
                context.compensate_all().await?; // Manual!
                return Err(e);
            }
        }
    }
}
```

**After (v4.0):**
```rust
#[async_trait]
impl DurableWorkflow for OrderWorkflow {
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: OrderInput,
    ) -> Result<OrderOutput, WorkflowError> {
        // Execute steps
        context.execute_activity(CreateOrderActivity::new(), input.create).await?;
        context.execute_activity(ValidatePaymentActivity::new(), input.payment).await?;
        context.execute_activity(ConfirmOrderActivity::new(), input.confirm).await?;
        
        // No manual compensation needed!
        // Compensation is automatic on error
        // Just return the result
        Ok(OrderOutput { order_id: input.order_id })
    }
}
```

**Action:**
- Remove `context.clear_compensations()` calls
- Remove `context.compensate_all()` calls
- Remove `match` statements that handle compensation
- **Automatic compensation is now handled by the saga engine**

---

### Step 4: Update Activity Error Handling

**Before (v3.0):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum MyActivityError {
    #[error("Activity failed")]
    Failed(String),
}

#[async_trait]
impl Activity for CreateOrderActivity {
    const TYPE_ID: &'static str = "create-order";
    type Input = CreateInput;
    type Output = CreateOutput;
    type Error = MyActivityError;
    
    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // ...activity logic...
        
        // Return minimal error
        Err(MyActivityError::Failed("Create failed".to_string()))
    }
}
```

**After (v4.0):**
```rust
use saga_engine_core::error::ActivityExecutionError;

#[async_trait]
impl Activity for CreateOrderActivity {
    const TYPE_ID: &'static str = "create-order";
    type Input = CreateInput;
    type Output = CreateOutput;
    
    // Use rich error context - or custom error
    type Error = ActivityExecutionError;  // Or implement your own
    
    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // ...activity logic...
        
        // Return rich error with context
        Err(ActivityExecutionError::ExecutionFailed {
            activity_type: Self::TYPE_ID,
            activity_id: "activity-123".to_string(),
            saga_id: self.saga_id().unwrap(),
            input: format!("{:?}", input),
            source_error: anyhow::anyhow!("Create failed"),
        })
    }
}
```

**Action:**
- Use `ActivityExecutionError` or implement rich error types
- Include `activity_type`, `activity_id`, `saga_id`, `input`, `source_error`
- Avoid minimal string-only errors

---

### Step 5: Update Workflow Context Configuration

**Before (v3.0):**
```rust
// Configuration via CONFIGURATION constant
const CONFIGURATION: WorkflowConfiguration = WorkflowConfiguration {
    max_step_retries: 3,
    timeout_ms: 30000,
};

// Used in workflow
config.max_step_retries();
```

**After (v4.0):**
```rust
// Configuration via context methods
context
    .configure(|config| {
        config
            .max_step_retries(3)
            .timeout_ms(30000)
    })?;

// Configuration used automatically
let config = context.configuration();
config.max_step_retries();
```

**Action:**
- Use `context.configure()` with closure
- Access configuration via `context.configuration()`
- Remove `CONFIGURATION` constant

---

### Step 6: Update Tests

**Before (v3.0):**
```rust
#[tokio::test]
async fn test_order_workflow() {
    let mut context = WorkflowContext::new();
    let workflow = OrderWorkflow::new();
    let input = OrderInput::default();
    
    let result = workflow.execute(&context, input).await;
    assert!(result.is_ok());
}
```

**After (v4.0):**
```rust
#[tokio::test]
async fn test_order_workflow() {
    let mut context = WorkflowContext::new();
    let workflow = OrderWorkflow::new();
    let input = OrderInput::default();
    
    let result = workflow.run(&mut context, input).await;
    assert!(result.is_ok());
    
    // Test automatic compensation
    let failing_input = OrderInput {
        fail_on: "confirm",
        ..Default::default()
    };
    let failing_result = workflow.run(&mut context, failing_input).await;
    assert!(failing_result.is_err());
}
```

**Action:**
- Update `execute()` calls to `run()`
- Update `&context` to `&mut context`
- Add tests for automatic compensation behavior
- Test that compensation triggers on workflow failure

---

## Before/After Examples

### Example 1: Simple Order Workflow

**Before (v3.0 - WorkflowDefinition):**
```rust
pub struct OrderWorkflow;

impl OrderWorkflow {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WorkflowDefinition for OrderWorkflow {
    const TYPE_ID: &'static str = "order-workflow";
    const CONFIGURATION: WorkflowConfiguration = WorkflowConfiguration {
        max_step_retries: 3,
    };
    
    type Input = OrderInput;
    type Output = OrderOutput;
    type Error = WorkflowError;
    
    async fn execute(
        &self,
        context: WorkflowContext,
        input: OrderInput,
    ) -> Result<Self::Output, Self::Error> {
        // Step 1: Create order
        context.execute_activity(CreateOrderActivity::new(), input.create).await?;
        
        // Step 2: Validate payment
        context.execute_activity(ValidatePaymentActivity::new(), input.payment).await?;
        
        // Step 3: Confirm order
        context.execute_activity(ConfirmOrderActivity::new(), input.confirm).await?;
        
        // Manual compensation handling
        match &output {
            Ok(_) => {
                context.clear_compensations(); // Manual clear
                Ok(OrderOutput { order_id: input.order_id })
            },
            Err(e) => {
                context.compensate_all().await?; // Manual compensate
                Err(e)
            }
        }
    }
}
```

**After (v4.0 - DurableWorkflow):**
```rust
pub struct OrderWorkflow;

impl OrderWorkflow {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DurableWorkflow for OrderWorkflow {
    const TYPE_ID: &'static str = "order-workflow";
    const VERSION: u32 = 1;
    
    type Input = OrderInput;
    type Output = OrderOutput;
    type Error = WorkflowError;
    
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: OrderInput,
    ) -> Result<Self::Output, Self::Error> {
        // Step 1: Create order
        context.execute_activity(CreateOrderActivity::new(), input.create).await?;
        
        // Step 2: Validate payment
        context.execute_activity(ValidatePaymentActivity::new(), input.payment).await?;
        
        // Step 3: Confirm order
        context.execute_activity(ConfirmOrderActivity::new(), input.confirm).await?;
        
        // No manual compensation needed!
        // Automatic compensation is handled by the saga engine
        Ok(OrderOutput { order_id: input.order_id })
    }
}
```

**Key Changes:**
- ✅ Removed `CONFIGURATION` constant
- ✅ Added `VERSION` constant
- ✅ Changed `execute()` to `run()`
- ✅ Changed `&context` to `&mut context`
- ✅ Removed manual compensation (`clear_compensations`, `compensate_all`)
- ✅ Cleaner, simpler code

---

### Example 2: Workflow with Error Handling

**Before (v3.0):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum OrderWorkflowError {
    #[error("Order processing failed")]
    Failed(String),
}

#[async_trait]
impl DurableWorkflow for OrderWorkflow {
    // ...
    
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: OrderInput,
    ) -> Result<Self::Output, Self::Error> {
        // ...
        
        // Manual error handling with compensation
        match context.execute_activity(ProcessPaymentActivity::new(), input.payment).await {
            Ok(_) => Ok(OrderOutput { order_id: input.order_id }),
            Err(e) => {
                context.compensate_all().await?;
                Err(OrderWorkflowError::Failed(e.to_string()))
            }
        }
    }
}
```

**After (v4.0):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum OrderWorkflowError {
    #[error("Payment processing failed: {0}")]
    PaymentFailed(String, u32),  // Rich error with attempt count
    #[error("Order validation failed: {0}")]
    ValidationFailed(String),
}

#[async_trait]
impl DurableWorkflow for OrderWorkflow {
    // ...
    
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: OrderInput,
    ) -> Result<Self::Output, Self::Error> {
        // ...
        
        // No manual compensation needed!
        // Just execute the activity
        context.execute_activity(ProcessPaymentActivity::new(), input.payment).await?;
        
        Ok(OrderOutput { order_id: input.order_id })
    }
}
```

**Key Changes:**
- ✅ Rich error types with context
- ✅ No manual compensation handling
- ✅ Simpler control flow
- ✅ Better error messages for debugging

---

### Example 3: Using SagaEngine Directly

**Before (v3.0 - WorkflowExecutor):**
```rust
use saga_engine_core::workflow::{WorkflowExecutor, WorkflowDefinition};

pub async fn execute_order(input: OrderInput) -> anyhow::Result<OrderOutput> {
    let context = WorkflowContext::new();
    let workflow = OrderWorkflow::new();
    let executor = WorkflowExecutor::new(workflow);
    
    executor.execute(&context, input).await
        .map_err(|e| anyhow::anyhow!("Workflow failed: {}", e))
}
```

**After (v4.0 - SagaEngine):**
```rust
use saga_engine_core::saga_engine::SagaEngine;

pub async fn execute_order(input: OrderInput) -> anyhow::Result<OrderOutput> {
    let saga_id = SagaId::new();
    let engine = PostgresSagaEngine::new(&pool).await?;
    
    let result = engine
        .start_workflow::<OrderWorkflow>(saga_id, input)
        .await?;
    
    result.map(|output| OrderOutput {
        order_id: output.order_id,
    })
}
```

**Key Changes:**
- ✅ Direct saga engine usage
- ✅ Automatic event sourcing
- ✅ Better observability (spans, metrics)
- ✅ Durable replay support

---

## Frequently Asked Questions

### Q: Do I need to manually compensate anymore?

**A:** No! The `DurableWorkflow` trait has built-in automatic compensation. When your workflow returns an error, the saga engine automatically executes all tracked compensations in LIFO order.

### Q: What happens to my existing tests?

**A:** Tests that use `execute()` will need to be updated to use `run()`. The test structure remains the same, but you'll need to change `&context` to `&mut context` if your test modifies the context.

### Q: Can I still use WorkflowExecutor?

**A:** No, `WorkflowExecutor` is deprecated. Use `SagaEngine` directly:
- `engine.start_workflow::<W>(input)` for one-shot workflows
- `engine.start_workflow_with_id::<W>(saga_id, input)` for workflows with a known ID

### Q: How do I access workflow configuration now?

**A:** Use the `WorkflowContext::configure()` method:
```rust
context.configure(|config| {
    config
        .timeout_ms(30000)
        .max_step_retries(3)
})?;
```

Access configuration via `context.configuration()`.

### Q: What if I need different compensation logic per activity?

**A:** Use the `WorkflowContext::track_compensation()` method to register custom compensation activities with their own compensation logic:
```rust
context.track_compensation::<CreateOrderActivity, RefundPaymentActivity>("refund-order", |ctx| {
    if ctx.input.payment_method == "credit_card" {
        ctx.execute_activity(RefundPaymentActivity::new(), input).await
    } else if ctx.input.payment_method == "paypal" {
        // PayPal doesn't support automatic refunds
        ctx.execute_activity(NotifyCustomerActivity::new(), input).await
    }
});
```

### Q: Are there performance improvements?

**A:** Yes! EPIC-96 provides:
- **20x throughput** for event appends (batching)
- **10-50x replay speedup** (reset points)
- **<5ms notification latency** (reactive vs polling)
- Run `cargo bench --bench saga_engine_benchmarks` to validate

### Q: Can I migrate incrementally?

**A:** Yes! Both `WorkflowDefinition` and `DurableWorkflow` coexist in v4.0. Migrate one workflow at a time:
1. Update trait implementation
2. Update calls
3. Update tests
4. Remove when fully migrated

### Q: What about my custom workflow executors?

**A:** If you've created custom executors wrapping `WorkflowExecutor`, they should be removed. The `SagaEngine` provides better functionality:
- Automatic event sourcing
- Built-in retry logic
- Observability integration
- Compensation tracking

---

## Testing Your Migration

After migrating, verify your workflow works correctly:

```bash
# Run unit tests
cargo test --package saga-engine-core --lib

# Run benchmarks
cargo bench --bench saga_engine_benchmarks

# Run integration tests
cargo test --package hodei-server-application --lib
```

### Checklist

- [ ] All workflow trait implementations updated to `DurableWorkflow`
- [ ] All `execute()` calls changed to `run()`
- [ ] All `&context` changed to `&mut context`
- [ ] Manual compensation code removed
- [ ] Error types updated with rich context
- [ ] Tests updated and passing
- [ ] Benchmarks run successfully
- [ ] No compilation warnings

---

## Need Help?

- **Documentation**: See [EPIC-96](../epics/EPIC-96-saga-engine-v4-optimization.md)
- **Examples**: Check `crates/server/application/src/saga/workflows/` for production examples
- **Issues**: Report bugs or ask questions in [GitHub Issues](https://github.com/hodei-team/hodei-jobs/issues)

---

**Happy migrating!** 🚀

The `DurableWorkflow` pattern provides better ergonomics, safety, and observability. Take your time with the migration, and let us know if you encounter any issues.

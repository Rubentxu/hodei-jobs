# Saga Engine V4 - Appendix: E-Commerce Example

## Complete Tutorial: Building an Online Sales System with Saga Engine

---

> **What is this document?**
> This appendix presents a **complete online sales system** implemented with Saga Engine V4. Unlike other manuals that explain theoretical concepts, here you'll see how to apply all those concepts in a real, concrete case.
>
> **What you'll learn**:
> - How to model an e-commerce domain with DDD
> - How to design cohesive events, commands, and activities
> - How to implement the compensation pattern in a real case
> - How to structure code to avoid duplications
> - Multiple perspectives: API, Admin, CLI

---

## Table of Contents

1. [Introduction: The Use Case](#1-introduction-the-use-case)
2. [Domain Context (DDD)](#2-domain-context-ddd)
3. [Unified Events Catalog](#3-unified-events-catalog)
4. [Commands: The Domain User Interface](#4-commands-the-domain-user-interface)
5. [Activities: The Building Blocks](#5-activities-the-building-blocks)
6. [Order Processing Workflow: The Complete Saga](#6-order-processing-workflow-the-complete-saga)
7. [Command Handlers: Decoupling](#7-command-handlers-decoupling)
8. [Multiple Perspectives](#8-multiple-perspectives)
9. [Optimization Summary](#9-optimization-summary)
10. [Complete Reference Code](#10-complete-reference-code)

---

## 1. Introduction: The Use Case

### 1.1 Scenario: Online Order Processing

Imagine you're building an e-commerce system. When a customer makes a purchase, the system must:

```mermaid
flowchart LR
    subgraph "Order flow"
        A[Customer<br/>selects products] --> B[Shopping<br/>Cart]
        B --> C[Checkout<br/>with payment]
        C --> D[System<br/>processes order]
        D --> E[Inventory<br/>reserved]
        E --> F[Payment<br/>charged]
        F --> G[Shipping<br/>scheduled]
        G --> H[Customer<br/>receives product]
    end
```

**The challenge**: Each step can fail, and if one fails after others have succeeded, we need to revert changes.

### 1.2 Design Objectives

| Objective | Description | Technique |
|-----------|-------------|-----------|
| **DRY** | No code repetition | Generic traits, composition |
| **Cohesion** | Each component has one clear responsibility | DDD, Bounded Contexts |
| **Compensation** | Automatic rollback | Compensation Pattern |
| **Testability** | Easy to test | Ports and adapters |

### 1.3 System Architecture

```mermaid
graph TB
    subgraph "E-Commerce Platform"
        
        subgraph "Sales Context"
            OC[Order Context]
            PC[Payment Context]
            IC[Inventory Context]
            SC[Shipping Context]
        end
        
        subgraph "Customer Context"
            CC[Customer Context]
            AC[Address Context]
        end
        
        subgraph "Product Context"
            PRC[Product Context]
        end
    end
    
    OC --> PC : "payments"
    OC --> IC : "inventory"
    OC --> SC : "shipping"
    PC --> CC : "customer"
    IC --> PRC : "products"
    
    style OC fill:#e3f2fd
    style PC fill:#f3e5f5
    style IC fill:#e8f5e8
    style SC fill:#fff3e0
```

---

## 2. Domain Context (DDD)

### 2.1 E-Commerce Bounded Contexts

In DDD, a **Bounded Context** is a boundary within which a consistent domain model exists. Each context has its own "language" (ubiquitous language).

```mermaid
graph TB
    subgraph "Sales Bounded Context (The King)"
        O[Order<br/>"An order has items and a total"]
        P[Payment<br/>"A payment has status and amount"]
    end
    
    subgraph "Inventory Bounded Context"
        I[Inventory<br/>"Inventory has available stock"]
        R[Reservation<br/>"A reservation blocks stock for time"]
    end
    
    subgraph "Shipping Bounded Context"
        S[Shipment<br/>"A shipment has tracking number"]
        C[Carrier<br/>"The carrier delivers the package"]
    end
    
    O --> P : "processes"
    O --> I : "reserves"
    O --> S : "coordinates"
    
    style O fill:#e3f2fd
    style I fill:#e8f5e8
    style S fill:#fff3e0
```

### 2.2 Domain Aggregates

```mermaid
classDiagram
    class Order {
        <<aggregate root>>
        +OrderId id
        +CustomerId customer_id
        +OrderStatus status
        +Vec~OrderItem~ items
        +Money total
        +Address shipping_address
        +PaymentInfo payment_info
        +start()
        +add_item()
        +confirm()
        +cancel()
    }
    
    class OrderItem {
        +ProductId product_id
        +u32 quantity
        +Money unit_price
        +Money subtotal
    }
    
    class Payment {
        <<value object>>
        +PaymentMethod method
        +Money amount
        +TransactionId transaction_id
        +PaymentStatus status
    }
    
    class InventoryReservation {
        <<value object>>
        +ReservationId id
        +ProductId product_id
        +u32 quantity
        +ExpiryDate expires_at
    }
    
    class Shipment {
        <<value object>>
        +ShipmentId id
        +TrackingNumber tracking
        +Carrier carrier
        +Address destination
        +ShipmentStatus status
    }
    
    Order "1" --> "*" OrderItem : contains
    Order "1" --> "1" Payment : has
    Order "1" --> "*" InventoryReservation : reserves
    Order "1" --> "1" Shipment : ships to
```

### 2.3 Ubiquitous Language

| Term | Context | Definition |
|------|---------|------------|
| **Order** | Sales | The complete order with all items |
| **Payment** | Sales/Payment | The charge to the customer |
| **Reservation** | Inventory | Temporary stock blocking |
| **Shipment** | Shipping | Physical delivery to customer |

---

## 3. Unified Events Catalog

### 3.1 Event Design Principles

Events represent **something that already happened**. They're named in past tense and contain all necessary information.

```mermaid
flowchart LR
    subgraph "Command (verb)"
        C[CreateOrder<br/>"Create an order"]
    end
    
    subgraph "Event (past)"
        E[OrderCreated<br/>"The order was created"]
    end
    
    C --> E
```

### 3.2 Base Event Structure

```rust
// ═══════════════════════════════════════════════════════════════════════
// BASE PAYLOAD - Reusable across the system
// ═══════════════════════════════════════════════════════════════════════

/// Common metadata for all events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseEventPayload {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub correlation_id: Option<String>,  // For tracing
    pub trace_id: Option<String>,
    pub source: String,  // Who generated the event
}

impl Default for BaseEventPayload {
    fn default() -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            correlation_id: None,
            trace_id: None,
            source: "ecommerce".to_string(),
        }
    }
}
```

### 3.3 Generic Event Types (DomainEventType)

These events are reusable in any domain:

```rust
// ═══════════════════════════════════════════════════════════════════════
// SHARED EVENT TYPES - Reusable across the system
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainEventType {
    // Lifecycle states
    Created,
    Updated,
    Deleted,
    Activated,
    Deactivated,
    
    // Process states
    Started,
    Completed,
    Failed,
    Cancelled,
    
    // Time events
    Expired,
    TimeoutReached,
    
    // Integration events
    IntegrationStarted,
    IntegrationCompleted,
    IntegrationFailed,
}

impl DomainEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
            Self::Activated => "activated",
            Self::Deactivated => "deactivated",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::TimeoutReached => "timeout_reached",
            Self::IntegrationStarted => "integration_started",
            Self::IntegrationCompleted => "integration_completed",
            Self::IntegrationFailed => "integration_failed",
        }
    }
}
```

### 3.4 E-Commerce Specific Events

```rust
// ═══════════════════════════════════════════════════════════════════════
// ORDER EVENTS - Specialized but coherent
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEventType {
    OrderCreated,
    ItemAdded,
    ItemRemoved,
    OrderConfirmed,
    OrderShipped,
    OrderDelivered,
    OrderCompleted,
    OrderFailed,
    OrderCancelled,
    PaymentInitiated,
    PaymentCompleted,
    PaymentFailed,
    InventoryReserved,
    InventoryReservationFailed,
    InventoryReleased,
    ShippingScheduled,
    ShippingCompleted,
    RefundInitiated,
    RefundCompleted,
    RefundFailed,
}

impl OrderEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OrderCreated => "order_created",
            Self::ItemAdded => "item_added",
            Self::ItemRemoved => "item_removed",
            Self::OrderConfirmed => "order_confirmed",
            Self::OrderShipped => "order_shipped",
            Self::OrderDelivered => "order_delivered",
            Self::OrderCompleted => "order_completed",
            Self::OrderFailed => "order_failed",
            Self::OrderCancelled => "order_cancelled",
            Self::PaymentInitiated => "payment_initiated",
            Self::PaymentCompleted => "payment_completed",
            Self::PaymentFailed => "payment_failed",
            Self::InventoryReserved => "inventory_reserved",
            Self::InventoryReservationFailed => "inventory_reservation_failed",
            Self::InventoryReleased => "inventory_released",
            Self::ShippingScheduled => "shipping_scheduled",
            Self::ShippingCompleted => "shipping_completed",
            Self::RefundInitiated => "refund_initiated",
            Self::RefundCompleted => "refund_completed",
            Self::RefundFailed => "refund_failed",
        }
    }
}
```

### 3.5 Type-Safe Payloads

```rust
// ═══════════════════════════════════════════════════════════════════════
// ORDER PAYLOADS - Type-safe payloads
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCreatedPayload {
    pub order_id: String,
    pub customer_id: String,
    pub item_count: usize,
    pub total_amount: Decimal,
    pub currency: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentCompletedPayload {
    pub order_id: String,
    pub transaction_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub payment_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReservedPayload {
    pub order_id: String,
    pub reservations: Vec<ReservationDetail>,
    pub total_items: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservationDetail {
    pub product_id: String,
    pub reserved_quantity: u32,
    pub reservation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingScheduledPayload {
    pub order_id: String,
    pub shipment_id: String,
    pub carrier: String,
    pub tracking_number: String,
    pub estimated_delivery: chrono::DateTime<chrono::Utc>,
}
```

### 3.6 Compensation Payloads

```rust
// ═══════════════════════════════════════════════════════════════════════
// COMPENSATION PAYLOADS - Unified structure
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationActionPayload {
    pub action_type: String,
    pub target_entity_id: String,
    pub original_data: Value,
    pub compensation_reason: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub result: CompensationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompensationResult {
    Success,
    Failed { error: String },
    Skipped { reason: String },
}
```

---

## 4. Commands: The Domain User Interface

### 4.1 What is a Command?

A **Command** represents **something we want to happen**. Unlike events (past), commands are intention (future).

```mermaid
flowchart LR
    subgraph "API Layer"
        API[REST API<br/>POST /orders]
    end
    
    subgraph "Command"
        CMD[CreateOrderCommand<br/>"I want to create an order"]
    end
    
    subgraph "Domain"
        E[OrderCreated Event<br/>"The order was created"]
    end
    
    API --> CMD
    CMD --> E
```

### 4.2 Command Bus Architecture

```rust
// ═══════════════════════════════════════════════════════════════════════
// COMMAND BUS - Framework agnostic
// ═══════════════════════════════════════════════════════════════════════

/// Base trait for all commands
pub trait Command: Send + Sync + Debug {
    type Result;
    fn command_type(&self) -> &'static str;
    fn aggregate_id(&self) -> &str;
}

/// Generic command result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult<T = ()> {
    Success(T),
    Failure(String),
    ValidationError(Vec<ValidationError>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}

/// Base handler with generic typing
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    async fn handle(&self, command: C) -> CommandResult<C::Result>;
}
```

### 4.3 Order Creation Command

```rust
// ═══════════════════════════════════════════════════════════════════════
// ORDER COMMANDS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderCommand {
    pub customer_id: String,
    pub items: Vec<OrderItemInput>,
    pub shipping_address: AddressInput,
    pub payment_method: PaymentMethodInput,
    pub idempotency_key: Option<String>,
}

impl Command for CreateOrderCommand {
    type Result = String; // order_id
    fn command_type(&self) -> &'static str {
        "order.create"
    }
    fn aggregate_id(&self) -> &str {
        &self.customer_id
    }
}

// Typed inputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemInput {
    pub product_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInput {
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodInput {
    pub method: String,
    pub token: String,
}
```

### 4.4 Payment Command

```rust
// ═══════════════════════════════════════════════════════════════════════
// PAYMENT COMMANDS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPaymentCommand {
    pub order_id: String,
    pub customer_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub payment_token: String,
    pub metadata: Value,
}

impl Command for ProcessPaymentCommand {
    type Result = PaymentResult;
    fn command_type(&self) -> &'static str {
        "payment.process"
    }
    fn aggregate_id(&self) -> &str {
        &self.order_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentResult {
    pub transaction_id: String,
    pub status: PaymentStatus,
    pub approved_amount: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaymentStatus {
    Approved,
    Declined,
    Failed,
    Pending,
    Refunded,
}
```

### 4.5 Inventory Command

```rust
// ═══════════════════════════════════════════════════════════════════════
// INVENTORY COMMANDS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveInventoryCommand {
    pub order_id: String,
    pub items: Vec<InventoryReservationInput>,
    pub expires_in_seconds: u32,
}

impl Command for ReserveInventoryCommand {
    type Result = InventoryReservationResult;
    fn command_type(&self) -> &'static str {
        "inventory.reserve"
    }
    fn aggregate_id(&self) -> &str {
        &self.order_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReservationInput {
    pub product_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReservationResult {
    pub reservation_id: String,
    pub reservations: Vec<ReservationOutcome>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservationOutcome {
    pub product_id: String,
    pub reserved_quantity: u32,
    pub success: bool,
    pub error: Option<String>,
}
```

---

## 5. Activities: The Building Blocks

### 5.1 What is an Activity?

An **Activity** is the smallest unit of work. It's an atomic operation that can execute independently.

```mermaid
flowchart LR
    subgraph "Activity"
        I[Input] --> A[execute]
        A -->|Success| O[Output]
        A -->|Error| E[Error]
    end
```

### 5.2 Extended Activity Trait for E-Commerce

```rust
// ═══════════════════════════════════════════════════════════════════════
// ACTIVITY TRAIT - Extended for e-commerce domain
// ═══════════════════════════════════════════════════════════════════════

/// Base activity with compensation capabilities
#[async_trait]
pub trait EcommerceActivity<I: Serialize + Deserialize<'static> + Send + Clone, O: Serialize + Deserialize<'static> + Send + Clone>: Activity {
    /// Name of the compensation activity (if exists)
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    /// Check if activity can be compensated
    fn can_compensate(&self, _output: &O) -> bool {
        true
    }
    
    /// Prepare data for compensation
    fn prepare_compensation_data(&self, _output: &O) -> Option<serde_json::Value> {
        None
    }
}
```

### 5.3 Payment Activity

```rust
// ═══════════════════════════════════════════════════════════════════════
// PAYMENT ACTIVITIES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ProcessPaymentActivity;

#[async_trait]
impl EcommerceActivity<ProcessPaymentInput, PaymentOutput> for ProcessPaymentActivity {
    const TYPE_ID: &'static str = "payment.process";
    
    /// ⭐ This activity can be compensated with a refund
    const COMPENSATION_ACTIVITY: Option<&'static str> = Some("payment.refund");
    
    async fn execute(&self, input: ProcessPaymentInput) -> Result<PaymentOutput, PaymentError> {
        // ═══════════════════════════════════════════════════════════════
        // REAL LOGIC: Integrate with payment gateway here
        // ═══════════════════════════════════════════════════════════════
        
        let transaction_id = format!("txn_{}", uuid::Uuid::new_v4());
        
        Ok(PaymentOutput {
            transaction_id,
            status: PaymentStatus::Approved,
            approved_amount: input.amount,
            processed_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPaymentInput {
    pub order_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub payment_token: String,
    pub customer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentOutput {
    pub transaction_id: String,
    pub status: PaymentStatus,
    pub approved_amount: Decimal,
    pub processed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum PaymentError {
    #[error("Payment declined: {0}")]
    Declined(String),
    #[error("Gateway error: {0}")]
    GatewayError(String),
    #[error("Invalid token: {0}")]
    InvalidToken(String),
}
```

### 5.4 Refund Activity (Compensation)

```rust
// ═══════════════════════════════════════════════════════════════════════
// REFUND ACTIVITY - Payment compensation
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct RefundPaymentActivity;

#[async_trait]
impl EcommerceActivity<RefundInput, RefundOutput> for RefundPaymentActivity {
    const TYPE_ID: &'static str = "payment.refund";
    /// ⭐ Refunds don't have compensation (we don't refund refunds)
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    async fn execute(&self, input: RefundInput) -> Result<RefundOutput, RefundError> {
        Ok(RefundOutput {
            refund_id: format!("ref_{}", uuid::Uuid::new_v4()),
            original_transaction_id: input.original_transaction_id,
            amount: input.amount,
            status: RefundStatus::Completed,
            processed_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundInput {
    pub original_transaction_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundOutput {
    pub refund_id: String,
    pub original_transaction_id: String,
    pub amount: Decimal,
    pub status: RefundStatus,
    pub processed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefundStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum RefundError {
    #[error("Original transaction not found: {0}")]
    TransactionNotFound(String),
    #[error("Refund failed: {0}")]
    RefundFailed(String),
}
```

### 5.5 Inventory Activity

```rust
// ═══════════════════════════════════════════════════════════════════════
// INVENTORY ACTIVITIES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ReserveInventoryActivity;

#[async_trait]
impl EcommerceActivity<ReserveInventoryInput, ReserveInventoryOutput> for ReserveInventoryActivity {
    const TYPE_ID: &'static str = "inventory.reserve";
    
    /// ⭐ This activity can be compensated by releasing the reservation
    const COMPENSATION_ACTIVITY: Option<&'static str> = Some("inventory.release");
    
    async fn execute(&self, input: ReserveInventoryInput) -> Result<ReserveInventoryOutput, InventoryError> {
        let reservations = self.reserve_items(&input.items, input.expires_in_seconds).await?;
        
        Ok(ReserveInventoryOutput {
            reservation_id: format!("res_{}", uuid::Uuid::new_v4()),
            reservations,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(input.expires_in_seconds as i64),
        })
    }
    
    fn prepare_compensation_data(&self, output: &ReserveInventoryOutput) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "reservation_id": output.reservation_id,
            "reservations": output.reservations,
        }))
    }
    
    async fn reserve_items(&self, items: &[InventoryReservationInput], _expires_secs: u32) 
        -> Result<Vec<ReservationOutcome>, InventoryError> {
        Ok(items.iter().map(|item| ReservationOutcome {
            product_id: item.product_id.clone(),
            reserved_quantity: item.quantity,
            success: true,
            error: None,
        }).collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveInventoryInput {
    pub order_id: String,
    pub items: Vec<InventoryReservationInput>,
    pub expires_in_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveInventoryOutput {
    pub reservation_id: String,
    pub reservations: Vec<ReservationOutcome>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservationOutcome {
    pub product_id: String,
    pub reserved_quantity: u32,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InventoryError {
    #[error("Insufficient stock for product {0}: available={1}, requested={2}")]
    InsufficientStock(String, u32, u32),
    #[error("Product not found: {0}")]
    ProductNotFound(String),
    #[error("Reservation failed: {0}")]
    ReservationFailed(String),
}
```

### 5.6 Shipping Activity

```rust
// ═══════════════════════════════════════════════════════════════════════
// SHIPPING ACTIVITIES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ScheduleShippingActivity;

#[async_trait]
impl EcommerceActivity<ScheduleShippingInput, ScheduleShippingOutput> for ScheduleShippingActivity {
    const TYPE_ID: &'static str = "shipping.schedule";
    
    /// ⭐ Shipping can be cancelled
    const COMPENSATION_ACTIVITY: Option<&'static str> = Some("shipping.cancel");
    
    async fn execute(&self, input: ScheduleShippingInput) -> Result<ScheduleShippingOutput, ShippingError> {
        Ok(ScheduleShippingOutput {
            shipment_id: format!("ship_{}", uuid::Uuid::new_v4()),
            tracking_number: format!("TRK{}", uuid::Uuid::new_v4().to_string().chars().take(12).collect::<String>()),
            carrier: input.preferences.carrier.unwrap_or_else(|| "default".to_string()),
            estimated_delivery: chrono::Utc::now() + chrono::Duration::days(5),
            label_url: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleShippingInput {
    pub order_id: String,
    pub address: AddressInput,
    pub items: Vec<ShippingItemInput>,
    pub preferences: ShippingPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingItemInput {
    pub product_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingPreferences {
    pub carrier: Option<String>,
    pub express: bool,
    pub signature_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleShippingOutput {
    pub shipment_id: String,
    pub tracking_number: String,
    pub carrier: String,
    pub estimated_delivery: chrono::DateTime<chrono::Utc>,
    pub label_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShippingError {
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Carrier unavailable: {0}")]
    CarrierUnavailable(String),
    #[error("Shipping failed: {0}")]
    ShippingFailed(String),
}
```

---

## 6. Order Processing Workflow: The Complete Saga

### 6.1 Workflow Definition

```rust
// ═══════════════════════════════════════════════════════════════════════
// ORDER WORKFLOW - Main order processing saga
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct OrderProcessingWorkflow;

#[async_trait::async_trait]
impl DurableWorkflow for OrderProcessingWorkflow {
    const TYPE_ID: &'static str = "order.processing";
    const VERSION: u32 = 1;
    
    type Input = OrderProcessingInput;
    type Output = OrderProcessingOutput;
    type Error = OrderWorkflowError;
    
    async fn run(
        &self,
        ctx: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // ═══════════════════════════════════════════════════════════════
        // INITIALIZATION
        // ═══════════════════════════════════════════════════════════════
        
        ctx.init_compensation_tracker_with_auto_compensate(true);
        
        tracing::info!("[Order {}] Processing order for customer {}", input.order_id, input.customer_id);
        
        // ═══════════════════════════════════════════════════════════════
        // STEP 1: Create order (metadata only, no activity)
        // ═══════════════════════════════════════════════════════════════
        
        let order = self.create_order(&input).await?;
        ctx.set_step_output("order".to_string(), serde::json::json!(order));
        
        // ═══════════════════════════════════════════════════════════════
        // STEP 2: Reserve inventory (WITH COMPENSATION)
        // ═══════════════════════════════════════════════════════════════
        
        let inventory_result = ctx
            .execute_activity(
                &ReserveInventoryActivity,
                ReserveInventoryInput {
                    order_id: input.order_id.clone(),
                    items: input.items.clone(),
                    expires_in_seconds: 3600, // 1 hour
                },
            )
            .await
            .map_err(|e| OrderWorkflowError::ActivityFailed(e.to_string()))?;
        
        ctx.track_compensatable_step_auto(
            "reserve-inventory",
            "ReserveInventoryActivity",
            serde::json::json!({ "order_id": input.order_id }),
            serde::json::json!(inventory_result),
            1,
        );
        
        tracing::info!("[Order {}] Inventory reserved: {}", input.order_id, inventory_result.reservation_id);
        
        // ═══════════════════════════════════════════════════════════════
        // STEP 3: Process payment (WITH COMPENSATION)
        // ═══════════════════════════════════════════════════════════════
        
        let payment_result = ctx
            .execute_activity(
                &ProcessPaymentActivity,
                ProcessPaymentInput {
                    order_id: input.order_id.clone(),
                    amount: input.total_amount,
                    currency: input.currency.clone(),
                    payment_token: input.payment_token.clone(),
                    customer_id: input.customer_id.clone(),
                },
            )
            .await
            .map_err(|e| OrderWorkflowError::ActivityFailed(e.to_string()))?;
        
        ctx.track_compensatable_step_auto(
            "process-payment",
            "ProcessPaymentActivity",
            serde::json::json!({ 
                "order_id": input.order_id,
                "amount": input.total_amount,
            }),
            serde::json::json!(payment_result),
            2,
        );
        
        tracing::info!("[Order {}] Payment completed: {}", input.order_id, payment_result.transaction_id);
        
        // ═══════════════════════════════════════════════════════════════
        // STEP 4: Schedule shipping (WITH COMPENSATION)
        // ═══════════════════════════════════════════════════════════════
        
        let shipping_result = ctx
            .execute_activity(
                &ScheduleShippingActivity,
                ScheduleShippingInput {
                    order_id: input.order_id.clone(),
                    address: input.shipping_address.clone(),
                    items: self.items_from_order(&input.items),
                    preferences: ShippingPreferences {
                        carrier: None,
                        express: input.express_shipping,
                        signature_required: true,
                    },
                },
            )
            .await
            .map_err(|e| OrderWorkflowError::ActivityFailed(e.to_string()))?;
        
        ctx.track_compensatable_step_auto(
            "schedule-shipping",
            "ScheduleShippingActivity",
            serde::json::json!({ "order_id": input.order_id }),
            serde::json::json!(shipping_result),
            3,
        );
        
        tracing::info!("[Order {}] Shipping scheduled: {}", input.order_id, shipping_result.tracking_number);
        
        // ═══════════════════════════════════════════════════════════════
        // COMPLETE WORKFLOW
        // ═══════════════════════════════════════════════════════════════
        
        let output = OrderProcessingOutput {
            order_id: input.order_id.clone(),
            status: OrderStatus::Processing,
            transaction_id: payment_result.transaction_id,
            reservation_id: inventory_result.reservation_id,
            shipment_id: shipping_result.shipment_id,
            tracking_number: shipping_result.tracking_number,
            estimated_delivery: shipping_result.estimated_delivery,
            completed_at: chrono::Utc::now(),
        };
        
        ctx.compensation_tracker.take();
        
        Ok(output)
    }
}

impl OrderProcessingWorkflow {
    async fn create_order(&self, input: &OrderProcessingInput) -> Result<OrderInfo, OrderWorkflowError> {
        Ok(OrderInfo {
            order_id: input.order_id.clone(),
            customer_id: input.customer_id.clone(),
            item_count: input.items.len(),
            total_amount: input.total_amount,
            currency: input.currency.clone(),
            created_at: chrono::Utc::now(),
        })
    }
    
    fn items_from_order(&self, items: &[OrderItemInput]) -> Vec<ShippingItemInput> {
        items.iter()
            .map(|i| ShippingItemInput {
                product_id: i.product_id.clone(),
                quantity: i.quantity,
            })
            .collect()
    }
}
```

### 6.2 Input/Output Types

```rust
// ═══════════════════════════════════════════════════════════════════════
// INPUT/OUTPUT TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderProcessingInput {
    pub order_id: String,
    pub customer_id: String,
    pub items: Vec<OrderItemInput>,
    pub total_amount: Decimal,
    pub currency: String,
    pub shipping_address: AddressInput,
    pub payment_token: String,
    pub express_shipping: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderProcessingOutput {
    pub order_id: String,
    pub status: OrderStatus,
    pub transaction_id: String,
    pub reservation_id: String,
    pub shipment_id: String,
    pub tracking_number: String,
    pub estimated_delivery: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub order_id: String,
    pub customer_id: String,
    pub item_count: usize,
    pub total_amount: Decimal,
    pub currency: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Processing,
    Confirmed,
    Shipped,
    Delivered,
    Completed,
    Cancelled,
    Failed,
}

// ═══════════════════════════════════════════════════════════════════════
// WORKFLOW ERROR - Unified
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum OrderWorkflowError {
    #[error("Validation failed: {0}")]
    Validation(String),
    
    #[error("Activity failed: {0}")]
    ActivityFailed(String),
    
    #[error("Compensation required: {0}")]
    CompensationRequired(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Payment declined: {0}")]
    PaymentDeclined(String),
    
    #[error("Insufficient inventory: {0}")]
    InsufficientInventory(String),
}
```

### 6.3 Workflow Visual Flow

```mermaid
flowchart TD
    subgraph "Order Processing Workflow"
        A[Start] --> B[Create Order<br/>meta]
        B --> C[Reserve Inventory<br/>WITH COMPENSATION]
        C --> D[Process Payment<br/>WITH COMPENSATION]
        D --> E[Schedule Shipping<br/>WITH COMPENSATION]
        E --> F[End<br/>✓ Order Complete]
    end
    
    subgraph "Compensation Path (if fails)"
        C -->|Fails| C1[Release Inventory]
        D -->|Fails| D1[Refund Payment]
        D1 --> C1
        E -->|Fails| E1[Cancel Shipping]
        E1 --> D1
    end
    
    style C fill:#c8e6c9
    style D fill:#c8e6c9
    style E fill:#c8e6c9
    style C1 fill:#ffcdd2
    style D1 fill:#ffcdd2
    style E1 fill:#ffcdd2
```

---

## 7. Command Handlers: Decoupling

### 7.1 Unified Command Handler

```rust
// ═══════════════════════════════════════════════════════════════════════
// COMMAND HANDLER FACTORY - Reduces boilerplate
// ═══════════════════════════════════════════════════════════════════════

pub struct EcommerceCommandHandler {
    saga_engine: Arc<SagaEngine<Es, Tq, Ts>>,
    activity_registry: Arc<ActivityRegistry>,
}

impl EcommerceCommandHandler {
    pub fn new(
        saga_engine: Arc<SagaEngine<Es, Tq, Ts>>,
        activity_registry: Arc<ActivityRegistry>,
    ) -> Self {
        Self { saga_engine, activity_registry }
    }
    
    /// Register all activities once
    pub fn register_activities(&self) {
        self.activity_registry.register_activity(ProcessPaymentActivity);
        self.activity_registry.register_activity(RefundPaymentActivity);
        self.activity_registry.register_activity(ReserveInventoryActivity);
        self.activity_registry.register_activity(ReleaseInventoryActivity);
        self.activity_registry.register_activity(ScheduleShippingActivity);
        self.activity_registry.register_activity(CancelShippingActivity);
    }
}
```

### 7.2 Order Command Handler

```rust
// ═══════════════════════════════════════════════════════════════════════
// ORDER COMMAND HANDLER
// ═══════════════════════════════════════════════════════════════════════

#[async_trait]
impl CommandHandler<CreateOrderCommand> for EcommerceCommandHandler {
    async fn handle(&self, command: CreateOrderCommand) -> CommandResult<String> {
        // ═══════════════════════════════════════════════════════════════
        // VALIDATION
        // ═══════════════════════════════════════════════════════════════
        
        if command.items.is_empty() {
            return CommandResult::ValidationError(vec![ValidationError {
                field: "items",
                message: "Order must have at least one item",
                code: "EMPTY_ITEMS",
            }]);
        }
        
        // ═══════════════════════════════════════════════════════════════
        // BUSINESS LOGIC
        // ═══════════════════════════════════════════════════════════════
        
        let total_amount = self.calculate_total(&command.items).await?;
        
        // ═══════════════════════════════════════════════════════════════
        // START WORKFLOW
        // ═══════════════════════════════════════════════════════════════
        
        let saga_id = SagaId::new();
        
        let workflow_input = OrderProcessingInput {
            order_id: saga_id.0.clone(),
            customer_id: command.customer_id,
            items: command.items,
            total_amount,
            currency: "USD".to_string(),
            shipping_address: command.shipping_address,
            payment_token: command.payment_method.token,
            express_shipping: false,
        };
        
        self.saga_engine
            .start_workflow::<OrderProcessingWorkflow>(saga_id.clone(), workflow_input)
            .await
            .map_err(|e| CommandResult::Failure(e.to_string()))?;
        
        CommandResult::Success(saga_id.0)
    }
}

impl EcommerceCommandHandler {
    async fn calculate_total(&self, items: &[OrderItemInput]) -> Result<Decimal, CommandResult> {
        Ok(Decimal::from(100)) // Simplified
    }
}
```

### 7.3 Cancel Order Handler (with Compensation)

```rust
// ═══════════════════════════════════════════════════════════════════════
// CANCEL ORDER HANDLER - With automatic compensation
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderCommand {
    pub order_id: String,
    pub reason: String,
    pub request_refund: bool,
}

impl Command for CancelOrderCommand {
    type Result = CancelOrderResult;
    fn command_type(&self) -> &'static str {
        "order.cancel"
    }
    fn aggregate_id(&self) -> &str {
        &self.order_id
    }
}

#[async_trait]
impl CommandHandler<CancelOrderCommand> for EcommerceCommandHandler {
    async fn handle(&self, command: CancelOrderCommand) -> CommandResult<CancelOrderResult> {
        // Get saga history
        let saga_id = SagaId(command.order_id.clone());
        let history = self.saga_engine.event_store().get_history(&saga_id).await
            .map_err(|e| CommandResult::Failure(e.to_string()))?;
        
        // Extract completed steps
        let completed_steps = self.extract_completed_steps(&history)?;
        
        // Execute compensations in reverse order (LIFO)
        let mut compensation_results = Vec::new();
        
        for step in completed_steps.into_iter().rev() {
            if let Some(compensate_activity) = step.get_compensation_activity() {
                let result = self.execute_compensation(&step, &command.reason).await;
                compensation_results.push(result);
            }
        }
        
        let all_success = compensation_results.iter().all(|r| r.is_success());
        
        CommandResult::Success(CancelOrderResult {
            order_id: command.order_id,
            cancelled: true,
            refund_processed: if command.request_refund { Some(true) } else { None },
            compensations_executed: compensation_results.len(),
            all_compensations_successful: all_success,
            cancelled_at: chrono::Utc::now(),
        })
    }
    
    fn extract_completed_steps(&self, history: &[HistoryEvent]) -> Result<Vec<CompletedStep>, CommandResult> {
        Ok(vec![])
    }
    
    async fn execute_compensation(&self, step: &CompletedStep, reason: &str) -> CompensationResult {
        CompensationResult::Success
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderResult {
    pub order_id: String,
    pub cancelled: bool,
    pub refund_processed: Option<bool>,
    pub compensations_executed: usize,
    pub all_compensations_successful: bool,
    pub cancelled_at: chrono::DateTime<chrono::Utc>,
}
```

---

## 8. Multiple Perspectives

### 8.1 REST API (For External Clients)

```rust
// ═══════════════════════════════════════════════════════════════════════
// REST API HANDLERS - For external clients
// ═══════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct CreateOrderRequest {
    pub customer_id: String,
    pub items: Vec<OrderItemInput>,
    pub shipping_address: AddressInput,
    pub payment_token: String,
}

#[post("/api/v1/orders")]
async fn create_order(
    handler: web::Data<EcommerceCommandHandler>,
    req: web::Json<CreateOrderRequest>,
) -> impl Responder {
    let command = CreateOrderCommand {
        customer_id: req.customer_id.clone(),
        items: req.items.clone(),
        shipping_address: req.shipping_address.clone(),
        payment_method: PaymentMethodInput {
            method: "card".to_string(),
            token: req.payment_token.clone(),
        },
        idempotency_key: None,
    };
    
    match handler.handle(command).await {
        CommandResult::Success(order_id) => HttpResponse::Created().json(json!({
            "order_id": order_id,
            "status": "processing",
            "message": "Order created successfully"
        })),
        CommandResult::ValidationError(errors) => HttpResponse::BadRequest().json(json!({
            "error": "Validation failed",
            "details": errors
        })),
        CommandResult::Failure(msg) => HttpResponse::InternalServerError().json(json!({
            "error": msg
        })),
    }
}

#[get("/api/v1/orders/{order_id}")]
async fn get_order_status(
    saga_engine: web::Data<Arc<SagaEngine<Es, Tq, Ts>>>,
    order_id: web::Path<String>,
) -> impl Responder {
    let saga_id = SagaId(order_id.into_inner());
    
    match saga_engine.get_workflow_status(&saga_id).await {
        Ok(Some(status)) => HttpResponse::Ok().json(status),
        Ok(None) => HttpResponse::NotFound().json(json!({ "error": "Order not found" })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

#[post("/api/v1/orders/{order_id}/cancel")]
async fn cancel_order(
    handler: web::Data<EcommerceCommandHandler>,
    order_id: web::Path<String>,
    req: web::Json<CancelOrderRequest>,
) -> impl Responder {
    let command = CancelOrderCommand {
        order_id: order_id.into_inner(),
        reason: req.reason.clone(),
        request_refund: req.request_refund,
    };
    
    match handler.handle(command).await {
        CommandResult::Success(result) => HttpResponse::Ok().json(result),
        CommandResult::Failure(msg) => HttpResponse::InternalServerError().json(json!({ "error": msg })),
        _ => HttpResponse::BadRequest().json(json!({})),
    }
}

#[derive(Deserialize)]
pub struct CancelOrderRequest {
    pub reason: String,
    pub request_refund: bool,
}
```

### 8.2 Admin Console

```rust
// ═══════════════════════════════════════════════════════════════════════
// ADMIN API - For administration panel
// ═══════════════════════════════════════════════════════════════════════

#[get("/admin/api/v1/orders")]
async fn list_orders(
    repo: web::Data<OrderRepository>,
    query: web::Query<OrderListQuery>,
) -> impl Responder {
    let orders = repo.find_orders(query.into_inner()).await;
    HttpResponse::Ok().json(orders)
}

#[get("/admin/api/v1/orders/{order_id}/events")]
async fn get_order_events(
    saga_engine: web::Data<Arc<SagaEngine<Es, Tq, Ts>>>,
    order_id: web::Path<String>,
) -> impl Responder {
    let saga_id = SagaId(order_id.into_inner());
    let history = saga_engine.event_store().get_history(&saga_id).await;
    HttpResponse::Ok().json(history)
}

#[post("/admin/api/v1/orders/{order_id}/retry")]
async fn retry_order(
    saga_engine: web::Data<Arc<SagaEngine<Es, Tq, Ts>>>,
    order_id: web::Path<String>,
) -> impl Responder {
    HttpResponse::Ok().json(json!({ "message": "Retry initiated" }))
}
```

### 8.3 CLI for Customers

```rust
// ═══════════════════════════════════════════════════════════════════════
// CLI COMMANDS - For customers
// ═══════════════════════════════════════════════════════════════════════

#[derive(Parser)]
#[command(name = "order")]
pub enum OrderCommands {
    Create {
        #[arg(short, long)]
        customer_id: String,
        
        #[arg(short, long, value_parser = parse_items)]
        items: String,
        
        #[arg(short, long)]
        address: String,
        
        #[arg(short, long)]
        payment_token: String,
    },
    
    Status {
        #[arg(short, long)]
        order_id: String,
    },
    
    Cancel {
        #[arg(short, long)]
        order_id: String,
        
        #[arg(short, long)]
        reason: String,
    },
    
    Track {
        #[arg(short, long)]
        order_id: String,
    },
}

impl OrderCommands {
    pub async fn execute(&self, handler: &EcommerceCommandHandler) -> Result<(), CliError> {
        match self {
            Self::Create { customer_id, items, address, payment_token } => {
                let order_id = self.create_order(handler, customer_id, items, address, payment_token).await?;
                println!("Order created: {}", order_id);
                Ok(())
            }
            Self::Status { order_id } => {
                self.show_status(handler, order_id).await?;
                Ok(())
            }
            Self::Cancel { order_id, reason } => {
                self.cancel_order(handler, order_id, reason).await?;
                Ok(())
            }
            Self::Track { order_id } => {
                self.show_tracking(handler, order_id).await?;
                Ok(())
            }
        }
    }
}
```

---

## 9. Optimization Summary

### 9.1 Duplicities Eliminated

| Before (Duplicated) | After (Optimized) |
|---------------------|-------------------|
| Events `PaymentStarted`, `PaymentFinished` | `PaymentInitiated`, `PaymentCompleted` |
| Commands separated by context | `Command` trait unified + specialization |
| Independent activities | `EcommerceActivity` trait with `COMPENSATION_ACTIVITY` |
| Different payloads | `BaseEventPayload` + specific structs |
| Manual handlers | `CommandHandler<C>` generic trait |

### 9.2 Patterns Applied

| Pattern | Application | Benefit |
|---------|-------------|---------|
| **Template Method** | `EcommerceActivity` defines structure, specialized implementations | DRY |
| **Strategy** | `DomainEventType` with dynamic conversions | Flexibility |
| **Command** | Typed commands with `CommandHandler<C>` generic | Decoupling |
| **Event Sourcing** | All changes produce events | Audit, replay |
| **Compensation** | Auto-tracking via `track_compensatable_step_auto()` | Automatic rollback |

### 9.3 Final File Structure

```
ecommerce-example/
├── domain/
│   ├── mod.rs
│   ├── aggregates/
│   │   ├── mod.rs
│   │   └── order.rs
│   ├── events/
│   │   ├── mod.rs
│   │   ├── order_events.rs
│   │   └── payment_events.rs
│   ├── commands/
│   │   ├── mod.rs
│   │   └── order_commands.rs
│   └── value_objects/
│       ├── mod.rs
│       └── money.rs
│
├── application/
│   ├── mod.rs
│   ├── workflows/
│   │   ├── mod.rs
│   │   └── order_processing.rs
│   ├── activities/
│   │   ├── mod.rs
│   │   ├── payment_activities.rs
│   │   └── inventory_activities.rs
│   └── handlers/
│       ├── mod.rs
│       └── command_handler.rs
│
├── infrastructure/
│   ├── repositories/
│   └── adapters/
│
└── api/
    ├── rest/
    ├── admin/
    └── cli/
```

---

## 10. Complete Reference Code

### 10.1 Visual Architecture Summary

```mermaid
graph TB
    subgraph "Client Layer"
        API[REST API]
        CLI[CLI]
        ADMIN[Admin Panel]
    end
    
    subgraph "Command Layer"
        CMD[Commands<br/>CreateOrder, Cancel, etc.]
        HAND[CommandHandler<br/>Generic C]
    end
    
    subgraph "Workflow Layer"
        WF[OrderProcessingWorkflow<br/>Saga Orchestrator]
        CT[CompensationTracker<br/>Auto-rollback]
    end
    
    subgraph "Activity Layer"
        PA[ProcessPayment]
        RI[ReserveInventory]
        SS[ScheduleShipping]
    end
    
    subgraph "Event Layer"
        ES[Event Store<br/>Append-only]
        EV[Events<br/>OrderCreated, etc.]
    end
    
    API --> CMD
    CLI --> CMD
    ADMIN --> CMD
    
    CMD --> HAND
    HAND --> WF
    
    WF --> CT
    WF --> PA
    WF --> RI
    WF --> SS
    
    PA --> ES
    RI --> ES
    SS --> ES
    CT --> ES
    
    ES --> EV
    
    style CMD fill:#bbdefb
    style WF fill:#c8e6c9
    style ES fill:#fff9c4
```

### 10.2 Compensation Flow on Error

```mermaid
flowchart TD
    A[Start Workflow] --> B[Reserve Inventory]
    B --> C[Process Payment]
    C -->|FAILED| D[Start Compensation]
    
    D --> E[Cancel Shipping<br/>If executed]
    E --> F[Refund Payment]
    F --> G[Release Inventory]
    
    G --> H[Order Cancelled<br/>All compensations done]
    
    style C fill:#ffcdd2
    style D fill:#ffecb3
    style E fill:#ffcdd2
    style F fill:#ffcdd2
    style G fill:#ffcdd2
    style H fill:#ffcdd2
```

### 10.3 Implementation Checklist

```markdown
## Checklist for Implementing a New Workflow

### Domain Layer
- [ ] Define Value Objects (IDs, Money, etc.)
- [ ] Define Events (in past tense)
- [ ] Define Commands (verb + input)

### Application Layer
- [ ] Implement Activities (with compensation)
- [ ] Implement Workflow (DurableWorkflow)
- [ ] Configure compensation tracker

### Infrastructure Layer
- [ ] Implement Command Handlers
- [ ] Configure Event Store
- [ ] Configure Task Queue

### Testing
- [ ] Unit tests for Activities
- [ ] Integration tests for Workflow
- [ ] Compensation test (intentional failure)
```

---

## Appendix A: Glossary

| Term | Meaning in This Context |
|------|--------------------------|
| **Saga** | The complete sequence of steps to process an order |
| **Workflow** | The definition of how the saga executes |
| **Activity** | A single operation (payment, inventory, shipping) |
| **Command** | User intention (create order, cancel) |
| **Event** | Something that already happened (payment completed) |
| **Compensation** | The inverse operation to revert changes |

---

## Appendix B: v3 vs v4 Differences

| Feature | v3 (Old) | v4 (New) |
|---------|----------|----------|
| Workflow Definition | Steps array | `DurableWorkflow::run()` |
| Activities | `ActivityStep::new()` | `ctx.execute_activity()` |
| Compensation | Manual | `track_compensatable_step_auto()` |
| Events | Basic | Type-safe payloads |
| Commands | None | `CommandHandler<C>` trait |

---

*Document Version: 2.0.0*
*Example: E-Commerce Order Processing Saga*
*Language: English*
*Last Updated: 2026-01-28*

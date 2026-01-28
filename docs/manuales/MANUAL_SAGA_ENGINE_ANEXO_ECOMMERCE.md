# Saga Engine V4 - Anexo: Ejemplo E-Commerce

## Tutorial Completo: Construyendo un Sistema de Ventas Online con Saga Engine

---

> **¿Qué es este documento?**
> Este anexo presenta un **sistema de ventas online completo** implementado con Saga Engine V4. A diferencia de los otros manuales que explican conceptos teóricos, aquí verás cómo aplicar todos esos conceptos en un caso real y concreto.
>
> **Lo que aprenderás**:
> - Cómo modelar un dominio de e-commerce con DDD
> - Cómo diseñar eventos, commands y activities cohesivos
> - Cómo implementar el patrón de compensación en un caso real
> - Cómo estructurar el código para evitar duplicidades
> - Perspectivas múltiples: API, Admin, CLI

---

## Tabla de Contenidos

1. [Introducción: El Caso de Uso](#1-introducción-el-caso-de-uso)
2. [Contexto del Dominio (DDD)](#2-contexto-del-dominio-ddd)
3. [Catálogo de Eventos Unificados](#3-catálogo-de-eventos-unificados)
4. [Commands: La Interfaz de Usuario del Dominio](#4-commands-la-interfaz-de-usuario-del-dominio)
5. [Activities: Los Bloques de Construcción](#5-activities-los-bloques-de-construcción)
6. [Workflow del Pedido: La Saga Completa](#6-workflow-del-pedido-la-saga-completa)
7. [Command Handlers: Desacoplamiento](#7-command-handlers-desacoplamiento)
8. [Perspectivas Múltiples](#8-perspectivas-múltiples)
9. [Resumen de Optimizaciones](#9-resumen-de-optimizaciones)
10. [Código Completo de Referencia](#10-código-completo-de-referencia)

---

## 1. Introducción: El Caso de Uso

### 1.1 Escenario: Procesamiento de Pedidos Online

Imagina que estás construyendo un sistema de comercio electrónico. Cuando un cliente realiza una compra, el sistema debe:

```mermaid
flowchart LR
    subgraph "Flujo de un pedido"
        A[Cliente<br/>selecciona productos] --> B[Carrito<br/>de compra]
        B --> C[Checkout<br/>con pago]
        C --> D[Sistema<br/>procesa pedido]
        D --> E[Inventario<br/>reservado]
        E --> F[Pago<br/>cobrado]
        F --> G[Envío<br/>programado]
        G --> H[Cliente<br/>recibe producto]
    end
```

**El desafío**: Cada paso puede fallar, y si falla uno después de que otros hayan succeedido, necesitamos revertir los cambios.

### 1.2 Objetivos de Diseño

| Objetivo | Descripción | Técnica |
|----------|-------------|---------|
| **DRY** | No repetir código | Traits genéricos, composición |
| **Cohesión** | Cada componente tiene una responsabilidad clara | DDD, Bounded Contexts |
| **Compensación** | Rollback automático | Compensation Pattern |
| **Testabilidad** | Fácil de probar | Puertos y adaptadores |

### 1.3 Arquitectura General del Sistema

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
    
    OC --> PC : "pagos"
    OC --> IC : "inventario"
    OC --> SC : "envío"
    PC --> CC : "cliente"
    IC --> PRC : "productos"
    
    style OC fill:#e3f2fd
    style PC fill:#f3e5f5
    style IC fill:#e8f5e8
    style SC fill:#fff3e0
```

---

## 2. Contexto del Dominio (DDD)

### 2.1 Bounded Contexts del E-Commerce

En DDD, un **Bounded Context** es un límite dentro del cual existe un modelo de dominio consistente. Cada contexto tiene su propio "idioma" (lenguaje ubicuo).

```mermaid
graph TB
    subgraph "Sales Bounded Context (El Rey)"
        O[Order<br/>"Una orden tiene items y un total"]
        P[Payment<br/>"Un pago tiene estado y monto"]
    end
    
    subgraph "Inventory Bounded Context"
        I[Inventory<br/>"El inventario tiene stock disponible"]
        R[Reservation<br/>"Una reserva bloquea stock por tiempo"]
    end
    
    subgraph "Shipping Bounded Context"
        S[Shipment<br/>"Un envío tiene tracking number"]
        C[Carrier<br/>"El carrier entrega el paquete"]
    end
    
    O --> P : "procesa"
    O --> I : "reserva"
    O --> S : "coordina"
    
    style O fill:#e3f2fd
    style I fill:#e8f5e8
    style S fill:#fff3e0
```

### 2.2 Agregados del Dominio

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

### 2.3 Lenguaje Ubicuo

| Término | Contexto | Definición |
|---------|----------|------------|
| **Order** | Sales | El pedido completo con todos los items |
| **Payment** | Sales/Payment | El cobro al cliente |
| **Reservation** | Inventory | Bloqueo temporal de stock |
| **Shipment** | Shipping | El envío físico al cliente |

---

## 3. Catálogo de Eventos Unificados

### 3.1 Principios de Diseño de Eventos

Los eventos representan **algo que ya ocurrió**. Se nombran en pasado y contienen toda la información necesaria.

```mermaid
flowchart LR
    subgraph "Command (verbo)"
        C[CreateOrder<br/>"Crea una orden"]
    end
    
    subgraph "Event (pasado)"
        E[OrderCreated<br/>"La orden fue creada"]
    end
    
    C --> E
```

### 3.2 Estructura Base de Eventos

```rust
// ═══════════════════════════════════════════════════════════════
// PAYLOAD BASE - Reutilizable en todo el sistema
// ═══════════════════════════════════════════════════════════════

/// Metadata común para todos los eventos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseEventPayload {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub correlation_id: Option<String>,  // Para tracing
    pub trace_id: Option<String>,
    pub source: String,  // Quién generó el evento
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

### 3.3 Tipos de Eventos Genéricos (DomainEventType)

Estos eventos son reutilizables en cualquier dominio:

```rust
// ═══════════════════════════════════════════════════════════════
// SHARED EVENT TYPES - Reutilizables en todo el sistema
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainEventType {
    // Estados del ciclo de vida
    Created,
    Updated,
    Deleted,
    Activated,
    Deactivated,
    
    // Estados de proceso
    Started,
    Completed,
    Failed,
    Cancelled,
    
    // Eventos de tiempo
    Expired,
    TimeoutReached,
    
    // Eventos de integración
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

### 3.4 Eventos Específicos del E-Commerce

```rust
// ═══════════════════════════════════════════════════════════════
// ORDER EVENTS - Especializados pero coherentes
// ═══════════════════════════════════════════════════════════════

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

### 3.5 Payloads Tipados

```rust
// ═══════════════════════════════════════════════════════════════
// ORDER PAYLOADS - Type-safe payloads
// ═══════════════════════════════════════════════════════════════

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

### 3.6 Payloads de Compensación

```rust
// ═══════════════════════════════════════════════════════════════
// COMPENSATION PAYLOADS - Estructura unificada
// ═══════════════════════════════════════════════════════════════

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

## 4. Commands: La Interfaz de Usuario del Dominio

### 4.1 ¿Qué es un Command?

Un **Command** representa **algo que queremos que ocurra**. A diferencia de los eventos (pasado), los commands son intención (futuro).

```mermaid
flowchart LR
    subgraph "API Layer"
        API[REST API<br/>POST /orders]
    end
    
    subgraph "Command"
        CMD[CreateOrderCommand<br/>"Quiero crear una orden"]
    end
    
    subgraph "Domain"
        E[OrderCreated Event<br/>"La orden fue creada"]
    end
    
    API --> CMD
    CMD --> E
```

### 4.2 Arquitectura del Command Bus

```rust
// ═══════════════════════════════════════════════════════════════
// COMMAND BUS - Framework agnóstico
// ═══════════════════════════════════════════════════════════════

/// Trait base para todos los commands
pub trait Command: Send + Sync + Debug {
    type Result;
    fn command_type(&self) -> &'static str;
    fn aggregate_id(&self) -> &str;
}

/// Resultado genérico de command
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

/// Handler base con tipado genérico
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    async fn handle(&self, command: C) -> CommandResult<C::Result>;
}
```

### 4.3 Command de Creación de Orden

```rust
// ═══════════════════════════════════════════════════════════════
// ORDER COMMANDS
// ═══════════════════════════════════════════════════════════════

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

// Inputs tipados
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

### 4.4 Command de Pago

```rust
// ═══════════════════════════════════════════════════════════════
// PAYMENT COMMANDS
// ═══════════════════════════════════════════════════════════════

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

### 4.5 Command de Inventario

```rust
// ═══════════════════════════════════════════════════════════════
// INVENTORY COMMANDS
// ═══════════════════════════════════════════════════════════════

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

## 5. Activities: Los Bloques de Construcción

### 5.1 ¿Qué es una Activity?

Una **Activity** es la unidad más pequeña de trabajo. Es una operación atómica que puede ejecutarse independientemente.

```mermaid
flowchart LR
    subgraph "Activity"
        I[Input] --> A[execute]
        A -->|Éxito| O[Output]
        A -->|Error| E[Error]
    end
```

### 5.2 Trait Base Extendido para E-Commerce

```rust
// ═══════════════════════════════════════════════════════════════
// ACTIVITY TRAIT - Extendido para el dominio e-commerce
// ═══════════════════════════════════════════════════════════════

/// Activity base con capacidades de compensación automática
#[async_trait]
pub trait EcommerceActivity<I: Serialize + Deserialize<'static> + Send + Clone, O: Serialize + Deserialize<'static> + Send + Clone>: Activity {
    /// Nombre del compensation activity asociado (si existe)
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    /// Verifica si la actividad puede ser compensada
    fn can_compensate(&self, _output: &O) -> bool {
        true
    }
    
    /// Prepara datos para compensación
    fn prepare_compensation_data(&self, _output: &O) -> Option<serde_json::Value> {
        None
    }
}
```

### 5.3 Activity de Procesamiento de Pago

```rust
// ═══════════════════════════════════════════════════════════════
// PAYMENT ACTIVITIES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ProcessPaymentActivity;

#[async_trait]
impl EcommerceActivity<ProcessPaymentInput, PaymentOutput> for ProcessPaymentActivity {
    const TYPE_ID: &'static str = "payment.process";
    
    /// ⭐ Esta actividad puede ser compensada con un reembolso
    const COMPENSATION_ACTIVITY: Option<&'static str> = Some("payment.refund");
    
    async fn execute(&self, input: ProcessPaymentInput) -> Result<PaymentOutput, PaymentError> {
        // ════════════════════════════════════════════════════════════
        // LÓGICA REAL: Aquí integrarías con el gateway de pago
        // ════════════════════════════════════════════════════════════
        
        // Simular llamada al gateway
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

### 5.4 Activity de Reembolso (Compensación)

```rust
// ═══════════════════════════════════════════════════════════════
// REFUND ACTIVITY - Compensación de Payment
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct RefundPaymentActivity;

#[async_trait]
impl EcommerceActivity<RefundInput, RefundOutput> for RefundPaymentActivity {
    const TYPE_ID: &'static str = "payment.refund";
    /// ⭐ El reembolso no tiene compensación (no compensamos reembolsos)
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    async fn execute(&self, input: RefundInput) -> Result<RefundOutput, RefundError> {
        // Integración real con gateway de pago para reembolsar
        
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

### 5.5 Activity de Reserva de Inventario

```rust
// ═══════════════════════════════════════════════════════════════
// INVENTORY ACTIVITIES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ReserveInventoryActivity;

#[async_trait]
impl EcommerceActivity<ReserveInventoryInput, ReserveInventoryOutput> for ReserveInventoryActivity {
    const TYPE_ID: &'static str = "inventory.reserve";
    
    /// ⭐ Esta actividad puede ser compensada liberando la reserva
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
        // ════════════════════════════════════════════════════════════
        // LÓGICA REAL: Aquí verificarías stock y harías la reserva
        // ════════════════════════════════════════════════════════════
        
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

### 5.6 Activity de Liberación de Inventario (Compensación)

```rust
// ═══════════════════════════════════════════════════════════════
// RELEASE INVENTORY ACTIVITY - Compensación
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ReleaseInventoryActivity;

#[async_trait]
impl EcommerceActivity<ReleaseInventoryInput, ReleaseInventoryOutput> for ReleaseInventoryActivity {
    const TYPE_ID: &'static str = "inventory.release";
    /// ⭐ La liberación no tiene compensación
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    async fn execute(&self, input: ReleaseInventoryInput) -> Result<ReleaseInventoryOutput, InventoryError> {
        // ════════════════════════════════════════════════════════════
        // LÓGICA REAL: Aquí liberarías la reserva en la base de datos
        // ════════════════════════════════════════════════════════════
        
        Ok(ReleaseInventoryOutput {
            released: true,
            reservation_id: input.reservation_id,
            released_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInventoryInput {
    pub reservation_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInventoryOutput {
    pub released: bool,
    pub reservation_id: String,
    pub released_at: chrono::DateTime<chrono::Utc>,
}
```

### 5.7 Activity de Envío

```rust
// ═══════════════════════════════════════════════════════════════
// SHIPPING ACTIVITIES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ScheduleShippingActivity;

#[async_trait]
impl EcommerceActivity<ScheduleShippingInput, ScheduleShippingOutput> for ScheduleShippingActivity {
    const TYPE_ID: &'static str = "shipping.schedule";
    
    /// ⭐ El envío puede ser cancelado
    const COMPENSATION_ACTIVITY: Option<&'static str> = Some("shipping.cancel");
    
    async fn execute(&self, input: ScheduleShippingInput) -> Result<ScheduleShippingOutput, ShippingError> {
        // ════════════════════════════════════════════════════════════
        // LÓGICA REAL: Aquí integrarías con el carrier (FedEx, UPS, etc.)
        // ════════════════════════════════════════════════════════════
        
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

### 5.8 Activity de Cancelación de Envío (Compensación)

```rust
// ═══════════════════════════════════════════════════════════════
// CANCEL SHIPPING ACTIVITY - Compensación
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct CancelShippingActivity;

#[async_trait]
impl EcommerceActivity<CancelShippingInput, CancelShippingOutput> for CancelShippingActivity {
    const TYPE_ID: &'static str = "shipping.cancel";
    const COMPENSATION_ACTIVITY: Option<&'static str> = None;
    
    async fn execute(&self, input: CancelShippingInput) -> Result<CancelShippingOutput, ShippingError> {
        // Cancelar shipment con carrier
        
        Ok(CancelShippingOutput {
            cancelled: true,
            shipment_id: input.shipment_id,
            refund_amount: input.refund_amount,
            cancelled_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelShippingInput {
    pub shipment_id: String,
    pub order_id: String,
    pub refund_amount: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelShippingOutput {
    pub cancelled: bool,
    pub shipment_id: String,
    pub refund_amount: Decimal,
    pub cancelled_at: chrono::DateTime<chrono::Utc>,
}
```

---

## 6. Workflow del Pedido: La Saga Completa

### 6.1 Definición del Workflow

```rust
// ═══════════════════════════════════════════════════════════════
// ORDER WORKFLOW - Saga principal de procesamiento de pedidos
// ═══════════════════════════════════════════════════════════════

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
        // ════════════════════════════════════════════════════════════
        // INICIALIZACIÓN
        // ════════════════════════════════════════════════════════════
        
        // ⭐ Inicializar tracker de compensación con auto-compensate
        ctx.init_compensation_tracker_with_auto_compensate(true);
        
        tracing::info!("[Order {}] Processing order for customer {}", input.order_id, input.customer_id);
        
        // ════════════════════════════════════════════════════════════
        // STEP 1: Crear orden (solo metadata, no actividad)
        // ════════════════════════════════════════════════════════════
        
        let order = self.create_order(&input).await?;
        ctx.set_step_output("order".to_string(), serde_json::json!(order));
        
        // ════════════════════════════════════════════════════════════
        // STEP 2: Reservar inventario (CON COMPENSACIÓN)
        // ════════════════════════════════════════════════════════════
        
        let inventory_result = ctx
            .execute_activity(
                &ReserveInventoryActivity,
                ReserveInventoryInput {
                    order_id: input.order_id.clone(),
                    items: input.items.clone(),
                    expires_in_seconds: 3600, // 1 hora
                },
            )
            .await
            .map_err(|e| OrderWorkflowError::ActivityFailed(e.to_string()))?;
        
        // ⭐ Trackear para compensación automática
        ctx.track_compensatable_step_auto(
            "reserve-inventory",
            "ReserveInventoryActivity",
            serde_json::json!({ "order_id": input.order_id }),
            serde_json::json!(inventory_result),
            1,
        );
        
        tracing::info!("[Order {}] Inventory reserved: {}", input.order_id, inventory_result.reservation_id);
        
        // ════════════════════════════════════════════════════════════
        // STEP 3: Procesar pago (CON COMPENSACIÓN)
        // ════════════════════════════════════════════════════════════
        
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
            serde_json::json!({ 
                "order_id": input.order_id,
                "amount": input.total_amount,
            }),
            serde_json::json!(payment_result),
            2,
        );
        
        tracing::info!("[Order {}] Payment completed: {}", input.order_id, payment_result.transaction_id);
        
        // ════════════════════════════════════════════════════════════
        // STEP 4: Programar envío (CON COMPENSACIÓN)
        // ════════════════════════════════════════════════════════════
        
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
            serde_json::json!({ "order_id": input.order_id }),
            serde_json::json!(shipping_result),
            3,
        );
        
        tracing::info!("[Order {}] Shipping scheduled: {}", input.order_id, shipping_result.tracking_number);
        
        // ════════════════════════════════════════════════════════════
        // COMPLETAR WORKFLOW
        // ════════════════════════════════════════════════════════════
        
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
        
        // Limpiar tracker (no hay compensación pendiente porque todo salió bien)
        ctx.compensation_tracker.take();
        
        Ok(output)
    }
}

impl OrderProcessingWorkflow {
    async fn create_order(&self, input: &OrderProcessingInput) -> Result<OrderInfo, OrderWorkflowError> {
        // Aquí crearías la orden en tu base de datos
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

### 6.2 Input/Output del Workflow

```rust
// ═══════════════════════════════════════════════════════════════
// INPUT/OUTPUT TYPES
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// WORKFLOW ERROR - Unificado
// ═══════════════════════════════════════════════════════════════

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

### 6.3 Flujo Visual del Workflow

```mermaid
flowchart TD
    subgraph "Order Processing Workflow"
        A[Start] --> B[Crear Order<br/>meta]
        B --> C[Reservar Inventory<br/>CON COMPENSACIÓN]
        C --> D[Procesar Pago<br/>CON COMPENSACIÓN]
        D --> E[Programar Envío<br/>CON COMPENSACIÓN]
        E --> F[End<br/>✓ Orden Completa]
    end
    
    subgraph "Compensation Path (si falla)"
        C -->|Falla| C1[Release Inventory]
        D -->|Falla| D1[Refund Payment]
        D1 --> C1
        E -->|Falla| E1[Cancel Shipping]
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

## 7. Command Handlers: Desacoplamiento

### 7.1 Un Command Handler Unificado

```rust
// ═══════════════════════════════════════════════════════════════
// COMMAND HANDLER FACTORY - Reduce boilerplate
// ═══════════════════════════════════════════════════════════════

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
    
    /// Registrar todas las actividades una vez
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

### 7.2 Handler para Crear Orden

```rust
// ═══════════════════════════════════════════════════════════════
// ORDER COMMAND HANDLER
// ═══════════════════════════════════════════════════════════════

#[async_trait]
impl CommandHandler<CreateOrderCommand> for EcommerceCommandHandler {
    async fn handle(&self, command: CreateOrderCommand) -> CommandResult<String> {
        // ════════════════════════════════════════════════════════════
        // VALIDACIÓN
        // ════════════════════════════════════════════════════════════
        
        if command.items.is_empty() {
            return CommandResult::ValidationError(vec![ValidationError {
                field: "items",
                message: "Order must have at least one item",
                code: "EMPTY_ITEMS",
            }]);
        }
        
        // ════════════════════════════════════════════════════════════
        // LÓGICA DE NEGOCIO
        // ════════════════════════════════════════════════════════════
        
        let total_amount = self.calculate_total(&command.items).await?;
        
        // ════════════════════════════════════════════════════════════
        // INICIAR WORKFLOW
        // ════════════════════════════════════════════════════════════
        
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
        // Calcular total desde base de datos de productos
        Ok(Decimal::from(100)) // Simplificado
    }
}
```

### 7.3 Handler para Procesar Pago Directo

```rust
// ═══════════════════════════════════════════════════════════════
// PAYMENT COMMAND HANDLER - Ejecución directa (sin saga)
// ═══════════════════════════════════════════════════════════════

#[async_trait]
impl CommandHandler<ProcessPaymentCommand> for EcommerceCommandHandler {
    async fn handle(&self, command: ProcessPaymentCommand) -> CommandResult<PaymentResult> {
        // ════════════════════════════════════════════════════════════
        // EJECUTAR ACTIVIDAD DIRECTAMENTE (sin saga)
        // ════════════════════════════════════════════════════════════
        
        let activity = self.activity_registry
            .get_activity("payment.process")
            .ok_or_else(|| CommandResult::Failure("Activity not found".to_string()))?;
        
        let input = ProcessPaymentInput {
            order_id: command.order_id,
            amount: command.amount,
            currency: command.currency,
            payment_token: command.payment_token,
            customer_id: command.customer_id,
        };
        
        let output = activity.execute_dyn(serde_json::to_value(input).unwrap())
            .await
            .map_err(|e| CommandResult::Failure(e.to_string()))?;
        
        let payment_output: PaymentOutput = serde_json::from_value(output)
            .map_err(|e| CommandResult::Failure(e.to_string()))?;
        
        CommandResult::Success(PaymentResult {
            transaction_id: payment_output.transaction_id,
            status: payment_output.status,
            approved_amount: payment_output.approved_amount,
        })
    }
}
```

### 7.4 Handler para Cancelar Orden (con Compensación)

```rust
// ═══════════════════════════════════════════════════════════════
// CANCEL ORDER HANDLER - Con compensación automática
// ═══════════════════════════════════════════════════════════════

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
        // ════════════════════════════════════════════════════════════
        // OBTENER HISTORIAL DEL SAGA
        // ════════════════════════════════════════════════════════════
        
        let saga_id = SagaId(command.order_id.clone());
        let history = self.saga_engine.event_store().get_history(&saga_id).await
            .map_err(|e| CommandResult::Failure(e.to_string()))?;
        
        // ════════════════════════════════════════════════════════════
        // EXTRAER STEPS COMPLETADOS
        // ════════════════════════════════════════════════════════════
        
        let completed_steps = self.extract_completed_steps(&history)?;
        
        // ════════════════════════════════════════════════════════════
        // EJECUTAR COMPENSACIONES EN ORDEN INVERSO (LIFO)
        // ════════════════════════════════════════════════════════════
        
        let mut compensation_results = Vec::new();
        
        for step in completed_steps.into_iter().rev() {
            if let Some(compensate_activity) = step.get_compensation_activity() {
                let result = self.execute_compensation(&step, &command.reason).await;
                compensation_results.push(result);
            }
        }
        
        // ════════════════════════════════════════════════════════════
        // RESULTADO
        // ════════════════════════════════════════════════════════════
        
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
        // Extraer steps completados del historial de eventos
        Ok(vec![]) // Simplificado
    }
    
    async fn execute_compensation(&self, step: &CompletedStep, reason: &str) -> CompensationResult {
        // Ejecutar actividad de compensación
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

## 8. Perspectivas Múltiples

### 8.1 API REST (Para Clientes)

```rust
// ═══════════════════════════════════════════════════════════════
// REST API HANDLERS - Para clientes externos
// ═══════════════════════════════════════════════════════════════

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
        Ok(None) => HttpResponse::NotFound().json(json!({
            "error": "Order not found"
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
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
        CommandResult::Failure(msg) => HttpResponse::InternalServerError().json(json!({
            "error": msg
        })),
        _ => HttpResponse::BadRequest().json(json!({})),
    }
}

#[derive(Deserialize)]
pub struct CancelOrderRequest {
    pub reason: String,
    pub request_refund: bool,
}
```

### 8.2 Panel de Administración

```rust
// ═══════════════════════════════════════════════════════════════
// ADMIN API - Para panel de administración
// ═══════════════════════════════════════════════════════════════

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
    // Re-ejecutar desde el último punto de fallo
    HttpResponse::Ok().json(json!({"message": "Retry initiated"}))
}

#[get("/admin/api/v1/orders/{order_id}/compensation")]
async fn get_compensation_status(
    saga_engine: web::Data<Arc<SagaEngine<Es, Tq, Ts>>>,
    order_id: web::Path<String>,
) -> impl Responder {
    HttpResponse::Ok().json(json!({}))
}
```

### 8.3 CLI para Clientes

```rust
// ═══════════════════════════════════════════════════════════════
// CLI COMMANDS - Para clientes
// ═══════════════════════════════════════════════════════════════

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

## 9. Resumen de Optimizaciones

### 9.1 Duplicidades Eliminadas

| Antes (Duplicado) | Después (Optimizado) |
|-------------------|---------------------|
| Eventos `PaymentStarted`, `PaymentFinished` | `PaymentInitiated`, `PaymentCompleted` |
| Commands separados por contexto | `Command` trait unificado + especialización |
| Activities independientes | `EcommerceActivity` trait con `COMPENSATION_ACTIVITY` |
| Payloads diferentes | `BaseEventPayload` + structs específicos |
| Handlers manuales | `CommandHandler<C>` trait genérico |

### 9.2 Patrones Aplicados

| Patrón | Aplicación | Beneficio |
|--------|------------|-----------|
| **Template Method** | `EcommerceActivity` define estructura, implementaciones especializadas | DRY |
| **Strategy** | `DomainEventType` con conversiones dinámicas | Flexibilidad |
| **Command** | Commands tipados con `CommandHandler<C>` genérico | Desacoplamiento |
| **Event Sourcing** | Todos los cambios producen eventos | Auditoría, replay |
| **Compensation** | Auto-tracking via `track_compensatable_step_auto()` | Rollback automático |

### 9.3 Estructura de Archivos Final

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

## 10. Código Completo de Referencia

### 10.1 Resumen Visual de la Arquitectura

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

### 10.2 Flujo de Compensación en Caso de Error

```mermaid
flowchart TD
    A[Start Workflow] --> B[Reserve Inventory]
    B --> C[Process Payment]
    C -->|FAILED| D[Start Compensation]
    
    D --> E[Cancel Shipping<br/>Si se ejecutó]
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

### 10.3 Checklist de Implementación

```markdown
## Checklist para Implementar un Nuevo Workflow

### Domain Layer
- [ ] Definir Value Objects (IDs, Money, etc.)
- [ ] Definir Events (en pasado)
- [ ] Definir Commands (verbo + input)

### Application Layer
- [ ] Implementar Activities (con compensación)
- [ ] Implementar Workflow (DurableWorkflow)
- [ ] Configurar compensation tracker

### Infrastructure Layer
- [ ] Implementar Command Handlers
- [ ] Configurar Event Store
- [ ] Configurar Task Queue

### Testing
- [ ] Unit tests para Activities
- [ ] Integration tests para Workflow
- [ ] Test de compensación (fallo intencional)
```

---

## Apéndice A: Glosario del Ejemplo

| Término | Significado en este Contexto |
|---------|------------------------------|
| **Saga** | La secuencia completa de pasos para procesar un pedido |
| **Workflow** | La definición de cómo se ejecuta la saga |
| **Activity** | Una operación individual (pago, inventario, envío) |
| **Command** | Una intención del usuario (crear orden, cancelar) |
| **Event** | Algo que ya ocurrió (pago completado) |
| **Compensación** | La operación inversa para revertir cambios |

---

## Apéndice B: Diferencias con v3

| Característica | v3 (Old) | v4 (New) |
|----------------|----------|----------|
| Workflow Definition | Steps array | `DurableWorkflow::run()` |
| Activities | `ActivityStep::new()` | `ctx.execute_activity()` |
| Compensation | Manual | `track_compensatable_step_auto()` |
| Events | Basic | Type-safe payloads |
| Commands | None | `CommandHandler<C>` trait |

---

*Document Version: 2.0.0*
*Example: E-Commerce Order Processing Saga*
*Language: English with Spanish terminology for domain concepts*
*Last Updated: 2026-01-28*

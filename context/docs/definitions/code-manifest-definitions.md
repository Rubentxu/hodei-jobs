# Code Manifest Definitions - Guide for Rules Classification

This document provides precise guidance for classifying rules (Integration, Validation, Invariants, Business Rules) when cataloging classes in the code manifest.

## Rule Classification Overview

| Rule Type | When to Mark | When NOT to Mark |
|-----------|--------------|------------------|
| Integration Rules | Classes that filter/reject external data or act as boundary protection | Data containers, internal logic |
| Validation Rules | Technical/format validation, mappers, input sanitization | Business logic, domain rules |
| Invariants | Structural domain constraints, data consistency rules | Technical validation, formatting |
| Business Rules | Application services, domain services with business logic, workflow orchestration | Simple containers, basic DTOs |

---

## Integration Rules (`✓`)

Mark with `✓` when the class:

### Criteria
- **Consumer classes** that validate incoming external data
- **Anti-corruption layers** that translate between bounded contexts
- **External data filters** that reject invalid inputs from outside the system
- **API boundary protection** classes

### Examples

| Class Type | Integration Rules | Rationale |
|------------|-------------------|-----------|
| gRPC Controllers | ✓ | Filter external API requests |
| API Gateway | ✓ | Boundary between external clients and internal services |
| DTO Mappers (external → domain) | ✓ | Transform and validate external data |
| External API Clients | ✓ | Act as boundaries with external systems |
| Repository Implementations | ✓ | Anti-corruption layer for persistence |

### Non-Examples
- Internal domain entities (no external boundary)
- Value objects with business constraints
- Application services orchestrating internal logic

---

## Validation Rules (`✓`)

Mark with `✓` when the class performs:

### Types of Validation
1. **Technical/Format Validation**
   - Email format, URL validation, UUID parsing
   - Date/time format checking
   - Numeric range validation (technical limits)

2. **Data Transformation/Mapping**
   - DTO to domain conversion with validation
   - External model to internal model transformation

3. **Input Sanitization**
   - String escaping, SQL injection prevention
   - HTML sanitization

4. **Schema Validation**
   - JSON schema validation
   - Protocol buffer validation

### Examples

| Class Type | Validation Rules | Rationale |
|------------|------------------|-----------|
| EmailValidator | ✓ | Technical format validation |
| DTO Structs with validation methods | ✓ | Data format checking |
| Mappers (DTO→Domain) | ✓ | Transform and validate external data |
| Request Controllers with validation | ✓ | Input sanitization and validation |
| Configuration Parsers | ✓ | Parse and validate config format |

### Non-Examples
- Domain entities enforcing business rules (use Business Rules)
- Value objects with domain constraints (use Invariants)
- Simple data containers without validation logic

---

## Invariants (`✓`)

Mark with `✓` when the class enforces:

### Types of Invariants
1. **Structural Domain Constraints**
   - Entity state transitions must follow valid paths
   - Aggregate root consistency rules
   - Referential integrity within aggregate

2. **Data Consistency Rules**
   - Non-null business-required fields
   - Business-rule numeric constraints (not technical limits)
   - Temporal consistency (end > start)

3. **Aggregate Boundaries**
   - Invariants that span multiple entities in an aggregate
   - Consistency rules for aggregate root + children

### Examples

| Class Type | Invariants | Rationale |
|------------|------------|-----------|
| Aggregate Roots | ✓ | Enforce state machine transitions |
| Value Objects with business constraints | ✓ | Domain validity rules (e.g., Amount > 0) |
| Entity with state machine | ✓ | Valid state transitions only |
| Domain Services enforcing invariants | ✓ | Cross-entity consistency |

### Examples of Invariant Constraints
```rust
// Domain invariant: Amount must be positive
fn validate(&self) -> Result<()> {
    if self.amount <= 0 {
        Err(DomainError::InvalidAmount)
    } else {
        Ok(())
    }
}

// State machine invariant: Only valid transitions allowed
fn transition_to(&mut self, new_state: State) -> Result<()> {
    if !self.current_state.can_transition_to(&new_state) {
        return Err(DomainError::InvalidStateTransition);
    }
    self.state = new_state;
    Ok(())
}
```

### Non-Examples
- Technical validation (use Validation Rules)
- API request validation (use Validation Rules)
- Simple data presence checks (DTOs)

---

## Business Rules (`✓`)

Mark with `✓` when the class implements:

### Types of Business Logic
1. **Workflow Orchestration**
   - Multi-step business processes
   - Saga coordination
   - Long-running business transactions

2. **Domain Services**
   - Business logic that doesn't naturally fit in an entity
   - Calculations based on domain concepts
   - Business policy enforcement

3. **Application Services (Use Cases)**
   - Use case implementations
   - Transaction boundaries
   - Coordination between multiple aggregates

4. **Policy Enforcement**
   - Pricing calculations with business rules
   - Discount eligibility determination
   - Authorization policies with business context

### Examples

| Class Type | Business Rules | Rationale |
|------------|----------------|-----------|
| CreateUserUseCase | ✓ | Orchestrates user creation with policies |
| PricingCalculator | ✓ | Business pricing logic |
| OrderWorkflowService | ✓ | Multi-step order processing |
| DiscountEligibilityService | ✓ | Business policy evaluation |
| InventoryReservationService | ✓ | Complex inventory management |

### Example Business Rule Implementation
```rust
// Business rule: Discount applies only if customer has gold status and order > $100
pub fn calculate_discount(&self, customer: &Customer, order: &Order) -> Result<Discount> {
    if customer.status == CustomerStatus::Gold && order.total > 100 {
        Ok(Discount::percentage(10))
    } else if order.total > 500 {
        Ok(Discount::percentage(5))
    } else {
        Ok(Discount::none())
    }
}
```

### Non-Examples
- Simple CRUD operations (DTOs)
- Technical validation (use Validation Rules)
- Infrastructure concerns (repositories, external clients)

---

## Empty (Leave Blank)

Leave rule columns empty when the class is:

### Types of Classes
1. **Simple Data Containers**
   - DTOs without validation
   - Plain structs with getters/setters
   - Enums without associated logic

2. **Interface Definitions**
   - Marker traits
   - Simple trait definitions without default implementations

3. **Basic Events**
   - Domain events as simple structs
   - Events without transformation logic

4. **Configuration Objects**
   - Simple config structs without validation

### Examples

| Class Type | Rules | Rationale |
|------------|-------|-----------|
| Plain DTO | Empty | Just data container |
| Status Enum | Empty | Simple values, no logic |
| Marker Trait | Empty | Just interface definition |
| Event Struct | Empty | Data carrier only |

---

## Classification Decision Tree

Use this flow to classify rules:

```
Is this class a boundary with external systems?
├─ YES → Is it filtering/transforming external data?
│   ├─ YES → Integration Rules ✓
│   └─ NO → Integration Rules ✓ (boundary protection)
└─ NO → Does it perform technical format validation?
    ├─ YES → Validation Rules ✓
    └─ NO → Does it enforce domain structural constraints?
        ├─ YES → Invariants ✓
        └─ NO → Does it implement business workflow/logic?
            ├─ YES → Business Rules ✓
            └─ NO → Empty (simple container/interface)
```

---

## Layer-Specific Guidelines

### Domain Layer
- **Entities/Aggregates**: Usually Invariants ✓ (state machine, domain constraints)
- **Value Objects**: Usually Invariants ✓ (business validity)
- **Domain Services**: Usually Business Rules ✓ (business logic)
- **Domain Events**: Usually Empty (simple carriers)
- **Repository Interfaces**: Usually Integration Rules ✓ (boundary)

### Application Layer
- **Use Cases/Services**: Usually Business Rules ✓ (workflow orchestration)
- **Event Handlers**: Usually Business Rules ✓ (reactive business logic)
- **DTOs**: Usually Empty or Validation Rules ✓ (depends on validation)

### Infrastructure Layer
- **Repository Implementations**: Integration Rules ✓ (persistence boundary)
- **External Clients**: Integration Rules ✓ (external system boundary)
- **Mappers (infra)**: Validation Rules ✓ (data transformation)

### Interface Layer
- **Controllers**: Integration Rules ✓ (API boundary)
- **DTOs (interface)**: Validation Rules ✓ (input validation)
- **Presenters**: Usually Empty or Validation Rules ✓ (output formatting)

---

## Common Classification Mistakes to Avoid

### 1. Over-classifying Validation
❌ **WRONG**: Marking every struct with validation methods
✅ **RIGHT**: Only mark if the validation is technical/format-focused

### 2. Under-classifying Business Logic
❌ **WRONG**: Leaving complex workflow orchestration blank
✅ **RIGHT**: Mark complex business workflows with Business Rules ✓

### 3. Confusing Validation with Invariants
❌ **WRONG**: Marking "email format check" as Invariants
✅ **RIGHT**: Technical format check = Validation Rules ✓
❌ **WRONG**: Marking "amount > 0" as Validation Rules
✅ **RIGHT**: Business constraint = Invariants ✓

### 4. Marking Infrastructure as Business Rules
❌ **WRONG**: Marking database repositories as Business Rules
✅ **RIGHT**: Repository implementations = Integration Rules ✓ (persistence boundary)

---

## Quick Reference Table

| Pattern | Layer | Integration | Validation | Invariants | Business Rules |
|---------|-------|-------------|------------|------------|----------------|
| Aggregate Root | Domain | | | ✓ | |
| Value Object (business constraint) | Domain | | | ✓ | |
| Domain Service | Domain | | | | ✓ |
| Use Case | Application | | | | ✓ |
| Repository Interface | Domain | ✓ | | | |
| Repository Impl | Infra | ✓ | | | |
| External Client | Infra | ✓ | | | |
| Controller | Interface | ✓ | | | |
| DTO (no validation) | Any | | | | |
| DTO (with validation) | Interface | | ✓ | | |
| Mapper | Infra | | ✓ | | |
| Domain Event | Domain | | | | |

---

## Document Version
- **Version:** 1.0
- **Created:** 2025-12-30
- **Last Updated:** 2025-12-30

# Análisis de Arquitectura: Worker Provisioning Traits

## Fecha: 2024-01-22
## Estado: En Progreso

---

## 1. Problema Identificado

### 1.1 Traits Actuales

#### `WorkerProvisioning` (Domain Layer)
**Archivo**: `crates/server/domain/src/workers/provisioning.rs`

```rust
#[async_trait]
pub trait WorkerProvisioning: Send + Sync {
    async fn provision_worker(...) -> Result<WorkerProvisioningResult>;
    async fn destroy_worker(...) -> Result<()>;
    async fn terminate_worker(...) -> Result<()>;
    async fn is_provider_available(...) -> Result<bool>;
}
```

#### `WorkerProvisioningService` (Application Layer)
**Archivo**: `crates/server/application/src/workers/provisioning.rs`

```rust
#[async_trait]
pub trait WorkerProvisioningService: Send + Sync {
    async fn provision_worker(...) -> Result<ProvisioningResult>;  // Diferente tipo de retorno
    async fn is_provider_available(...) -> Result<bool>;
    async fn default_worker_spec(...) -> Option<WorkerSpec>;
    async fn list_providers(...) -> Result<Vec<ProviderId>>;
    async fn get_provider_config(...) -> Result<Option<ProviderConfig>>;
    async fn validate_spec(...) -> Result<()>;
    async fn terminate_worker(...) -> Result<()>;
    async fn destroy_worker(...) -> Result<()>;
}
```

### 1.2 Violaciones a Principios SOLID

#### Interface Segregation Principle (ISP)

**Problema**: `WorkerProvisioningService` es un "god trait" que mezcla:
- **Operaciones de aprovisionamiento**: `provision_worker`, `destroy_worker`, `terminate_worker`
- **Operaciones de consulta**: `default_worker_spec`, `list_providers`, `get_provider_config`
- **Operaciones de validación**: `validate_spec`

Esto viola ISP porque:
1. Un cliente que solo necesita aprovisionar se ve obligado a conocer métodos de consulta
2. Un cliente que solo necesita listar proveedores se ve obligado a implementar aprovisionamiento
3. Cambios en un área afectan a todos los clientes

#### Dependency Inversion Principle (DIP)

**Problema**: Múltiples implementaciones concretas sin abstracciones claras:

- `DefaultWorkerProvisioningService`: Implementa ambos traits
- `MockProvisioningService`: Solo implementa `WorkerProvisioningService`
- `MockWorkerProvisioning`: Solo implementa `WorkerProvisioning`

**Violación**: La infraestructura depende de implementaciones concretas en lugar de abstracciones.

---

## 2. Arquitectura Actual (Problemas de Integración)

### 2.1 Flujo de Dependencias Invertido

```
┌─────────────────────────────────────────────────────────────────┐
│                    INFRASTRUCTURE LAYER                         │
│  saga_command_consumers.rs                                      │
│  - Depende de: WorkerProvisioning (domain trait)                │
│  - Handlers del dominio esperan: WorkerProvisioning             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ (usa)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                            │
│  provisioning.rs / provisioning_impl.rs                         │
│  - DefaultWorkerProvisioningService implementa:                 │
│    * WorkerProvisioningService (propia)                         │
│    * WorkerProvisioning (delegando al trait propio)             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ (implementa)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     DOMAIN LAYER                                │
│  workers/provisioning.rs                                        │
│  - Define: WorkerProvisioning                                   │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Problema de Coerción de Traits

Rust **no permite** crear un `dyn TraitA + TraitB` cuando ambos traits tienen métodos no-object-safe (como métodos async).

**Error típico**:
```
error[E0225]: only auto traits can be used as additional traits in a trait object
```

**Solución temporal**: Usar coerción implícita en tiempo de compilación con `as _`.

---

## 3. Propuesta de Refactorización

### 3.1 Segregación de Interfaces (ISP)

#### Trait 1: `WorkerProvisioner` (Operaciones de Aprovisionamiento)
```rust
#[async_trait]
pub trait WorkerProvisioner: Send + Sync {
    async fn provision_worker(...) -> Result<WorkerProvisioningResult>;
    async fn destroy_worker(...) -> Result<()>;
    async fn terminate_worker(...) -> Result<()>;
}
```

#### Trait 2: `WorkerProviderQuery` (Operaciones de Lectura)
```rust
#[async_trait]
pub trait WorkerProviderQuery: Send + Sync {
    async fn is_provider_available(...) -> Result<bool>;
    async fn default_worker_spec(...) -> Option<WorkerSpec>;
    async fn list_providers(...) -> Result<Vec<ProviderId>>;
}
```

#### Trait 3: `WorkerSpecValidator` (Validación)
```rust
#[async_trait]
pub trait WorkerSpecValidator: Send + Sync {
    async fn validate_spec(...) -> Result<()>;
}
```

#### Trait 4: `WorkerProvisioningConfig` (Configuración)
```rust
#[async_trait]
pub trait WorkerProvisioningConfig: Send + Sync {
    async fn get_provider_config(...) -> Result<Option<ProviderConfig>>;
}
```

### 3.2 Composición de Traits

```rust
// Trait combinado para uso general
trait WorkerProvisioningService: WorkerProvisioner 
    + WorkerProviderQuery 
    + WorkerSpecValidator 
    + WorkerProvisioningConfig 
{}

// Trait mínimo para sagas (dependencia del dominio)
trait WorkerProvisioningSaga: WorkerProvisioner + WorkerProviderQuery {}
```

### 3.3 Ventajas de la Segregación

| Antes (Monolítico) | Después (Segregado) |
|-------------------|---------------------|
| 1 trait con 8 métodos | 4 traits con 2-3 métodos cada uno |
| Cambios afectan a todos | Cambios aislados |
| Dificultad para testing | Mock por responsabilidad |
| Dependencias coupling | Composición flexible |

---

## 4. Problemas Encontrados

### 4.1 Coerción de Traits en Rust

**Problema**: No se puede crear `dyn TraitA + TraitB` directamente.

**Causa**: Rust necesita conocer el tamaño en tiempo de compilación para trait objects.

**Solución Actual**: Coerción implícita con `as _`:
```rust
let provisioning_domain: Arc<dyn WorkerProvisioning + Send + Sync> = provisioning as _;
```

### 4.2 Herencia de Traits con async_trait

**Problema**: `trait A: B + Send + Sync` causa conflictos de método.

**Causa**: El macro `#[async_trait]` genera código que entra en conflicto con implementaciones heredadas.

**Solución**: Mantener traits independientes y usar composición.

### 4.3 Dependencias Circulares

**Problema**: Handlers del dominio dependen de `WorkerProvisioning`.

**Causa**: El dominio define el contrato, la infraestructura lo usa.

**Solución**: Mantener `WorkerProvisioning` en el dominio, crear adapters en aplicación.

---

## 5. Solución Meditada (Propuesta Final)

### 5.1 Arquitectura Propuesta

```
                    ┌─────────────────────┐
                    │  Domain Layer       │
                    │  WorkerProvisioning │  ← Contrato mínimo
                    └─────────────────────┘
                              ▲
                              │ implementa
                              │
                    ┌────────┴────────┐
                    │  Application     │
                    │  Layer           │
                    │  WorkerProvision │  ← Aprovisionamiento
                    │  WorkerProviderQ │  ← Consultas
                    │  WorkerSpecVal   │  ← Validación
                    └────────┬────────┘
                              │ composes
                              │
                              ▼
                    ┌─────────────────────┐
                    │  Infrastructure     │
                    │  saga_command_      │
                    │  consumers          │  ← Depende de abstracciones
                    └─────────────────────┘
```

### 5.2 Implementación Recomendada

```rust
// domain/workers/provisioning.rs
#[async_trait]
pub trait WorkerProvisioning: Send + Sync {
    async fn provision_worker(...) -> Result<WorkerProvisioningResult>;
    async fn destroy_worker(...) -> Result<()>;
    async fn terminate_worker(...) -> Result<()>;
    async fn is_provider_available(...) -> Result<bool>;
}

// application/workers/provisioning.rs

// Trait de consultas (nuevo)
#[async_trait]
pub trait WorkerProviderQuery: Send + Sync {
    async fn default_worker_spec(...) -> Option<WorkerSpec>;
    async fn list_providers(...) -> Result<Vec<ProviderId>>;
    async fn get_provider_config(...) -> Result<Option<ProviderConfig>>;
}

// Trait de validación (nuevo)
#[async_trait]
pub trait WorkerSpecValidator: Send + Sync {
    async fn validate_spec(...) -> Result<()>;
}

// Servicio completo
#[async_trait]
pub trait WorkerProvisioningService: 
    WorkerProvisioning          // Delega al domain
    + WorkerProviderQuery       // Consultas
    + WorkerSpecValidator       // Validación
{
    // provision_worker tiene diferente retorno que WorkerProvisioning
    async fn provision_worker(...) -> Result<ProvisioningResult>;
}

// DefaultWorkerProvisioningService implementa todos
#[async_trait]
impl WorkerProvisioning for DefaultWorkerProvisioningService { ... }
#[async_trait]
impl WorkerProviderQuery for DefaultWorkerProvisioningService { ... }
#[async_trait]
impl WorkerSpecValidator for DefaultWorkerProvisioningService { ... }
#[async_trait]
impl WorkerProvisioningService for DefaultWorkerProvisioningService { ... }
```

### 5.3 Inyección de Dependencias

```rust
// startup/mod.rs
let worker_provisioning: Arc<DefaultWorkerProvisioningService> = ...;

// Para infraestructura (usa domain trait)
let provisioning_domain: Arc<dyn WorkerProvisioning + Send + Sync> = 
    worker_provisioning.clone() as _;

// Para aplicación (usa service trait)
let provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync> = 
    worker_provisioning;
```

---

## 6. Pasos de Implementación

### Fase 1: Crear Traits Segregados
- [ ] Crear `WorkerProviderQuery` trait
- [ ] Crear `WorkerSpecValidator` trait
- [ ] Mover métodos correspondientes

### Fase 2: Actualizar Implementaciones
- [ ] Actualizar `DefaultWorkerProvisioningService`
- [ ] Actualizar `MockProvisioningService`
- [ ] Actualizar `MockWorkerProvisioning`

### Fase 3: Refactorizar Consumidores
- [ ] Actualizar `saga_command_consumers` para usar traits correctos
- [ ] Verificar que no hay dependencias circulares
- [ ] Actualizar tests

### Fase 4: Documentación
- [ ] Actualizar docs de arquitectura
- [ ] Crear ejemplos de uso
- [ ] Documentar decisiones de diseño

---

## 7. Conclusiones

### 7.1 Estado Actual
- El código compila pero viola principios SOLID
- Hay duplicación de métodos entre traits
- La integración entre capas requiere casts

### 7.2 Recomendación
1. **Corto plazo**: El diseño actual es funcional pero subóptimo
2. **Mediano plazo**: Implementar segregación de interfaces
3. **Largo plazo**: Considerar eliminar `WorkerProvisioning` y usar solo `WorkerProvisioningService`

### 7.3 Decisión Pendiente
Esperar feedback del equipo antes de proceder con la refactorización completa.

---

## Referencias

- [ISP - Interface Segregation Principle](https://en.wikipedia.org/wiki/Interface_segregation_principle)
- [DIP - Dependency Inversion Principle](https://en.wikipedia.org/wiki/Dependency_inversion_principle)
- [Rust Trait Objects](https://doc.rust-lang.org/book/ch17-02-trait-objects.html)
- [async_trait limitations](https://github.com/dtolnay/async-trait)

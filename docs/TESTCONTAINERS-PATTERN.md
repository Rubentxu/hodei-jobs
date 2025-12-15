# Patrón Single Instance + Resource Pooling para TestContainers

**Versión**: 1.0  
**Fecha**: 2025-12-14

## Resumen

Este documento describe el patrón optimizado de TestContainers implementado en Hodei Jobs Platform para minimizar el uso de recursos computacionales en tests de integración y E2E.

## Problema

Los tests de integración tradicionales con TestContainers tienen un problema de rendimiento:

```
Test 1: Crear container → Ejecutar test → Destruir container (~5s)
Test 2: Crear container → Ejecutar test → Destruir container (~5s)
Test 3: Crear container → Ejecutar test → Destruir container (~5s)
...
Test N: Crear container → Ejecutar test → Destruir container (~5s)

Total: N × 5s = Muy lento para suites grandes
```

**Problemas:**
- Cada test crea y destruye su propio container
- Overhead de ~2-5 segundos por container
- Consumo de memoria: ~50MB × N containers
- Saturación del Docker daemon con muchos tests paralelos

## Solución: Single Instance + Resource Pooling

### Arquitectura

```
┌─────────────────────────────────────────────────────────────────┐
│                    SINGLE INSTANCE                               │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              Postgres Container (único)                      ││
│  │              Creado una sola vez via OnceCell               ││
│  │                                                              ││
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐           ││
│  │  │  test_0_db  │ │  test_1_db  │ │  test_2_db  │  ...      ││
│  │  │  (Test 1)   │ │  (Test 2)   │ │  (Test 3)   │           ││
│  │  └─────────────┘ └─────────────┘ └─────────────┘           ││
│  │                                                              ││
│  │              RESOURCE POOLING                                ││
│  │              Cada test tiene su propia DB aislada           ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Componentes

#### 1. Single Instance (OnceCell)

```rust
/// Global singleton para el container de Postgres
static POSTGRES_CONTEXT: OnceCell<SharedPostgresContext> = OnceCell::const_new();

struct SharedPostgresContext {
    _container: ContainerAsync<Postgres>,  // Mantiene el container vivo
    admin_connection_string: String,        // Para crear/eliminar DBs
    host: String,
    port: u16,
}
```

**Características:**
- El container se crea solo en la primera llamada
- `OnceCell` garantiza inicialización thread-safe
- El container persiste durante toda la ejecución de tests
- Se destruye automáticamente al finalizar el proceso

#### 2. Resource Pooling (Database per Test)

```rust
/// Base de datos aislada por test
pub struct PostgresTestDatabase {
    pub connection_string: String,
    db_name: String,                    // Nombre único: test_<timestamp>_<counter>
    admin_connection_string: String,
}

impl Drop for PostgresTestDatabase {
    fn drop(&mut self) {
        // Limpieza automática: DROP DATABASE
    }
}
```

**Características:**
- Cada test obtiene su propia base de datos
- Nombres únicos usando contador atómico + timestamp
- Limpieza automática en `Drop`
- Aislamiento completo entre tests

#### 3. Atomic Counter para Nombres Únicos

```rust
static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

// Generar nombre único
let db_id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
let db_name = format!("test_{}_{}", timestamp, db_id);
```

**Ventajas sobre UUID:**
- Más rápido (no genera bytes aleatorios)
- Nombres más cortos y legibles
- Orden predecible para debugging

## Rendimiento

### Comparación

| Métrica | Sin Patrón | Con Patrón | Mejora |
|---------|------------|------------|--------|
| Tiempo primer test | ~5s | ~5s | - |
| Tiempo tests siguientes | ~5s | ~100ms | **50x** |
| Memoria (10 tests) | ~500MB | ~50MB | **10x** |
| Containers activos | N | 1 | **Nx** |

### Ejemplo Real

```
Suite de 20 tests:

Sin patrón:  20 × 5s = 100 segundos
Con patrón:  5s + 19 × 0.1s = 6.9 segundos

Mejora: ~14x más rápido
```

## Implementación

### Uso Básico

```rust
#[tokio::test]
async fn my_integration_test() {
    // Obtiene o crea el container compartido + crea DB aislada
    let db = get_postgres_context().await.unwrap();
    
    // Usar la conexión
    let pool = PgPool::connect(&db.connection_string).await.unwrap();
    
    // El test tiene su propia base de datos limpia
    // Al salir del scope, la DB se elimina automáticamente
}
```

### Con TestStack Completo

```rust
#[tokio::test]
async fn my_e2e_test() {
    // Stack completo: Postgres + gRPC Server + Provider
    let stack = TestStack::without_provider().await.unwrap();
    
    // Usar los clientes
    let mut job_client = stack.server.job_client().await.unwrap();
    
    // El stack tiene su propia DB aislada
}
```

### Tests Paralelos

```rust
// Estos tests pueden ejecutarse en paralelo sin conflictos
#[tokio::test]
async fn test_1() {
    let db = get_postgres_context().await.unwrap();
    // Usa test_1734567890_0
}

#[tokio::test]
async fn test_2() {
    let db = get_postgres_context().await.unwrap();
    // Usa test_1734567890_1
}

#[tokio::test]
async fn test_3() {
    let db = get_postgres_context().await.unwrap();
    // Usa test_1734567890_2
}
```

## Diagrama de Secuencia

```
Test 1                    OnceCell                 Docker                  Postgres
  │                          │                        │                       │
  ├─get_postgres_context()──►│                        │                       │
  │                          ├──start container──────►│                       │
  │                          │◄─────container ready───┤                       │
  │                          │                        │                       │
  │◄────SharedContext────────┤                        │                       │
  │                          │                        │                       │
  ├─CREATE DATABASE test_0───┼────────────────────────┼──────────────────────►│
  │◄───────────────OK────────┼────────────────────────┼───────────────────────┤
  │                          │                        │                       │
  │  [ejecutar test]         │                        │                       │
  │                          │                        │                       │
  ├─DROP DATABASE test_0─────┼────────────────────────┼──────────────────────►│
  │                          │                        │                       │

Test 2                    OnceCell                 Docker                  Postgres
  │                          │                        │                       │
  ├─get_postgres_context()──►│                        │                       │
  │◄──SharedContext (cached)─┤  (container ya existe) │                       │
  │                          │                        │                       │
  ├─CREATE DATABASE test_1───┼────────────────────────┼──────────────────────►│
  │◄───────────────OK────────┼────────────────────────┼───────────────────────┤
  │                          │                        │                       │
  │  [ejecutar test]         │                        │                       │
  │                          │                        │                       │
```

## Extensiones Futuras

### Docker Worker Pool

Para tests E2E que requieren workers Docker, se puede aplicar el mismo patrón:

```rust
static WORKER_CONTAINER_POOL: OnceCell<WorkerContainerPool> = OnceCell::const_new();

struct WorkerContainerPool {
    containers: RwLock<Vec<ContainerAsync<WorkerImage>>>,
    available: RwLock<Vec<usize>>,
}

impl WorkerContainerPool {
    async fn acquire(&self) -> WorkerHandle { ... }
    async fn release(&self, handle: WorkerHandle) { ... }
}
```

### Kubernetes Pod Pool

Para tests con Kubernetes provider:

```rust
static K8S_POD_POOL: OnceCell<PodPool> = OnceCell::const_new();
```

## Archivos Relacionados

- `crates/grpc/tests/common/mod.rs` - Implementación del patrón
- `crates/grpc/tests/e2e_docker_provider.rs` - Tests E2E usando el patrón
- `crates/infrastructure/tests/postgres_integration.rs` - Tests de repos

## Referencias

- [TestContainers for Rust](https://rust.testcontainers.org/)
- [tokio::sync::OnceCell](https://docs.rs/tokio/latest/tokio/sync/struct.OnceCell.html)
- [Atomic Operations in Rust](https://doc.rust-lang.org/std/sync/atomic/)

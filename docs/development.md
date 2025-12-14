# Guía de Desarrollo

**Versión**: 7.0  
**Última Actualización**: 2025-12-14

## Estructura del Proyecto

```
hodei-job-platform/
├── proto/                          # Definiciones Protocol Buffers
│   └── hodei_all_in_one.proto      # Proto principal
├── crates/
│   ├── domain/                     # Capa de dominio (DDD)
│   │   └── src/
│   │       ├── shared_kernel.rs    # WorkerId, JobId, ProviderId, States, Errors
│   │       ├── job_execution.rs    # Job aggregate, JobSpec, JobQueue trait
│   │       ├── job_template.rs     # JobTemplate aggregate
│   │       ├── worker.rs           # Worker aggregate, WorkerSpec, WorkerHandle
│   │       ├── worker_provider.rs  # WorkerProvider trait, ProviderCapabilities
│   │       ├── worker_registry.rs  # WorkerRegistry trait
│   │       ├── job_scheduler.rs    # Scheduling strategies
│   │       ├── provider_config.rs  # ProviderConfig
│   │       └── otp_token_store.rs  # OTP authentication
│   ├── application/                # Casos de uso
│   │   └── src/
│   │       ├── job_execution_usecases.rs  # CreateJob, CancelJob
│   │       ├── job_controller.rs          # Control loop
│   │       ├── smart_scheduler.rs         # Scheduling service
│   │       ├── worker_provisioning.rs     # Worker provisioning trait
│   │       ├── worker_provisioning_impl.rs # Default implementation
│   │       ├── worker_lifecycle.rs        # Worker lifecycle management
│   │       └── provider_registry.rs       # Provider management
│   ├── infrastructure/             # Implementaciones
│   │   └── src/
│   │       ├── providers/
│   │       │   ├── docker.rs       # DockerProvider (bollard)
│   │       │   ├── kubernetes.rs   # KubernetesProvider (kube-rs)
│   │       │   └── firecracker.rs  # FirecrackerProvider (KVM)
│   │       ├── persistence.rs      # Postgres repositories (sqlx)
│   │       └── repositories.rs     # In-memory repositories
│   ├── grpc/                       # Servicios gRPC
│   │   └── src/
│   │       ├── services/           # Service implementations
│   │       └── bin/
│   │           ├── server.rs       # Control plane server
│   │           └── worker.rs       # Worker agent
│   ├── interface/                  # REST API (Axum)
│   └── cli/                        # CLI
├── deploy/
│   └── kubernetes/                 # K8s manifests (RBAC, NetworkPolicy)
├── scripts/
│   ├── docker/                     # Docker image build scripts
│   ├── kubernetes/                 # K8s image build scripts
│   └── firecracker/                # Firecracker rootfs build scripts
└── docs/                           # Documentación
```

## Dependencias entre Crates

```mermaid
graph BT
    PROTO[hodei-jobs<br/>Proto Types]
    
    DOM[hodei-jobs-domain<br/>Business Logic]
    
    APP[hodei-jobs-application<br/>Use Cases]
    
    INFRA[hodei-jobs-infrastructure<br/>Implementations]
    
    GRPC[hodei-jobs-grpc<br/>gRPC Adapter]
    
    IFACE[hodei-jobs-interface<br/>REST Adapter]
    
    CLI[hodei-jobs-cli<br/>CLI Adapter]
    
    APP --> DOM
    INFRA --> DOM
    GRPC --> APP
    GRPC --> INFRA
    GRPC --> PROTO
    IFACE --> APP
    IFACE --> INFRA
    CLI --> PROTO
```

---

## Configuración del Entorno

### Requisitos

- **Rust**: 2024 edition (1.83+)
- **protoc**: Protocol Buffers compiler 3.x+
- **Postgres**: requerido para gRPC server y REST API
- **Docker**: Para DockerProvider y tests de integración
- **grpcurl**: Para testing manual de gRPC (opcional)

### Requisitos por Provider

| Provider | Requisitos |
|----------|------------|
| Docker | Docker daemon accesible |
| Kubernetes | kubectl + cluster accesible |
| Firecracker | Linux + KVM + Firecracker binary |

### Setup Inicial

```bash
# Clonar repositorio
git clone <repo-url>
cd hodei-job-platform

# Instalar protoc (Ubuntu/Debian)
sudo apt install protobuf-compiler

# Compilar proyecto
cargo build --workspace

# Ejecutar tests (138 tests)
cargo test --workspace
```

### Variables de Entorno

| Variable | Descripción | Default |
|----------|-------------|---------|
| `HODEI_DATABASE_URL` / `DATABASE_URL` | URL de Postgres (obligatoria) | - |
| `HODEI_DB_MAX_CONNECTIONS` | Máx conexiones SQLx | `10` |
| `HODEI_DB_CONNECTION_TIMEOUT_SECS` | Timeout obtener conexión | `30` |
| `GRPC_PORT` | Puerto del servidor gRPC | `50051` |
| `HODEI_DEV_MODE` | Modo desarrollo (acepta tokens `dev-*`) | `0` |
| `HODEI_SERVER` | URL del servidor gRPC | `http://localhost:50051` |
| `HODEI_TOKEN` | OTP token para autenticación Worker | (requerido) |
| `WORKER_ID` | ID del worker | auto-generado |
| `WORKER_NAME` | Nombre descriptivo del worker | auto-generado |
| `RUST_LOG` | Nivel de logging | `info` |

### Ejecutar Server y Worker

```bash
# Terminal 1: Iniciar servidor gRPC
export HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei"
export HODEI_DB_MAX_CONNECTIONS=10
export HODEI_DB_CONNECTION_TIMEOUT_SECS=30

HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
GRPC_PORT=50051 \
cargo run --bin server -p hodei-jobs-grpc

# Terminal 2: Iniciar worker agent (con OTP)
HODEI_TOKEN=<otp-token> cargo run --bin worker -p hodei-jobs-grpc
```

### Ejecutar REST API (Axum)

El adaptador REST vive en `crates/interface`. En este workspace existe un bin en `src/main.rs` que arranca el servidor HTTP en `127.0.0.1:3000`.

```bash
HODEI_DATABASE_URL="$HODEI_DATABASE_URL" cargo run --bin hodei-server
```

---

## Flujo de Desarrollo

```mermaid
flowchart TD
    START[Nueva Feature] --> DOMAIN{Cambio en Domain?}
    
    DOMAIN -->|Sí| D1[Modificar entidades/aggregates]
    DOMAIN -->|No| APP
    
    D1 --> D2[Agregar tests unitarios]
    D2 --> APP{Nuevo Use Case?}
    
    APP -->|Sí| A1[Crear use case en application]
    APP -->|No| PROTO
    
    A1 --> A2[Tests de use case]
    A2 --> PROTO{Cambio en API?}
    
    PROTO -->|Sí| P1[Modificar .proto]
    PROTO -->|No| IMPL
    
    P1 --> P2[Regenerar código]
    P2 --> IMPL[Implementar en adapter]
    
    IMPL --> I1[gRPC/REST/CLI]
    I1 --> I2[Tests de integración]
    I2 --> REVIEW[Code Review]
    REVIEW --> MERGE[Merge]
```

---

## Convenciones de Código

### Estructura de Archivos

```
crates/<crate-name>/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Exports públicos
│   ├── <module>.rs     # Módulos principales
│   └── tests.rs        # Tests unitarios (cfg(test))
└── tests/              # Tests de integración
    └── integration.rs
```

### Naming Conventions

| Elemento | Convención | Ejemplo |
|----------|------------|---------|
| Crate | kebab-case con prefijo | `hodei-jobs-domain` |
| Module | snake_case | `job_execution` |
| Struct | PascalCase | `JobExecutionService` |
| Trait | PascalCase | `JobRepository` |
| Function | snake_case | `find_by_id` |
| Const | SCREAMING_SNAKE | `MAX_RETRIES` |

### Patrón para Servicios gRPC

```rust
// crates/grpc/src/services/example.rs

use hodei_jobs::*;
use tonic::{Request, Response, Status};

pub struct ExampleServiceImpl {
    // Dependencias
}

impl ExampleServiceImpl {
    pub fn new() -> Self {
        Self { }
    }
}

#[tonic::async_trait]
impl example_service_server::ExampleService for ExampleServiceImpl {
    async fn method_name(
        &self,
        request: Request<MethodRequest>,
    ) -> Result<Response<MethodResponse>, Status> {
        let req = request.into_inner();
        
        // Validación
        // Lógica
        // Respuesta
        
        Ok(Response::new(MethodResponse {
            // campos
        }))
    }
}
```

---

## Testing

### Estadísticas de Tests

| Crate | Unit Tests | Integration Tests | Total |
|-------|------------|-------------------|-------|
| `hodei-jobs-domain` | 49 | - | 49 |
| `hodei-jobs-application` | 7 | - | 7 |
| `hodei-jobs-infrastructure` | 27 | 22 (Docker, K8s, FC) | 49 |
| `hodei-jobs-grpc` | 6 | - | 6 |
| `hodei-jobs-interface` | 6 | - | 6 |
| **Total** | **95** | **22** | **117+** |

### Tests por Provider

| Provider | Unit Tests | Integration Tests |
|----------|------------|-------------------|
| Docker | 6 | 6 (requiere Docker) |
| Kubernetes | 9 | 5 (requiere K8s cluster) |
| Firecracker | 12 | 5 (requiere KVM) |

### Pirámide de Tests

```mermaid
graph TB
    subgraph "E2E Tests"
        E2E[gRPC Integration]
    end
    
    subgraph "Integration Tests"
        INT[Provider Tests<br/>Docker + K8s + Firecracker]
    end
    
    subgraph "Unit Tests"
        UNIT[Domain + Application<br/>~95 tests]
    end
    
    E2E --> INT
    INT --> UNIT
    
    style UNIT fill:#90EE90
    style INT fill:#87CEEB
    style E2E fill:#DDA0DD
```

### Ejecutar Tests

```bash
# Todos los tests
cargo test --workspace

# Tests de un crate específico
cargo test -p hodei-jobs-domain
cargo test -p hodei-jobs-infrastructure

# Tests de providers (unitarios)
cargo test -p hodei-jobs-infrastructure providers::docker
cargo test -p hodei-jobs-infrastructure providers::kubernetes
cargo test -p hodei-jobs-infrastructure providers::firecracker

# Tests de integración por provider (requieren infraestructura)
HODEI_DOCKER_TEST=1 cargo test --test docker_integration -- --ignored
HODEI_K8S_TEST=1 cargo test --test kubernetes_integration -- --ignored
HODEI_FC_TEST=1 cargo test --test firecracker_integration -- --ignored

# Tests que requieren Postgres (marcados con #[ignore])
cargo test -p hodei-jobs-infrastructure --tests -- --ignored

# Tests con output
cargo test --workspace -- --nocapture

# Test específico
cargo test test_kubernetes_config_builder
```

### Tests de Integración gRPC

```rust
// crates/grpc/tests/grpc_integration.rs

#[tokio::test]
async fn test_full_registration_flow() { ... }

#[tokio::test]
async fn test_registration_without_otp_fails() { ... }

#[tokio::test]
async fn test_worker_stream_connection() { ... }

#[tokio::test]
async fn test_e2e_server_sends_job_to_worker() { ... }
```

### Estructura de Tests

```rust
// En src/tests.rs o tests/integration.rs

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers
    fn create_test_job() -> Job {
        // ...
    }

    mod module_tests {
        use super::*;

        #[test]
        fn test_feature_should_do_something() {
            // Arrange
            let job = create_test_job();
            
            // Act
            let result = job.some_method();
            
            // Assert
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_async_feature() {
            // ...
        }
    }
}
```

---

## Agregar Nuevo Servicio gRPC

### Paso 1: Definir en Proto

```protobuf
// proto/hodei_all_in_one.proto

service NewService {
    rpc NewMethod(NewRequest) returns (NewResponse);
}

message NewRequest {
    string field = 1;
}

message NewResponse {
    bool success = 1;
}
```

### Paso 2: Regenerar Código

```bash
cargo build -p hodei-jobs
```

### Paso 3: Implementar Servicio

```rust
// crates/grpc/src/services/new_service.rs

use hodei_jobs::{
    new_service_server::NewService,
    NewRequest, NewResponse,
};
use tonic::{Request, Response, Status};

pub struct NewServiceImpl;

impl NewServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl NewService for NewServiceImpl {
    async fn new_method(
        &self,
        request: Request<NewRequest>,
    ) -> Result<Response<NewResponse>, Status> {
        let req = request.into_inner();
        
        Ok(Response::new(NewResponse {
            success: true,
        }))
    }
}
```

### Paso 4: Registrar en mod.rs

```rust
// crates/grpc/src/services/mod.rs

pub mod new_service;
pub use new_service::NewServiceImpl;
```

### Paso 5: Agregar al Server

```rust
// En el servidor gRPC
Server::builder()
    .add_service(NewServiceServer::new(NewServiceImpl::new()))
    .serve(addr)
    .await?;
```

---

## Debugging

### Logging

```rust
use tracing::{info, warn, error, debug};

// En código
info!("Processing job: {:?}", job_id);
debug!("Detailed state: {:?}", state);
warn!("Retry attempt {} for job {}", attempt, job_id);
error!("Failed to process: {}", error);
```

### Configurar Logs

```bash
# Nivel de log
RUST_LOG=debug cargo run

# Filtrar por módulo
RUST_LOG=hodei_jobs_grpc=debug cargo run
```

---

## CI/CD

```mermaid
flowchart LR
    PUSH[Push/PR] --> CHECK[cargo check]
    CHECK --> FMT[cargo fmt --check]
    FMT --> CLIPPY[cargo clippy]
    CLIPPY --> TEST[cargo test]
    TEST --> BUILD[cargo build --release]
    BUILD --> DEPLOY{Branch?}
    
    DEPLOY -->|main| PROD[Deploy Prod]
    DEPLOY -->|develop| STAGE[Deploy Stage]
    DEPLOY -->|feature| SKIP[Skip Deploy]
```

### Comandos CI

```bash
# Check compilation
cargo check --all

# Format check
cargo fmt --all -- --check

# Linting
cargo clippy --all -- -D warnings

# Tests
cargo test --all

# Build release
cargo build --all --release
```

---

## Troubleshooting

### Errores Comunes

| Error | Causa | Solución |
|-------|-------|----------|
| `protoc not found` | protoc no instalado | `apt install protobuf-compiler` |
| `unresolved import` | Módulo no exportado | Verificar `mod` y `pub use` |
| `trait not implemented` | Falta impl de trait | Implementar trait requerido |
| `field is private` | Campo no público | Usar getters o hacer `pub` |
| `HODEI_TOKEN not set` | OTP token no configurado | Exportar `HODEI_TOKEN=<token>` |
| `registration failed` | OTP inválido o expirado | Generar nuevo token en servidor |
| `stream disconnected` | Conexión perdida | Worker se reconecta automáticamente |
| `Docker not available` | Docker daemon no corre | `sudo systemctl start docker` |

### Regenerar Protos

```bash
# Limpiar y regenerar
cargo clean -p hodei-jobs
cargo build -p hodei-jobs
```

### Depuración de gRPC

```bash
# Ver logs detallados
RUST_LOG=hodei_jobs_grpc=debug cargo run --bin server

# Probar con grpcurl
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext localhost:50051 hodei.WorkerAgentService/Register

# Ver métricas
# Nota: por defecto el backend de métricas está deshabilitado y responderá failed_precondition.
grpcurl -plaintext localhost:50051 hodei.MetricsService/GetAggregatedMetrics
```

### Docker Provider

```bash
# Verificar Docker disponible
docker version

# Listar contenedores de workers
docker ps -a --filter "label=hodei.worker"

# Ver logs de un worker
docker logs hodei-worker-<id>
```

---

## Arquitectura de Componentes

### WorkerAgentService (PRD v6.0)

```rust
// Flujo de autenticación OTP
pub async fn register(&self, request: Request<RegisterWorkerRequest>) 
    -> Result<Response<RegisterWorkerResponse>, Status> {
    let req = request.into_inner();
    
    // 1. Validar OTP token
    if !self.validate_otp(&req.auth_token).await {
        return Err(Status::unauthenticated("Invalid or expired OTP"));
    }
    
    // 2. Generar session_id para reconexiones
    let session_id = if req.session_id.is_empty() {
        format!("sess_{}", Uuid::new_v4())
    } else {
        req.session_id
    };
    
    // 3. Registrar worker
    // ...
}
```

### LogStreamService

```rust
// Servicio de logs en tiempo real
let service = LogStreamService::new();

// Añadir log
service.append_log(LogEntry {
    job_id: "job-123".to_string(),
    line: "Executing...".to_string(),
    is_stderr: false,
    timestamp: Some(now()),
}).await;

// Suscribirse a logs
let mut stream = service.subscribe("job-123").await;
while let Some(log) = stream.next().await {
    println!("{}: {}", log.job_id, log.line);
}
```

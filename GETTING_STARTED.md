# Getting Started - Hodei Jobs Platform

**Versión**: 7.0  
**Última Actualización**: 2025-12-14

Guía completa para desarrolladores y usuarios para probar toda la funcionalidad de la plataforma de ejecución de jobs distribuidos.

## 📋 Índice

1. [Requisitos Previos](#1-requisitos-previos)
2. [Compilación y Tests](#2-compilación-y-tests)
3. [Flujo de Pruebas Secuencial](#3-flujo-de-pruebas-secuencial)
   - [Fase 1: Infraestructura](#fase-1-infraestructura)
   - [Fase 2: Servidor y Worker](#fase-2-servidor-y-worker)
   - [Fase 3: Jobs Básicos](#fase-3-jobs-básicos)
   - [Fase 4: Providers](#fase-4-providers)
   - [Fase 5: Logs y Métricas](#fase-5-logs-y-métricas)
4. [Pruebas por Provider](#4-pruebas-por-provider)
5. [Scripts de Automatización](#5-scripts-de-automatización)
6. [API REST](#6-api-rest)
7. [Troubleshooting](#7-troubleshooting)

---

## 1. Requisitos Previos

### Requisitos Base

```bash
# Rust (1.83+ / 2024 edition)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Protocol Buffers compiler
# Ubuntu/Debian
sudo apt install protobuf-compiler

# macOS
brew install protobuf

# Verificar versiones
rustc --version    # >= 1.83.0
protoc --version   # >= 3.0.0

# grpcurl (para testing manual)
# macOS
brew install grpcurl

# Linux (con Go)
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest
export PATH=$PATH:$(go env GOPATH)/bin
```

### Requisitos por Provider

| Provider | Requisitos |
|----------|------------|
| **Docker** | Docker daemon accesible (`docker ps` funciona) |
| **Kubernetes** | kubectl + cluster accesible (Kind, Minikube, o real) |
| **Firecracker** | Linux + KVM (`/dev/kvm`) + Firecracker binary |

```bash
# Docker
sudo apt install docker.io
sudo usermod -aG docker $USER
# Reiniciar sesión

# Kubernetes (Kind para desarrollo)
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
chmod +x ./kind && sudo mv ./kind /usr/local/bin/kind
kind create cluster --name hodei-dev

# Firecracker (solo Linux con KVM)
# Ver scripts/firecracker/README.md
```

---

## 2. Compilación y Tests

### Clonar y Compilar

```bash
git clone <repo-url>
cd hodei-job-platform

# Compilar workspace completo
cargo build --workspace

# Compilar en release (optimizado)
cargo build --workspace --release
```

### Ejecutar Tests

```bash
# Todos los tests unitarios (~95 tests)
cargo test --workspace

# Tests con output detallado
cargo test --workspace -- --nocapture

# Tests por crate
cargo test -p hodei-jobs-domain           # 49 tests
cargo test -p hodei-jobs-application      # 7 tests
cargo test -p hodei-jobs-infrastructure   # 27 tests
cargo test -p hodei-jobs-grpc             # 6 tests

# Tests de providers (unitarios)
cargo test -p hodei-jobs-infrastructure providers::docker
cargo test -p hodei-jobs-infrastructure providers::kubernetes
cargo test -p hodei-jobs-infrastructure providers::firecracker
```

### Tests de Integración (requieren infraestructura)

```bash
# Docker provider (requiere Docker daemon)
cargo test --test docker_integration -- --ignored

# Kubernetes provider (requiere cluster)
HODEI_K8S_TEST=1 cargo test --test kubernetes_integration -- --ignored

# Firecracker provider (requiere KVM)
HODEI_FC_TEST=1 cargo test --test firecracker_integration -- --ignored

# Postgres (requiere Postgres o Testcontainers)
cargo test -p hodei-jobs-infrastructure --tests -- --ignored
```

### Verificar Calidad de Código

```bash
# Linting
cargo clippy --workspace -- -D warnings

# Formateo
cargo fmt --all -- --check

# Check sin compilar
cargo check --workspace
```

---

## 3. Flujo de Pruebas Secuencial

Este flujo está diseñado para probar toda la funcionalidad de forma ordenada y coherente.

### Fase 1: Infraestructura

**Objetivo:** Verificar que la infraestructura base funciona.

#### 1.1 Iniciar Postgres

```bash
# Terminal 0: Postgres
docker run --rm -d \
  --name hodei-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_DB=hodei \
  -p 5432:5432 \
  postgres:16-alpine

# Exportar variables
export HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei"
```

#### 1.2 Verificar conectividad

```bash
# Verificar Postgres
psql "$HODEI_DATABASE_URL" -c "SELECT 1"

# Verificar Docker (si usas Docker provider)
docker ps

# Verificar Kubernetes (si usas K8s provider)
kubectl cluster-info

# Verificar KVM (si usas Firecracker provider)
ls -la /dev/kvm
```

### Fase 2: Servidor y Worker

**Objetivo:** Levantar el control plane y un worker.

#### 2.1 Iniciar Servidor gRPC

```bash
# Terminal 1: Servidor (modo desarrollo)
HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
HODEI_DEV_MODE=1 \
HODEI_DOCKER_ENABLED=1 \
GRPC_PORT=50051 \
cargo run --bin server -p hodei-jobs-grpc

# Output esperado:
# ╔═══════════════════════════════════════════════════════════════╗
# ║           Hodei Jobs Platform - gRPC Server                   ║
# ╚═══════════════════════════════════════════════════════════════╝
# Starting server on 0.0.0.0:50051
# Services initialized:
#   ✓ WorkerAgentService (with OTP authentication)
#   ✓ JobExecutionService
#   ✓ SchedulerService
#   ✓ Docker provider initialized
```

#### 2.2 Verificar Servicios

```bash
# Terminal 2: Verificar servicios disponibles
grpcurl -plaintext localhost:50051 list

# Output esperado:
# hodei.JobExecutionService
# hodei.LogStreamService
# hodei.MetricsService
# hodei.ProviderManagementService
# hodei.SchedulerService
# hodei.WorkerAgentService
```

#### 2.3 Iniciar Worker Agent

```bash
# Terminal 3: Worker (modo desarrollo)
HODEI_TOKEN=dev-test \
HODEI_SERVER=http://localhost:50051 \
RUST_LOG=info \
cargo run --bin worker -p hodei-jobs-grpc

# Output esperado:
# ╔═══════════════════════════════════════════════════════════════╗
# ║           Hodei Jobs Worker Agent                             ║
# ╚═══════════════════════════════════════════════════════════════╝
# ✓ Connected to server
# ✓ Registered successfully (session: sess_xyz789)
# ✓ Connected to job stream. Waiting for commands...
```

### Fase 3: Jobs Básicos

**Objetivo:** Probar el ciclo de vida completo de un job.

#### 3.1 Registrar Worker manualmente (alternativa)

```bash
# Terminal 4: Cliente gRPC
grpcurl -plaintext -d '{
  "auth_token": "dev-test",
  "worker_info": {
    "worker_id": {"value": "worker-001"},
    "name": "Test Worker",
    "hostname": "localhost",
    "capabilities": ["shell", "docker"]
  }
}' localhost:50051 hodei.WorkerAgentService/Register

# Respuesta esperada:
# {"workerId": {"value": "worker-001"}, "success": true, "sessionId": "sess_..."}
```

#### 3.2 Encolar un Job

```bash
# Generar UUID para el job
JOB_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
echo "JOB_ID=$JOB_ID"

# Encolar job
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_ID"'"},
    "name": "Test Job",
    "command": "echo",
    "arguments": ["Hello from Hodei!"],
    "environment": {"DEBUG": "true"}
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Respuesta: {"success": true}
```

#### 3.3 Asignar Job a Worker

```bash
# Asignar job al worker
grpcurl -plaintext -d '{
  "job_id": {"value": "'"$JOB_ID"'"},
  "worker_id": {"value": "worker-001"}
}' localhost:50051 hodei.JobExecutionService/AssignJob

# Respuesta: {"executionId": {"value": "exec-..."}, "success": true}
# ⚠️ GUARDAR el executionId para los siguientes pasos
EXEC_ID="<valor-del-executionId>"
```

#### 3.4 Iniciar Job

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "'"$EXEC_ID"'"},
  "job_id": {"value": "'"$JOB_ID"'"},
  "worker_id": {"value": "worker-001"}
}' localhost:50051 hodei.JobExecutionService/StartJob
```

#### 3.5 Completar Job

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "'"$EXEC_ID"'"},
  "job_id": {"value": "'"$JOB_ID"'"},
  "exit_code": "0",
  "output": "Hello from Hodei!"
}' localhost:50051 hodei.JobExecutionService/CompleteJob
```

#### 3.6 Job de Larga Duración (con logs)

```bash
# Job que genera logs progresivos
JOB_LONG=$(uuidgen | tr '[:upper:]' '[:lower:]')

grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_LONG"'"},
    "name": "Long Running Job",
    "command": "bash",
    "arguments": ["-c", "for i in 1 2 3 4 5; do echo \"Step $i of 5\"; sleep 1; done; echo Done!"]
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Ver logs en Terminal 1 (servidor) mientras ejecuta
```

#### 3.7 Cancelar Job

```bash
grpcurl -plaintext -d '{
  "job_id": {"value": "'"$JOB_ID"'"},
  "reason": "User cancelled"
}' localhost:50051 hodei.JobExecutionService/CancelJob
```

### Fase 4: Providers

**Objetivo:** Probar los diferentes providers de workers.

#### 4.1 Verificar Providers Disponibles

```bash
grpcurl -plaintext localhost:50051 hodei.ProviderManagementService/ListProviders
```

#### 4.2 Verificar entorno de cada Provider

```bash
# Docker
./scripts/docker/check-env.sh

# Kubernetes
./scripts/kubernetes/check-env.sh

# Firecracker
./scripts/firecracker/check-env.sh
```

#### 4.3 Habilitar Providers (reiniciar servidor)

```bash
# Con Docker
HODEI_DOCKER_ENABLED=1 cargo run --bin server -p hodei-jobs-grpc

# Con Kubernetes
HODEI_K8S_ENABLED=1 HODEI_K8S_NAMESPACE=hodei-workers cargo run --bin server -p hodei-jobs-grpc

# Con Firecracker
HODEI_FC_ENABLED=1 HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux cargo run --bin server -p hodei-jobs-grpc

# Múltiples providers
HODEI_DOCKER_ENABLED=1 HODEI_K8S_ENABLED=1 cargo run --bin server -p hodei-jobs-grpc
```

### Fase 5: Logs y Métricas

**Objetivo:** Probar streaming de logs y métricas.

#### 5.1 Suscribirse a Logs de un Job

```bash
# Suscribirse a logs (streaming)
grpcurl -plaintext -d '{
  "job_id": "'"$JOB_LONG"'",
  "include_history": true,
  "tail_lines": 100
}' localhost:50051 hodei.LogStreamService/SubscribeLogs

# Output (streaming):
# {"jobId":"...","line":"Step 1 of 5","isStderr":false}
# {"jobId":"...","line":"Step 2 of 5","isStderr":false}
# ...
```

#### 5.2 Obtener Logs Históricos

```bash
grpcurl -plaintext -d '{
  "job_id": "'"$JOB_LONG"'",
  "limit": 50
}' localhost:50051 hodei.LogStreamService/GetLogs
```

#### 5.3 Ver Estado de Cola

```bash
grpcurl -plaintext -d '{
  "scheduler_name": "default"
}' localhost:50051 hodei.SchedulerService/GetQueueStatus
```

#### 5.4 Listar Workers Disponibles

```bash
grpcurl -plaintext -d '{
  "scheduler_name": "default"
}' localhost:50051 hodei.SchedulerService/GetAvailableWorkers
```

---

## 4. Pruebas por Provider

### Docker Provider

```bash
# 1. Verificar entorno
./scripts/docker/check-env.sh

# 2. Construir imagen de worker
./scripts/docker/build-worker-image.sh -t hodei-worker:dev

# 3. Iniciar servidor con Docker provider
HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
HODEI_DOCKER_ENABLED=1 \
HODEI_WORKER_IMAGE=hodei-worker:dev \
cargo run --bin server -p hodei-jobs-grpc

# 4. El servidor provisionará workers automáticamente cuando haya jobs
```

### Kubernetes Provider

```bash
# 1. Verificar entorno
./scripts/kubernetes/check-env.sh

# 2. Instalar manifiestos base
./deploy/kubernetes/install.sh

# 3. Construir y push imagen
./scripts/kubernetes/build-worker-image.sh -r your-registry.io/hodei -t v1.0.0 -p

# 4. Iniciar servidor con K8s provider
HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
HODEI_K8S_ENABLED=1 \
HODEI_K8S_NAMESPACE=hodei-workers \
HODEI_WORKER_IMAGE=your-registry.io/hodei/hodei-worker:v1.0.0 \
cargo run --bin server -p hodei-jobs-grpc

# 5. Verificar pods creados
kubectl get pods -n hodei-workers -w
```

### Firecracker Provider (Linux con KVM)

```bash
# 1. Verificar entorno
./scripts/firecracker/check-env.sh

# 2. Construir rootfs (requiere root)
sudo ./scripts/firecracker/build-rootfs.sh

# 3. Descargar kernel
curl -L -o /tmp/vmlinux \
  "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.9/x86_64/vmlinux-5.10.217"
sudo mv /tmp/vmlinux /var/lib/hodei/vmlinux

# 4. Iniciar servidor con Firecracker provider
HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
HODEI_FC_ENABLED=1 \
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux \
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4 \
sudo cargo run --bin server -p hodei-jobs-grpc
```

---

## 5. Scripts de Automatización

### Script de Prueba Completa

Crea `test_e2e.sh`:

```bash
#!/bin/bash
# test_e2e.sh - Prueba E2E completa

set -e

SERVER="localhost:50051"
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "═══════════════════════════════════════════════════"
echo "  HODEI - Prueba E2E Completa"
echo "═══════════════════════════════════════════════════"

# 1. Verificar servidor
echo -e "\n[1/6] Verificando servidor..."
if grpcurl -plaintext $SERVER list > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Servidor disponible"
else
    echo -e "${RED}✗${NC} Servidor no disponible"
    exit 1
fi

# 2. Registrar Worker
echo -e "\n[2/6] Registrando Worker..."
WORKER_RESULT=$(grpcurl -plaintext -d '{
  "auth_token": "dev-test",
  "worker_info": {
    "worker_id": {"value": "worker-e2e"},
    "name": "E2E Test Worker",
    "hostname": "localhost",
    "capabilities": ["shell"]
  }
}' $SERVER hodei.WorkerAgentService/Register 2>&1)

if echo "$WORKER_RESULT" | grep -q '"success": true'; then
    echo -e "${GREEN}✓${NC} Worker registrado"
else
    echo -e "${RED}✗${NC} Error registrando worker: $WORKER_RESULT"
fi

# 3. Encolar Job
echo -e "\n[3/6] Encolando Job..."
JOB_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_ID"'"},
    "name": "E2E Test Job",
    "command": "bash",
    "arguments": ["-c", "echo Start; sleep 2; echo Done"]
  },
  "queued_by": "e2e-test"
}' $SERVER hodei.JobExecutionService/QueueJob
echo -e "${GREEN}✓${NC} Job encolado: $JOB_ID"

# 4. Asignar Job
echo -e "\n[4/6] Asignando Job..."
ASSIGN_RESULT=$(grpcurl -plaintext -d '{
  "job_id": {"value": "'"$JOB_ID"'"},
  "worker_id": {"value": "worker-e2e"}
}' $SERVER hodei.JobExecutionService/AssignJob)
EXEC_ID=$(echo "$ASSIGN_RESULT" | grep -oP '"value":\s*"\K[^"]+' | head -1)
echo -e "${GREEN}✓${NC} Job asignado, execution_id: $EXEC_ID"

# 5. Iniciar Job
echo -e "\n[5/6] Iniciando Job..."
grpcurl -plaintext -d '{
  "execution_id": {"value": "'"$EXEC_ID"'"},
  "job_id": {"value": "'"$JOB_ID"'"},
  "worker_id": {"value": "worker-e2e"}
}' $SERVER hodei.JobExecutionService/StartJob > /dev/null
echo -e "${GREEN}✓${NC} Job iniciado"

# 6. Esperar y completar
echo -e "\n[6/6] Esperando ejecución (3s)..."
sleep 3

grpcurl -plaintext -d '{
  "execution_id": {"value": "'"$EXEC_ID"'"},
  "job_id": {"value": "'"$JOB_ID"'"},
  "exit_code": "0",
  "output": "E2E test completed"
}' $SERVER hodei.JobExecutionService/CompleteJob > /dev/null
echo -e "${GREEN}✓${NC} Job completado"

echo -e "\n═══════════════════════════════════════════════════"
echo -e "  ${GREEN}✅ Prueba E2E Completada${NC}"
echo "═══════════════════════════════════════════════════"
```

```bash
chmod +x test_e2e.sh
./test_e2e.sh
```

### Script de Verificación de Entorno

```bash
#!/bin/bash
# check_all_env.sh - Verificar todos los entornos

echo "=== Verificación de Entorno Hodei ==="
echo

echo "--- Requisitos Base ---"
echo -n "Rust: "; rustc --version 2>/dev/null || echo "NO INSTALADO"
echo -n "Protoc: "; protoc --version 2>/dev/null || echo "NO INSTALADO"
echo -n "grpcurl: "; grpcurl --version 2>/dev/null || echo "NO INSTALADO"
echo

echo "--- Docker Provider ---"
./scripts/docker/check-env.sh 2>/dev/null || echo "Script no encontrado"
echo

echo "--- Kubernetes Provider ---"
./scripts/kubernetes/check-env.sh 2>/dev/null || echo "Script no encontrado"
echo

echo "--- Firecracker Provider ---"
./scripts/firecracker/check-env.sh 2>/dev/null || echo "Script no encontrado"
```

---

## 6. API REST

La API REST vive en `crates/interface` (Axum) con los endpoints:

- `POST /api/v1/jobs`
- `GET /api/v1/jobs/{job_id}`
- `POST /api/v1/jobs/{job_id}/cancel`
- `POST /api/v1/providers`
- `GET /api/v1/providers`
- `GET /api/v1/providers/{provider_id}`
- `GET /health`

### Prueba manual REST (curl)

```bash
# Health
curl -s http://127.0.0.1:3000/health | jq

# Crear job
curl -s -X POST http://127.0.0.1:3000/api/v1/jobs \
  -H 'content-type: application/json' \
  -d '{
    "spec": {
      "command": ["echo", "hello"],
      "env": {"DEBUG": "true"},
      "timeout_ms": 10000
    },
    "correlation_id": "manual-test"
  }' | jq

# Consultar status
JOB_ID="<job_id_uuid>"
curl -s http://127.0.0.1:3000/api/v1/jobs/$JOB_ID | jq

# Cancelar
curl -s -X POST http://127.0.0.1:3000/api/v1/jobs/$JOB_ID/cancel | jq
```

---

## 7. Troubleshooting

### Errores Comunes

#### "Cannot connect to gRPC server"
```bash
# Verificar que el servidor está corriendo
ps aux | grep server
ss -tlnp | grep 50051

# Reiniciar servidor
cargo run --bin server -p hodei-jobs-grpc
```

#### "HODEI_TOKEN not set" o "Invalid OTP"
```bash
# En modo desarrollo, usar token con prefijo "dev-"
HODEI_DEV_MODE=1 cargo run --bin server -p hodei-jobs-grpc  # Servidor
HODEI_TOKEN=dev-test cargo run --bin worker -p hodei-jobs-grpc  # Worker
```

#### "Docker not available"
```bash
docker version
sudo systemctl start docker
sudo usermod -aG docker $USER  # Reiniciar sesión después
```

#### "Kubernetes cluster not reachable"
```bash
kubectl cluster-info
kubectl config current-context
```

#### "KVM not available" (Firecracker)
```bash
ls -la /dev/kvm
sudo modprobe kvm_intel  # o kvm_amd
sudo chmod 666 /dev/kvm
```

#### Error de compilación con protobuf
```bash
protoc --version
sudo apt install protobuf-compiler  # Ubuntu/Debian
cargo clean -p hodei-jobs && cargo build -p hodei-jobs
```

### Ver Logs Detallados

```bash
# Servidor con debug
RUST_LOG=debug cargo run --bin server -p hodei-jobs-grpc

# Worker con debug
RUST_LOG=debug cargo run --bin worker -p hodei-jobs-grpc

# Solo módulos específicos
RUST_LOG=hodei_jobs_grpc::services=debug cargo run --bin server -p hodei-jobs-grpc
```

---

## Variables de Entorno

### Servidor

| Variable | Descripción | Default |
|----------|-------------|---------|
| `HODEI_DATABASE_URL` | URL de Postgres (obligatoria) | - |
| `GRPC_PORT` | Puerto del servidor gRPC | `50051` |
| `HODEI_DEV_MODE` | Modo desarrollo (acepta tokens `dev-*`) | `0` |
| `HODEI_DOCKER_ENABLED` | Habilitar Docker provider | `0` |
| `HODEI_K8S_ENABLED` | Habilitar Kubernetes provider | `0` |
| `HODEI_K8S_NAMESPACE` | Namespace para workers K8s | `hodei-workers` |
| `HODEI_FC_ENABLED` | Habilitar Firecracker provider | `0` |
| `HODEI_FC_KERNEL_PATH` | Path al kernel de Firecracker | - |
| `HODEI_FC_ROOTFS_PATH` | Path al rootfs de Firecracker | - |
| `HODEI_WORKER_IMAGE` | Imagen de worker por defecto | `hodei-worker:latest` |
| `RUST_LOG` | Nivel de logging | `info` |

### Worker

| Variable | Descripción | Default |
|----------|-------------|---------|
| `HODEI_TOKEN` | OTP token (o `dev-*` en desarrollo) | - |
| `HODEI_SERVER` | URL del servidor gRPC | `http://localhost:50051` |
| `WORKER_ID` | ID único del worker | Auto-generado |
| `RUST_LOG` | Nivel de logging | `info` |

---

## 📚 Documentación Adicional

| Documento | Descripción |
|-----------|-------------|
| [`docs/architecture.md`](docs/architecture.md) | Arquitectura DDD y servicios |
| [`docs/development.md`](docs/development.md) | Guía de desarrollo |
| [`docs/use-cases.md`](docs/use-cases.md) | Casos de uso y diagramas |
| [`docs/workflows.md`](docs/workflows.md) | Flujos de trabajo |
| [`scripts/README.md`](scripts/README.md) | Scripts de build |
| [`deploy/kubernetes/README.md`](deploy/kubernetes/README.md) | Despliegue en K8s |

---

## Resumen de Comandos

```bash
# === SETUP ===
cargo build --workspace
cargo test --workspace

# === INFRAESTRUCTURA ===
docker run --rm -d --name hodei-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16-alpine
export HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei"

# === SERVIDOR ===
HODEI_DEV_MODE=1 HODEI_DOCKER_ENABLED=1 cargo run --bin server -p hodei-jobs-grpc

# === WORKER ===
HODEI_TOKEN=dev-test cargo run --bin worker -p hodei-jobs-grpc

# === VERIFICAR ===
grpcurl -plaintext localhost:50051 list

# === TESTS ===
cargo test -p hodei-jobs-infrastructure providers::docker
cargo test -p hodei-jobs-infrastructure providers::kubernetes
cargo test -p hodei-jobs-infrastructure providers::firecracker
```

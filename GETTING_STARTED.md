# Getting Started - Hodei Jobs Platform

**Versión**: 7.1  
**Última Actualización**: 2025-12-15

Guía completa para desarrolladores y usuarios para probar toda la funcionalidad de la plataforma de ejecución de jobs distribuidos.

## 📋 Índice

1. [Arquitectura y Ciclo de Vida del Job](#-arquitectura-y-ciclo-de-vida-del-job)
2. [Requisitos Previos](#1-requisitos-previos)
3. [Compilación y Tests](#2-compilación-y-tests)
4. [Flujo de Pruebas Secuencial](#3-flujo-de-pruebas-secuencial)
   - [Fase 1: Infraestructura](#fase-1-infraestructura)
   - [Fase 2: Servidor gRPC](#fase-2-servidor-grpc)
   - [Fase 3: Ejecutar Jobs](#fase-3-ejecutar-jobs)
   - [Fase 4: Providers](#fase-4-providers)
   - [Fase 5: Logs y Métricas](#fase-5-logs-y-métricas)
5. [Pruebas por Provider](#4-pruebas-por-provider)
6. [Scripts de Automatización](#5-scripts-de-automatización)
7. [API REST](#6-api-rest)
8. [Troubleshooting](#7-troubleshooting)
9. [Variables de Entorno](#variables-de-entorno)

---

## 🏗️ Arquitectura y Ciclo de Vida del Job

### Componentes del Sistema

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           HODEI JOBS PLATFORM                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐     ┌──────────────────────────────────────────────────┐  │
│  │   Cliente   │     │              gRPC Server (Control Plane)         │  │
│  │  (grpcurl)  │────▶│  ┌────────────────┐  ┌─────────────────────────┐ │  │
│  └─────────────┘     │  │ JobExecution   │  │ WorkerAgentService      │ │  │
│                      │  │ Service        │  │ (OTP Auth + Streams)    │ │  │
│                      │  └───────┬────────┘  └────────────┬────────────┘ │  │
│                      │          │                        │              │  │
│                      │  ┌───────▼────────┐  ┌────────────▼────────────┐ │  │
│                      │  │ Scheduler      │  │ ProviderManagement      │ │  │
│                      │  │ Service        │  │ Service                 │ │  │
│                      │  └───────┬────────┘  └────────────┬────────────┘ │  │
│                      │          │                        │              │  │
│                      │  ┌───────▼────────────────────────▼────────────┐ │  │
│                      │  │         Worker Provisioning Service         │ │  │
│                      │  │  (Genera OTP + Aprovisiona Workers)         │ │  │
│                      │  └───────────────────┬─────────────────────────┘ │  │
│                      └──────────────────────┼───────────────────────────┘  │
│                                             │                              │
│                      ┌──────────────────────▼───────────────────────────┐  │
│                      │              Worker Providers                    │  │
│                      │  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │  │
│                      │  │  Docker  │  │   K8s    │  │ Firecracker  │   │  │
│                      │  │ Provider │  │ Provider │  │   Provider   │   │  │
│                      │  └────┬─────┘  └────┬─────┘  └──────┬───────┘   │  │
│                      └───────┼─────────────┼───────────────┼───────────┘  │
│                              │             │               │              │
│                      ┌───────▼─────┐ ┌─────▼─────┐ ┌───────▼───────┐     │
│                      │  Container  │ │    Pod    │ │    microVM    │     │
│                      │  (Worker)   │ │  (Worker) │ │   (Worker)    │     │
│                      │             │ │           │ │               │     │
│                      │ ┌─────────┐ │ │┌─────────┐│ │ ┌───────────┐ │     │
│                      │ │ Worker  │ │ ││ Worker  ││ │ │  Worker   │ │     │
│                      │ │ Binary  │ │ ││ Binary  ││ │ │  Binary   │ │     │
│                      │ └─────────┘ │ │└─────────┘│ │ └───────────┘ │     │
│                      └─────────────┘ └───────────┘ └───────────────┘     │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### Ciclo de Vida del Job (Flujo Automático)

El sistema opera con **aprovisionamiento bajo demanda**. Los usuarios solo encolan jobs; el resto es automático.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        CICLO DE VIDA DEL JOB                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. QUEUE JOB                                                                │
│  ───────────                                                                 │
│  Cliente ──▶ JobExecutionService.QueueJob(job_definition)                    │
│              │                                                               │
│              ▼                                                               │
│  Job guardado en DB con estado: PENDING                                      │
│                                                                              │
│  2. SCHEDULING                                                               │
│  ────────────                                                                │
│  SchedulerService detecta job PENDING                                        │
│              │                                                               │
│              ▼                                                               │
│  ¿Hay workers disponibles? ──NO──▶ WorkerProvisioningService                 │
│              │                              │                                │
│             YES                             ▼                                │
│              │                     Genera OTP para worker                    │
│              │                              │                                │
│              │                              ▼                                │
│              │                     Provider.create_worker(spec + OTP)        │
│              │                              │                                │
│              │                              ▼                                │
│              │                     Container/Pod/microVM arranca             │
│              │                              │                                │
│              │                              ▼                                │
│              │                     Worker binary se ejecuta                  │
│              │                              │                                │
│              │  ┌───────────────────────────┘                                │
│              │  │                                                            │
│  3. WORKER REGISTRATION (Automático)                                         │
│  ───────────────────────────────────                                         │
│              │  │                                                            │
│              │  ▼                                                            │
│              │  Worker lee HODEI_OTP_TOKEN del entorno                       │
│              │              │                                                │
│              │              ▼                                                │
│              │  Worker ──▶ WorkerAgentService.Register(otp_token)            │
│              │              │                                                │
│              │              ▼                                                │
│              │  Server valida OTP ──▶ Worker registrado (session_id)         │
│              │              │                                                │
│              │              ▼                                                │
│              │  Worker ──▶ WorkerAgentService.WorkerStream (bidireccional)   │
│              │              │                                                │
│              ▼              ▼                                                │
│  Worker disponible en el pool                                                │
│                                                                              │
│  4. JOB DISPATCH                                                             │
│  ──────────────                                                              │
│  SchedulerService asigna job al worker                                       │
│              │                                                               │
│              ▼                                                               │
│  Server ──▶ WorkerStream.send(RunJob{job_id, command, args})                 │
│              │                                                               │
│              ▼                                                               │
│  Job estado: RUNNING                                                         │
│                                                                              │
│  5. JOB EXECUTION                                                            │
│  ────────────────                                                            │
│  Worker ejecuta comando                                                      │
│              │                                                               │
│              ├──▶ Worker envía logs via stream (LogEntry)                    │
│              │                                                               │
│              ▼                                                               │
│  Worker ──▶ WorkerStream.send(JobResult{exit_code, success})                 │
│                                                                              │
│  6. JOB COMPLETION                                                           │
│  ─────────────────                                                           │
│  Server recibe resultado                                                     │
│              │                                                               │
│              ▼                                                               │
│  Job estado: COMPLETED (o FAILED)                                            │
│              │                                                               │
│              ▼                                                               │
│  Worker vuelve al pool (o se destruye si es efímero)                         │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Estados del Job

| Estado | Descripción |
|--------|-------------|
| `PENDING` | Job encolado, esperando worker disponible |
| `ASSIGNED` | Job asignado a un worker, esperando inicio |
| `RUNNING` | Job ejecutándose en el worker |
| `COMPLETED` | Job terminó exitosamente (exit_code = 0) |
| `FAILED` | Job terminó con error (exit_code ≠ 0) |
| `CANCELLED` | Job cancelado por el usuario |
| `TIMEOUT` | Job excedió el tiempo límite |

### Servicios gRPC

| Servicio | Responsabilidad |
|----------|-----------------|
| `JobExecutionService` | Encolar, consultar estado, cancelar jobs |
| `WorkerAgentService` | Registro de workers (OTP), stream bidireccional |
| `SchedulerService` | Estado de cola, workers disponibles |
| `ProviderManagementService` | Listar/configurar providers |
| `LogStreamService` | Streaming de logs en tiempo real |
| `MetricsService` | Métricas del sistema |

### Autenticación OTP

Los workers se autentican usando **One-Time Passwords (OTP)**:

1. **Server genera OTP** cuando aprovisiona un worker
2. **OTP se inyecta** como variable de entorno `HODEI_OTP_TOKEN`
3. **Worker usa OTP** para registrarse (`WorkerAgentService.Register`)
4. **OTP se invalida** después de usarse (single-use)

En **modo desarrollo** (`HODEI_DEV_MODE=1`), se aceptan tokens con prefijo `dev-` sin validación.

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

Los tests de integración prueban los **providers reales** contra infraestructura real. No son simulaciones.

#### Docker Provider Integration

**Qué prueba:** Ciclo de vida completo de workers usando el Docker daemon real.

| Test | Descripción |
|------|-------------|
| `test_docker_provider_health_check` | Verifica que el daemon Docker responde correctamente |
| `test_docker_provider_capabilities` | Obtiene CPU, memoria y arquitecturas disponibles del host |
| `test_docker_provider_create_and_destroy_worker` | Crea un container `alpine:latest`, verifica estado, lo destruye |
| `test_docker_provider_get_worker_logs` | Obtiene logs de un container en ejecución |
| `test_docker_provider_worker_lifecycle` | Flujo completo: Create → Status → Logs → Destroy → Verify |

**Requisitos:**
- Docker daemon corriendo (`docker ps` funciona)
- Imagen `alpine:latest` disponible (se descarga automáticamente)

```bash
# Ejecutar tests de Docker (NO requiere Postgres ni servidor gRPC)
cargo test --test docker_integration -- --ignored

# Con output detallado
cargo test --test docker_integration -- --ignored --nocapture
```

**Qué NO prueba:** No levanta el servidor gRPC ni Postgres. Solo prueba el provider aislado.

---

#### Kubernetes Provider Integration

**Qué prueba:** Ciclo de vida de workers como Pods en un cluster Kubernetes real.

| Test | Descripción |
|------|-------------|
| `test_kubernetes_provider_health_check` | Verifica conexión al cluster y permisos |
| `test_kubernetes_provider_create_and_destroy_worker` | Crea Pod, espera estado Ready, lo elimina |
| `test_kubernetes_provider_destroy_nonexistent_worker` | Verifica idempotencia de destroy |
| `test_kubernetes_provider_capabilities` | Obtiene recursos del cluster |
| `test_kubernetes_provider_duplicate_worker_fails` | Verifica que no se pueden crear Pods duplicados |

**Requisitos:**
- Cluster Kubernetes accesible (`kubectl cluster-info` funciona)
- Namespace `hodei-workers` creado (o usar `HODEI_K8S_TEST_NAMESPACE`)
- Permisos para crear/eliminar Pods

```bash
# Setup con Kind (desarrollo local)
kind create cluster --name hodei-test
kubectl create namespace hodei-workers

# Ejecutar tests
HODEI_K8S_TEST=1 cargo test --test kubernetes_integration -- --ignored

# Con namespace personalizado
HODEI_K8S_TEST=1 HODEI_K8S_TEST_NAMESPACE=my-namespace \
  cargo test --test kubernetes_integration -- --ignored
```

**Qué NO prueba:** No levanta el servidor gRPC ni Postgres. Solo prueba el provider aislado.

---

#### Firecracker Provider Integration

**Qué prueba:** Ciclo de vida de workers como microVMs usando Firecracker real.

| Test | Descripción |
|------|-------------|
| `test_firecracker_provider_health_check` | Verifica KVM y binarios de Firecracker |
| `test_firecracker_provider_capabilities` | Verifica que no soporta GPU, obtiene recursos |
| `test_firecracker_provider_create_and_destroy_worker` | Crea microVM, verifica estado, la destruye |
| `test_firecracker_provider_destroy_nonexistent_worker` | Verifica idempotencia de destroy |
| `test_ip_pool_allocation_and_release` | Prueba unitaria del pool de IPs para networking |

**Requisitos:**
- Linux con KVM habilitado (`/dev/kvm` existe)
- Binario `firecracker` instalado
- Kernel Linux (`vmlinux`) y rootfs (`rootfs.ext4`) preparados

```bash
# Verificar requisitos
ls -la /dev/kvm
which firecracker

# Preparar kernel y rootfs (ver scripts/firecracker/README.md)
sudo ./scripts/firecracker/build-rootfs.sh

# Ejecutar tests
HODEI_FC_TEST=1 cargo test --test firecracker_integration -- --ignored

# Con paths personalizados
HODEI_FC_TEST=1 \
HODEI_FC_KERNEL_PATH=/path/to/vmlinux \
HODEI_FC_ROOTFS_PATH=/path/to/rootfs.ext4 \
  cargo test --test firecracker_integration -- --ignored
```

**Qué NO prueba:** No levanta el servidor gRPC ni Postgres. Solo prueba el provider aislado.

---

#### Postgres Integration (Testcontainers)

**Qué prueba:** Repositorios de persistencia contra PostgreSQL real usando **Testcontainers**.

| Test | Descripción |
|------|-------------|
| `test_postgres_save_and_find` | Guarda y recupera ProviderConfig |
| `test_postgres_find_by_name` | Búsqueda por nombre |
| `test_postgres_find_by_type` | Búsqueda por tipo de provider |
| `test_postgres_find_enabled` | Lista providers habilitados |
| `test_postgres_update` | Actualiza configuración existente |
| `test_postgres_delete` | Elimina configuración |
| `test_postgres_exists_by_name` | Verifica existencia |

**Cómo funciona:**
1. Testcontainers levanta un container `postgres:16-alpine` automáticamente
2. Ejecuta migraciones SQL
3. Corre los tests contra ese Postgres efímero
4. El container se destruye al terminar

**Requisitos:**
- Docker daemon corriendo (Testcontainers lo usa)
- NO requiere Postgres instalado en el host

```bash
# Ejecutar tests de Postgres (usa Testcontainers)
cargo test -p hodei-jobs-infrastructure --tests -- --ignored

# Tests específicos de repositorios
cargo test -p hodei-jobs-infrastructure postgres_job_repo -- --ignored
cargo test -p hodei-jobs-infrastructure postgres_worker_repo -- --ignored
cargo test -p hodei-jobs-infrastructure postgres_provider_config -- --ignored
```

**Qué NO prueba:** No levanta el servidor gRPC. Solo prueba los repositorios aislados.

---

#### Resumen: ¿Qué ejecuta cada test?

| Test Suite | Docker Daemon | K8s Cluster | KVM | Postgres | Servidor gRPC |
|------------|:-------------:|:-----------:|:---:|:--------:|:-------------:|
| `docker_integration` | ✅ Real | ❌ | ❌ | ❌ | ❌ |
| `kubernetes_integration` | ❌ | ✅ Real | ❌ | ❌ | ❌ |
| `firecracker_integration` | ❌ | ❌ | ✅ Real | ❌ | ❌ |
| `postgres_*_integration` | ✅ (Testcontainers) | ❌ | ❌ | ✅ Efímero | ❌ |
| `e2e_docker_provider` | ✅ Real | ❌ | ❌ | ✅ Testcontainers | ✅ Test Server |
| `e2e_kubernetes_provider` | ✅ (Testcontainers) | ✅ Real | ❌ | ✅ Testcontainers | ✅ Test Server |
| `e2e_firecracker_provider` | ✅ (Testcontainers) | ❌ | ✅ Real | ✅ Testcontainers | ✅ Test Server |

---

### Tests E2E (Stack Completo)

Los tests E2E prueban el **stack completo**: PostgreSQL (via Testcontainers) + gRPC Server + Provider + Worker Agent simulado.

#### Docker Provider E2E

**Qué prueba:** Flujo completo de aprovisionamiento, registro de workers, dispatch de jobs y logs.

```bash
# Requisitos: Docker Desktop o Docker Engine
# Configurar Testcontainers para Docker Desktop (Ubuntu)
export DOCKER_HOST=unix://$HOME/.docker/desktop/docker.sock
export TESTCONTAINERS_RYUK_DISABLED=true

# Ejecutar tests E2E de Docker
cargo test --test e2e_docker_provider -- --ignored --nocapture
```

| Test | Descripción |
|------|-------------|
| `test_01_docker_available` | Docker CLI funciona |
| `test_02_postgres_testcontainer` | PostgreSQL via Testcontainers |
| `test_03_grpc_server_starts` | Server gRPC inicia correctamente |
| `test_04_worker_registration_with_otp` | Worker se registra con OTP |
| `test_05_worker_stream_connection` | Stream bidireccional funciona |
| `test_06_job_queued_and_dispatched` | Job se encola y despacha al worker |
| `test_07_docker_provider_creates_container` | DockerProvider crea container |
| `test_08_docker_provider_container_lifecycle` | Ciclo de vida completo |

---

#### Kubernetes Provider E2E

**Qué prueba:** Flujo completo con workers como Pods en Kubernetes.

```bash
# Requisitos: Kubernetes cluster (kind, minikube, o real)
# Setup con Kind
kind create cluster --name hodei-test
kubectl create namespace hodei-workers

# Configurar Testcontainers
export DOCKER_HOST=unix://$HOME/.docker/desktop/docker.sock
export TESTCONTAINERS_RYUK_DISABLED=true

# Ejecutar tests E2E de Kubernetes
HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored --nocapture

# Con namespace personalizado
HODEI_K8S_TEST=1 HODEI_K8S_TEST_NAMESPACE=my-namespace \
  cargo test --test e2e_kubernetes_provider -- --ignored --nocapture
```

| Test | Descripción |
|------|-------------|
| `test_01_kubernetes_available` | kubectl y cluster accesibles |
| `test_07_kubernetes_provider_creates_pod` | KubernetesProvider crea Pod |
| `test_08_kubernetes_provider_pod_lifecycle` | Ciclo de vida completo del Pod |
| `test_11_kubernetes_provider_health_check` | Health check del provider |
| `test_12_kubernetes_provider_capabilities` | Capacidades del provider |

---

#### Firecracker Provider E2E

**Qué prueba:** Flujo completo con workers como microVMs usando Firecracker.

```bash
# Requisitos: Linux con KVM + Firecracker instalado
# Verificar KVM
ls -la /dev/kvm
sudo modprobe kvm_intel  # o kvm_amd

# Instalar Firecracker
curl -L https://github.com/firecracker-microvm/firecracker/releases/download/v1.9.0/firecracker-v1.9.0-x86_64.tgz | tar xz
sudo mv release-v1.9.0-x86_64/firecracker-v1.9.0-x86_64 /usr/bin/firecracker

# Descargar kernel
sudo mkdir -p /var/lib/hodei
curl -L -o /tmp/vmlinux \
  "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.9/x86_64/vmlinux-5.10.217"
sudo mv /tmp/vmlinux /var/lib/hodei/vmlinux

# Construir rootfs (requiere root)
sudo ./scripts/firecracker/build-rootfs.sh

# Configurar Testcontainers
export DOCKER_HOST=unix://$HOME/.docker/desktop/docker.sock
export TESTCONTAINERS_RYUK_DISABLED=true

# Ejecutar tests E2E de Firecracker
HODEI_FC_TEST=1 cargo test --test e2e_firecracker_provider -- --ignored --nocapture

# Con paths personalizados
HODEI_FC_TEST=1 \
HODEI_FC_KERNEL_PATH=/path/to/vmlinux \
HODEI_FC_ROOTFS_PATH=/path/to/rootfs.ext4 \
  cargo test --test e2e_firecracker_provider -- --ignored --nocapture
```

| Test | Descripción |
|------|-------------|
| `test_01_firecracker_available` | KVM y Firecracker disponibles |
| `test_07_firecracker_provider_creates_vm` | FirecrackerProvider crea microVM |
| `test_08_firecracker_provider_vm_lifecycle` | Ciclo de vida completo de microVM |
| `test_11_firecracker_provider_health_check` | Health check del provider |
| `test_12_firecracker_provider_capabilities` | Capacidades del provider |
| `test_13_ip_pool_allocation` | Pool de IPs para networking (unit test) |

---

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

### Fase 2: Servidor gRPC

**Objetivo:** Levantar el control plane con al menos un provider habilitado.

#### 2.1 Iniciar Servidor gRPC

```bash
# Terminal 1: Servidor (modo desarrollo con Docker provider)
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

> **NOTA:** Los workers se aprovisionan **automáticamente** cuando hay jobs pendientes.
> No es necesario iniciar workers manualmente en producción.

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

#### 2.3 Verificar Providers Habilitados

```bash
# Listar providers disponibles
grpcurl -plaintext localhost:50051 hodei.ProviderManagementService/ListProviders

# Output esperado (con Docker habilitado):
# {
#   "providers": [
#     {"name": "docker", "type": "DOCKER", "enabled": true}
#   ]
# }
```

#### 2.4 Worker Manual (Solo para Desarrollo/Debug)

> **⚠️ SOLO PARA DESARROLLO:** En producción, los workers se aprovisionan automáticamente.
> Este paso es opcional y solo útil para debug sin providers habilitados.

```bash
# Terminal 3: Worker manual (modo desarrollo)
HODEI_OTP_TOKEN=dev-test \
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

### Fase 3: Ejecutar Jobs

**Objetivo:** Probar el ciclo de vida completo de jobs con ejemplos prácticos.

> **FLUJO AUTOMÁTICO:** Solo necesitas encolar jobs. El servidor:
> 1. Aprovisiona workers automáticamente cuando hay demanda
> 2. Los workers se auto-registran con OTP
> 3. Los jobs se despachan automáticamente

#### 3.1 Job Simple (echo)

```bash
# Generar UUID para el job
JOB_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
echo "JOB_ID=$JOB_ID"

# Encolar job simple
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_ID"'"},
    "name": "Hello World Job",
    "command": "echo",
    "arguments": ["Hello from Hodei!"]
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Respuesta: {"success": true}
```

#### 3.2 Consultar Estado del Job

```bash
# Ver estado del job (PENDING → RUNNING → COMPLETED)
grpcurl -plaintext -d '{
  "job_id": {"value": "'"$JOB_ID"'"}
}' localhost:50051 hodei.JobExecutionService/GetJobStatus

# Estados posibles:
# - PENDING: Esperando worker disponible
# - RUNNING: Ejecutándose en un worker
# - COMPLETED: Terminó exitosamente
# - FAILED: Terminó con error
```

#### 3.3 Job con Variables de Entorno

```bash
JOB_ENV=$(uuidgen | tr '[:upper:]' '[:lower:]')

grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_ENV"'"},
    "name": "Environment Variables Job",
    "command": "sh",
    "arguments": ["-c", "echo \"User: $MY_USER, Mode: $MY_MODE\""],
    "environment": {
      "MY_USER": "developer",
      "MY_MODE": "production"
    }
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob
```

#### 3.4 Job de Larga Duración (con logs progresivos)

```bash
JOB_LONG=$(uuidgen | tr '[:upper:]' '[:lower:]')

grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_LONG"'"},
    "name": "Long Running Job",
    "command": "sh",
    "arguments": ["-c", "for i in 1 2 3 4 5; do echo \"Processing step $i of 5...\"; sleep 2; done; echo \"All steps completed!\""]
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Mientras ejecuta, puedes ver los logs en streaming (ver Fase 5)
```

#### 3.5 Job que Falla (para probar manejo de errores)

```bash
JOB_FAIL=$(uuidgen | tr '[:upper:]' '[:lower:]')

grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_FAIL"'"},
    "name": "Failing Job",
    "command": "sh",
    "arguments": ["-c", "echo \"Starting...\"; exit 1"]
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Este job terminará con estado FAILED y exit_code=1
```

#### 3.6 Job con Timeout

```bash
JOB_TIMEOUT=$(uuidgen | tr '[:upper:]' '[:lower:]')

grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_TIMEOUT"'"},
    "name": "Timeout Test Job",
    "command": "sleep",
    "arguments": ["300"],
    "timeout": {"seconds": 5}
  },
  "queued_by": "developer"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Este job será cancelado después de 5 segundos con estado TIMEOUT
```

#### 3.7 Cancelar Job

```bash
grpcurl -plaintext -d '{
  "job_id": {"value": "'"$JOB_LONG"'"},
  "reason": "User requested cancellation"
}' localhost:50051 hodei.JobExecutionService/CancelJob
```

#### 3.8 Múltiples Jobs en Paralelo

```bash
# Encolar 3 jobs simultáneamente
for i in 1 2 3; do
  JOB=$(uuidgen | tr '[:upper:]' '[:lower:]')
  grpcurl -plaintext -d '{
    "job_definition": {
      "job_id": {"value": "'"$JOB"'"},
      "name": "Parallel Job '"$i"'",
      "command": "sh",
      "arguments": ["-c", "echo \"Job '"$i"' started\"; sleep 3; echo \"Job '"$i"' done\""]
    },
    "queued_by": "developer"
  }' localhost:50051 hodei.JobExecutionService/QueueJob &
done
wait

# El servidor aprovisionará workers según demanda y capacidad
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
# test_e2e.sh - Prueba E2E completa (flujo automático)
#
# FLUJO REAL:
# 1. QueueJob → Job se encola
# 2. Server aprovisiona worker automáticamente (con OTP)
# 3. Worker se auto-registra usando HODEI_OTP_TOKEN
# 4. Job se despacha automáticamente al worker
# 5. Worker ejecuta y reporta resultado
#
# NOTA: El registro del worker es AUTOMÁTICO, no manual.
# El servidor aprovisiona workers bajo demanda cuando hay jobs pendientes.

set -e

SERVER="localhost:50051"
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "═══════════════════════════════════════════════════"
echo "  HODEI - Prueba E2E Completa (Flujo Automático)"
echo "═══════════════════════════════════════════════════"

# 1. Verificar servidor
echo -e "\n[1/4] Verificando servidor..."
if grpcurl -plaintext $SERVER list > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Servidor disponible"
else
    echo -e "${RED}✗${NC} Servidor no disponible"
    echo "  Iniciar con: HODEI_DEV_MODE=1 HODEI_DOCKER_ENABLED=1 cargo run --bin server -p hodei-jobs-grpc"
    exit 1
fi

# 2. Verificar providers habilitados
echo -e "\n[2/4] Verificando providers..."
PROVIDERS=$(grpcurl -plaintext $SERVER hodei.ProviderManagementService/ListProviders 2>&1)
if echo "$PROVIDERS" | grep -q '"name"'; then
    echo -e "${GREEN}✓${NC} Providers disponibles"
    echo "$PROVIDERS" | grep '"name"' | head -3
else
    echo -e "${YELLOW}⚠${NC} No hay providers habilitados"
    echo "  El servidor necesita al menos un provider (Docker, K8s, o Firecracker)"
    echo "  Reiniciar con: HODEI_DOCKER_ENABLED=1 cargo run --bin server"
fi

# 3. Encolar Job (esto dispara el aprovisionamiento automático)
echo -e "\n[3/4] Encolando Job..."
JOB_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
QUEUE_RESULT=$(grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "'"$JOB_ID"'"},
    "name": "E2E Test Job",
    "command": "echo",
    "arguments": ["Hello from Hodei E2E test!"]
  },
  "queued_by": "e2e-test"
}' $SERVER hodei.JobExecutionService/QueueJob 2>&1)

if echo "$QUEUE_RESULT" | grep -q '"success": true'; then
    echo -e "${GREEN}✓${NC} Job encolado: $JOB_ID"
    echo -e "${YELLOW}→${NC} El servidor aprovisionará un worker automáticamente"
    echo -e "${YELLOW}→${NC} El worker se auto-registrará con OTP"
    echo -e "${YELLOW}→${NC} El job se despachará al worker"
else
    echo -e "${RED}✗${NC} Error encolando job: $QUEUE_RESULT"
    exit 1
fi

# 4. Monitorear estado del job
echo -e "\n[4/4] Monitoreando estado del job..."
echo "  (El flujo completo puede tardar 10-30s dependiendo del provider)"

for i in {1..30}; do
    sleep 2
    STATUS=$(grpcurl -plaintext -d '{
      "job_id": {"value": "'"$JOB_ID"'"}
    }' $SERVER hodei.JobExecutionService/GetJobStatus 2>&1 || echo "")
    
    if echo "$STATUS" | grep -q '"status":\s*"COMPLETED"'; then
        echo -e "${GREEN}✓${NC} Job completado exitosamente"
        break
    elif echo "$STATUS" | grep -q '"status":\s*"FAILED"'; then
        echo -e "${RED}✗${NC} Job falló"
        echo "$STATUS"
        exit 1
    elif echo "$STATUS" | grep -q '"status":\s*"RUNNING"'; then
        echo -e "  [${i}/30] Job ejecutándose..."
    elif echo "$STATUS" | grep -q '"status":\s*"PENDING"'; then
        echo -e "  [${i}/30] Job pendiente (esperando worker)..."
    else
        echo -e "  [${i}/30] Esperando..."
    fi
    
    if [ $i -eq 30 ]; then
        echo -e "${YELLOW}⚠${NC} Timeout esperando job (puede continuar en background)"
    fi
done

echo -e "\n═══════════════════════════════════════════════════"
echo -e "  ${GREEN}✅ Prueba E2E Completada${NC}"
echo ""
echo "  El flujo automático fue:"
echo "  1. Job encolado → Server detectó demanda"
echo "  2. Server aprovisionó worker con OTP"
echo "  3. Worker se auto-registró"
echo "  4. Job despachado y ejecutado"
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

#### "HODEI_OTP_TOKEN not set" o "Invalid OTP"
```bash
# En modo desarrollo, usar token con prefijo "dev-"
HODEI_DEV_MODE=1 cargo run --bin server -p hodei-jobs-grpc  # Servidor
HODEI_OTP_TOKEN=dev-test cargo run --bin worker -p hodei-jobs-grpc  # Worker
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
| `HODEI_OTP_TOKEN` | OTP token (o `dev-*` en desarrollo) | - |
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

### Épicas de Tests E2E (Stack Completo)

| Documento | Descripción | Estado |
|-----------|-------------|--------|
| [`docs/E2E-TESTING-ROADMAP.md`](docs/epics/E2E-TESTING-ROADMAP.md) | Roadmap general de tests E2E | Planificado |
| [`docs/EPIC-9-E2E-Docker.md`](docs/epics/EPIC-9-E2E-Docker.md) | Tests E2E con Docker Provider | Planificado |
| [`docs/EPIC-10-E2E-Kubernetes.md`](docs/epics/EPIC-10-E2E-Kubernetes.md) | Tests E2E con Kubernetes Provider | Planificado |
| [`docs/EPIC-11-E2E-Firecracker.md`](docs/epics/EPIC-11-E2E-Firecracker.md) | Tests E2E con Firecracker Provider | Planificado |

> **Nota:** Los tests E2E prueban el stack completo: PostgreSQL + gRPC Server + Provider + Worker Agent.
> Actualmente los tests de integración prueban componentes aislados. Ver sección "Tests de Integración" para detalles.

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
HODEI_OTP_TOKEN=dev-test cargo run --bin worker -p hodei-jobs-grpc

# === VERIFICAR ===
grpcurl -plaintext localhost:50051 list

# === TESTS UNITARIOS ===
cargo test -p hodei-jobs-infrastructure providers::docker
cargo test -p hodei-jobs-infrastructure providers::kubernetes
cargo test -p hodei-jobs-infrastructure providers::firecracker

# === TESTS E2E (requieren infraestructura) ===
# Docker E2E
cargo test --test e2e_docker_provider -- --ignored --nocapture

# Kubernetes E2E (requiere cluster)
HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored --nocapture

# Firecracker E2E (requiere KVM + kernel + rootfs)
HODEI_FC_TEST=1 cargo test --test e2e_firecracker_provider -- --ignored --nocapture
```

# Scripts E2E - Hodei Jobs Platform

Scripts para ejecutar tests End-to-End (E2E) con stack completo.

## Descripción

Los tests E2E prueban el flujo completo:
1. **PostgreSQL** (via Testcontainers)
2. **gRPC Server** (con todos los servicios)
3. **Provider** (Docker/Kubernetes/Firecracker)
4. **Worker Agent** (ejecutando jobs reales)
5. **Logs y métricas** (flujo completo)

## Scripts Disponibles

| Script | Provider | Estado |
|--------|----------|--------|
| `run-docker-e2e.sh` | Docker | ✅ Implementado |
| `run-kubernetes-e2e.sh` | Kubernetes | 🔜 Planificado (EPIC-10) |
| `run-firecracker-e2e.sh` | Firecracker | 🔜 Planificado (EPIC-11) |

## Docker E2E

### Requisitos

- Docker daemon corriendo
- Rust toolchain instalado
- (Opcional) Imagen worker: `hodei-worker:e2e-test`

### Ejecución

```bash
# Ejecutar tests E2E de Docker
./scripts/e2e/run-docker-e2e.sh

# Con construcción de imagen worker
./scripts/e2e/run-docker-e2e.sh --build-image
```

### Tests Incluidos

| Test | Descripción |
|------|-------------|
| `test_e2e_stack_starts_correctly` | Verifica que el stack completo arranca |
| `test_e2e_job_execution_with_manual_worker` | Ejecución de job con worker manual |
| `test_e2e_docker_provider_initialization` | Inicialización del DockerProvider |
| `test_e2e_job_failure_handling` | Manejo de jobs fallidos |
| `test_e2e_log_streaming_setup` | Infraestructura de streaming de logs |
| `test_e2e_multiple_jobs_queued` | Encolar múltiples jobs |
| `test_e2e_scheduler_queue_status` | Estado de la cola del scheduler |

### Ejecución Manual

```bash
# Ejecutar todos los tests E2E de Docker
cargo test --test e2e_docker_provider -- --ignored --nocapture

# Ejecutar un test específico
cargo test --test e2e_docker_provider test_e2e_stack_starts_correctly -- --ignored --nocapture
```

## Arquitectura de Tests

```
┌─────────────────────────────────────────────────────────────────┐
│                    TEST E2E                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  crates/grpc/tests/                                              │
│  ├── common/mod.rs          # Infraestructura compartida        │
│  │   ├── PostgresTestDatabase  # Testcontainers Postgres        │
│  │   ├── TestServer            # gRPC Server completo           │
│  │   └── TestStack             # Postgres + Server + Provider   │
│  │                                                               │
│  ├── e2e_docker_provider.rs    # Tests E2E Docker               │
│  ├── e2e_kubernetes_provider.rs # Tests E2E K8s (futuro)        │
│  └── e2e_firecracker_provider.rs # Tests E2E FC (futuro)        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Troubleshooting

### "Skipping test: Docker not available"

El DockerProvider no puede conectarse al daemon Docker.

**Verificar:**
```bash
# Docker está corriendo?
docker ps

# El usuario tiene permisos?
groups | grep docker

# El socket existe?
ls -la /var/run/docker.sock
```

**Soluciones:**
```bash
# Añadir usuario al grupo docker
sudo usermod -aG docker $USER
newgrp docker

# O ejecutar con sudo (no recomendado)
sudo cargo test --test e2e_docker_provider -- --ignored
```

### "Failed to start Postgres container"

Testcontainers no puede iniciar PostgreSQL.

**Verificar:**
```bash
# Docker puede descargar imágenes?
docker pull postgres:16-alpine

# Hay espacio en disco?
df -h
```

## Documentación Relacionada

- [EPIC-9: E2E Docker](../../docs/EPIC-9-E2E-Docker.md)
- [EPIC-10: E2E Kubernetes](../../docs/EPIC-10-E2E-Kubernetes.md)
- [EPIC-11: E2E Firecracker](../../docs/EPIC-11-E2E-Firecracker.md)
- [E2E Testing Roadmap](../../docs/E2E-TESTING-ROADMAP.md)

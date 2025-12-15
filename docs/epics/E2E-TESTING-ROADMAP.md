# Roadmap de Tests E2E - Hodei Jobs Platform

**Versión**: 1.0  
**Fecha**: 2025-12-14

## Visión General

Este documento describe el plan para implementar tests End-to-End (E2E) que prueben el stack completo de Hodei Jobs Platform con cada uno de los providers disponibles.

## Estado Actual vs Objetivo

### Estado Actual

```
┌─────────────────────────────────────────────────────────────────┐
│                    TESTS ACTUALES                                │
├─────────────────────────────────────────────────────────────────┤
│  ✅ Unit Tests (~95 tests)                                       │
│     - Lógica de dominio aislada                                  │
│     - Sin infraestructura externa                                │
│                                                                  │
│  ✅ Integration Tests (Providers aislados)                       │
│     - docker_integration: Provider ↔ Docker daemon              │
│     - kubernetes_integration: Provider ↔ K8s cluster            │
│     - firecracker_integration: Provider ↔ KVM                   │
│     - postgres_*_integration: Repos ↔ Postgres (Testcontainers) │
│                                                                  │
│  ✅ gRPC Integration Tests                                       │
│     - grpc_integration: OTP, registro, stream                   │
│     - job_controller_integration: Dispatch de jobs              │
│     - Usa repositorios in-memory                                │
│                                                                  │
│  ❌ E2E Tests (Stack Completo)                                   │
│     - NO EXISTE: Postgres + Server + Provider + Worker          │
└─────────────────────────────────────────────────────────────────┘
```

### Objetivo

```
┌─────────────────────────────────────────────────────────────────┐
│                    TESTS E2E (OBJETIVO)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  E2E Docker (EPIC-9)                                        ││
│  │  Postgres + Server + DockerProvider + Worker Container      ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  E2E Kubernetes (EPIC-10)                                   ││
│  │  Postgres + Server + K8sProvider + Worker Pod               ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  E2E Firecracker (EPIC-11)                                  ││
│  │  Postgres + Server + FCProvider + Worker MicroVM            ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Épicas

| Épica | Provider | Prioridad | Estimación | Dependencias |
|-------|----------|-----------|------------|--------------|
| [EPIC-9](EPIC-9-E2E-Docker.md) | Docker | Alta | 22h | - |
| [EPIC-10](EPIC-10-E2E-Kubernetes.md) | Kubernetes | Media | 21h | EPIC-9 |
| [EPIC-11](EPIC-11-E2E-Firecracker.md) | Firecracker | Baja | 28h | EPIC-9 |

**Total estimado:** 71 horas

## Flujo E2E Común

Todos los tests E2E siguen el mismo flujo:

```
┌─────────────────────────────────────────────────────────────────┐
│                    FLUJO E2E COMÚN                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. SETUP                                                        │
│     ├── Levantar PostgreSQL (Testcontainers)                    │
│     ├── Ejecutar migraciones                                    │
│     ├── Iniciar gRPC Server en puerto random                    │
│     └── Habilitar Provider específico                           │
│                                                                  │
│  2. PROVISIONING                                                 │
│     ├── Encolar Job via JobExecutionService                     │
│     ├── JobController detecta job pendiente                     │
│     ├── Scheduler no encuentra workers disponibles              │
│     ├── ProvisioningService crea worker via Provider            │
│     └── Provider genera OTP y lo pasa al worker                 │
│                                                                  │
│  3. REGISTRO                                                     │
│     ├── Worker arranca (container/pod/microVM)                  │
│     ├── Worker se conecta al server                             │
│     ├── Worker se registra con OTP                              │
│     └── Worker conecta WorkerStream bidireccional               │
│                                                                  │
│  4. EJECUCIÓN                                                    │
│     ├── JobController asigna job al worker                      │
│     ├── Server envía RunJobCommand via stream                   │
│     ├── Worker ejecuta comando                                  │
│     ├── Worker envía logs via stream                            │
│     └── Worker envía JobResult                                  │
│                                                                  │
│  5. VERIFICACIÓN                                                 │
│     ├── Job marcado como completado en Postgres                 │
│     ├── Logs recibidos via LogStreamService                     │
│     └── Exit code correcto                                      │
│                                                                  │
│  6. CLEANUP                                                      │
│     ├── Destruir worker via Provider                            │
│     ├── Verificar recursos eliminados                           │
│     └── Parar server y Postgres                                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Componentes Compartidos

### e2e_common.rs

Helpers reutilizables para todos los tests E2E:

```rust
// Pseudocódigo
pub struct TestStack {
    postgres: TestcontainersPostgres,
    server_addr: SocketAddr,
    shutdown_tx: oneshot::Sender<()>,
}

impl TestStack {
    pub async fn new(provider_config: ProviderConfig) -> Self { ... }
    pub async fn queue_job(&self, spec: JobSpec) -> JobId { ... }
    pub async fn wait_for_job_completion(&self, job_id: &JobId, timeout: Duration) -> Job { ... }
    pub async fn get_job_logs(&self, job_id: &JobId) -> Vec<LogEntry> { ... }
    pub async fn shutdown(self) { ... }
}
```

### Scripts Compartidos

```
scripts/e2e/
├── common.sh              # Funciones compartidas
├── run-docker-e2e.sh      # EPIC-9
├── run-kubernetes-e2e.sh  # EPIC-10
├── run-firecracker-e2e.sh # EPIC-11
└── run-all-e2e.sh         # Ejecutar todos
```

## Requisitos por Provider

| Requisito | Docker | Kubernetes | Firecracker |
|-----------|:------:|:----------:|:-----------:|
| Docker daemon | ✅ | ✅ (Kind) | ❌ |
| kubectl | ❌ | ✅ | ❌ |
| Kind | ❌ | ✅ | ❌ |
| KVM (/dev/kvm) | ❌ | ❌ | ✅ |
| Firecracker binary | ❌ | ❌ | ✅ |
| Root/sudo | ❌ | ❌ | ✅ |
| Imagen worker | ✅ | ✅ | ❌ |
| Rootfs con worker | ❌ | ❌ | ✅ |

## Orden de Implementación Recomendado

```
Semana 1-2: EPIC-9 (Docker)
├── Infraestructura común (e2e_common.rs)
├── Tests E2E Docker
└── Scripts de ejecución

Semana 3-4: EPIC-10 (Kubernetes)
├── Setup Kind cluster
├── Tests E2E Kubernetes
└── Integración CI/CD

Semana 5-6: EPIC-11 (Firecracker)
├── Preparación kernel/rootfs
├── Tests E2E Firecracker
└── Documentación final
```

## Métricas de Éxito

| Métrica | Objetivo |
|---------|----------|
| Cobertura de flujos | 100% de flujos principales |
| Estabilidad | < 5% tests flaky |
| Tiempo de ejecución | < 5 min por provider |
| Cleanup | 0 recursos huérfanos |

## Integración con CI/CD

```yaml
# .github/workflows/e2e.yml (ejemplo)
jobs:
  e2e-docker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build worker image
        run: ./scripts/docker/build-worker-image.sh -t hodei-worker:e2e
      - name: Run E2E tests
        run: ./scripts/e2e/run-docker-e2e.sh

  e2e-kubernetes:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Setup Kind
        run: ./scripts/e2e/setup-kind-cluster.sh
      - name: Run E2E tests
        run: ./scripts/e2e/run-kubernetes-e2e.sh

  e2e-firecracker:
    runs-on: ubuntu-latest
    # Requiere runner con KVM habilitado
    steps:
      - uses: actions/checkout@v4
      - name: Prepare assets
        run: sudo ./scripts/e2e/prepare-firecracker-assets.sh
      - name: Run E2E tests
        run: sudo ./scripts/e2e/run-firecracker-e2e.sh
```

## Referencias

- [EPIC-9: E2E Docker](EPIC-9-E2E-Docker.md)
- [EPIC-10: E2E Kubernetes](EPIC-10-E2E-Kubernetes.md)
- [EPIC-11: E2E Firecracker](EPIC-11-E2E-Firecracker.md)
- [GETTING_STARTED.md](../../GETTING_STARTED.md)
- [architecture.md](../architecture.md)

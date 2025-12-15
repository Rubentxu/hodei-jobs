# EPIC-9: Tests E2E con Stack Completo - Docker Provider

**Versión**: 1.0  
**Fecha**: 2025-12-14  
**Estado**: Planificado  
**Prioridad**: Alta

## Resumen Ejecutivo

Implementar tests End-to-End (E2E) que prueben el stack completo de Hodei Jobs Platform usando el Docker Provider. Estos tests verificarán el flujo completo desde el encolado de un job hasta su ejecución y reporte de resultados, incluyendo:

- PostgreSQL (persistencia real)
- gRPC Server (con todos los servicios)
- Docker Provider (provisioning de workers)
- Worker Agent (ejecutando dentro de container Docker)
- Logs y métricas

## Objetivos

1. Validar el flujo E2E completo con infraestructura real
2. Detectar problemas de integración entre componentes
3. Verificar el provisioning automático de workers via Docker
4. Asegurar que los logs fluyen correctamente del worker al servidor
5. Crear base reutilizable para tests E2E de otros providers

## Arquitectura del Test

```
┌─────────────────────────────────────────────────────────────────┐
│                    TEST E2E DOCKER                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────────────────────────────┐  │
│  │  Testcontainers │   │         gRPC Server                   │  │
│  │  PostgreSQL     │◄──┤  - WorkerAgentService                 │  │
│  │  :5432          │   │  - JobExecutionService                │  │
│  └──────────────┘     │  - SchedulerService                   │  │
│                        │  - LogStreamService                   │  │
│                        │  - JobController (loop)               │  │
│                        │  - DockerProvider ✓                   │  │
│                        └──────────────┬───────────────────────┘  │
│                                       │                          │
│                                       │ create_worker()          │
│                                       ▼                          │
│                        ┌──────────────────────────────────────┐  │
│                        │      Docker Container                 │  │
│                        │  ┌────────────────────────────────┐  │  │
│                        │  │     Worker Agent               │  │  │
│                        │  │  - HODEI_TOKEN=<otp>           │  │  │
│                        │  │  - HODEI_SERVER=host.docker... │  │  │
│                        │  │  - Ejecuta jobs                │  │  │
│                        │  │  - Envía logs via stream       │  │  │
│                        │  └────────────────────────────────┘  │  │
│                        └──────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Historias de Usuario

### HU-9.1: Infraestructura de Test E2E

**Como** desarrollador  
**Quiero** una infraestructura de test que levante Postgres y el servidor gRPC  
**Para** poder ejecutar tests E2E sin configuración manual

**Criterios de Aceptación:**
- [ ] Testcontainers levanta PostgreSQL automáticamente
- [ ] El servidor gRPC se inicia en un puerto aleatorio disponible
- [ ] Las migraciones se ejecutan automáticamente
- [ ] El cleanup se realiza al finalizar el test

**Tareas:**
- [ ] Crear `crates/grpc/tests/e2e_common.rs` con helpers compartidos
- [ ] Implementar `TestServer` struct que encapsula el setup
- [ ] Implementar `TestPostgres` usando Testcontainers
- [ ] Crear macro o función para setup/teardown

### HU-9.2: Build de Imagen Worker para Tests

**Como** desarrollador  
**Quiero** que la imagen del worker se construya automáticamente antes de los tests  
**Para** no tener que construirla manualmente

**Criterios de Aceptación:**
- [ ] Script verifica si la imagen existe
- [ ] Si no existe, la construye automáticamente
- [ ] La imagen incluye el binario `worker` compilado
- [ ] Tag específico para tests: `hodei-worker:e2e-test`

**Tareas:**
- [ ] Crear `scripts/e2e/build-test-image.sh`
- [ ] Modificar `Dockerfile.worker` para soportar builds de test
- [ ] Documentar el proceso en README

### HU-9.3: Test E2E de Provisioning Automático

**Como** desarrollador  
**Quiero** un test que verifique que el DockerProvider crea workers automáticamente  
**Para** validar el flujo de provisioning

**Criterios de Aceptación:**
- [ ] El test encola un job via gRPC
- [ ] El SchedulerService detecta que no hay workers disponibles
- [ ] El DockerProvider crea un container con el worker
- [ ] El worker se registra con OTP
- [ ] El container aparece en `docker ps`

**Tareas:**
- [ ] Implementar `test_docker_auto_provisioning()` en `e2e_docker_provider.rs`
- [ ] Verificar creación de container via Docker API
- [ ] Verificar registro del worker en WorkerRegistry

### HU-9.4: Test E2E de Ejecución de Job Completo

**Como** desarrollador  
**Quiero** un test que ejecute un job de principio a fin  
**Para** validar todo el flujo de ejecución

**Criterios de Aceptación:**
- [ ] Job se encola via `QueueJob`
- [ ] Worker recibe el job via stream
- [ ] Worker ejecuta el comando (ej: `echo "Hello E2E"`)
- [ ] Logs se reciben en el servidor
- [ ] Job se marca como completado en Postgres
- [ ] Exit code es 0

**Tareas:**
- [ ] Implementar `test_docker_full_job_execution()` 
- [ ] Verificar estado del job en cada fase
- [ ] Verificar logs recibidos via LogStreamService
- [ ] Verificar persistencia en Postgres

### HU-9.5: Test E2E de Job con Error

**Como** desarrollador  
**Quiero** un test que verifique el manejo de jobs fallidos  
**Para** validar el flujo de errores

**Criterios de Aceptación:**
- [ ] Job ejecuta comando que falla (exit code != 0)
- [ ] Worker reporta el error correctamente
- [ ] Job se marca como `Failed` en Postgres
- [ ] El error message se persiste

**Tareas:**
- [ ] Implementar `test_docker_job_failure()`
- [ ] Verificar estado `Failed` en JobRepository
- [ ] Verificar que stderr se captura

### HU-9.6: Test E2E de Logs en Tiempo Real

**Como** desarrollador  
**Quiero** un test que verifique el streaming de logs  
**Para** validar que los logs llegan en tiempo real

**Criterios de Aceptación:**
- [ ] Job ejecuta comando con output progresivo
- [ ] Cliente se suscribe a logs via `SubscribeLogs`
- [ ] Logs llegan mientras el job está ejecutando
- [ ] Orden de logs se preserva

**Tareas:**
- [ ] Implementar `test_docker_log_streaming()`
- [ ] Usar job con `sleep` y output progresivo
- [ ] Verificar timestamps y secuencia

### HU-9.7: Test E2E de Cleanup de Workers

**Como** desarrollador  
**Quiero** un test que verifique que los workers se destruyen correctamente  
**Para** evitar containers huérfanos

**Criterios de Aceptación:**
- [ ] Después de completar jobs, el worker se puede destruir
- [ ] El container Docker se elimina
- [ ] El worker se desregistra del WorkerRegistry

**Tareas:**
- [ ] Implementar `test_docker_worker_cleanup()`
- [ ] Verificar que container no existe después de destroy
- [ ] Verificar estado `Terminated` en registry

### HU-9.8: Script de Ejecución E2E

**Como** desarrollador  
**Quiero** un script que ejecute todos los tests E2E de Docker  
**Para** poder correrlos fácilmente en CI/CD

**Criterios de Aceptación:**
- [ ] Script verifica prerequisitos (Docker daemon)
- [ ] Construye imagen de worker si no existe
- [ ] Ejecuta todos los tests E2E de Docker
- [ ] Reporta resultados claramente
- [ ] Limpia recursos al finalizar

**Tareas:**
- [ ] Crear `scripts/e2e/run-docker-e2e.sh`
- [ ] Integrar con CI/CD (GitHub Actions)
- [ ] Documentar en GETTING_STARTED.md

## Requisitos Técnicos

### Dependencias

```toml
# Cargo.toml (dev-dependencies)
[dev-dependencies]
testcontainers = "0.15"
testcontainers-modules = { version = "0.3", features = ["postgres"] }
tokio-test = "0.4"
```

### Variables de Entorno para Tests

| Variable | Valor | Descripción |
|----------|-------|-------------|
| `HODEI_E2E_DOCKER` | `1` | Habilita tests E2E de Docker |
| `HODEI_WORKER_IMAGE` | `hodei-worker:e2e-test` | Imagen del worker |
| `HODEI_SERVER_HOST` | `host.docker.internal` | Host del server desde container |

### Estructura de Archivos

```
crates/grpc/tests/
├── e2e_common.rs              # Helpers compartidos
├── e2e_docker_provider.rs     # Tests E2E Docker
├── grpc_integration.rs        # Tests existentes
└── job_controller_integration.rs

scripts/e2e/
├── run-docker-e2e.sh          # Script principal
├── build-test-image.sh        # Construir imagen
└── cleanup.sh                 # Limpiar recursos
```

## Criterios de Éxito

1. **Cobertura**: Todos los flujos principales probados
2. **Estabilidad**: Tests pasan consistentemente (no flaky)
3. **Velocidad**: Suite completa < 5 minutos
4. **Aislamiento**: Tests no interfieren entre sí
5. **Cleanup**: No quedan recursos huérfanos

## Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Networking Docker complejo | Media | Alto | Usar `host.docker.internal`, documentar alternativas |
| Tests flaky por timing | Alta | Medio | Usar timeouts generosos, reintentos |
| Imagen worker no disponible | Baja | Alto | Build automático en script |
| Containers huérfanos | Media | Bajo | Cleanup en teardown + script de limpieza |

## Estimación

| Historia | Complejidad | Estimación |
|----------|-------------|------------|
| HU-9.1 | Alta | 4h |
| HU-9.2 | Media | 2h |
| HU-9.3 | Alta | 3h |
| HU-9.4 | Alta | 4h |
| HU-9.5 | Media | 2h |
| HU-9.6 | Media | 3h |
| HU-9.7 | Media | 2h |
| HU-9.8 | Baja | 2h |
| **Total** | | **22h** |

## Dependencias con Otras Épicas

- **EPIC-7** (Docker Provider): Debe estar completado ✅
- **EPIC-8** (Firecracker Provider): Independiente
- **EPIC-10** (E2E Kubernetes): Puede reutilizar `e2e_common.rs`
- **EPIC-11** (E2E Firecracker): Puede reutilizar `e2e_common.rs`

## Siguiente Paso

Una vez completada esta épica, los patrones establecidos se reutilizarán para EPIC-10 (Kubernetes) y EPIC-11 (Firecracker).

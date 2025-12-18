# Hodei Jobs Platform - Scripts Overview

Este documento describe todos los scripts disponibles en el proyecto organizados por categorías según su funcionalidad.

## 📋 Scripts Disponibles

### 🚀 start.sh - Inicio Rápido
**Propósito**: Script principal para iniciar toda la plataforma con un solo comando.

**Ubicación**: `scripts/Core Development/start.sh`

**Uso**:
```bash
./scripts/Core_Development/start.sh              # Inicia la plataforma
./scripts/Core_Development/start.sh --build-worker   # Incluye build de worker
./scripts/Core_Development/start.sh --help          # Ver ayuda
```

**Qué hace**:
1. Verifica dependencias (Docker, Docker Compose)
2. Ejecuta cleanup (opcional)
3. Construye imagen del worker (opcional)
4. Crea archivo .env si no existe
5. Inicia todos los servicios con docker-compose.prod.yml
6. Espera a que la API esté lista
7. Muestra estado y URLs de acceso

---

### 🔧 setup.sh - Configuración Inicial
**Propósito**: Configura el entorno de desarrollo completo.

**Ubicación**: `scripts/Core Development/setup.sh`

**Uso**:
```bash
./scripts/Core_Development/setup.sh              # Setup completo
./scripts/Core_Development/setup.sh --minimal    # Setup mínimo
./scripts/Core_Development/setup.sh --help       # Ver ayuda
```

**Qué instala**:
- Rust (cargo, rustc)
- Node.js & npm
- Docker & docker-compose
- Herramientas auxiliares (just, bacon, buf)
- Dependencias del proyecto

---

### 🏗️ dev.sh - Entorno de Desarrollo
**Propósito**: Inicia el entorno de desarrollo con hot reload.

**Ubicación**: `scripts/Core Development/dev.sh`

**Uso**:
```bash
./scripts/Core_Development/dev.sh                # Full stack
./scripts/Core_Development/dev.sh db            # Solo base de datos
./scripts/Core_Development/dev.sh backend       # Solo backend
./scripts/Core_Development/dev.sh frontend      # Solo frontend
./scripts/Core_Development/dev.sh test          # Ejecutar tests
./scripts/Core_Development/dev.sh clean         # Limpiar todo
```

**Características**:
- PostgreSQL en Docker
- Backend con hot reload (Bacon)
- Frontend con HMR (Vite)

---

### 🧹 cleanup.sh - Limpieza de Recursos
**Propósito**: Limpia recursos de Docker (contenedores, imágenes, volúmenes, redes).

**Ubicación**: `scripts/Core Development/cleanup.sh`

**Uso**:
```bash
./scripts/Core_Development/cleanup.sh              # Con confirmación
./scripts/Core_Development/cleanup.sh --force      # Sin confirmación
```

**Qué limpia**:
- Contenedores detenidos
- Imágenes no utilizadas (excepto hodei-jobs-*)
- Volúmenes no utilizados
- Redes no utilizadas
- Cache de build

---

### 🔨 rebuild_worker.sh - Reconstruir Worker
**Propósito**: Reconstruye la imagen del worker con el código más reciente.

**Ubicación**: `scripts/Worker Management/rebuild_worker.sh`

**Uso**:
```bash
./scripts/Worker_Management/rebuild_worker.sh
./scripts/Worker_Management/rebuild_worker.sh --restart
```

**Qué hace**:
1. Compila el binary del worker (release mode)
2. Reconstruye la imagen Docker del worker
3. Opcionalmente reinicia contenedores de workers

---

### 🔐 generate-certificates.sh - Certificados mTLS
**Propósito**: Genera jerarquía completa de certificados PKI para Zero Trust.

**Ubicación**: `scripts/Worker Management/generate-certificates.sh`

**Uso**:
```bash
./scripts/Worker_Management/generate-certificates.sh
```

**Qué genera**:
- Root CA (10 años)
- Intermediate CA (3 años)
- Certificados de worker (90 días)
- Certificados de servidor (1 año)

---

### 🎯 run_maven_job.sh - Ejecutor de Job Maven
**Propósito**: Ejecuta el job complejo de verificación Maven.

**Ubicación**: `scripts/Job Execution/run_maven_job.sh`

**Uso**:
```bash
./scripts/Job_Execution/run_maven_job.sh
```

**Qué hace**:
1. Verifica que la API esté corriendo
2. Delega a maven_job_with_logs.sh --complex
3. Proporciona mejor experiencia con live log streaming

---

### 📊 maven_job_with_logs.sh - Maven con Live Logs
**Propósito**: Ejecuta job Maven con streaming de logs en tiempo real.

**Ubicación**: `scripts/Job Execution/maven_job_with_logs.sh`

**Uso**:
```bash
./scripts/Job_Execution/maven_job_with_logs.sh --simple   # Job simple
./scripts/Job_Execution/maven_job_with_logs.sh --complex  # Job con asdf
```

**Qué hace**:
- Encola job Maven con configuración apropiada
- Stream de logs en tiempo real
- Monitoreo de progreso automático

---

### 🔍 trace-job.sh - Rastreo de Job
**Propósito**: Rastrea job desde inicio hasta finalización.

**Ubicación**: `scripts/Job Execution/trace-job.sh`

**Uso**:
```bash
./scripts/Job_Execution/trace-job.sh <job-id>
./scripts/Job_Execution/trace-job.sh <job-id> --no-logs
```

**Qué muestra**:
1. Estado del job en tiempo real
2. Detalles de ejecución (worker, progreso, exit code)
3. Logs en tiempo real (opcional)
4. Duración total

---

### 📈 watch_logs.sh - Monitor de Logs
**Propósito**: Monitorea y guarda logs de jobs en ejecución.

**Ubicación**: `scripts/Monitoring & Debugging/watch_logs.sh`

**Uso**:
```bash
./scripts/Monitoring_and_Debugging/watch_logs.sh
./scripts/Monitoring_and_Debugging/watch_logs.sh <job-id>
```

**Características**:
- Detecta jobs en estado RUNNING o ASSIGNED
- Stream de logs en tiempo real
- Guarda logs en `build/logs/<job_id>.log`
- Soporte para stdout y stderr
- **Optimizado con LogBatching** (90-99% reducción gRPC)

---

### 📋 list-jobs.sh - Listar Jobs
**Propósito**: Lista jobs con diversos filtros y formatos.

**Ubicación**: `scripts/Monitoring & Debugging/list-jobs.sh`

**Uso**:
```bash
./scripts/Monitoring_and_Debugging/list-jobs.sh                # Todos los jobs
./scripts/Monitoring_and_Debugging/list-jobs.sh --running      # Solo en ejecución
./scripts/Monitoring_and_Debugging/list-jobs.sh --search maven # Buscar por nombre
./scripts/Monitoring_and_Debugging/list-jobs.sh --json         # Formato JSON
```

**Filtros disponibles**:
- --running, --queued, --completed, --failed
- --search <texto>
- --limit <n>
- --json, --table

---

### 🧪 test_e2e.sh - Tests End-to-End
**Propósito**: Ejecuta tests E2E para verificar el flujo completo de jobs.

**Ubicación**: `scripts/Monitoring & Debugging/test_e2e.sh`

**Uso**:
```bash
./scripts/Monitoring_and_Debugging/test_e2e.sh --e2e          # Tests E2E
./scripts/Monitoring_and_Debugging/test_e2e.sh --maven        # Solo test Maven
./scripts/Monitoring_and_Debugging/test_e2e.sh --all          # Todos los tests
./scripts/Monitoring_and_Debugging/test_e2e.sh --unit         # Tests unitarios
./scripts/Monitoring_and_Debugging/test_e2e.sh --integration  # Tests integración
```

**Tests incluidos**:
- Job simple (echo)
- Job Python
- Job con variables de entorno
- Job largo
- Job Maven complejo
- Verificación de lifecycle
- Jobs concurrentes
- Manejo de errores

---

## 🔗 Flujos de Trabajo Comunes

### Inicio Rápido

```bash
# 1. Setup inicial (solo primera vez)
./scripts/Core_Development/setup.sh

# 2. Iniciar plataforma
./scripts/Core_Development/start.sh --build-worker

# 3. Monitorear logs
./scripts/Monitoring_and_Debugging/watch_logs.sh
```

### Desarrollo Local

```bash
# Setup completo
./scripts/Core_Development/setup.sh

# Iniciar desarrollo con hot reload
./scripts/Core_Development/dev.sh

# Ejecutar tests E2E
./scripts/Monitoring_and_Debugging/test_e2e.sh --all

# Limpiar recursos
./scripts/Core_Development/cleanup.sh
```

### Certificados mTLS (Zero Trust)

```bash
# Generar certificados PKI
./scripts/Worker_Management/generate-certificates.sh

# Reconstruir worker con certificados
./scripts/Worker_Management/rebuild_worker.sh --restart
```

### Ejecución y Monitoreo de Jobs

```bash
# Ejecutar job Maven con logs
./scripts/Job_Execution/maven_job_with_logs.sh --complex

# En otra terminal, monitorear
./scripts/Monitoring_and_Debugging/watch_logs.sh

# Listar jobs
./scripts/Monitoring_and_Debugging/list-jobs.sh --running

# Rastrear job específico
./scripts/Job_Execution/trace-job.sh <job-id>
```

---

## 🏗️ Estructura de Directorios

```
scripts/
├── Core Development
│   ├── setup.sh              # Configuración inicial
│   ├── dev.sh                # Desarrollo con hot reload
│   ├── start.sh              # Inicio rápido
│   └── cleanup.sh            # Limpieza Docker
│
├── Worker Management
│   ├── rebuild_worker.sh     # Reconstruir imagen worker
│   └── generate-certificates.sh # Certificados mTLS
│
├── Job Execution
│   ├── run_maven_job.sh      # Ejecutor Maven (delegado)
│   ├── maven_job_with_logs.sh # Maven con live logs
│   └── trace-job.sh          # Rastreo de jobs
│
├── Monitoring & Debugging
│   ├── watch_logs.sh         # Monitor de logs
│   ├── list-jobs.sh          # Listar jobs
│   └── test_e2e.sh           # Tests E2E
│
└── Firecracker Provider
    └── firecracker/          # Scripts Firecracker (opcional)
```

---

## ⚡ Comandos Just Actualizados

Los siguientes comandos `just` están disponibles y actualizados:

```bash
just dev                    # Desarrollo completo
just dev-db                 # Solo base de datos
just maven-job              # Job Maven (simple)
just maven-job-complex      # Job Maven (complejo)
just watch-logs             # Monitor de logs
just cert-generate          # Generar certificados
just rebuild-worker         # Reconstruir worker
just test                   # Ejecutar tests
```

---

## 📚 Documentación Relacionada

- [GETTING_STARTED.md](../GETTING_STARTED.md) - Guía completa de inicio
- [GETTING_STARTED_KUBERNETES.md](../GETTING_STARTED_KUBERNETES.md) - Setup Kubernetes
- [README.md](../README.md) - Visión general del proyecto
- [docs/architecture.md](../docs/architecture.md) - Arquitectura del sistema
- [docs/workflows.md](../docs/workflows.md) - Flujos de trabajo detallados

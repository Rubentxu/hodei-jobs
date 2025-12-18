<p align="center">
  <img src="docs/assets/hodei-jobs.png" alt="Hodei Jobs Logo" width="800" />
</p>

<h1 align="center">Hodei Jobs Platform</h1>

<p align="center">
  <strong>Plataforma de ejecución de jobs distribuidos lista para HPC con seguridad Zero Trust</strong>
</p>

<p align="center">
  <a href="#características">Características</a> •
  <a href="#inicio-rápido">Inicio Rápido</a> •
  <a href="#arquitectura">Arquitectura</a> •
  <a href="#proveedores">Proveedores</a> •
  <a href="#documentación">Documentación</a> •
  <a href="#contribuir">Contribuir</a>
</p>

<p align="center">
  <a href="https://github.com/Rubentxu/hodei-jobs/actions/workflows/ci.yml">
    <img src="https://github.com/Rubentxu/hodei-jobs/actions/workflows/ci.yml/badge.svg" alt="Estado CI" />
  </a>
  <a href="https://github.com/Rubentxu/hodei-jobs/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/licencia-MIT-blue.svg" alt="Licencia" />
  </a>
  <a href="https://rust-lang.org">
    <img src="https://img.shields.io/badge/rust-1.83%2B-orange.svg" alt="Versión Rust" />
  </a>
  <a href="./README.md">
    <img src="https://img.shields.io/badge/lang-english-blue.svg" alt="English" />
  </a>
</p>

---

## 🎯 ¿Qué es Hodei Jobs?

**Hodei Jobs** es una plataforma de ejecución de jobs distribuidos lista para producción y HPC, construida en Rust. Aprovisiona workers automáticamente bajo demanda usando tu infraestructura preferida (Docker, Kubernetes o microVMs Firecracker) y ejecuta jobs con observabilidad completa y seguridad Zero Trust.

### ¿Por qué Hodei v8.0?

- **🚀 Escalado Bajo Demanda**: Los workers se aprovisionan automáticamente cuando se encolan jobs
- **🔌 Proveedores Intercambiables**: Elige Docker para simplicidad, Kubernetes para orquestación, o Firecracker para aislamiento
- **🔐 Seguridad Zero Trust**: Infraestructura mTLS con PKI, inyección de secretos vía stdin, redacción de logs
- **📊 Observabilidad Completa**: Streaming de logs con batching (90-99% reducción overhead), métricas cacheadas, integración cgroups
- **⚡ High Performance Computing**: LogBatching, I/O Zero-Copy, manejo de backpressure, operaciones asíncronas
- **🏗️ Listo para Producción**: Arquitectura DDD, testing exhaustivo (277+ tests), patrones probados

---

## ✨ Características

| Característica | Descripción |
|----------------|-------------|
| **Aprovisionamiento Automático** | Los workers se crean bajo demanda cuando se encolan jobs |
| **Múltiples Proveedores** | Contenedores Docker, pods Kubernetes o microVMs Firecracker |
| **Autenticación OTP** | Autenticación segura con contraseñas de un solo uso |
| **LogBatching (v8.0)** | 90-99% reducción en overhead gRPC con batching automático |
| **I/O Zero-Copy (v8.0)** | Streaming eficiente con FramedRead + BytesCodec |
| **Inyección de Secretos (v8.0)** | Inyección segura vía stdin con serialización JSON y redacción |
| **mTLS/Zero Trust (v8.0)** | Infraestructura PKI completa con gestión de certificados |
| **Métricas Cacheadas (v8.0)** | Métricas no bloqueantes con cache TTL 35s e integración cgroups |
| **Patrón Write-Execute (v8.0)** | Ejecución de scripts estilo Jenkins/K8s con safety headers |
| **Manejo de Backpressure (v8.0)** | Operaciones asíncronas no bloqueantes con try_send() |
| **Gestión del Ciclo de Vida** | Encolar, monitorizar, cancelar y reintentar jobs |
| **API gRPC** | API de alto rendimiento con streaming bidireccional |
| **API REST** | Endpoints HTTP para integración sencilla |
| **Escalado Horizontal** | Ejecuta múltiples instancias del servidor |

---

## 📊 Métricas de Rendimiento (v8.0)

El worker agent v8.0 ofrece mejoras significativas de rendimiento:

| Optimización | Mejora | Métrica |
|--------------|--------|---------|
| **LogBatching** | 90-99% | Reducción llamadas gRPC |
| **I/O Zero-Copy** | ~40% | Reducción allocaciones memoria |
| **Métricas Cacheadas** | ~60% | Overhead recolección métricas |
| **Manejo de Backpressure** | 100% | Estabilidad runtime async |
| **Patrón Write-Execute** | N/A | Robustez ejecución scripts |

**Resultados de Tests**: Todos los 277 tests pasando, incluyendo 30 tests de performance específicos del worker.

---

## 🚀 Inicio Rápido

### Requisitos Previos

```bash
# Rust 1.83+ (edición 2024)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Compilador Protocol Buffers
sudo apt install protobuf-compiler  # Ubuntu/Debian
brew install protobuf               # macOS

# Docker (para el proveedor Docker)
sudo apt install docker.io && sudo usermod -aG docker $USER
```

### Instalación

```bash
# Clonar el repositorio
git clone https://github.com/Rubentxu/hodei-jobs.git
cd hodei-jobs

# Compilar el proyecto
cargo build --workspace --release
```

### Ejecutar el Servidor

```bash
# Iniciar PostgreSQL
docker run -d --name hodei-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=hodei \
  -p 5432:5432 postgres:16-alpine

# Iniciar el servidor con proveedor Docker
HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei" \
HODEI_DEV_MODE=1 \
HODEI_DOCKER_ENABLED=1 \
cargo run --bin server -p hodei-jobs-grpc
```

### Encolar Tu Primer Job

```bash
# Instalar grpcurl
brew install grpcurl  # macOS
# o: go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# Encolar un job
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "mi-primer-job"},
    "name": "Hola Mundo",
    "command": "echo",
    "arguments": ["¡Hola desde Hodei!"]
  },
  "queued_by": "quickstart"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Verificar estado del job
grpcurl -plaintext -d '{
  "job_id": {"value": "mi-primer-job"}
}' localhost:50051 hodei.JobExecutionService/GetJobStatus
```

¡Eso es todo! El servidor aprovisiona automáticamente un contenedor Docker, el worker se registra, ejecuta el job y reporta el resultado.

---

## 🏗️ Arquitectura

### Componentes de Alto Nivel

El siguiente diagrama ilustra la interacción entre los servicios principales:

<p align="center">
  <img src="docs/assets/diagrama-jobs-services.png" alt="Diagrama de Servicios Hodei Jobs" width="800" />
</p>

### Flujo de Ejecución de Jobs

Vista detallada de cómo fluye un job a través del sistema:

<p align="center">
  <img src="docs/assets/diagrama-jobs-eng.png" alt="Flujo de Ejecución Hodei Jobs" width="800" />
</p>

```
┌─────────────────────────────────────────────────────────────────┐
│                     HODEI JOBS PLATFORM                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────┐     ┌────────────────────────────────────────┐    │
│  │ Cliente │────▶│      Servidor gRPC (Control Plane)     │    │
│  └─────────┘     │  ┌──────────────┐ ┌─────────────────┐  │    │
│                  │  │ JobExecution │ │ WorkerAgent     │  │    │
│                  │  │ Service      │ │ Service (OTP)   │  │    │
│                  │  └──────┬───────┘ └────────┬────────┘  │    │
│                  │         │                  │           │    │
│                  │  ┌──────▼──────────────────▼────────┐  │    │
│                  │  │  Servicio de Aprovisionamiento   │  │    │
│                  │  └──────────────┬───────────────────┘  │    │
│                  └─────────────────┼──────────────────────┘    │
│                                    │                           │
│                  ┌─────────────────▼───────────────────┐       │
│                  │       Proveedores de Workers        │       │
│                  │  ┌────────┐ ┌─────┐ ┌───────────┐  │       │
│                  │  │ Docker │ │ K8s │ │Firecracker│  │       │
│                  │  └───┬────┘ └──┬──┘ └─────┬─────┘  │       │
│                  └──────┼─────────┼──────────┼────────┘       │
│                         │         │          │                 │
│                   Contenedor    Pod      microVM               │
│                    (Worker)   (Worker)  (Worker)               │
└─────────────────────────────────────────────────────────────────┘
```

### Ciclo de Vida del Job

1. **Encolar** → El cliente envía el job via `QueueJob`
2. **Programar** → El servidor detecta el job pendiente, aprovisiona worker con OTP
3. **Registrar** → El worker arranca, lee el OTP, se registra con el servidor
4. **Despachar** → El servidor envía el job al worker via stream bidireccional
5. **Ejecutar** → El worker ejecuta el comando, transmite logs
6. **Completar** → El worker reporta el resultado, el job se marca como completado

---

## 🔌 Proveedores

### Proveedor Docker

Ideal para: **Desarrollo, CI/CD, despliegues simples**

```bash
HODEI_DOCKER_ENABLED=1 \
HODEI_WORKER_IMAGE=hodei-worker:latest \
cargo run --bin server -p hodei-jobs-grpc
```

### Proveedor Kubernetes

Ideal para: **Producción, auto-escalado, despliegues cloud-native**

```bash
HODEI_K8S_ENABLED=1 \
HODEI_K8S_NAMESPACE=hodei-workers \
HODEI_WORKER_IMAGE=tu-registro/hodei-worker:v1.0.0 \
cargo run --bin server -p hodei-jobs-grpc
```

### Proveedor Firecracker

Ideal para: **Máximo aislamiento, entornos multi-tenant, cargas críticas de seguridad**

```bash
HODEI_FC_ENABLED=1 \
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux \
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4 \
sudo cargo run --bin server -p hodei-jobs-grpc
```

---

## 📚 Documentación

| Documento | Descripción |
|-----------|-------------|
| [**GETTING_STARTED.md**](GETTING_STARTED.md) | Guía completa de configuración con ejemplos |
| [**docs/architecture.md**](docs/architecture.md) | Arquitectura DDD y decisiones de diseño |
| [**docs/development.md**](docs/development.md) | Guía de desarrollo para contribuidores |
| [**docs/use-cases.md**](docs/use-cases.md) | Casos de uso y diagramas de secuencia |
| [**docs/workflows.md**](docs/workflows.md) | Diagramas de flujos detallados (v8.0) |
| [**docs/security/PKI-DESIGN.md**](docs/security/PKI-DESIGN.md) | Arquitectura PKI mTLS (v8.0) |
| [**docs/security/CERTIFICATE-MANAGEMENT.md**](docs/security/CERTIFICATE-MANAGEMENT.md) | Operaciones de certificados (v8.0) |

---

## 🧪 Testing

```bash
# Tests unitarios (~95 tests)
cargo test --workspace

# Tests de integración (requieren Docker)
cargo test --test docker_integration -- --ignored

# Tests E2E (stack completo)
cargo test --test e2e_docker_provider -- --ignored --nocapture

# E2E Kubernetes (requiere cluster)
HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored

# E2E Firecracker (requiere KVM)
HODEI_FC_TEST=1 cargo test --test e2e_firecracker_provider -- --ignored
```

---

## 🤝 Contribuir

¡Las contribuciones son bienvenidas! Así puedes ayudar:

### Formas de Contribuir

- 🐛 **Reportar bugs** - Abre un issue con pasos de reproducción
- 💡 **Sugerir características** - Comparte tus ideas en las discusiones
- 📖 **Mejorar documentación** - Corrige errores, añade ejemplos, clarifica explicaciones
- 🔧 **Enviar PRs** - Correcciones, características o mejoras

### Configuración de Desarrollo

```bash
# Fork y clonar
git clone https://github.com/TU_USUARIO/hodei-jobs.git
cd hodei-jobs

# Crear una rama
git checkout -b feature/mi-caracteristica

# Hacer cambios y probar
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all

# Enviar PR
```

### Estilo de Código

- Sigue los idiomas y mejores prácticas de Rust
- Usa `cargo fmt` para formateo
- Asegúrate de que `cargo clippy` pase sin warnings
- Añade tests para nueva funcionalidad
- Actualiza la documentación según sea necesario

---

## 📊 Estado del Proyecto

| Componente | Estado |
|------------|--------|
| Plataforma Core | ✅ Producción |
| Proveedor Docker | ✅ Estable |
| Proveedor Kubernetes | ✅ Estable |
| Proveedor Firecracker | 🔶 Beta |
| API REST | ✅ Estable |
| API gRPC | ✅ Estable |
| Interfaz Web | 🔜 Planificado |

---

## 🗺️ Roadmap

- [ ] Dashboard web para monitorización de jobs
- [ ] Programación de jobs (tipo cron)
- [ ] Dependencias y workflows de jobs
- [ ] Endpoint de métricas Prometheus
- [ ] Trazas OpenTelemetry
- [ ] Soporte multi-región
- [ ] Caché de resultados de jobs

---

## 📄 Licencia

Este proyecto está licenciado bajo la Licencia MIT - ver el archivo [LICENSE](LICENSE) para más detalles.

---

## 🙏 Agradecimientos

- [Tokio](https://tokio.rs/) - Runtime async para Rust
- [Tonic](https://github.com/hyperium/tonic) - Implementación gRPC
- [Bollard](https://github.com/fussybeaver/bollard) - Cliente API Docker
- [kube-rs](https://github.com/kube-rs/kube) - Cliente Kubernetes
- [Firecracker](https://firecracker-microvm.github.io/) - Tecnología microVM

---

<p align="center">
  <strong>⭐ ¡Dale una estrella a este repo si te resulta útil!</strong>
</p>

<p align="center">
  Hecho con ❤️ por <a href="https://github.com/Rubentxu">Rubentxu</a>
</p>

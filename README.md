<p align="center">
  <img src="docs/assets/hodei-jobs.png" alt="Hodei Jobs Logo" width="800" />
</p>

<h1 align="center">Hodei Jobs Platform</h1>

<p align="center">
  <strong>A distributed job execution platform with pluggable worker providers</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#providers">Providers</a> •
  <a href="#documentation">Documentation</a> •
  <a href="#contributing">Contributing</a>
</p>

<p align="center">
  <a href="https://github.com/Rubentxu/hodei-jobs/actions/workflows/ci.yml">
    <img src="https://github.com/Rubentxu/hodei-jobs/actions/workflows/ci.yml/badge.svg" alt="CI Status" />
  </a>
  <a href="https://github.com/Rubentxu/hodei-jobs/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" />
  </a>
  <a href="https://rust-lang.org">
    <img src="https://img.shields.io/badge/rust-1.83%2B-orange.svg" alt="Rust Version" />
  </a>
  <a href="./README_ES.md">
    <img src="https://img.shields.io/badge/lang-español-red.svg" alt="Español" />
  </a>
</p>

---

## 🎯 What is Hodei Jobs?

**Hodei Jobs** is a production-ready distributed job execution platform built in Rust. It automatically provisions workers on-demand using your preferred infrastructure (Docker, Kubernetes, or Firecracker microVMs) and executes jobs with full observability.

### Why Hodei?

- **🚀 On-Demand Scaling**: Workers are provisioned automatically when jobs are queued
- **🔌 Pluggable Providers**: Choose Docker for simplicity, Kubernetes for orchestration, or Firecracker for isolation
- **🔐 Secure by Design**: OTP-based worker authentication prevents unauthorized access
- **📊 Full Observability**: Real-time log streaming, metrics, and job status tracking
- **⚡ High Performance**: Built in Rust with async/await for maximum throughput
- **🏗️ Production Ready**: DDD architecture, comprehensive testing, and battle-tested patterns

---

## ✨ Features


| Feature                           | Description                                                 |
| --------------------------------- | ----------------------------------------------------------- |
| **Automatic Worker Provisioning** | Workers are created on-demand when jobs are queued          |
| **Multiple Providers**            | Docker containers, Kubernetes pods, or Firecracker microVMs |
| **OTP Authentication**            | Secure one-time password authentication for workers         |
| **Real-time Logs**                | Stream job logs as they're generated                        |
| **Job Lifecycle Management**      | Queue, monitor, cancel, and retry jobs                      |
| **gRPC API**                      | High-performance API with bidirectional streaming           |
| **REST API**                      | HTTP endpoints for easy integration                         |
| **Horizontal Scaling**            | Run multiple server instances for high availability         |

---

## 🚀 Quick Start

### Prerequisites

```bash
# Rust 1.83+ (2024 edition)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Protocol Buffers compiler
sudo apt install protobuf-compiler  # Ubuntu/Debian
brew install protobuf               # macOS

# Docker (for Docker provider)
sudo apt install docker.io && sudo usermod -aG docker $USER
```

### Installation

```bash
# Clone the repository
git clone https://github.com/Rubentxu/hodei-jobs.git
cd hodei-jobs

# Build the project
cargo build --workspace --release
```

### Run the Server

```bash
# Start PostgreSQL
docker run -d --name hodei-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=hodei \
  -p 5432:5432 postgres:16-alpine

# Start the server with Docker provider
HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei" \
HODEI_DEV_MODE=1 \
HODEI_DOCKER_ENABLED=1 \
cargo run --bin server -p hodei-jobs-grpc
```

### Queue Your First Job

```bash
# Install grpcurl
brew install grpcurl  # macOS
# or: go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# Queue a job
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "my-first-job"},
    "name": "Hello World",
    "command": "echo",
    "arguments": ["Hello from Hodei!"]
  },
  "queued_by": "quickstart"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Check job status
grpcurl -plaintext -d '{
  "job_id": {"value": "my-first-job"}
}' localhost:50051 hodei.JobExecutionService/GetJobStatus
```

That's it! The server automatically provisions a Docker container, the worker registers itself, executes the job, and reports the result.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     HODEI JOBS PLATFORM                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────┐     ┌────────────────────────────────────────┐    │
│  │ Client  │────▶│         gRPC Server (Control Plane)    │    │
│  └─────────┘     │  ┌──────────────┐ ┌─────────────────┐  │    │
│                  │  │ JobExecution │ │ WorkerAgent     │  │    │
│                  │  │ Service      │ │ Service (OTP)   │  │    │
│                  │  └──────┬───────┘ └────────┬────────┘  │    │
│                  │         │                  │           │    │
│                  │  ┌──────▼──────────────────▼────────┐  │    │
│                  │  │    Worker Provisioning Service   │  │    │
│                  │  └──────────────┬───────────────────┘  │    │
│                  └─────────────────┼──────────────────────┘    │
│                                    │                           │
│                  ┌─────────────────▼───────────────────┐       │
│                  │         Worker Providers            │       │
│                  │  ┌────────┐ ┌─────┐ ┌───────────┐  │       │
│                  │  │ Docker │ │ K8s │ │Firecracker│  │       │
│                  │  └───┬────┘ └──┬──┘ └─────┬─────┘  │       │
│                  └──────┼─────────┼──────────┼────────┘       │
│                         │         │          │                 │
│                    Container    Pod      microVM               │
│                    (Worker)   (Worker)  (Worker)               │
└─────────────────────────────────────────────────────────────────┘
```

### Job Lifecycle

1. **Queue** → Client submits job via `QueueJob`
2. **Schedule** → Server detects pending job, provisions worker with OTP
3. **Register** → Worker starts, reads OTP, registers with server
4. **Dispatch** → Server sends job to worker via bidirectional stream
5. **Execute** → Worker runs command, streams logs
6. **Complete** → Worker reports result, job marked as completed

---

## 🔌 Providers

### Docker Provider

Best for: **Development, CI/CD, simple deployments**

```bash
HODEI_DOCKER_ENABLED=1 \
HODEI_WORKER_IMAGE=hodei-worker:latest \
cargo run --bin server -p hodei-jobs-grpc
```

### Kubernetes Provider

Best for: **Production, auto-scaling, cloud-native deployments**

```bash
HODEI_K8S_ENABLED=1 \
HODEI_K8S_NAMESPACE=hodei-workers \
HODEI_WORKER_IMAGE=your-registry/hodei-worker:v1.0.0 \
cargo run --bin server -p hodei-jobs-grpc
```

### Firecracker Provider

Best for: **Maximum isolation, multi-tenant environments, security-critical workloads**

```bash
HODEI_FC_ENABLED=1 \
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux \
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4 \
sudo cargo run --bin server -p hodei-jobs-grpc
```

---

## 📚 Documentation


| Document                                         | Description                           |
| ------------------------------------------------ | ------------------------------------- |
| [**GETTING_STARTED.md**](GETTING_STARTED.md)     | Complete setup guide with examples    |
| [**docs/architecture.md**](docs/architecture.md) | DDD architecture and design decisions |
| [**docs/development.md**](docs/development.md)   | Development guide for contributors    |
| [**docs/use-cases.md**](docs/use-cases.md)       | Use cases and sequence diagrams       |

---

## 🧪 Testing

```bash
# Unit tests (~95 tests)
cargo test --workspace

# Integration tests (require Docker)
cargo test --test docker_integration -- --ignored

# E2E tests (full stack)
cargo test --test e2e_docker_provider -- --ignored --nocapture

# Kubernetes E2E (requires cluster)
HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored

# Firecracker E2E (requires KVM)
HODEI_FC_TEST=1 cargo test --test e2e_firecracker_provider -- --ignored
```

---

## 🤝 Contributing

We welcome contributions! Here's how you can help:

### Ways to Contribute

- 🐛 **Report bugs** - Open an issue with reproduction steps
- 💡 **Suggest features** - Share your ideas in discussions
- 📖 **Improve docs** - Fix typos, add examples, clarify explanations
- 🔧 **Submit PRs** - Bug fixes, features, or improvements

### Development Setup

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/hodei-jobs.git
cd hodei-jobs

# Create a branch
git checkout -b feature/my-feature

# Make changes and test
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all

# Submit PR
```

### Code Style

- Follow Rust idioms and best practices
- Use `cargo fmt` for formatting
- Ensure `cargo clippy` passes with no warnings
- Add tests for new functionality
- Update documentation as needed

---

## 📊 Project Status


| Component            | Status              |
| -------------------- | ------------------- |
| Core Platform        | ✅ Production Ready |
| Docker Provider      | ✅ Stable           |
| Kubernetes Provider  | ✅ Stable           |
| Firecracker Provider | 🔶 Beta             |
| REST API             | ✅ Stable           |
| gRPC API             | ✅ Stable           |
| Web Dashboard        | ✅ Production Ready |
| Helm Chart           | ✅ Available        |

---

## 🗺️ Roadmap

- [X]  Web dashboard for job monitoring
- [ ]  Job scheduling (cron-like)
- [ ]  Job dependencies and workflows
- [ ]  Prometheus metrics endpoint
- [ ]  OpenTelemetry tracing
- [ ]  Multi-region support
- [ ]  Job result caching

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- [Tokio](https://tokio.rs/) - Async runtime for Rust
- [Tonic](https://github.com/hyperium/tonic) - gRPC implementation
- [Bollard](https://github.com/fussybeaver/bollard) - Docker API client
- [kube-rs](https://github.com/kube-rs/kube) - Kubernetes client
- [Firecracker](https://firecracker-microvm.github.io/) - microVM technology

---

<p align="center">
  <strong>⭐ Star this repo if you find it useful!</strong>
</p>

<p align="center">
  Made with ❤️ by <a href="https://github.com/Rubentxu">Rubentxu</a>
</p>

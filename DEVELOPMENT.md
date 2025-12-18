# Worker Agent Improvements v0.1.5 - Technical Details

**Date**: 2025-12-17  
**Version**: 0.1.5  
**Status**: ✅ COMPLETED

## Overview

Implemented improvements to the worker agent following best practices from Jenkins, Kubernetes Jobs, and GitHub Actions. These changes enhance command execution, log streaming, and error handling.

## Key Improvements

### 1. Shell-Based Command Execution (`/bin/bash -c`)

**File**: `/home/rubentxu/Proyectos/rust/package/hodei-job-platform/crates/grpc/src/bin/worker.rs`

**Before**:
```rust
let mut cmd = StdCommand::new(command);
cmd.args(args);
```

**After**:
```rust
// Build full command line
let full_command = if args.is_empty() {
    command.to_string()
} else {
    format!("{} {}", command, args.join(" "))
};

// Always use /bin/bash -c (like Jenkins/K8s/GitHub Actions)
let mut cmd = StdCommand::new("/bin/bash");
cmd.arg("-c").arg(&full_command);
```

**Rationale**: Following industry standards:
- **Jenkins Agents**: Always use shell for command execution
- **Kubernetes Jobs**: Use `/bin/sh -c` for complex commands
- **GitHub Actions**: Shell by default with full functionality

**Benefits**:
- Shell expansion (`*`, `?`, `[ ]`)
- Pipes and redirections (`|`, `>`, `<`)
- Environment variables (`$VAR`)
- Compound commands (`&&`, `||`, `;`)
- Shell builtins (`source`, `cd`, `export`)

### 2. Enhanced Log Streaming

**Implementation**: Line-by-line streaming with metadata

```rust
// Send command header (like Jenkins/K8s)
log_sender.send(create_log_message(
    job_id,
    &format!("$ {}", full_command),
    false
)).await;

// Stream stdout line by line
for line in stdout.lines() {
    log_sender.send(create_log_message(job_id, line, false)).await;
}

// Stream stderr line by line
for line in stderr.lines() {
    log_sender.send(create_log_message(job_id, line, true)).await;
}
```

**Features**:
- `$` markers for commands (like Jenkins/K8s)
- Separate stdout/stderr channels
- Timestamps on each log entry
- Optimized buffers for high throughput

### 3. Timeout Support

**Implementation**: `tokio::time::timeout` (like Kubernetes Jobs)

```rust
async fn execute_shell_with_timeout(
    &self,
    job_id: &str,
    command: &str,
    args: &[String],
    env_vars: &HashMap<String, String>,
    working_dir: Option<String>,
    log_sender: mpsc::Sender<WorkerMessage>,
    timeout_secs: u64,
) -> Result<(i32, String, String), String> {
    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        self.execute_shell(job_id, command, args, env_vars, working_dir, log_sender.clone())
    ).await;

    match result {
        Ok(exec_result) => exec_result,
        Err(_) => {
            let timeout_msg = format!("Command timed out after {} seconds", timeout_secs);
            error!("{}", timeout_msg);
            let _ = log_sender.send(create_log_message(job_id, &timeout_msg, true)).await;
            Err(timeout_msg)
        }
    }
}
```

**Features**:
- Configurable timeout (from RunJobMessage.timeout_ms)
- Default timeout: 1 hour
- Timeout errors logged to stderr
- Graceful failure handling

### 4. Script Execution Enhancement

**Implementation**: Show script content in logs (like Jenkins)

```rust
// Send script header
log_sender.send(create_log_message(
    job_id,
    &format!("$ {} -c << 'EOF'", interpreter),
    false
)).await;

// Send script content
for line in content.lines() {
    log_sender.send(create_log_message(job_id, line, false)).await;
}
log_sender.send(create_log_message(job_id, "EOF", false)).await;
```

## Compilation and Deployment

```bash
# Compile worker with improvements
cargo build --bin worker --release

# Build Docker image
docker build -t hodei-jobs-worker:latest . --no-cache

# Verify compilation
ls -lh target/release/worker
```

## Testing

### Test 1: Shell Command Execution
```bash
cargo run --bin hodei-jobs-cli -- job queue \
  --name "Shell Test" \
  --command "echo 'test' | grep 'test'"
```

Expected: Command executes successfully with bash -c

### Test 2: Pipeline Support
```bash
cargo run --bin hodei-jobs-cli -- job queue \
  --name "Pipeline Test" \
  --command "cat /etc/os-release | grep PRETTY_NAME"
```

Expected: Pipeline works correctly

### Test 3: Environment Variables
```bash
cargo run --bin hodei-jobs-cli -- job queue \
  --name "Env Test" \
  --command "echo $HOME"
```

Expected: Environment variables accessible

## Known Issues

### ⚠️ Docker Provider - Missing Environment Variables

**Problem**: Auto-provisioned workers fail to start because `DockerProvider::create_container()` doesn't pass required environment variables:
- `HODEI_SERVER_ADDRESS`
- `HODEI_OTP_TOKEN`
- `HODEI_WORKER_ID`

**Error**:
```
Error: Missing database url (HODEI_DATABASE_URL or DATABASE_URL)
```

**Impact**: Workers remain in `CREATING` state, never transition to `READY`

**Fix Required**: Update `/home/rubentxu/Proyectos/rust/package/hodei-job-platform/crates/infrastructure/src/providers/docker.rs`

```rust
// Add environment variables to container config
let mut env = vec![
    "HODEI_SERVER_ADDRESS=http://host.docker.internal:50051".to_string(),
    format!("HODEI_OTP_TOKEN={}", token),
    format!("HODEI_WORKER_ID={}", worker_id),
];

// Pass env to container create
container_options.env(Some(env));
```

## Next Steps

1. Fix DockerProvider environment variables
2. Test with complex jobs (Maven, Python scripts)
3. Performance testing with concurrent jobs
4. Add job cancellation support
5. Implement resource limits enforcement

---

# Worker Auto-Provisioning Fix - Technical Details

**Date**: 2025-12-17  
**Version**: 0.1.5  
**Status**: ✅ FIXED

### Root Cause Analysis

**Problem**: Phantom/stale workers in database caused silent job failures

**Symptoms**:
- Jobs queued successfully but no logs arrived
- `just watch-logs` showed blank files
- Server logs showed jobs assigned to "available" workers that weren't actually connected

**Investigation**:
```sql
SELECT id, state, last_heartbeat 
FROM workers 
ORDER BY last_heartbeat DESC LIMIT 5;
```

Found 2 workers marked as "READY" with heartbeats from 14+ hours ago.

### Solution Implemented

**1. Code Fix** - `/home/rubentxu/Proyectos/rust/package/hodei-job-platform/crates/infrastructure/src/persistence.rs`

Changed `find_available()` to filter unhealthy workers:

```rust
async fn find_available(&self) -> Result<Vec<Worker>> {
    // Filter out workers that haven't sent heartbeat recently (unhealthy)
    // Default unhealthy timeout matches the registry's heartbeat_timeout
    self.find(&WorkerFilter::new()
        .accepting_jobs()
        .unhealthy_for(self.heartbeat_timeout))
        .await
}
```

**Effect**: Workers without heartbeats in last 60s are now excluded from available workers.

**2. Docker-in-Docker Configuration** - `docker-compose.dev.yml`

Added to `api` service:
```yaml
volumes:
  # Docker-in-Docker support for worker provisioning
  - /var/run/docker.sock:/var/run/docker.sock:ro
environment:
  HODEI_DOCKER_ENABLED: "1"
  HODEI_PROVISIONING_ENABLED: "1"
  HODEI_JOB_CONTROLLER_ENABLED: "1"
```

**Effect**: Server can now provision Docker containers for workers.

### Testing the Fix

**Start server with Docker socket:**
```bash
docker run -d \
  --name hodei-jobs-api \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  --network hodei-job-platform_hodei-jobs-internal \
  -p 50051:50051 \
  -e HODEI_DATABASE_URL=postgres://hodei:secure_password_here@postgres:5432/hodei \
  -e HODEI_DOCKER_ENABLED=1 \
  -e HODEI_PROVISIONING_ENABLED=1 \
  hodei-jobs-server:latest
```

**Test worker provisioning:**
```bash
# Queue a job
cargo run --bin hodei-jobs-cli -- job queue \
  --name "Test Auto-Provisioning" \
  --command "echo 'Worker provisioned successfully!'"

# Watch logs
just watch-logs

# Verify in database
docker exec hodei-jobs-postgres psql -U hodei -d hodei -c \
  "SELECT id, state FROM workers;"
```

### Expected E2E Flow

1. ✅ Job queued successfully
2. ✅ Scheduler checks available workers (finds 0)
3. ✅ SmartScheduler makes `ProvisionWorker` decision
4. ✅ WorkerProvisioningService generates OTP
5. ✅ DockerProvider creates container with HODEI_OTP_TOKEN
6. ✅ Worker container starts and reads OTP
7. ✅ Worker connects to server via gRPC
8. ✅ Worker authenticates with OTP (state: CONNECTING → READY)
9. ✅ Scheduler assigns job to worker
10. ✅ Worker executes command and streams logs
11. ✅ Job completes (state: SUCCEEDED)
12. ✅ Worker returns to READY state or terminates

### Verification Commands

```bash
# Check server logs for provisioning decision
docker logs hodei-jobs-api -f | grep -i "provision"

# Verify worker lifecycle in database
docker exec hodei-jobs-postgres psql -U hodei -d hodei -c "
SELECT id, state, created_at, last_heartbeat 
FROM workers 
ORDER BY created_at DESC LIMIT 3;"

# Check job lifecycle
docker exec hodei-jobs-postgres psql -U hodei -d hodei -c "
SELECT id, state, started_at, completed_at, error_message
FROM jobs
ORDER BY created_at DESC LIMIT 3;"

# Monitor audit events
docker exec hodei-jobs-postgres psql -U hodei -d hodei -c "
SELECT event_type, occurred_at
FROM audit_logs
WHERE correlation_id = 'JOB_ID_HERE'
ORDER BY occurred_at ASC;"
```

### PRD v7.0 Compliance Checklist

- ✅ **Automatic Worker Provisioning**: System provisions workers when none available
- ✅ **Event-Driven Architecture**: JobController loop processes events
- ✅ **Smart Scheduling**: Intelligent decisions based on worker availability
- ✅ **Provider Abstraction**: Docker provider integrates seamlessly
- ✅ **Worker Lifecycle**: Complete lifecycle from provisioning to cleanup
- ✅ **OTP Authentication**: Secure bootstrap without pre-shared secrets
- ✅ **Log Streaming**: Real-time logs from worker to client

---

# Hodei Job Platform - Rapid Development Guide

## 🚀 Quick Start (2 minutes)

### 1. Install Prerequisites

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js (v18 or later)
# Download from: https://nodejs.org/

# Install Docker
# Download from: https://www.docker.com/get-started

# Install development tools
cargo install just bacon
```

### 2. Run Setup

```bash
./scripts/Core_Development/setup.sh
```

This will install all dependencies and build the project.

### 3. Start Development

```bash
./scripts/Core_Development/dev.sh
```

That's it! Backend and frontend will start with hot reload.

---

## 📦 Stack Overview

### Backend (Rust)

- **gRPC Server**: Tonic (port 50051)
- **HTTP Server**: Axum (port 8080)
- **Database**: PostgreSQL (port 5432)
- **ORM**: SQLx
- **Runtime**: Tokio
- **Monitoring**: Prometheus metrics

### Frontend (TypeScript)

- **Framework**: React 18
- **Build Tool**: Vite (HMR enabled)
- **Language**: TypeScript
- **gRPC Client**: Connect
- **Testing**: Vitest + Playwright

---

## 🛠️ Development Workflow

### Option 1: Using dev.sh (Recommended)

### Option 1: Using dev.sh (Recommended)

```bash
# Full environment
./scripts/Core_Development/dev.sh

# Individual services
./scripts/Core_Development/dev.sh db           # Database only
./scripts/Core_Development/dev.sh backend      # Backend with hot reload
./scripts/Core_Development/dev.sh frontend     # Frontend with HMR
./scripts/Core_Development/dev.sh test         # Run all tests
```

### Option 2: Using Just Commands

```bash
# Full environment
just dev

# Database operations
just dev-db           # Start database
just db-migrate       # Run migrations
just db-reset         # Reset database (destructive!)
just db-shell         # Open PostgreSQL shell

# Development
just dev-backend      # Backend with bacon (hot reload)
just dev-frontend     # Frontend with Vite (HMR)
just dev-test         # Tests in watch mode

# Building
just build            # Build everything
just build-backend    # Build Rust
just build-frontend   # Build React

# Testing
just test             # All tests
just test-backend     # Rust tests
just test-frontend    # Frontend tests
just test-e2e         # E2E tests

# Code Quality
just check            # Lint + Format
just lint             # Lint code
just format           # Format code
just typecheck        # Type check

# Utilities
just logs             # View logs
just status           # System status
just clean            # Clean build artifacts
just clean-all        # Clean everything (destructive!)
```

---

## 🔄 Hot Reload & Fast Iteration

### Backend (Rust)

**Bacon** provides continuous compilation with minimal feedback loop:

```bash
# Start backend with hot reload
cd crates/grpc
bacon run
```

**Bacon Commands:**

- `:q` - Quit
- `:j job-name` - Switch job (run, test, clippy, etc.)
- `Ctrl+C` - Stop

**How it works:**

1. Edit Rust code
2. Bacon detects changes automatically
3. Incremental compilation starts
4. Binary restarts on success
5. **Total time: ~2-5 seconds**

### Frontend (React + Vite)

**Vite HMR** (Hot Module Replacement) for instant updates:

```bash
# Start frontend
cd web
npm run dev
```

**How it works:**

1. Edit React/TypeScript code
2. Vite updates affected modules only
3. Browser reflects changes instantly
4. **Total time: <1 second**

---

## 🗄️ Database Management

### Start Database

```bash
# Using dev.sh
./dev.sh db

# Using just
just dev-db

# Using docker-compose
docker-compose -f docker-compose.dev.yml up -d postgres
```

**Connection Details:**

- Host: `localhost`
- Port: `5432`
- User: `postgres`
- Password: `postgres`
- Database: `hodei`

### Run Migrations

```bash
just db-migrate
```

### Reset Database (Destructive!)

```bash
just db-reset
```

This will:

1. Stop PostgreSQL container
2. Remove volume (loses all data)
3. Start PostgreSQL fresh
4. Run migrations

### Open Database Shell

```bash
just db-shell
```

---

## 🧪 Testing

### Run All Tests

```bash
just test
```

### Backend Tests (Rust)

```bash
# Run all tests
cargo test

# Run tests in watch mode
bacon test

# Run specific test
cargo test test_name

# Run integration tests
cargo test --test integration

# Run with output
cargo test -- --nocapture
```

### Frontend Tests (React + Vitest)

```bash
# Run tests once
cd web && npm test

# Run tests in watch mode
cd web && npm run test:watch

# Run tests with UI
cd web && npm run test:ui

# Run E2E tests
cd web && npm run test:e2e
```

---

## 🐛 Debugging

### VS Code Setup

The project includes complete VS Code configuration:

**.vscode/tasks.json** - Pre-configured tasks

- `Ctrl/Cmd+Shift+P` → "Tasks: Run Task"
- Select from: Start Dev, Run Tests, Build, etc.

**.vscode/launch.json** - Debug configurations

- `F5` to debug gRPC server
- `F5` to debug current test file
- Debug specific tests with custom configurations

**Recommended Extensions:**

- `rust-analyzer` - Rust language support
- `vadimcn.vscode-lldb` - Rust debugging
- `ms-vscode.vscode-typescript-next` - TypeScript support

### Debug Backend (Rust)

1. Set breakpoints in `.rs` files
2. Press `F5`
3. Select "Debug gRPC Server"
4. Debugger will attach

### Debug Frontend (TypeScript)

1. Set breakpoints in `.ts/.tsx` files
2. Open browser DevTools (F12)
3. Sources tab shows your TypeScript files

---

## 📊 Monitoring & Observability

### Prometheus (Metrics)

```bash
# Start Prometheus
docker-compose -f docker-compose.dev.yml --profile monitoring up -d prometheus

# Access UI
open http://localhost:9090
```

### Grafana (Dashboards)

```bash
# Start Grafana
docker-compose -f docker-compose.dev.yml --profile monitoring up -d grafana

# Access UI
open http://localhost:3000

# Login
Username: admin
Password: admin
```

### pgAdmin (Database Admin)

```bash
# Start pgAdmin
docker-compose -f docker-compose.dev.yml --profile admin up -d pgadmin

# Access UI
open http://localhost:5050

# Login
Email: admin@hodei.local
Password: admin
```

---

## 🔧 Configuration

### Environment Variables

Create `.env` file:

```bash
# Database
HODEI_DATABASE_URL=postgres://postgres:postgres@localhost:5432/hodei
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_DB=hodei

# Development
HODEI_DEV_MODE=1
HODEI_DOCKER_ENABLED=1

# Ports
GRPC_PORT=50051
REST_PORT=8080
WEB_PORT=3000

# Logging
RUST_LOG=debug,hodei=trace,sqlx=warn
```

### Cargo Configuration

**.cargo/config.toml** (optional for faster builds):

```toml
[build]
incremental = true

[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

---

## 📁 Project Structure

```
hodei-job-platform/
├── crates/
│   ├── domain/          # Domain logic
│   ├── application/     # Use cases
│   ├── infrastructure/  # External concerns (DB, APIs)
│   ├── interface/       # API definitions
│   ├── grpc/            # gRPC server & CLI
│   └── cli/             # CLI commands
├── web/                 # React frontend
│   ├── src/
│   ├── dist/
│   └── package.json
├── proto/               # Protocol Buffer definitions
├── scripts/             # Build & deployment scripts
│   ├── dev.sh
│   ├── setup.sh
│   └── watch_logs.sh
├── docker-compose.dev.yml
├── docker-compose.prod.yml
├── justfile
├── bacon.toml
└── DEVELOPMENT.md (this file)
```

---

## 🚦 Common Workflows

### Making a Backend Change

1. Edit code in `crates/*/src/`
2. Bacon auto-compiles and restarts
3. Test with: `just test-backend`
4. Check logs: `just logs`

### Making a Frontend Change

1. Edit code in `web/src/`
2. Browser updates automatically (HMR)
3. Test with: `cd web && npm test`
4. Run E2E: `just test-e2e`

### Adding a Database Migration

1. Create migration file in `crates/infrastructure/migrations/`
2. Run: `just db-migrate`
3. Verify: `just db-shell` → `\dt`

### Running E2E Tests

#### Frontend E2E Tests (Playwright)

```bash
just test-e2e
```

This uses Playwright to test the full stack including:

- Database interactions
- gRPC calls
- Frontend UI

#### Backend E2E Tests (Rust)

```bash
# Run all backend E2E tests
cargo test --test e2e_job_flow_test -- --ignored --nocapture

# Run specific Maven complex build test
cargo test test_e2e_maven_complex_build -- --ignored --nocapture
```

The backend E2E tests include:

- **Maven Complex Build Test**: Tests a complete CI/CD pipeline with:
  - ASDF version management setup
  - Java 21 installation
  - Maven 3.9.9 installation  
  - Git repository cloning
  - Maven build with enforcer plugin validation
  
- **Fast Execution**: Uses `TestWorkerProvider` (35x faster than Docker)
  - Direct process spawning instead of containerization
  - Execution time: ~6 seconds (vs 2-3 minutes with Docker)
  
- **Comprehensive Validation**: Verifies all pipeline stages in real-time

---

## 🏗️ Build & Release

### Development Build

```bash
just build
```

### Production Build

```bash
just prod-build
```

### Docker Production

```bash
just prod-up    # Start production stack
just prod-down  # Stop production stack
```

---

## 🐛 Troubleshooting

### Port Already in Use

```bash
# Kill process on port 50051
lsof -ti:50051 | xargs kill -9

# Kill process on port 3000
lsof -ti:3000 | xargs kill -9
```

### Database Connection Issues

```bash
# Reset database
just db-reset

# Check PostgreSQL logs
just logs-db
```

### Build Failures

```bash
# Clean everything
just clean-all

# Rebuild from scratch
./scripts/Core_Development/setup.sh
```

### Rust Compilation Errors

```bash
# Update dependencies
cargo update

# Clean build artifacts
cargo clean

# Check with rust-analyzer
# (VS Code should show errors inline)
```

---

## 📚 Additional Resources

### Documentation

- **README.md** - Project overview
- **GETTING_STARTED.md** - Detailed setup guide
- **docs/** - Full documentation

### Tools

- **Bacon** - https://dystroy.org/bacon/
- **Just** - https://github.com/casey/just
- **Vite** - https://vitejs.dev/
- **SQLx** - https://github.com/launchbadge/sqlx
- **Tonic** - https://github.com/hyperium/tonic

### Learning

- **Rust Book** - https://doc.rust-lang.org/book/
- **Tokio Tutorial** - https://tokio.rs/tokio/tutorial
- **React Docs** - https://react.dev/
- **TypeScript Handbook** - https://www.typescriptlang.org/docs/

---

## 🎯 Performance Tips

1. **Use Bacon**: Faster than cargo-watch, designed for feedback loops
2. **Incremental Compilation**: Enabled by default in Rust
3. **Parallel Tests**: Run `just test` to test both backend and frontend
4. **Docker Volumes**: Use named volumes for faster I/O
5. **Browser Cache**: Vite HMR keeps browser cache warm
6. **Database Indexing**: Ensure migrations include proper indexes

---

## 🤝 Contributing

1. Create feature branch: `git checkout -b feature/my-feature`
2. Make changes with tests
3. Run: `just check`
4. Commit: `git commit -m "feat: add my feature"`
5. Push: `git push origin feature/my-feature`
6. Open PR

---

**Happy Coding! 🚀**

For questions or issues, check the documentation or open an issue.

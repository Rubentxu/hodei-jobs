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
./scripts/setup.sh
```

This will install all dependencies and build the project.

### 3. Start Development

```bash
./scripts/dev.sh
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
./scripts/dev.sh

# Individual services
./scripts/dev.sh db           # Database only
./scripts/dev.sh backend      # Backend with hot reload
./scripts/dev.sh frontend     # Frontend with HMR
./scripts/dev.sh test         # Run all tests
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
./scripts/setup.sh
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

# Database Migrations - Hodei Jobs Platform

This document describes the centralized migration system for Hodei Jobs Platform.

## Overview

The migration system provides:
- **Centralized SQL migrations** in `crates/server/infrastructure/migrations/`
- **Central MigrationService** for automatic execution on startup
- **sqlx-cli integration** for manual control
- **Rollback support** via `down.sql` files
- **Schema validation** and history tracking

## Directory Structure

```
crates/server/infrastructure/
├── migrations/                          # SQL migration files
│   ├── 20241221000000_init_schema/
│   │   ├── up.sql                       # Create tables (workers, jobs, job_queue, audit_logs, provider_configs)
│   │   └── down.sql                     # Rollback (drop tables)
│   ├── 20241223150400_outbox_pattern/
│   │   ├── up.sql                       # Transactional outbox
│   │   └── down.sql
│   ├── 20251228120000_outbox_dlq/
│   │   ├── up.sql                       # Dead letter queue
│   │   └── down.sql
│   ├── 20251230120000_saga_pattern/
│   │   ├── up.sql                       # Saga tables (sagas, saga_steps, saga_audit_events)
│   │   └── down.sql
│   └── 20251230000001_consolidate_log_files/
│       ├── up.sql                       # job_log_files table
│       └── down.sql
└── src/persistence/postgres/
    └── migrations.rs                    # Central MigrationService
```

## Migration Naming Convention

Format: `YYYYMMDDHHMMSS_description/`
- `up.sql`: Migration forward (create tables, add columns, etc.)
- `down.sql`: Rollback (reverse the up.sql changes)

## Using sqlx-cli

### Installation

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

### Configuration

Set the DATABASE_URL environment variable:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
```

### Commands

```bash
# Run all pending migrations
sqlx migrate run

# Show migration history
sqlx migrate list

# Revert last migration
sqlx migrate revert

# Revert to specific version
sqlx migrate revert --version 20241221000000

# Verify migrations
sqlx migrate verify
```

## Using MigrationService (Rust)

### Automatic on Startup

The server automatically runs migrations on startup:

```rust
use hodei_server_infrastructure::persistence::postgres::run_migrations;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    
    // Migrations run automatically
    let validation = run_migrations(&pool).await?;
    
    println!("{}", validation.summary());
    
    Ok(())
}
```

### Manual Control

```rust
use hodei_server_infrastructure::persistence::postgres::{
    MigrationService, MigrationConfig,
};

let config = MigrationConfig::new("crates/server/infrastructure/migrations");
let service = MigrationService::new(pool, config);

// Run all migrations
let results = service.run_all().await?;

// Get history
let history = service.history().await?;

// Validate schema
let validation = service.validate().await?;
println!("{}", validation.summary());

// Rollback last migration
service.rollback().await?;
```

## Schema Validation

The `SchemaValidationResult` provides:

```rust
pub struct SchemaValidationResult {
    pub applied_migrations: usize,      // How many migrations have run
    pub expected_migrations: usize,      // How many migration files exist
    pub pending_migrations: usize,       // How many are waiting to run
    pub failed_migrations: usize,        // How many failed
    pub last_migration: Option<MigrationResult>,
    pub is_valid: bool,                  // All migrations applied successfully
}
```

## Idempotency

All migrations use `IF NOT EXISTS` and `IF EXISTS` to be safe to run multiple times:

```sql
CREATE TABLE IF NOT EXISTS workers (...);
CREATE INDEX IF NOT EXISTS idx_workers_state ON workers(state);
```

## Rollback Procedures

### Using sqlx-cli

```bash
# Revert last migration
sqlx migrate revert

# Check what will be reverted
sqlx migrate list --revert
```

### Using Rust API

```rust
// Rollback last migration
service.rollback().await?;

// Rollback to specific version
service.rollback_to_version(20241221000000).await?;
```

### Emergency Reset (DANGEROUS)

```bash
# WARNING: This drops all tables!
sqlx migrate reset

# Then re-run migrations
sqlx migrate run
```

## Adding a New Migration

### Step 1: Create migration directory

```bash
mkdir -p crates/server/infrastructure/migrations/$(date +%Y%m%d%H%M%S)_my_feature
```

### Step 2: Create up.sql

```sql
-- Description of what this migration does
-- Version: YYYYMMDDHHMMSS

CREATE TABLE IF NOT EXISTS my_new_table (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_my_new_table_name ON my_new_table(name);
```

### Step 3: Create down.sql (optional but recommended)

```sql
-- Rollback for my_feature migration

DROP TABLE IF EXISTS my_new_table CASCADE;
```

### Step 4: Run migrations

```bash
sqlx migrate run
```

## Troubleshooting

### Migration Fails

1. Check the error message
2. Verify database connection with `DATABASE_URL`
3. Check for syntax errors in SQL
4. Ensure dependent tables exist

```bash
# Test connection
sqlx database check

# List current migrations
sqlx migrate list
```

### Duplicate Migration Error

If you see `duplicate key value violates unique constraint "__sqlx_migrations_version_key"`:

```sql
-- Delete the duplicate entry
DELETE FROM __sqlx_migrations WHERE version = <version>;
```

### Missing Migration Files

If migrations directory is not found:

```bash
# Verify directory exists
ls crates/server/infrastructure/migrations/

# Check current working directory
pwd
```

## Best Practices

1. **Always create down.sql** for reversible changes
2. **Test migrations** in a development environment first
3. **Backup production** before running migrations
4. **Use descriptive names** in migration filenames
5. **Keep migrations small** and focused on single changes
6. **Never modify** an already-applied migration (create a new one instead)
7. **Use transactions** within migrations when possible:

```sql
BEGIN;

-- Your changes

COMMIT;
```

## Migration Checklist

Before deploying:

- [ ] All migrations tested in dev environment
- [ ] `down.sql` files created for reversible changes
- [ ] No syntax errors in SQL
- [ ] Foreign key constraints verified
- [ ] Indexes created for frequently queried columns
- [ ] Rollback procedure documented
- [ ] Team notified of schema changes

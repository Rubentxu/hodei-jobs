#!/usr/bin/env bash
set -e

echo "🗄️  Starting PostgreSQL database..."

if ! docker info >/dev/null 2>&1; then
    echo "❌ Docker daemon is not running"
    exit 1
fi

CONTAINER_ID=$(docker ps -q -f name=hodei-jobs-postgres 2>/dev/null || true)
if [ -n "$CONTAINER_ID" ]; then
    echo "✅ PostgreSQL container already running (ID: $CONTAINER_ID)"
else
    docker compose -f docker-compose.dev.yml up -d postgres 2>&1 || \
    (docker rm -f hodei-jobs-postgres 2>/dev/null || true; docker compose -f docker-compose.dev.yml up -d postgres)
fi

sleep 2
echo "✅ Database ready"

# Run all migrations
echo ""
echo "📦 Running database migrations..."

# Main schema
echo "  → Core schema..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < crates/server/infrastructure/migrations/20241221000000_init_schema/up.sql 2>&1 | grep -v "already exists" || true

# Job log files
echo "  → Job log files..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < crates/server/infrastructure/migrations/20251223100453_add_job_log_files_table/up.sql 2>&1 | grep -v "already exists" || true

# Outbox events
echo "  → Outbox events..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < crates/server/infrastructure/migrations/20241223_add_outbox_events.sql 2>&1 | grep -v "already exists" || true

# Saga tables
echo "  → Saga tables..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < migrations/20251230_add_saga_tables.sql 2>&1 | grep -v "already exists" || true

# Reactive system tables
echo "  → Reactive system..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < migrations/20260101_add_reactive_system_tables.sql 2>&1 | grep -v "already exists" || true

# Fix job_queue schema (ensure enqueued_at column exists)
echo "  → Fix job_queue schema..."
docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs < migrations/20260109_fix_job_queue_schema.sql 2>&1 | grep -v "already exists" || true

echo ""
echo "✅ All migrations applied"

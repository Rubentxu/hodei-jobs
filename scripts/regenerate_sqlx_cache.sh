#!/bin/bash
# =============================================================================
# Regenerate sqlx query cache
# =============================================================================
# This script must be run outside the sandbox (with network access to PostgreSQL)
# Usage: ./scripts/regenerate_sqlx_cache.sh
# =============================================================================

set -e

# Default DATABASE_URL - adjust if needed
DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/hodei_jobs}"

echo "Regenerating sqlx query cache..."
echo "Using DATABASE_URL: $DATABASE_URL"
echo ""

# Clean old cache
rm -rf .sqlx

# Run cargo sqlx prepare to generate new cache
cargo sqlx prepare --database-url "$DATABASE_URL"

echo ""
echo "Sqlx cache regenerated successfully!"
echo "You can now build in offline mode with: SQLX_OFFLINE=true cargo build"

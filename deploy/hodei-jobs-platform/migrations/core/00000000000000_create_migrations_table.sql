-- Migration tracking table for Hodei Jobs Platform
-- This table tracks which migrations have been applied

CREATE TABLE IF NOT EXISTS __sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description VARCHAR(255),
    installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    execution_time BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    checksum VARCHAR(255)
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_sqlx_migrations_installed_on ON __sqlx_migrations(installed_on);

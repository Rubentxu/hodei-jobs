-- Rollback: Init Schema
-- Version: 20241221000000
-- Purpose: Remove all core domain tables (reverse of init_schema)

-- Drop tables in reverse order of dependencies
DROP TABLE IF EXISTS job_queue CASCADE;
DROP TABLE IF EXISTS audit_logs CASCADE;
DROP TABLE IF EXISTS provider_configs CASCADE;
DROP TABLE IF EXISTS jobs CASCADE;
DROP TABLE IF EXISTS workers CASCADE;

-- Drop extension (optional, may be used by other tables)
-- DROP EXTENSION IF EXISTS pgcrypto;

-- Migration: Consolidate Log Files - Unified job log storage references
-- Version: 20251230000001
-- Purpose: Consolidate job_log_files definitions and add missing columns

-- First, check if table exists and has the correct schema
DO $$
DECLARE
    has_storage_uri BOOLEAN;
    has_job_log_files BOOLEAN;
BEGIN
    -- Check if job_log_files table exists
    SELECT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'job_log_files' AND table_schema = 'public'
    ) INTO has_job_log_files;

    IF has_job_log_files THEN
        -- Check if storage_uri column exists
        SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'job_log_files' AND column_name = 'storage_uri'
        ) INTO has_storage_uri;

        IF NOT has_storage_uri THEN
            -- Schema v1: has job_id, file_path, created_at, expires_at
            -- Need to migrate to v2
            RAISE NOTICE 'Migrating job_log_files from v1 to v2 schema...';

            -- Rename old table
            ALTER TABLE job_log_files RENAME TO job_log_files_old;

            -- Create new table with correct schema
            CREATE TABLE job_log_files (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                storage_uri TEXT NOT NULL,
                size_bytes BIGINT NOT NULL DEFAULT 0,
                entry_count INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                expires_at TIMESTAMPTZ NOT NULL
            );

            -- Migrate data (map file_path to storage_uri)
            INSERT INTO job_log_files (id, job_id, storage_uri, size_bytes, entry_count, created_at, expires_at)
            SELECT
                gen_random_uuid() AS id,
                job_id,
                file_path AS storage_uri,
                0 AS size_bytes,
                0 AS entry_count,
                created_at,
                expires_at
            FROM job_log_files_old;

            -- Drop old table
            DROP TABLE job_log_files_old;

            RAISE NOTICE 'job_log_files migrated successfully';
        ELSE
            RAISE NOTICE 'job_log_files already has v2 schema';
        END IF;
    ELSE
        -- Table doesn't exist, create with v2 schema
        CREATE TABLE job_log_files (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
            storage_uri TEXT NOT NULL,
            size_bytes BIGINT NOT NULL DEFAULT 0,
            entry_count INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ NOT NULL
        );
        RAISE NOTICE 'job_log_files created with v2 schema';
    END IF;
END $$;

-- Add unique constraint if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_job_log_files_job_id'
    ) THEN
        ALTER TABLE job_log_files ADD CONSTRAINT uq_job_log_files_job_id UNIQUE (job_id);
    END IF;
END $$;

-- Add indexes if not exist
CREATE INDEX IF NOT EXISTS idx_job_log_files_job_id ON job_log_files(job_id);
CREATE INDEX IF NOT EXISTS idx_job_log_files_expires_at ON job_log_files(expires_at);
CREATE INDEX IF NOT EXISTS idx_job_log_files_storage_uri ON job_log_files(storage_uri);

COMMENT ON TABLE job_log_files IS 'References to persistent log files for job output';
COMMENT ON COLUMN job_log_files.storage_uri IS 'URI to log storage (file://, s3://, etc.)';
COMMENT ON COLUMN job_log_files.size_bytes IS 'Size of the log file in bytes';
COMMENT ON COLUMN job_log_files.entry_count IS 'Number of log entries in the file';

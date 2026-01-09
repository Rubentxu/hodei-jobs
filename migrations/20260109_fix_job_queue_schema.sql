-- Migration: Fix job_queue schema - ensure enqueued_at column exists
-- Date: 2026-01-09
-- Issue: Old database instances may have job_queue without enqueued_at column
-- Fix: Add column if missing and recreate indexes

-- Check if enqueued_at column exists, add if missing
DO $$
BEGIN
    -- Add enqueued_at column if it doesn't exist
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'job_queue' AND column_name = 'enqueued_at'
    ) THEN
        ALTER TABLE job_queue ADD COLUMN enqueued_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
        RAISE NOTICE 'Added enqueued_at column to job_queue';
    ELSE
        RAISE NOTICE 'enqueued_at column already exists in job_queue';
    END IF;
END $$;

-- Ensure priority column exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'job_queue' AND column_name = 'priority'
    ) THEN
        ALTER TABLE job_queue ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
        RAISE NOTICE 'Added priority column to job_queue';
    END IF;
END $$;

-- Drop old indexes if they exist (to avoid conflicts)
DROP INDEX IF EXISTS idx_job_queue_priority;
DROP INDEX IF EXISTS idx_job_queue_enqueued_at;
DROP INDEX IF EXISTS uq_job_queue_job_id;

-- Recreate indexes with correct schema
CREATE UNIQUE INDEX uq_job_queue_job_id ON job_queue(job_id);
CREATE INDEX idx_job_queue_priority ON job_queue(priority DESC, enqueued_at ASC);
CREATE INDEX idx_job_queue_enqueued_at ON job_queue(enqueued_at);

-- Ensure foreign key constraint exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_name = 'job_queue' AND constraint_name = 'job_queue_job_id_fkey'
    ) THEN
        -- Clean up orphaned records first
        DELETE FROM job_queue WHERE job_id NOT IN (SELECT id FROM jobs);
        RAISE NOTICE 'Cleaned up orphaned records from job_queue';

        -- Now add the foreign key constraint
        ALTER TABLE job_queue ADD CONSTRAINT job_queue_job_id_fkey
        FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT;
        RAISE NOTICE 'Added foreign key constraint to job_queue';
    END IF;
END $$;

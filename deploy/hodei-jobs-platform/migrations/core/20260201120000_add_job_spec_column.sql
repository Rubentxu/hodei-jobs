-- =============================================================================
-- Migration: Add job_spec column to jobs table
-- Purpose: Add JSONB column to store complete job specification
-- =============================================================================

-- Add spec column to jobs table if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'jobs'
        AND column_name = 'spec'
    ) THEN
        ALTER TABLE jobs ADD COLUMN spec JSONB NOT NULL DEFAULT '{}'::jsonb;
        RAISE NOTICE 'Added spec column to jobs table';
    ELSE
        RAISE NOTICE 'spec column already exists in jobs table';
    END IF;
END $$;

-- Create index on spec for faster queries
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'jobs'
        AND indexname = 'idx_jobs_spec'
    ) THEN
        CREATE INDEX idx_jobs_spec ON jobs USING GIN (spec jsonb_path_ops);
        RAISE NOTICE 'Created index idx_jobs_spec on jobs.spec';
    ELSE
        RAISE NOTICE 'Index idx_jobs_spec already exists';
    END IF;
END $$;

-- =============================================================================
-- Migration: Update jobs table to populate spec from existing columns
-- Purpose: Backfill spec column from individual columns for backward compatibility
-- =============================================================================

-- Backfill spec from existing columns (only if spec is empty)
UPDATE jobs
SET spec = (
    SELECT jsonb_build_object(
        'command', COALESCE(command, ''),
        'arguments', COALESCE(arguments, '[]'::jsonb),
        'environment', COALESCE(environment, '{}'::jsonb),
        'resources', COALESCE(resources, '{"cpu": 1, "memory": 134217728}'::jsonb),
        'timeout_ms', COALESCE(timeout_seconds * 1000, 3600000),
        'image', NULL,
        'preferences', jsonb_build_object(
            'required_labels', '{}'::jsonb,
            'required_annotations', '{}'::jsonb,
            'preferred_provider', provider,
            'preferred_region', NULL,
            'priority', COALESCE(priority, 0),
            'allow_retry', COALESCE(max_retries, 3) > 0,
            'max_retries', COALESCE(max_retries, 3)
        ),
        'metadata', COALESCE(metadata, '{}'::jsonb)
    )
)
WHERE spec = '{}'::jsonb OR spec IS NULL;

-- =============================================================================
-- Migration: Add spec column to job_queue table if needed
-- Purpose: Ensure job_queue also has spec column for consistency
-- =============================================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'job_queue'
        AND column_name = 'spec'
    ) THEN
        ALTER TABLE job_queue ADD COLUMN spec JSONB;
        RAISE NOTICE 'Added spec column to job_queue table';
    ELSE
        RAISE NOTICE 'spec column already exists in job_queue table';
    END IF;
END $$;

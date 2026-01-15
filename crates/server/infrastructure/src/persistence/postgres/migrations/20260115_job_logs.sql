-- Migration: Create job_logs table for log aggregation and cleanup (EPIC-85 US-09)
-- Created: 2026-01-15
-- Purpose: Store job execution logs for retrieval and cleanup

-- Create job_logs table if not exists
CREATE TABLE IF NOT EXISTS job_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    level VARCHAR(20) NOT NULL DEFAULT 'INFO',
    message TEXT NOT NULL,
    source VARCHAR(50) NOT NULL DEFAULT 'stdout',
    line_number INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Index for efficient log retrieval by job
    CONSTRAINT fk_job FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

-- Create indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_job_logs_job_id ON job_logs(job_id);
CREATE INDEX IF NOT EXISTS idx_job_logs_timestamp ON job_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_job_logs_job_timestamp ON job_logs(job_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_job_logs_level ON job_logs(level);

-- Function to cleanup old logs (US-09)
-- Returns the number of deleted log entries
CREATE OR REPLACE FUNCTION cleanup_job_logs_older_than(cutoff_timestamp TIMESTAMPTZ)
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM job_logs WHERE timestamp < cutoff_timestamp;
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to get log count by job
CREATE OR REPLACE FUNCTION count_job_logs_by_job(p_job_id UUID)
RETURNS INTEGER AS $$
DECLARE
    log_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO log_count FROM job_logs WHERE job_id = p_job_id;
    RETURN log_count;
END;
$$ LANGUAGE plpgsql;

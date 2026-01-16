-- =============================================================================
-- Migration: Create jobs table
-- Purpose: Create the main jobs table for job execution tracking
-- Version: 20260202
-- =============================================================================

-- Create jobs table
CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    spec JSONB NOT NULL DEFAULT '{}'::jsonb,
    state VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    provider_id UUID,
    selected_provider_id UUID,
    worker_id UUID,
    execution_context JSONB,
    result JSONB,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    scheduled_at TIMESTAMP WITH TIME ZONE,
    enqueued_at TIMESTAMP WITH TIME ZONE,
    correlation_id VARCHAR(255),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- Create indexes for jobs table
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_provider_id ON jobs(provider_id);
CREATE INDEX IF NOT EXISTS idx_jobs_selected_provider_id ON jobs(selected_provider_id);
CREATE INDEX IF NOT EXISTS idx_jobs_worker_id ON jobs(worker_id);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_priority ON jobs(priority DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_scheduled_at ON jobs(scheduled_at) WHERE state = 'SCHEDULED';

-- Comments
COMMENT ON TABLE jobs IS 'Main table for tracking job executions in Hodei Jobs Platform';
COMMENT ON COLUMN jobs.spec IS 'Complete job specification as JSONB';
COMMENT ON COLUMN jobs.state IS 'Current job state (PENDING, ASSIGNED, SCHEDULED, RUNNING, SUCCEEDED, FAILED, CANCELLED, TIMEOUT)';
COMMENT ON COLUMN jobs.execution_context IS 'Runtime context including worker assignment and provisioning details';
COMMENT ON COLUMN jobs.result IS 'Job execution result including output and metrics';
COMMENT ON COLUMN jobs.metadata IS 'Additional metadata for job tracking';

-- =============================================================================
-- Migration: Create job_queue table
-- Purpose: Table for pending jobs waiting to be dispatched
-- Version: 20260202
-- =============================================================================

CREATE TABLE IF NOT EXISTS job_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL UNIQUE,
    spec JSONB,
    priority INTEGER NOT NULL DEFAULT 0,
    state VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    provider_id UUID,
    claim_id UUID,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMP WITH TIME ZONE,
    enqueued_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- Create indexes for job_queue
CREATE INDEX IF NOT EXISTS idx_job_queue_state ON job_queue(state);
CREATE INDEX IF NOT EXISTS idx_job_queue_priority ON job_queue(priority DESC, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_job_queue_claim_id ON job_queue(claim_id) WHERE claim_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_job_queue_next_retry ON job_queue(next_retry_at) WHERE next_retry_at IS NOT NULL;

COMMENT ON TABLE job_queue IS 'Queue table for pending jobs waiting to be dispatched to workers';
COMMENT ON COLUMN job_queue.claim_id IS 'Unique claim ID for distributed job claiming';
COMMENT ON COLUMN job_queue.attempts IS 'Number of dispatch attempts made';

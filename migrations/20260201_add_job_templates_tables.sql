-- Migration: Add Job Templates, Executions, and Scheduling Tables
-- Created: 2026-02-01
-- Description: Adds tables for EPIC-34: Job Templates and Cron Scheduling

-- =====================================================
-- JOB TEMPLATES TABLE (already exists, but ensure it has all needed columns)
-- =====================================================

-- Create job_template_parameters table
CREATE TABLE IF NOT EXISTS job_template_parameters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES job_templates(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    parameter_type VARCHAR(50) NOT NULL CHECK (parameter_type IN ('string', 'number', 'boolean', 'choice', 'secret')),
    description TEXT,
    required BOOLEAN NOT NULL DEFAULT false,
    default_value TEXT,
    validation_pattern TEXT,
    min_value DOUBLE PRECISION,
    max_value DOUBLE PRECISION,
    choices TEXT[], -- Array of allowed choices
    display_order INTEGER NOT NULL DEFAULT 0,
    secret BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(template_id, name)
);

-- Create indexes for job_template_parameters
CREATE INDEX IF NOT EXISTS idx_job_template_parameters_template_id ON job_template_parameters(template_id);
CREATE INDEX IF NOT EXISTS idx_job_template_parameters_display_order ON job_template_parameters(template_id, display_order);

-- =====================================================
-- JOB EXECUTIONS TABLE
-- =====================================================

CREATE TABLE IF NOT EXISTS job_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    execution_number BIGINT NOT NULL,
    template_id UUID NOT NULL REFERENCES job_templates(id) ON DELETE CASCADE,
    template_version INTEGER NOT NULL,
    job_id UUID REFERENCES jobs(id) ON DELETE SET NULL,
    job_name VARCHAR(255) NOT NULL,
    job_spec JSONB NOT NULL,
    state VARCHAR(50) NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'error')),
    result JSONB, -- ExecutionResult as JSON
    queued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    triggered_by VARCHAR(50) NOT NULL CHECK (triggered_by IN ('manual', 'scheduled', 'api', 'webhook', 'retry')),
    scheduled_job_id UUID,
    triggered_by_user VARCHAR(255),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    resource_usage JSONB, -- ResourceUsageSnapshot as JSON
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(template_id, execution_number)
);

-- Create indexes for job_executions
CREATE INDEX IF NOT EXISTS idx_job_executions_template_id ON job_executions(template_id);
CREATE INDEX IF NOT EXISTS idx_job_executions_job_id ON job_executions(job_id);
CREATE INDEX IF NOT EXISTS idx_job_executions_state ON job_executions(state);
CREATE INDEX IF NOT EXISTS idx_job_executions_queued_at ON job_executions(queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_executions_triggered_by ON job_executions(triggered_by);
CREATE INDEX IF NOT EXISTS idx_job_executions_scheduled_job_id ON job_executions(scheduled_job_id);

-- =====================================================
-- SCHEDULED JOBS TABLE
-- =====================================================

CREATE TABLE IF NOT EXISTS scheduled_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    template_id UUID NOT NULL REFERENCES job_templates(id) ON DELETE CASCADE,
    cron_expression VARCHAR(255) NOT NULL,
    timezone VARCHAR(100) NOT NULL DEFAULT 'UTC',
    next_execution_at TIMESTAMPTZ NOT NULL,
    last_execution_at TIMESTAMPTZ,
    last_execution_status VARCHAR(50) CHECK (last_execution_status IN ('queued', 'running', 'succeeded', 'failed', 'error')),
    enabled BOOLEAN NOT NULL DEFAULT true,
    max_consecutive_failures INTEGER NOT NULL DEFAULT 3,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    pause_on_failure BOOLEAN NOT NULL DEFAULT false,
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),
    UNIQUE(name)
);

-- Create indexes for scheduled_jobs
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_template_id ON scheduled_jobs(template_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_enabled ON scheduled_jobs(enabled);
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_next_execution ON scheduled_jobs(next_execution_at);
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_last_execution ON scheduled_jobs(last_execution_at DESC);

-- =====================================================
-- TRIGGERS FOR UPDATED_AT
-- =====================================================

-- Function to update updated_at column
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create triggers for updated_at
CREATE TRIGGER update_job_template_parameters_updated_at
    BEFORE UPDATE ON job_template_parameters
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_scheduled_jobs_updated_at
    BEFORE UPDATE ON scheduled_jobs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- =====================================================
-- COMMENTS FOR DOCUMENTATION
-- =====================================================

COMMENT ON TABLE job_template_parameters IS 'Parameters for job templates with validation rules';
COMMENT ON COLUMN job_template_parameters.parameter_type IS 'Type of parameter: string, number, boolean, choice, or secret';
COMMENT ON COLUMN job_template_parameters.validation_pattern IS 'Regex pattern for string validation';
COMMENT ON COLUMN job_template_parameters.choices IS 'Array of allowed values for choice parameters';
COMMENT ON COLUMN job_template_parameters.secret IS 'Whether this parameter contains sensitive data';

COMMENT ON TABLE job_executions IS 'Execution instances of job templates';
COMMENT ON COLUMN job_executions.execution_number IS 'Sequential number for this template (1, 2, 3, ...)';
COMMENT ON COLUMN job_executions.job_spec IS 'Complete job specification with parameter substitutions';
COMMENT ON COLUMN job_executions.result IS 'ExecutionResult as JSON: exit_code error_output, duration, output_summary,_ms';
COMMENT ON COLUMN job_executions.parameters IS 'Parameters used for this execution';
COMMENT ON COLUMN job_executions.resource_usage IS 'ResourceUsageSnapshot as JSON';

COMMENT ON TABLE scheduled_jobs IS 'Cron-scheduled job executions';
COMMENT ON COLUMN scheduled_jobs.cron_expression IS 'Cron expression for scheduling (e.g., "0 0 * * *")';
COMMENT ON COLUMN scheduled_jobs.timezone IS 'Timezone for cron expression (e.g., "America/New_York")';
COMMENT ON COLUMN scheduled_jobs.consecutive_failures IS 'Current count of consecutive failures';
COMMENT ON COLUMN scheduled_jobs.pause_on_failure IS 'Whether to automatically pause after max consecutive failures';

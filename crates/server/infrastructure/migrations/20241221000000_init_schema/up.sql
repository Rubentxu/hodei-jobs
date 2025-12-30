-- Migration: Init Schema - Core tables for Hodei Jobs Platform
-- Version: 20241221000000
-- Purpose: Create core domain tables (workers, jobs, job_queue, audit_logs)

-- Extension for UUID generation
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================
-- Table: workers
-- Registry of provisioned workers across all providers
-- ============================================
CREATE TABLE IF NOT EXISTS workers (
    id UUID PRIMARY KEY,
    provider_id UUID NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('docker', 'kubernetes', 'firecracker')),
    provider_resource_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('REGISTERED', 'BUSY', 'READY', 'TERMINATED', 'FAILED')),
    spec JSONB NOT NULL,
    handle JSONB NOT NULL,
    current_job_id UUID,
    last_heartbeat TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for workers
CREATE INDEX IF NOT EXISTS idx_workers_provider_id ON workers(provider_id);
CREATE INDEX IF NOT EXISTS idx_workers_state ON workers(state);
CREATE INDEX IF NOT EXISTS idx_workers_current_job ON workers(current_job_id) WHERE current_job_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workers_last_heartbeat ON workers(last_heartbeat) WHERE last_heartbeat IS NOT NULL;

-- ============================================
-- Table: jobs
-- Job definitions and execution state
-- ============================================
CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY,
    spec JSONB NOT NULL,
    state VARCHAR(50) NOT NULL CHECK (state IN ('CREATED', 'PENDING', 'ASSIGNED', 'RUNNING', 'SUCCEEDED', 'FAILED', 'CANCELLED')),
    selected_provider_id UUID,
    execution_context JSONB,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMPTZ NOT NULL,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    result JSONB,
    error_message TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'
);

-- Indexes for jobs
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_selected_provider ON jobs(selected_provider_id);
CREATE INDEX IF NOT EXISTS idx_jobs_assigned_provider ON jobs(selected_provider_id, state) WHERE state = 'ASSIGNED';

-- ============================================
-- Table: job_queue
-- Pending jobs waiting for worker assignment
-- ============================================
CREATE TABLE IF NOT EXISTS job_queue (
    id BIGSERIAL PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    priority INTEGER NOT NULL DEFAULT 0
);

-- Constraints and indexes for job_queue
CREATE UNIQUE INDEX IF NOT EXISTS uq_job_queue_job_id ON job_queue(job_id);
CREATE INDEX IF NOT EXISTS idx_job_queue_priority ON job_queue(priority DESC, enqueued_at ASC);
CREATE INDEX IF NOT EXISTS idx_job_queue_enqueued_at ON job_queue(enqueued_at);

-- ============================================
-- Table: audit_logs
-- Complete audit trail of all domain actions
-- ============================================
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY,
    correlation_id VARCHAR(255),
    event_type VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    actor VARCHAR(255)
);

-- Indexes for audit_logs
CREATE INDEX IF NOT EXISTS idx_audit_correlation_id ON audit_logs(correlation_id) WHERE correlation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_occurred_at ON audit_logs(occurred_at);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs(actor) WHERE actor IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_event_occurred ON audit_logs(event_type, occurred_at);

-- ============================================
-- Table: provider_configs
-- Provider configuration and status
-- ============================================
CREATE TABLE IF NOT EXISTS provider_configs (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    provider_type VARCHAR(50) NOT NULL CHECK (provider_type IN ('docker', 'kubernetes', 'firecracker')),
    config JSONB NOT NULL DEFAULT '{}',
    status VARCHAR(50) NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'INACTIVE', 'MAINTENANCE', 'ERROR')),
    priority INTEGER NOT NULL DEFAULT 0,
    max_workers INTEGER NOT NULL DEFAULT 10,
    tags JSONB NOT NULL DEFAULT '[]',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for provider_configs
CREATE INDEX IF NOT EXISTS idx_provider_configs_name ON provider_configs(name);
CREATE INDEX IF NOT EXISTS idx_provider_configs_type ON provider_configs(provider_type);
CREATE INDEX IF NOT EXISTS idx_provider_configs_status ON provider_configs(status);

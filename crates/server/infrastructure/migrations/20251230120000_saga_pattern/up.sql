-- Migration: Saga Pattern - Long-running transaction support
-- Version: 20251230120000
-- Purpose: Support for Provisioning, Execution, and Recovery sagas

-- ============================================
-- Table: sagas
-- Main saga instances table
-- ============================================
CREATE TABLE IF NOT EXISTS sagas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_type VARCHAR(20) NOT NULL CHECK (saga_type IN ('PROVISIONING', 'EXECUTION', 'RECOVERY')),
    state VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (state IN ('PENDING', 'IN_PROGRESS', 'COMPENSATING', 'COMPLETED', 'FAILED', 'CANCELLED')),
    correlation_id VARCHAR(255),
    actor VARCHAR(255),
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error_message TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for sagas
CREATE INDEX IF NOT EXISTS idx_sagas_id ON sagas(id);
CREATE INDEX IF NOT EXISTS idx_sagas_type ON sagas(saga_type);
CREATE INDEX IF NOT EXISTS idx_sagas_state ON sagas(state);
CREATE INDEX IF NOT EXISTS idx_sagas_correlation_id ON sagas(correlation_id) WHERE correlation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sagas_started_at ON sagas(started_at);
CREATE INDEX IF NOT EXISTS idx_sagas_type_state ON sagas(saga_type, state);

-- ============================================
-- Table: saga_steps
-- Individual steps within a saga
-- ============================================
CREATE TABLE IF NOT EXISTS saga_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
    step_name VARCHAR(100) NOT NULL,
    step_order INTEGER NOT NULL,
    state VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (state IN ('PENDING', 'IN_PROGRESS', 'COMPLETED', 'FAILED', 'COMPENSATING', 'COMPENSATED')),
    input_data JSONB,
    output_data JSONB,
    compensation_data JSONB,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for saga_steps
CREATE INDEX IF NOT EXISTS idx_saga_steps_saga_id ON saga_steps(saga_id);
CREATE INDEX IF NOT EXISTS idx_saga_steps_state ON saga_steps(state);
CREATE INDEX IF NOT EXISTS idx_saga_steps_saga_order ON saga_steps(saga_id, step_order);
CREATE INDEX IF NOT EXISTS idx_saga_steps_pending ON saga_steps(state, step_order) WHERE state = 'PENDING';

-- ============================================
-- Table: saga_audit_events
-- Audit trail for saga execution
-- ============================================
CREATE TABLE IF NOT EXISTS saga_audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL,
    step_name VARCHAR(100),
    message TEXT,
    payload JSONB,
    occurred_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for saga_audit_events
CREATE INDEX IF NOT EXISTS idx_saga_audit_saga_id ON saga_audit_events(saga_id);
CREATE INDEX IF NOT EXISTS idx_saga_audit_occurred_at ON saga_audit_events(occurred_at);
CREATE INDEX IF NOT EXISTS idx_saga_audit_type ON saga_audit_events(event_type, occurred_at);

-- Comments
COMMENT ON TABLE sagas IS 'Saga pattern - long-running transaction management';
COMMENT ON TABLE saga_steps IS 'Individual steps in saga execution with compensation support';
COMMENT ON TABLE saga_audit_events IS 'Audit trail for saga execution and debugging';

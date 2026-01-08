-- Migration: Add hodei_commands table for Transactional Command Outbox Pattern
-- Date: 2026-01-08
-- Purpose: Store commands for reliable processing in saga transactions

CREATE TABLE IF NOT EXISTS hodei_commands (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Command identity
    command_type VARCHAR(100) NOT NULL,
    target_type VARCHAR(50) NOT NULL,
    target_id UUID NOT NULL,

    -- Command content
    payload JSONB NOT NULL,
    metadata JSONB,

    -- Idempotency
    idempotency_key VARCHAR(100) UNIQUE,

    -- Audit trail
    created_at TIMESTAMPTZ DEFAULT NOW(),
    processed_at TIMESTAMPTZ,

    -- Processing state
    status VARCHAR(20) DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PROCESSING', 'COMPLETED', 'FAILED', 'CANCELLED')),
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    last_error TEXT,

    -- Saga context (for compensation)
    saga_id UUID,
    saga_type VARCHAR(50),
    step_order INTEGER,

    -- Constraints
    CONSTRAINT fk_saga FOREIGN KEY (saga_id) REFERENCES hodei_sagas(id) ON DELETE SET NULL
);

-- Indexes for hodei_commands
CREATE INDEX IF NOT EXISTS idx_commands_status_created
ON hodei_commands(status, created_at)
WHERE status IN ('PENDING', 'PROCESSING');

CREATE INDEX IF NOT EXISTS idx_commands_target
ON hodei_commands(target_type, target_id);

CREATE INDEX IF NOT EXISTS idx_commands_saga
ON hodei_commands(saga_id, step_order);

CREATE INDEX IF NOT EXISTS idx_commands_idempotency
ON hodei_commands(idempotency_key)
WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_commands_pending_retries
ON hodei_commands(status, retry_count)
WHERE status = 'PENDING' AND retry_count < max_retries;

-- Comments
COMMENT ON TABLE hodei_commands IS 'Transactional Command Outbox Pattern - stores commands for reliable saga-based processing';
COMMENT ON COLUMN hodei_commands.command_type IS 'Fully qualified command type name';
COMMENT ON COLUMN hodei_commands.target_type IS 'Type of aggregate being operated on (Job, Worker, Provider)';
COMMENT ON COLUMN hodei_commands.target_id IS 'ID of the aggregate being operated on';
COMMENT ON COLUMN hodei_commands.idempotency_key IS 'Unique key to prevent duplicate command processing';
COMMENT ON COLUMN hodei_commands.status IS 'PENDING: waiting, PROCESSING: executing, COMPLETED: success, FAILED: exhausted retries, CANCELLED: saga compensation';
COMMENT ON COLUMN hodei_commands.saga_id IS 'Reference to saga orchestrating this command';
COMMENT ON COLUMN hodei_commands.step_order IS 'Order of this command within the saga';

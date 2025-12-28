-- Migration: Add Dead Letter Queue table for outbox events
-- Date: 2025-12-28
-- Purpose: Store failed events that exceed retry limit for manual investigation

CREATE TABLE IF NOT EXISTS outbox_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Original event reference
    original_event_id UUID NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER')),
    event_type VARCHAR(50) NOT NULL,

    -- Original event content
    payload JSONB NOT NULL,
    metadata JSONB,

    -- Error context
    error_message TEXT NOT NULL,
    retry_count INTEGER NOT NULL,

    -- Audit trail
    original_created_at TIMESTAMPTZ NOT NULL,
    moved_at TIMESTAMPTZ DEFAULT NOW(),

    -- Resolution tracking
    resolved_at TIMESTAMPTZ,
    resolution_notes TEXT,
    resolved_by VARCHAR(100),

    -- Constraints
    UNIQUE(original_event_id)
);

-- Indexes for querying
CREATE INDEX IF NOT EXISTS idx_dlq_status
ON outbox_dlq(moved_at)
WHERE resolved_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_dlq_aggregate
ON outbox_dlq(aggregate_type, aggregate_id);

CREATE INDEX IF NOT EXISTS idx_dlq_event_type
ON outbox_dlq(event_type, moved_at);

-- Comments
COMMENT ON TABLE outbox_dlq IS 'Dead Letter Queue for outbox events that exceed retry limit';
COMMENT ON COLUMN outbox_dlq.original_event_id IS 'Reference to the original outbox event';
COMMENT ON COLUMN outbox_dlq.error_message IS 'Error from the last publish attempt';
COMMENT ON COLUMN outbox_dlq.resolution_notes IS 'Notes about how the issue was resolved';
COMMENT ON COLUMN outbox_dlq.resolved_by IS 'Who resolved this DLQ entry';

-- Migration: Outbox DLQ - Dead Letter Queue for failed events
-- Version: 20251228120000
-- Purpose: Handle events that repeatedly fail to publish

CREATE TABLE IF NOT EXISTS outbox_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_event_id UUID NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER', 'SAGA')),
    event_type VARCHAR(50) NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB,
    error_message TEXT NOT NULL,
    retry_count INTEGER NOT NULL,
    original_created_at TIMESTAMPTZ NOT NULL,
    moved_at TIMESTAMPTZ DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolution_notes TEXT,
    resolved_by VARCHAR(100),
    UNIQUE(original_event_id)
);

-- Indexes for outbox_dlq
CREATE INDEX IF NOT EXISTS idx_dlq_status ON outbox_dlq(moved_at) WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dlq_aggregate ON outbox_dlq(aggregate_type, aggregate_id);
CREATE INDEX IF NOT EXISTS idx_dlq_event_type ON outbox_dlq(event_type, moved_at);
CREATE INDEX IF NOT EXISTS idx_dlq_moved_at ON outbox_dlq(moved_at DESC);

-- Comments
COMMENT ON TABLE outbox_dlq IS 'Dead Letter Queue - stores events that failed to publish after max retries';
COMMENT ON COLUMN outbox_dlq.original_event_id IS 'Reference to the original outbox_events.id';
COMMENT ON COLUMN outbox_dlq.resolved_at IS 'When the event was manually resolved';

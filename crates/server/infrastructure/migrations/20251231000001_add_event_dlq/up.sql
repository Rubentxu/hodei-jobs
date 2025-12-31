-- Migration: EPIC-32 Add event processing dead letter queue
-- Purpose: Handle poison pills and failed event processing

-- UP migration
CREATE TABLE IF NOT EXISTS event_processing_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    aggregate_id VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    error_message TEXT NOT NULL,
    error_count SMALLINT NOT NULL DEFAULT 1,
    first_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolution_action VARCHAR(50),  -- 'reprocessed', 'skipped', 'manual'
    resolution_metadata JSONB,
    subscription_id VARCHAR(255) NOT NULL,
    retry_count SMALLINT NOT NULL DEFAULT 0,
    max_retries SMALLINT NOT NULL DEFAULT 3,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for subscription-based cleanup
CREATE INDEX IF NOT EXISTS idx_event_dlq_subscription
ON event_processing_dlq(subscription_id);

-- Index for unresolved events (partial index)
CREATE INDEX IF NOT EXISTS idx_event_dlq_unresolved
ON event_processing_dlq(first_failure_at DESC)
WHERE resolved_at IS NULL;

-- Index for error aggregation by type
CREATE INDEX IF NOT EXISTS idx_event_dlq_type
ON event_processing_dlq(event_type)
WHERE resolved_at IS NULL;

-- Index for retry logic
CREATE INDEX IF NOT EXISTS idx_event_dlq_retry
ON event_processing_dlq(error_count, max_retries)
WHERE resolved_at IS NULL;

-- Prevent duplicate event_id per subscription
CREATE UNIQUE INDEX IF NOT EXISTS idx_event_dlq_event_subscription
ON event_processing_dlq(event_id, subscription_id);

-- DOWN migration
DROP TABLE IF EXISTS event_processing_dlq;

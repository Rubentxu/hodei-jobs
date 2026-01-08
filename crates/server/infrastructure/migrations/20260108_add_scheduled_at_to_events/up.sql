-- Migration: Add scheduled_at and DLQ columns to outbox_events
-- Date: 2026-01-08
-- Purpose: Enable retry scheduling and dead letter queue for EPIC-64
--
-- This migration adds:
-- 1. scheduled_at column for retry scheduling
-- 2. max_retries column for configurable retry limits
-- 3. error_details column for detailed error info
-- 4. Updated status constraint to include RETRY and DEAD_LETTER
-- 5. Trigger for event_work channel notifications

-- Add scheduled_at column if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'outbox_events' AND column_name = 'scheduled_at'
    ) THEN
        ALTER TABLE outbox_events
        ADD COLUMN scheduled_at TIMESTAMPTZ DEFAULT NOW();
    END IF;

    -- Add max_retries column if not exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'outbox_events' AND column_name = 'max_retries'
    ) THEN
        ALTER TABLE outbox_events
        ADD COLUMN max_retries INTEGER DEFAULT 5;
    END IF;

    -- Add error_details column if not exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'outbox_events' AND column_name = 'error_details'
    ) THEN
        ALTER TABLE outbox_events
        ADD COLUMN error_details JSONB;
    END IF;
END $$;

-- Update status constraint to include RETRY and DEAD_LETTER
-- First drop existing constraint if exists
ALTER TABLE outbox_events DROP CONSTRAINT IF EXISTS outbox_events_status_check;

ALTER TABLE outbox_events ADD CONSTRAINT outbox_events_status_check
    CHECK (status IN ('PENDING', 'RETRY', 'PUBLISHED', 'FAILED', 'DEAD_LETTER', 'CANCELLED'));

-- Create or replace trigger function for event notifications
CREATE OR REPLACE FUNCTION notify_outbox_event_insertion()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('event_work', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Drop existing trigger if exists
DROP TRIGGER IF EXISTS outbox_event_inserted ON outbox_events;

-- Create trigger for INSERT notifications
CREATE TRIGGER outbox_event_inserted
AFTER INSERT ON outbox_events
FOR EACH ROW
EXECUTE FUNCTION notify_outbox_event_insertion();

-- Create index for scheduled_at filtering (for efficient retry queries)
CREATE INDEX IF NOT EXISTS idx_outbox_events_scheduled
ON outbox_events(status, scheduled_at)
WHERE status IN ('PENDING', 'RETRY');

-- Create index for dead letter queries
CREATE INDEX IF NOT EXISTS idx_outbox_events_dead_letter
ON outbox_events(status, created_at DESC)
WHERE status = 'DEAD_LETTER';

-- Comments
COMMENT ON COLUMN outbox_events.scheduled_at IS 'When this event should be processed (for retry scheduling)';
COMMENT ON COLUMN outbox_events.max_retries IS 'Maximum retry attempts before dead letter (default: 5)';
COMMENT ON COLUMN outbox_events.error_details IS 'Detailed error information including stack trace';
COMMENT ON FUNCTION notify_outbox_event_insertion() IS 'Triggers pg_notify when new event is inserted to outbox_events for hybrid relay';
COMMENT ON TRIGGER outbox_event_inserted ON outbox_events IS 'Fires NOTIFY on new outbox events for reactive hybrid processing';

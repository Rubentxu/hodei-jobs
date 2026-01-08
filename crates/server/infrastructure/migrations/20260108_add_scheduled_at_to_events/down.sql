-- Rollback: Remove scheduled_at and DLQ columns from outbox_events
-- Date: 2026-01-08
-- Purpose: Rollback EPIC-64 migration

-- Drop trigger
DROP TRIGGER IF EXISTS outbox_event_inserted ON outbox_events;

-- Drop trigger function
DROP FUNCTION IF EXISTS notify_outbox_event_insertion();

-- Drop indexes
DROP INDEX IF EXISTS idx_outbox_events_scheduled;
DROP INDEX IF EXISTS idx_outbox_events_dead_letter;

-- Drop constraints
ALTER TABLE outbox_events DROP CONSTRAINT IF EXISTS outbox_events_status_check;

-- Restore original status constraint
ALTER TABLE outbox_events ADD CONSTRAINT outbox_events_status_check
    CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED'));

-- Drop columns (optional - data preservation)
-- ALTER TABLE outbox_events DROP COLUMN IF EXISTS scheduled_at;
-- ALTER TABLE outbox_events DROP COLUMN IF EXISTS max_retries;
-- ALTER TABLE outbox_events DROP COLUMN IF EXISTS error_details;

-- Note: Columns are kept to preserve data if migration is re-applied
-- If you want to remove the data, uncomment the DROP COLUMN statements above

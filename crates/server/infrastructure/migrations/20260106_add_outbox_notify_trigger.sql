-- Migration: Add LISTEN/NOTIFY trigger for reactive outbox processing
-- Date: 2026-01-06
-- Purpose: Enable PostgreSQL NOTIFY/LISTEN for instant outbox event detection

-- Create function to notify on new outbox events
CREATE OR REPLACE FUNCTION notify_outbox_event()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('outbox_events_channel', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Drop trigger if exists (for idempotent migration)
DROP TRIGGER IF EXISTS outbox_event_inserted ON outbox_events;

-- Create trigger to fire NOTIFY on INSERT
CREATE TRIGGER outbox_event_inserted
AFTER INSERT ON outbox_events
FOR EACH ROW
EXECUTE FUNCTION notify_outbox_event();

-- Comments
COMMENT ON FUNCTION notify_outbox_event() IS 'Triggers pg_notify when new event is inserted to outbox_events';
COMMENT ON TRIGGER outbox_event_inserted ON outbox_events IS 'Fires NOTIFY on new outbox events for reactive processing';

-- Verify trigger exists
SELECT trigger_name, event_manipulation, action_statement
FROM information_schema.triggers
WHERE trigger_name = 'outbox_event_inserted';

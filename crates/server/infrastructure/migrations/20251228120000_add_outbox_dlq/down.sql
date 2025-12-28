-- Rollback: Remove Dead Letter Queue table

DROP TABLE IF EXISTS outbox_dlq CASCADE;

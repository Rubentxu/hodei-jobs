-- Rollback: Consolidate Log Files
-- Version: 20251230000001

-- Note: This migration is largely irreversible as it may modify data
-- The safest approach is to recreate the table with original structure

-- For rollback, we simply drop the table (data loss is expected)
DROP TABLE IF EXISTS job_log_files CASCADE;

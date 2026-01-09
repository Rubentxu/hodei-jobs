-- Drop Worker Bootstrap Tokens Table
DROP INDEX IF EXISTS idx_worker_bootstrap_tokens_consumed_at;
DROP INDEX IF EXISTS idx_worker_bootstrap_tokens_expires_at;
DROP INDEX IF EXISTS idx_worker_bootstrap_tokens_worker_id;
DROP TABLE IF EXISTS worker_bootstrap_tokens;

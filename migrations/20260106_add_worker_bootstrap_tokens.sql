-- Worker Bootstrap Tokens Table
-- Stores OTP tokens for ephemeral worker registration

CREATE TABLE IF NOT EXISTS worker_bootstrap_tokens (
    token UUID PRIMARY KEY,
    worker_id UUID NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX idx_worker_bootstrap_tokens_worker_id ON worker_bootstrap_tokens(worker_id);
CREATE INDEX idx_worker_bootstrap_tokens_expires_at ON worker_bootstrap_tokens(expires_at);
CREATE INDEX idx_worker_bootstrap_tokens_consumed_at ON worker_bootstrap_tokens(consumed_at) WHERE consumed_at IS NULL;

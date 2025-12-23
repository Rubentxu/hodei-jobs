CREATE TABLE IF NOT EXISTS job_log_files (
  job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  file_path TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ,
  PRIMARY KEY (job_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_job_log_files_expires_at ON job_log_files(expires_at);

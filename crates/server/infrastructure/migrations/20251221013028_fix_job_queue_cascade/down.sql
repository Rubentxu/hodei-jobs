-- Rollback: Restore CASCADE DELETE constraint

-- Drop the RESTRICT constraint
ALTER TABLE job_queue DROP CONSTRAINT IF EXISTS job_queue_job_id_fkey;

-- Restore the original CASCADE DELETE constraint
ALTER TABLE job_queue
ADD CONSTRAINT job_queue_job_id_fkey
FOREIGN KEY (job_id) REFERENCES jobs(id)
ON DELETE CASCADE;

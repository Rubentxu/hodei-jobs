-- Fix job_queue foreign key to prevent CASCADE DELETE
-- This prevents jobs from being deleted when they're removed from the queue

-- Drop the existing foreign key constraint
ALTER TABLE job_queue DROP CONSTRAINT IF EXISTS job_queue_job_id_fkey;

-- Add the foreign key constraint WITHOUT CASCADE DELETE
-- This ensures jobs remain in the database even after being dequeued
ALTER TABLE job_queue
ADD CONSTRAINT job_queue_job_id_fkey
FOREIGN KEY (job_id) REFERENCES jobs(id)
ON DELETE RESTRICT;

-- Add server_instance_id column to execution_processes for instance-scoped process management
-- Each server instance generates a unique ID on startup. Processes are tagged with this ID.
-- On shutdown, only processes belonging to THIS server instance are killed.
-- This prevents cargo watch restarts from killing processes started by other server instances.

ALTER TABLE execution_processes ADD COLUMN server_instance_id TEXT;

-- Index for efficient instance-scoped lookups on running processes
CREATE INDEX idx_execution_processes_instance_running
    ON execution_processes(server_instance_id)
    WHERE status = 'running' AND server_instance_id IS NOT NULL;

-- Add PID column to execution_processes for process management feature
-- This allows tracking system process IDs to discover descendant processes
-- spawned by AI agents (e.g., npm run dev started by Claude Code)

ALTER TABLE execution_processes ADD COLUMN pid INTEGER;

-- Create index for efficient PID lookups on running processes
CREATE INDEX idx_execution_processes_pid_running ON execution_processes(pid) WHERE status = 'running' AND pid IS NOT NULL;

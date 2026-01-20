-- Migration: Add swarm_project_id columns
-- This enables tracking tasks and activity at the swarm project level

-- Add swarm_project_id to shared_tasks
-- Foreign key ensures referential integrity
ALTER TABLE shared_tasks
ADD COLUMN swarm_project_id UUID REFERENCES swarm_projects(id) ON DELETE SET NULL;

-- Create index for queries filtered by swarm_project_id
-- Partial index (WHERE NOT NULL) saves space and improves performance
CREATE INDEX idx_shared_tasks_swarm_project
ON shared_tasks(swarm_project_id)
WHERE swarm_project_id IS NOT NULL;

-- Add swarm_project_id to activity (partitioned table)
-- Cannot add foreign key constraint due to partitioning
ALTER TABLE activity
ADD COLUMN swarm_project_id UUID;

-- Create composite index for activity queries (swarm_project + sequence)
-- Most queries will be: get activity for swarm_project after sequence X
CREATE INDEX idx_activity_swarm_project_seq
ON activity(swarm_project_id, seq DESC)
WHERE swarm_project_id IS NOT NULL;

-- Add swarm_project_id to project_activity_counters
-- ON DELETE CASCADE: if swarm_project deleted, counter should be removed too
ALTER TABLE project_activity_counters
ADD COLUMN swarm_project_id UUID REFERENCES swarm_projects(id) ON DELETE CASCADE;

-- Create unique index for swarm project counters
-- Ensures one counter per swarm_project
CREATE UNIQUE INDEX idx_project_activity_counters_swarm
ON project_activity_counters(swarm_project_id)
WHERE swarm_project_id IS NOT NULL;

-- Fix unique index on remote_project_id for ON CONFLICT upsert support
-- Must match exactly: ON CONFLICT(remote_project_id) WHERE remote_project_id IS NOT NULL
DROP INDEX IF EXISTS idx_projects_remote_project_id;
CREATE UNIQUE INDEX idx_projects_remote_project_id
    ON projects(remote_project_id)
    WHERE remote_project_id IS NOT NULL;

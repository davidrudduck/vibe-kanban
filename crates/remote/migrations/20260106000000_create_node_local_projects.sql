-- Create node_local_projects table to track all local projects from nodes
-- This enables the swarm settings UI to show all projects for linking,
-- not just those already linked via node_projects

CREATE TABLE IF NOT EXISTS node_local_projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    local_project_id UUID NOT NULL,
    name TEXT NOT NULL,
    git_repo_path TEXT NOT NULL,
    default_branch TEXT NOT NULL DEFAULT 'main',
    -- If linked to a swarm project, this tracks the link
    swarm_project_id UUID REFERENCES swarm_projects(id) ON DELETE SET NULL,
    -- When this project was last reported by the node
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Each node can only have one entry per local project
    UNIQUE(node_id, local_project_id)
);

-- Index for looking up projects by node
CREATE INDEX IF NOT EXISTS idx_node_local_projects_node_id
    ON node_local_projects(node_id);

-- Index for looking up projects by swarm project link
CREATE INDEX IF NOT EXISTS idx_node_local_projects_swarm_project_id
    ON node_local_projects(swarm_project_id)
    WHERE swarm_project_id IS NOT NULL;

-- Index for stale project cleanup (find projects not seen recently)
CREATE INDEX IF NOT EXISTS idx_node_local_projects_last_seen
    ON node_local_projects(last_seen_at);

COMMENT ON TABLE node_local_projects IS 'Local projects synced from swarm nodes for linking to swarm projects';
COMMENT ON COLUMN node_local_projects.last_seen_at IS 'Last time the node reported this project. Used for stale project cleanup.';

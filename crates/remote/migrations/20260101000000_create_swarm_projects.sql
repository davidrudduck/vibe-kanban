-- Create swarm_projects table for representing projects shared across the swarm.
-- This is distinct from the existing 'projects' table which represents hive-level projects.
-- swarm_projects are explicitly linked projects that can have multiple node projects attached.

CREATE TABLE IF NOT EXISTS swarm_projects (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_swarm_projects_org ON swarm_projects(organization_id);

-- Junction table linking swarm projects to node projects.
-- A swarm project can be linked to multiple node projects (one per node).
-- When a project is linked, its tasks become swarm-owned and sync across all linked nodes.
CREATE TABLE IF NOT EXISTS swarm_project_nodes (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_project_id    UUID NOT NULL REFERENCES swarm_projects(id) ON DELETE CASCADE,
    node_id             UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    local_project_id    UUID NOT NULL,  -- ID in node's SQLite
    git_repo_path       TEXT NOT NULL,
    os_type             TEXT,  -- 'linux', 'darwin', 'windows'
    linked_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Each node can only have one link per swarm project
    UNIQUE(swarm_project_id, node_id),
    -- Each local project on a node can only be linked once
    UNIQUE(node_id, local_project_id)
);

CREATE INDEX IF NOT EXISTS idx_swarm_project_nodes_swarm ON swarm_project_nodes(swarm_project_id);
CREATE INDEX IF NOT EXISTS idx_swarm_project_nodes_node ON swarm_project_nodes(node_id);

-- Trigger for updated_at
CREATE TRIGGER trg_swarm_projects_updated_at
    BEFORE UPDATE ON swarm_projects
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

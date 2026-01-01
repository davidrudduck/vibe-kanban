-- Create swarm_templates table for organization-wide task templates.
-- Swarm templates are org-global (project_id = NULL) and can be used across all nodes in the swarm.

CREATE TABLE IF NOT EXISTS swarm_templates (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    content         TEXT NOT NULL DEFAULT '',
    description     TEXT,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    version         BIGINT NOT NULL DEFAULT 1,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ,
    -- Unique constraint on name within organization (only for non-deleted)
    UNIQUE NULLS NOT DISTINCT (organization_id, name, deleted_at)
);

CREATE INDEX IF NOT EXISTS idx_swarm_templates_org ON swarm_templates(organization_id);
CREATE INDEX IF NOT EXISTS idx_swarm_templates_deleted ON swarm_templates(deleted_at) WHERE deleted_at IS NULL;

-- Trigger for updated_at
CREATE TRIGGER trg_swarm_templates_updated_at
    BEFORE UPDATE ON swarm_templates
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

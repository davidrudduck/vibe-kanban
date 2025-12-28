-- Create labels table for visual task categorization in the Hive
-- Labels can be organization-global (project_id IS NULL) or project-specific
-- Labels sync from nodes with origin_node_id tracking where they came from

CREATE TABLE labels (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id      UUID REFERENCES projects(id) ON DELETE CASCADE,
    origin_node_id  UUID REFERENCES nodes(id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    icon            TEXT NOT NULL DEFAULT 'tag',
    color           TEXT NOT NULL DEFAULT '#6b7280',
    version         BIGINT NOT NULL DEFAULT 1,
    deleted_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint: same name within org/project scope, excluding deleted
    UNIQUE NULLS NOT DISTINCT (organization_id, project_id, name, deleted_at)
);

-- Create junction table for shared_tasks to labels
CREATE TABLE shared_task_labels (
    shared_task_id  UUID NOT NULL REFERENCES shared_tasks(id) ON DELETE CASCADE,
    label_id        UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (shared_task_id, label_id)
);

-- Indexes for efficient queries
CREATE INDEX idx_labels_organization ON labels(organization_id);
CREATE INDEX idx_labels_project ON labels(project_id);
CREATE INDEX idx_labels_origin_node ON labels(origin_node_id);
CREATE INDEX idx_labels_deleted ON labels(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_shared_task_labels_label ON shared_task_labels(label_id);

-- Trigger for updated_at
CREATE TRIGGER trg_labels_updated_at
    BEFORE UPDATE ON labels
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

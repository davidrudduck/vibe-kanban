-- Create labels table for visual task categorization
-- Labels can be global (project_id IS NULL) or project-specific

CREATE TABLE labels (
    id            BLOB PRIMARY KEY,
    project_id    BLOB,
    name          TEXT NOT NULL,
    icon          TEXT NOT NULL DEFAULT 'tag',
    color         TEXT NOT NULL DEFAULT '#6b7280',
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Create junction table for many-to-many task-label relationship
CREATE TABLE task_labels (
    id            BLOB PRIMARY KEY,
    task_id       BLOB NOT NULL,
    label_id      BLOB NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE,
    UNIQUE(task_id, label_id)
);

-- Indexes for efficient queries
CREATE INDEX idx_labels_project_id ON labels(project_id);
CREATE INDEX idx_task_labels_task_id ON task_labels(task_id);
CREATE INDEX idx_task_labels_label_id ON task_labels(label_id);

-- Insert default global labels
INSERT INTO labels (id, project_id, name, icon, color) VALUES
    (randomblob(16), NULL, 'Planning', 'clipboard-list', '#f59e0b'),
    (randomblob(16), NULL, 'Implementation', 'code', '#3b82f6'),
    (randomblob(16), NULL, 'Bug Fix', 'bug', '#ef4444');

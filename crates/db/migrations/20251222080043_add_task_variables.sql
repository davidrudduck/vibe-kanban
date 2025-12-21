-- Create task_variables table for storing key-value pairs on tasks
-- Variables use shell-style syntax ($VAR or ${VAR}) and are expanded at execution time
-- Child tasks inherit variables from parent tasks (via parent_task_id)

CREATE TABLE task_variables (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK(name GLOB '[A-Z][A-Z0-9_]*'),
    value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    UNIQUE(task_id, name)
);

CREATE INDEX idx_task_variables_task_id ON task_variables(task_id);

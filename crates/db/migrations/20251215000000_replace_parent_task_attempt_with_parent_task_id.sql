-- Migration: Replace parent_task_attempt with parent_task_id
-- This changes the subtask relationship from task->attempt->task to task->task directly
-- for simpler navigation and cleaner data model
--
-- Following the official SQLite "12-step generalized ALTER TABLE" procedure:
-- https://www.sqlite.org/lang_altertable.html#otheralter

PRAGMA foreign_keys = OFF;

-- SQLx workaround: explicit transaction control to ensure PRAGMA persists
-- https://github.com/launchbadge/sqlx/issues/2085#issuecomment-1499859906
COMMIT TRANSACTION;

BEGIN TRANSACTION;

-- Step 1: Create new tasks table with parent_task_id instead of parent_task_attempt
CREATE TABLE tasks_new (
    id                        BLOB PRIMARY KEY,
    project_id                BLOB NOT NULL,
    title                     TEXT NOT NULL,
    description               TEXT,
    status                    TEXT NOT NULL DEFAULT 'todo'
                              CHECK (status IN ('todo','inprogress','done','cancelled','inreview')),
    parent_task_id            BLOB REFERENCES tasks_new(id) ON DELETE SET NULL,
    shared_task_id            BLOB,
    created_at                TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    is_remote                 INTEGER NOT NULL DEFAULT 0,
    remote_assignee_user_id   BLOB,
    remote_assignee_name      TEXT,
    remote_assignee_username  TEXT,
    remote_version            INTEGER NOT NULL DEFAULT 0,
    remote_last_synced_at     TEXT,
    remote_stream_node_id     BLOB,
    remote_stream_url         TEXT,
    validation_steps          TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Step 2: Migrate data, converting parent_task_attempt to parent_task_id
-- The subquery finds the task that owns the parent attempt
INSERT INTO tasks_new (
    id, project_id, title, description, status,
    parent_task_id,
    shared_task_id, created_at, updated_at,
    is_remote, remote_assignee_user_id, remote_assignee_name,
    remote_assignee_username, remote_version, remote_last_synced_at,
    remote_stream_node_id, remote_stream_url, validation_steps
)
SELECT
    t.id, t.project_id, t.title, t.description, t.status,
    (SELECT ta.task_id FROM task_attempts ta WHERE ta.id = t.parent_task_attempt),
    t.shared_task_id, t.created_at, t.updated_at,
    t.is_remote, t.remote_assignee_user_id, t.remote_assignee_name,
    t.remote_assignee_username, t.remote_version, t.remote_last_synced_at,
    t.remote_stream_node_id, t.remote_stream_url, t.validation_steps
FROM tasks t;

-- Step 3: Drop old table and rename new
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- Step 4: Recreate indexes
CREATE INDEX idx_tasks_project_id ON tasks(project_id);
CREATE INDEX idx_tasks_parent_task_id ON tasks(parent_task_id);
CREATE INDEX idx_tasks_is_remote ON tasks(is_remote);
CREATE UNIQUE INDEX idx_tasks_shared_task_unique ON tasks(shared_task_id) WHERE shared_task_id IS NOT NULL;

-- Verify foreign key constraints before committing
PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys = ON;

-- SQLx workaround: restore implicit transaction state
BEGIN TRANSACTION;

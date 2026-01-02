-- Drop is_remote column from tasks table
--
-- SQLite requires table recreation to drop columns.
-- This migration is part of the legacy task sync cleanup.
--
-- NOTE: Foreign key enforcement is disabled at the connection level in Rust code
-- (see run_migrations_with_fk_disabled) before migrations run. This prevents
-- CASCADE deletes when DROP TABLE is executed.
--
-- PRAGMA foreign_keys = OFF/ON cannot be used here because SQLite does not allow
-- changing the foreign_keys pragma inside a transaction, and SQLx wraps migrations
-- in transactions.

-- Step 1: Create new tasks table without is_remote
CREATE TABLE IF NOT EXISTS tasks_new (
    id            BLOB PRIMARY KEY,
    project_id    BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title         TEXT NOT NULL,
    description   TEXT,
    status        TEXT NOT NULL DEFAULT 'todo',
    parent_task_id BLOB REFERENCES tasks_new(id) ON DELETE SET NULL,
    shared_task_id BLOB,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    -- Keeping remote_* fields for task execution/streaming info
    remote_assignee_user_id BLOB,
    remote_assignee_name TEXT,
    remote_assignee_username TEXT,
    remote_version INTEGER NOT NULL DEFAULT 1,
    remote_last_synced_at TEXT,
    remote_stream_node_id BLOB,
    remote_stream_url TEXT,
    archived_at   TEXT,
    activity_at   TEXT
);

-- Step 2: Copy data (excluding is_remote if it exists)
-- Using INSERT OR IGNORE in case this runs on a restored database where
-- the data might already exist in tasks_new
INSERT OR IGNORE INTO tasks_new (
    id, project_id, title, description, status, parent_task_id, shared_task_id,
    created_at, updated_at, remote_assignee_user_id, remote_assignee_name,
    remote_assignee_username, remote_version, remote_last_synced_at,
    remote_stream_node_id, remote_stream_url, archived_at, activity_at
)
SELECT
    id, project_id, title, description, status, parent_task_id, shared_task_id,
    created_at, updated_at, remote_assignee_user_id, remote_assignee_name,
    remote_assignee_username, remote_version, remote_last_synced_at,
    remote_stream_node_id, remote_stream_url, archived_at, activity_at
FROM tasks;

-- Step 3: Drop old table (safe because FK enforcement is disabled at connection level)
DROP TABLE IF EXISTS tasks;

-- Step 4: Rename new table
ALTER TABLE tasks_new RENAME TO tasks;

-- Step 5: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_parent_task_id ON tasks(parent_task_id);

-- Recreate the partial unique constraint on shared_task_id
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_shared_task_id ON tasks(shared_task_id) WHERE shared_task_id IS NOT NULL;

-- Recreate activity_at composite indexes
CREATE INDEX IF NOT EXISTS idx_tasks_activity_at_created_at ON tasks(COALESCE(activity_at, created_at) DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_project_activity ON tasks(project_id, COALESCE(activity_at, created_at) DESC);

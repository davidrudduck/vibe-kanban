-- Migration: Rename Columns for Swarm Clarity (SQLite)
-- Part of Phase 1: Reset & Unlink Migration
-- Session 1.2: Rename remote_*/shared_* columns to swarm_* for consistency
--
-- This migration renames columns to use consistent "swarm_" prefix:
-- - remote_project_id -> swarm_project_id
-- - shared_task_id -> swarm_task_id
-- - shared_label_id -> swarm_label_id
--
-- Note: SQLite doesn't support ALTER COLUMN RENAME directly, so we use the
-- standard approach: create new table, copy data, drop old, rename new.

-- ============================================
-- Step 1: Rename projects columns
-- ============================================
-- Create new projects table with renamed columns
CREATE TABLE projects_new (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    git_repo_path TEXT NOT NULL,
    setup_script TEXT,
    dev_script TEXT,
    cleanup_script TEXT,
    copy_files TEXT,
    parallel_setup_script BOOLEAN NOT NULL DEFAULT 0,
    -- Renamed: remote_project_id -> swarm_project_id
    swarm_project_id BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    -- Remote project fields (for synced projects from other nodes)
    is_remote BOOLEAN NOT NULL DEFAULT 0,
    source_node_id BLOB,
    source_node_name TEXT,
    source_node_public_url TEXT,
    source_node_status TEXT DEFAULT 'unknown',
    remote_last_synced_at TEXT,
    -- GitHub integration fields
    github_enabled BOOLEAN NOT NULL DEFAULT 0,
    github_owner TEXT,
    github_repo TEXT,
    github_open_issues INTEGER NOT NULL DEFAULT 0,
    github_open_prs INTEGER NOT NULL DEFAULT 0,
    github_last_synced_at TEXT
);

-- Copy data from old table to new table
INSERT INTO projects_new (
    id, name, git_repo_path, setup_script, dev_script, cleanup_script,
    copy_files, parallel_setup_script, swarm_project_id, created_at, updated_at,
    is_remote, source_node_id, source_node_name, source_node_public_url,
    source_node_status, remote_last_synced_at, github_enabled, github_owner,
    github_repo, github_open_issues, github_open_prs, github_last_synced_at
)
SELECT
    id, name, git_repo_path, setup_script, dev_script, cleanup_script,
    copy_files, parallel_setup_script, remote_project_id, created_at, updated_at,
    is_remote, source_node_id, source_node_name, source_node_public_url,
    source_node_status, remote_last_synced_at, github_enabled, github_owner,
    github_repo, github_open_issues, github_open_prs, github_last_synced_at
FROM projects;

-- Drop old table
DROP TABLE projects;

-- Rename new table
ALTER TABLE projects_new RENAME TO projects;

-- Recreate indexes
CREATE INDEX idx_projects_is_remote ON projects(is_remote);
CREATE INDEX idx_projects_source_node ON projects(source_node_id) WHERE source_node_id IS NOT NULL;
CREATE UNIQUE INDEX idx_projects_swarm_project_id ON projects(swarm_project_id) WHERE swarm_project_id IS NOT NULL;
CREATE UNIQUE INDEX idx_projects_git_repo_path ON projects(git_repo_path);

-- ============================================
-- Step 2: Rename tasks columns
-- ============================================
-- Create new tasks table with renamed columns
CREATE TABLE tasks_new (
    id BLOB PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo' CHECK(status IN ('todo', 'inprogress', 'inreview', 'done', 'cancelled')),
    parent_task_id BLOB REFERENCES tasks_new(id) ON DELETE SET NULL,
    -- Renamed: shared_task_id -> swarm_task_id
    swarm_task_id BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    -- Remote task fields
    is_remote BOOLEAN NOT NULL DEFAULT 0,
    remote_assignee_user_id BLOB,
    remote_assignee_name TEXT,
    remote_assignee_username TEXT,
    remote_version INTEGER NOT NULL DEFAULT 0,
    remote_last_synced_at TEXT,
    remote_stream_node_id BLOB,
    remote_stream_url TEXT,
    archived_at TEXT,
    activity_at TEXT
);

-- Copy data from old table to new table
INSERT INTO tasks_new (
    id, project_id, title, description, status, parent_task_id, swarm_task_id,
    created_at, updated_at, is_remote, remote_assignee_user_id, remote_assignee_name,
    remote_assignee_username, remote_version, remote_last_synced_at,
    remote_stream_node_id, remote_stream_url, archived_at, activity_at
)
SELECT
    id, project_id, title, description, status, parent_task_id, shared_task_id,
    created_at, updated_at, is_remote, remote_assignee_user_id, remote_assignee_name,
    remote_assignee_username, remote_version, remote_last_synced_at,
    remote_stream_node_id, remote_stream_url, archived_at, activity_at
FROM tasks;

-- Drop old table
DROP TABLE tasks;

-- Rename new table
ALTER TABLE tasks_new RENAME TO tasks;

-- Recreate indexes
CREATE INDEX idx_tasks_project_id ON tasks(project_id);
CREATE INDEX idx_tasks_parent_task_id ON tasks(parent_task_id);
CREATE INDEX idx_tasks_is_remote ON tasks(is_remote);
CREATE UNIQUE INDEX idx_tasks_swarm_task_id ON tasks(swarm_task_id) WHERE swarm_task_id IS NOT NULL;
CREATE INDEX idx_tasks_activity_at ON tasks(activity_at);
CREATE INDEX idx_tasks_archived_at ON tasks(archived_at);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_project_activity ON tasks(project_id, COALESCE(activity_at, created_at) DESC);

-- ============================================
-- Step 3: Rename labels columns
-- ============================================
-- Create new labels table with renamed columns
CREATE TABLE labels_new (
    id BLOB PRIMARY KEY,
    project_id BLOB REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT 'tag',
    color TEXT NOT NULL DEFAULT '#6b7280',
    -- Renamed: shared_label_id -> swarm_label_id
    swarm_label_id BLOB,
    version INTEGER NOT NULL DEFAULT 1,
    synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

-- Copy data from old table to new table
INSERT INTO labels_new (
    id, project_id, name, icon, color, swarm_label_id, version, synced_at, created_at, updated_at
)
SELECT
    id, project_id, name, icon, color, shared_label_id, version, synced_at, created_at, updated_at
FROM labels;

-- Drop old table
DROP TABLE labels;

-- Rename new table
ALTER TABLE labels_new RENAME TO labels;

-- Recreate indexes
CREATE INDEX idx_labels_project_id ON labels(project_id);
CREATE INDEX idx_labels_synced_at ON labels(synced_at);
CREATE UNIQUE INDEX idx_labels_swarm_label_id ON labels(swarm_label_id) WHERE swarm_label_id IS NOT NULL;

-- ============================================
-- Step 4: Recreate task_labels junction table
-- ============================================
-- The task_labels table references tasks, so we need to recreate it
CREATE TABLE task_labels_new (
    id BLOB PRIMARY KEY,
    task_id BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    label_id BLOB NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    UNIQUE(task_id, label_id)
);

-- Copy data
INSERT INTO task_labels_new (id, task_id, label_id, created_at)
SELECT id, task_id, label_id, created_at FROM task_labels;

-- Drop old table
DROP TABLE task_labels;

-- Rename new table
ALTER TABLE task_labels_new RENAME TO task_labels;

-- Recreate indexes
CREATE INDEX idx_task_labels_task_id ON task_labels(task_id);
CREATE INDEX idx_task_labels_label_id ON task_labels(label_id);

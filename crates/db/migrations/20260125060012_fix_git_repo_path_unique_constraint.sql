-- Migration: Fix git_repo_path unique constraint for multi-node support
--
-- Problem: git_repo_path has a global UNIQUE constraint, but remote projects
-- from different nodes can legitimately share the same path string (e.g.,
-- /home/david/Code/Shohin exists on both Node A and Node B).
--
-- Solution:
-- 1. Remove the global UNIQUE constraint on git_repo_path
-- 2. Add partial unique index for LOCAL projects only (is_remote = 0)
-- 3. Add compound unique index for REMOTE projects (path + source_node_id)
--
-- This allows:
-- - Local projects: path must be unique among local projects
-- - Remote projects: path must be unique per source_node_id

-- Step 1: Create new table without the global UNIQUE constraint on git_repo_path
CREATE TABLE projects_new (
    id            BLOB PRIMARY KEY,
    name          TEXT NOT NULL,
    git_repo_path TEXT NOT NULL DEFAULT '',  -- UNIQUE removed!
    setup_script  TEXT DEFAULT '',
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    dev_script TEXT DEFAULT '',
    cleanup_script TEXT,
    copy_files TEXT,
    remote_project_id BLOB,
    is_remote BOOLEAN NOT NULL DEFAULT 0,
    source_node_id BLOB,
    source_node_name TEXT,
    source_node_public_url TEXT,
    source_node_status TEXT DEFAULT 'unknown',
    remote_last_synced_at TEXT,
    parallel_setup_script BOOLEAN NOT NULL DEFAULT 0,
    github_enabled INTEGER DEFAULT 0 NOT NULL,
    github_owner TEXT,
    github_repo TEXT,
    github_open_issues INTEGER DEFAULT 0 NOT NULL,
    github_open_prs INTEGER DEFAULT 0 NOT NULL,
    github_last_synced_at TEXT
);

-- Step 2: Copy all data from old table
INSERT INTO projects_new SELECT * FROM projects;

-- Step 3: Drop old table
DROP TABLE projects;

-- Step 4: Rename new table to projects
ALTER TABLE projects_new RENAME TO projects;

-- Step 5: Recreate existing indexes
-- Unique index on remote_project_id (for upsert ON CONFLICT)
CREATE UNIQUE INDEX idx_projects_remote_project_id
    ON projects(remote_project_id)
    WHERE remote_project_id IS NOT NULL;

-- Index for filtering remote vs local projects
CREATE INDEX idx_projects_is_remote ON projects(is_remote);

-- Index on source_node_id for remote project lookups
CREATE INDEX idx_projects_source_node ON projects(source_node_id) WHERE source_node_id IS NOT NULL;

-- Step 6: Create NEW partial unique indexes

-- Local projects: git_repo_path must be unique among local projects
-- This prevents creating two local projects with the same path
CREATE UNIQUE INDEX idx_projects_git_repo_path_local
    ON projects(git_repo_path)
    WHERE is_remote = 0;

-- Remote projects: combination of (git_repo_path, source_node_id) must be unique
-- This allows the same path from different nodes, but prevents duplicates from the same node
CREATE UNIQUE INDEX idx_projects_git_repo_path_remote
    ON projects(git_repo_path, source_node_id)
    WHERE is_remote = 1;

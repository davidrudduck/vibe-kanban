-- Add GitHub integration fields to projects table
-- These fields enable tracking of GitHub issues and PRs for linked projects

-- Whether GitHub integration is enabled for this project
ALTER TABLE projects ADD COLUMN github_enabled INTEGER DEFAULT 0 NOT NULL;

-- GitHub repository owner (e.g., "anthropics" from "anthropics/claude-code")
ALTER TABLE projects ADD COLUMN github_owner TEXT;

-- GitHub repository name (e.g., "claude-code" from "anthropics/claude-code")
ALTER TABLE projects ADD COLUMN github_repo TEXT;

-- Count of open issues (cached from GitHub API)
ALTER TABLE projects ADD COLUMN github_open_issues INTEGER DEFAULT 0 NOT NULL;

-- Count of open pull requests (cached from GitHub API)
ALTER TABLE projects ADD COLUMN github_open_prs INTEGER DEFAULT 0 NOT NULL;

-- Timestamp of last successful sync with GitHub API
ALTER TABLE projects ADD COLUMN github_last_synced_at TEXT;

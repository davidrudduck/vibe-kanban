ALTER TABLE issues
    ADD COLUMN archived BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX idx_issues_project_archived
    ON issues(project_id, archived);

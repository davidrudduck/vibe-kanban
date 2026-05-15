CREATE TABLE claude_terminal_sessions (
    execution_process_id BLOB PRIMARY KEY NOT NULL,
    workspace_id         BLOB NOT NULL,
    tmux_session_name    TEXT NOT NULL,
    claude_session_id    TEXT,
    transcript_path      TEXT,
    transcript_offset    INTEGER NOT NULL DEFAULT 0,
    cwd                  TEXT,
    created_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX idx_claude_terminal_sessions_workspace_id
    ON claude_terminal_sessions(workspace_id);

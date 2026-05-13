PRAGMA foreign_keys = ON;

CREATE TABLE execution_log_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id    BLOB NOT NULL,
    source          TEXT NOT NULL,
    source_event_id TEXT,
    event_type      TEXT NOT NULL CHECK (
        event_type IN (
            'execution_started',
            'execution_finished',
            'user_message',
            'assistant_message_delta',
            'assistant_message_final',
            'tool_started',
            'tool_delta',
            'tool_finished',
            'system_status',
            'raw_stdout',
            'raw_stderr',
            'json_patch',
            'reset_ignored',
            'refresh_required'
        )
    ),
    payload_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    UNIQUE(execution_id, source, source_event_id)
);

CREATE INDEX idx_execution_log_events_execution_id_id
    ON execution_log_events(execution_id, id);

CREATE INDEX idx_execution_log_events_execution_id_created_at
    ON execution_log_events(execution_id, created_at);

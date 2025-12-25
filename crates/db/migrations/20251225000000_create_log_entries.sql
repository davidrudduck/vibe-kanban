-- Create log_entries table for ElectricSQL compatibility
-- This stores individual log messages as rows (instead of JSONL batches)
-- enabling row-level sync with ElectricSQL's shape subscriptions.

PRAGMA foreign_keys = ON;

-- Log entries table - one row per log message
CREATE TABLE log_entries (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id    BLOB NOT NULL,
    output_type     TEXT NOT NULL CHECK (output_type IN ('stdout', 'stderr', 'system', 'json_patch', 'session_id', 'finished', 'refresh_required')),
    content         TEXT NOT NULL,
    timestamp       TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_id) REFERENCES execution_processes(id) ON DELETE CASCADE
);

-- Index for efficient pagination queries by execution
CREATE INDEX idx_log_entries_execution_id_id ON log_entries(execution_id, id);

-- Index for timestamp-based queries
CREATE INDEX idx_log_entries_execution_id_timestamp ON log_entries(execution_id, timestamp);

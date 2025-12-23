-- Activity dismissals table for tracking dismissed activity feed items
-- Single-user app, no dismissed_by needed

CREATE TABLE activity_dismissals (
    id            BLOB PRIMARY KEY,
    task_id       BLOB NOT NULL,
    dismissed_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id)
);

-- Index for efficient lookups when filtering activity feed
CREATE INDEX idx_activity_dismissals_task ON activity_dismissals(task_id);

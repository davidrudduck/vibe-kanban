-- Node Task Attempts - tracks task execution attempts from nodes
-- Each attempt represents a single run of an executor against a task
CREATE TABLE IF NOT EXISTS node_task_attempts (
    id                  UUID PRIMARY KEY,  -- Same as local task_attempt.id
    assignment_id       UUID REFERENCES node_task_assignments(id) ON DELETE CASCADE,
    shared_task_id      UUID NOT NULL REFERENCES shared_tasks(id) ON DELETE CASCADE,
    node_id             UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    executor            TEXT NOT NULL,     -- 'CLAUDE_CODE', 'GEMINI', etc.
    executor_variant    TEXT,              -- Optional variant (e.g., 'opus', 'sonnet')
    branch              TEXT NOT NULL,     -- Git branch name for this attempt
    target_branch       TEXT NOT NULL,     -- Target branch for PR/merge
    container_ref       TEXT,              -- Path to worktree or container ID
    worktree_deleted    BOOLEAN NOT NULL DEFAULT FALSE,
    setup_completed_at  TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_node_task_attempts_assignment ON node_task_attempts(assignment_id);
CREATE INDEX IF NOT EXISTS idx_node_task_attempts_task ON node_task_attempts(shared_task_id);
CREATE INDEX IF NOT EXISTS idx_node_task_attempts_node ON node_task_attempts(node_id);

-- Node Execution Processes - tracks individual process executions within attempts
-- Each execution represents a single run of setup/cleanup/agent/devserver
CREATE TABLE IF NOT EXISTS node_execution_processes (
    id                  UUID PRIMARY KEY,  -- Same as local execution_process.id
    attempt_id          UUID NOT NULL REFERENCES node_task_attempts(id) ON DELETE CASCADE,
    node_id             UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    run_reason          TEXT NOT NULL,     -- 'setupscript', 'cleanupscript', 'codingagent', 'devserver'
    executor_action     JSONB,             -- Action details (profile, request type, etc.)
    before_head_commit  TEXT,              -- Git HEAD before process ran
    after_head_commit   TEXT,              -- Git HEAD after process completed
    status              TEXT NOT NULL,     -- 'running', 'completed', 'failed', 'killed'
    exit_code           INTEGER,
    dropped             BOOLEAN NOT NULL DEFAULT FALSE,
    pid                 BIGINT,            -- System process ID
    started_at          TIMESTAMPTZ NOT NULL,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_node_execution_processes_attempt ON node_execution_processes(attempt_id);
CREATE INDEX IF NOT EXISTS idx_node_execution_processes_node ON node_execution_processes(node_id);
CREATE INDEX IF NOT EXISTS idx_node_execution_processes_status ON node_execution_processes(status) WHERE status = 'running';

-- Extend existing logs table to link to execution processes
ALTER TABLE node_task_output_logs
    ADD COLUMN IF NOT EXISTS execution_process_id UUID REFERENCES node_execution_processes(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_task_output_logs_execution ON node_task_output_logs(execution_process_id);

-- Trigger for updated_at on node_task_attempts
CREATE TRIGGER trg_node_task_attempts_updated_at
    BEFORE UPDATE ON node_task_attempts
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

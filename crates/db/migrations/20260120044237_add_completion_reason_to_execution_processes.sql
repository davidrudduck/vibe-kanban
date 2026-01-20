-- Add completion_reason and completion_message columns to execution_processes
-- These track WHY a session completed (result, eof, killed, error) and any associated message

ALTER TABLE execution_processes ADD COLUMN completion_reason TEXT;
ALTER TABLE execution_processes ADD COLUMN completion_message TEXT;

-- completion_reason values:
-- "result_success" - Normal completion with Result message (subtype: success)
-- "result_error" - Completion with Result message (subtype: error or is_error: true)
-- "eof" - EOF without Result message (abnormal termination)
-- "killed" - Session was killed by user
-- "error" - Session ended due to an error

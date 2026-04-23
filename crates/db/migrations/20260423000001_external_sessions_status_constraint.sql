-- SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so enforce the valid
-- `status` values on external_sessions with BEFORE INSERT / UPDATE triggers.
-- Keep the allowed set in sync with api-types/src/external_session.rs.

CREATE TRIGGER IF NOT EXISTS external_sessions_status_check_insert
BEFORE INSERT ON external_sessions
BEGIN
  SELECT CASE
    WHEN NEW.status NOT IN ('in_progress', 'in_review', 'done', 'blocked')
    THEN RAISE(ABORT, 'invalid external_sessions.status value')
  END;
END;

CREATE TRIGGER IF NOT EXISTS external_sessions_status_check_update
BEFORE UPDATE ON external_sessions
BEGIN
  SELECT CASE
    WHEN NEW.status NOT IN ('in_progress', 'in_review', 'done', 'blocked')
    THEN RAISE(ABORT, 'invalid external_sessions.status value')
  END;
END;

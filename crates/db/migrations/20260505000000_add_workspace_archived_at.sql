-- Add archived_at to track when a workspace was archived.
-- Backfill: use updated_at as a conservative approximation for rows
-- that were already archived before this migration ran.
ALTER TABLE workspaces ADD COLUMN archived_at TEXT;

UPDATE workspaces
   SET archived_at = updated_at
 WHERE archived = 1;

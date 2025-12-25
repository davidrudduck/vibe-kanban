-- Remove validation_steps columns from tasks and projects tables
-- These columns were added for the "Anthropic Harness" pattern but are being removed

-- SQLite doesn't support DROP COLUMN directly, so we need to recreate the tables
-- However, we can use ALTER TABLE DROP COLUMN in SQLite 3.35+ (released 2021-03-12)
-- Let's check if we can use the simpler approach

-- Drop validation_steps from tasks
ALTER TABLE tasks DROP COLUMN validation_steps;

-- Drop default_validation_steps from projects
ALTER TABLE projects DROP COLUMN default_validation_steps;

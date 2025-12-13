-- Drop plan_steps table (feature removed)
-- The automated plan parsing and sub-task suggestion feature has been removed.
-- This migration cleans up the table that stored parsed plan steps.

DROP TABLE IF EXISTS plan_steps;

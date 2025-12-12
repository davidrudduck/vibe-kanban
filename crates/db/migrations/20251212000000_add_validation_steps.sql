-- Add validation_steps to tasks (stored as JSON array of strings)
-- This field contains user-defined validation criteria that get injected into the agent prompt
ALTER TABLE tasks ADD COLUMN validation_steps TEXT;

-- Add default_validation_steps to projects (stored as JSON array of strings)
-- This provides default validation steps for new tasks in the project
ALTER TABLE projects ADD COLUMN default_validation_steps TEXT;

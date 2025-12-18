-- Add parallel_setup_script flag to projects table
ALTER TABLE projects ADD COLUMN parallel_setup_script BOOLEAN NOT NULL DEFAULT 0;

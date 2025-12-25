-- Enable logical replication for ElectricSQL
-- This migration sets up PostgreSQL for Electric sync service

-- Create a dedicated role for Electric replication
-- Default password is 'electric_sync' - override via ELECTRIC_ROLE_PASSWORD env var in docker-compose
-- In production, change the password: ALTER ROLE electric_sync WITH PASSWORD 'secure_password';
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'electric_sync') THEN
        CREATE ROLE electric_sync WITH LOGIN REPLICATION PASSWORD 'electric_sync';
    END IF;
END $$;

-- Grant database connection permission dynamically using current database
DO $$
BEGIN
    EXECUTE format('GRANT CONNECT ON DATABASE %I TO electric_sync', current_database());
END $$;

-- Grant schema usage permission
GRANT USAGE ON SCHEMA public TO electric_sync;

-- Create publication for Electric (if not already created by init-db)
-- Electric uses PostgreSQL logical replication to capture changes
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_publication WHERE pubname = 'electric_publication_default') THEN
        CREATE PUBLICATION electric_publication_default;
    END IF;
END $$;

-- Helper function to enable a table for Electric sync
-- This function:
-- 1. Sets REPLICA IDENTITY FULL (required for updates/deletes to sync all columns)
-- 2. Grants SELECT permission to the electric_sync role
-- 3. Adds the table to the Electric publication
CREATE OR REPLACE FUNCTION electric_sync_table(p_schema text, p_table text)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    qualified text := format('%I.%I', p_schema, p_table);
BEGIN
    EXECUTE format('ALTER TABLE %s REPLICA IDENTITY FULL', qualified);
    EXECUTE format('GRANT SELECT ON TABLE %s TO electric_sync', qualified);
    EXECUTE format('ALTER PUBLICATION %I ADD TABLE %s', 'electric_publication_default', qualified);
END;
$$;

-- Enable sync for core tables
-- These tables will be replicated to nodes via Electric shapes

-- Projects table - organization workspaces
SELECT electric_sync_table('public', 'projects');

-- Shared tasks table - main task registry for the hive
SELECT electric_sync_table('public', 'shared_tasks');

-- Nodes table - connected worker nodes
SELECT electric_sync_table('public', 'nodes');

-- Node projects table - links between nodes and projects
SELECT electric_sync_table('public', 'node_projects');

-- Node task assignments table - task execution tracking
SELECT electric_sync_table('public', 'node_task_assignments');

-- Node task output logs table - execution logs
SELECT electric_sync_table('public', 'node_task_output_logs');

-- Node task progress events table - execution milestones
SELECT electric_sync_table('public', 'node_task_progress_events');

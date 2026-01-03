/**
 * Electric SQL Shape Configuration
 *
 * This module provides configuration for Electric SQL shapes used for
 * real-time sync with the backend PostgreSQL database through the
 * Electric proxy route.
 */

/**
 * Base URL for the Electric proxy endpoint.
 * The proxy handles authentication and injects organization-based WHERE clauses
 * for row-level security.
 */
export const ELECTRIC_PROXY_BASE = '/api/electric/v1/shape';

/**
 * Get the base URL for Electric shape requests.
 */
export function getElectricBaseUrl(): string {
  return ELECTRIC_PROXY_BASE;
}

/**
 * Table definitions for Electric shapes.
 * Each table enabled for Electric sync is defined here with its configuration.
 */
export interface ElectricTableConfig {
  /** The PostgreSQL table name */
  table: string;
  /** Optional columns to sync (syncs all if not specified) */
  columns?: string[];
  /** Optional description for documentation */
  description?: string;
}

/**
 * All tables enabled for Electric SQL sync.
 * These must match the tables added to the PostgreSQL publication
 * in the electric_support migration.
 */
export const ELECTRIC_SHAPE_TABLES = {
  /**
   * Connected worker nodes.
   * Filtered by organization membership on the server side.
   */
  nodes: {
    table: 'nodes',
    description: 'Worker nodes connected to the hive',
  },

  /**
   * Organization projects.
   * Filtered by organization membership on the server side.
   */
  projects: {
    table: 'projects',
    description: 'Projects within organizations',
  },

  /**
   * Node-project links.
   * Shows which projects are available on which nodes.
   */
  node_projects: {
    table: 'node_projects',
    description: 'Links between nodes and projects',
  },

  /**
   * Task assignments to nodes.
   */
  node_task_assignments: {
    table: 'node_task_assignments',
    description: 'Task execution assignments to nodes',
  },

  /**
   * Task output logs.
   */
  node_task_output_logs: {
    table: 'node_task_output_logs',
    description: 'Task execution stdout/stderr logs',
  },

  /**
   * Task progress events.
   */
  node_task_progress_events: {
    table: 'node_task_progress_events',
    description: 'Task execution progress milestones',
  },
} as const;

/**
 * Type for valid Electric shape table names.
 */
export type ElectricShapeTable = keyof typeof ELECTRIC_SHAPE_TABLES;

/**
 * Create a shape URL for a specific table.
 *
 * @param table - The table name from ELECTRIC_SHAPE_TABLES
 * @returns The full URL for the shape endpoint
 * @throws Error if table is not a valid Electric shape table
 */
export function createShapeUrl(table: ElectricShapeTable): string {
  const config = ELECTRIC_SHAPE_TABLES[table];
  if (!config) {
    throw new Error(`Unknown Electric shape table: ${table}`);
  }
  return `${ELECTRIC_PROXY_BASE}/${config.table}`;
}

/**
 * Shape options for use with @electric-sql/client ShapeStream.
 * These are passed to the ShapeStream constructor.
 */
export interface ShapeStreamOptions {
  /** The shape URL (from createShapeUrl) */
  url: string;
  /** Optional where clause for client-side filtering */
  where?: string;
  /** Optional columns to fetch */
  columns?: string[];
}

/**
 * Create ShapeStream options for a table.
 *
 * @param table - The table name from ELECTRIC_SHAPE_TABLES
 * @param options - Additional options like where clause
 * @returns Options object for ShapeStream constructor
 */
export function createShapeStreamOptions(
  table: ElectricShapeTable,
  options?: Partial<Omit<ShapeStreamOptions, 'url'>>
): ShapeStreamOptions {
  return {
    url: createShapeUrl(table),
    ...options,
  };
}

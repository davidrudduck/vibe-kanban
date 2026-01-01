/**
 * Electric SQL Collections
 *
 * This module provides TanStack DB collections backed by Electric SQL shapes.
 * Each collection syncs data from the backend PostgreSQL database in real-time.
 */

import { createCollection } from '@tanstack/react-db';
import { electricCollectionOptions } from '@tanstack/electric-db-collection';
import { createShapeUrl } from './config';

// Types from shared types (re-export for convenience)
import type { SharedTask as SharedTaskType } from 'shared/types';

/**
 * Base row type that satisfies Row<unknown> constraint.
 * All Electric types must extend this to work with TanStack DB.
 */
type ElectricRow = Record<string, unknown>;

/**
 * Node type for Electric sync.
 * Matches the PostgreSQL nodes table structure.
 */
export type ElectricNode = ElectricRow & {
  id: string;
  organization_id: string;
  name: string;
  hostname: string | null;
  os_info: string | null;
  status: string;
  last_heartbeat_at: string | null;
  public_url: string | null;
  created_at: string;
  updated_at: string;
};

/**
 * Project type for Electric sync.
 * Matches the PostgreSQL projects table structure.
 */
export type ElectricProject = ElectricRow & {
  id: string;
  organization_id: string;
  name: string;
  repo_url: string | null;
  created_at: string;
  updated_at: string;
};

/**
 * Node-Project link type for Electric sync.
 * Shows which projects are available on which nodes.
 */
export type ElectricNodeProject = ElectricRow & {
  id: string;
  node_id: string;
  project_id: string;
  local_project_id: string;
  created_at: string;
  updated_at: string;
};

/**
 * Shared task type for Electric sync.
 * This extends the base SharedTask with fields from the PostgreSQL schema
 * that may not be in the base type, plus Row compatibility.
 *
 * Note: The Electric sync returns raw PostgreSQL data, which includes:
 * - project_id (UUID from PostgreSQL) - use this instead of swarm_project_id
 * - deleted_at instead of archived_at
 */
export type ElectricSharedTask = ElectricRow &
  Omit<SharedTaskType, 'swarm_project_id'> & {
    /** PostgreSQL project_id (maps to swarm_project_id in local type) */
    project_id: string;
    /** Organization this task belongs to */
    organization_id: string;
    /** Creator user ID */
    creator_user_id: string | null;
    /** User who deleted this task */
    deleted_by_user_id: string | null;
    /** When the task was soft-deleted */
    deleted_at: string | null;
    /** When the task was shared to the hive */
    shared_at: string | null;
    /** For backwards compatibility with existing code */
    swarm_project_id?: string;
    /** Archived at (alias for deleted_at for compatibility) */
    archived_at?: string | null;
  };

/**
 * Configuration type for Electric collection options.
 * Used for type inference in tests.
 */
export interface ElectricCollectionConfig<T> {
  shapeOptions: {
    url: string;
  };
  getKey: (item: T) => string | number;
}

/**
 * Create a collection for shared tasks.
 * Syncs tasks shared from the hive to connected nodes.
 *
 * @returns TanStack DB collection for shared tasks
 */
export function createSharedTasksCollection() {
  return createCollection(
    electricCollectionOptions<ElectricSharedTask>({
      shapeOptions: {
        url: createShapeUrl('shared_tasks'),
      },
      getKey: (item) => item.id,
    })
  );
}

/**
 * Create a collection for nodes.
 * Syncs worker nodes connected to the hive.
 *
 * @returns TanStack DB collection for nodes
 */
export function createNodesCollection() {
  return createCollection(
    electricCollectionOptions<ElectricNode>({
      shapeOptions: {
        url: createShapeUrl('nodes'),
      },
      getKey: (item) => item.id,
    })
  );
}

/**
 * Create a collection for projects.
 * Syncs organization projects from the hive.
 *
 * @returns TanStack DB collection for projects
 */
export function createProjectsCollection() {
  return createCollection(
    electricCollectionOptions<ElectricProject>({
      shapeOptions: {
        url: createShapeUrl('projects'),
      },
      getKey: (item) => item.id,
    })
  );
}

/**
 * Create a collection for node-project links.
 * Shows which projects are available on which nodes.
 *
 * @returns TanStack DB collection for node-project links
 */
export function createNodeProjectsCollection() {
  return createCollection(
    electricCollectionOptions<ElectricNodeProject>({
      shapeOptions: {
        url: createShapeUrl('node_projects'),
      },
      getKey: (item) => item.id,
    })
  );
}

/**
 * Electric SQL Collections
 *
 * This module provides TanStack DB collections backed by Electric SQL shapes.
 * Each collection syncs data from the backend PostgreSQL database in real-time.
 */

import { createCollection } from '@tanstack/react-db';
import { electricCollectionOptions } from '@tanstack/electric-db-collection';
import { createShapeUrl } from './config';


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

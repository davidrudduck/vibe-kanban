/**
 * Electric SQL Integration
 *
 * This module provides real-time sync capabilities using Electric SQL
 * with TanStack DB collections. Data is synced from the hive's PostgreSQL
 * database to the frontend via HTTP shape streams.
 *
 * @example
 * ```tsx
 * import { createSharedTasksCollection, createShapeUrl } from '@/lib/electric';
 *
 * // Create a collection for shared tasks
 * const tasksCollection = createSharedTasksCollection();
 *
 * // Or use the raw shape URL
 * const url = createShapeUrl('shared_tasks');
 * ```
 */

// Configuration exports
export {
  ELECTRIC_PROXY_BASE,
  ELECTRIC_SHAPE_TABLES,
  getElectricBaseUrl,
  createShapeUrl,
  createShapeStreamOptions,
  type ElectricTableConfig,
  type ElectricShapeTable,
  type ShapeStreamOptions,
} from './config';

// Collection exports
export {
  createSharedTasksCollection,
  createNodesCollection,
  createProjectsCollection,
  createNodeProjectsCollection,
  type ElectricNode,
  type ElectricProject,
  type ElectricNodeProject,
  type ElectricSharedTask,
  type ElectricCollectionConfig,
} from './collections';

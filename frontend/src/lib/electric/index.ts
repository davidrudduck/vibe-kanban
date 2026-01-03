/**
 * Electric SQL Integration
 *
 * This module provides real-time sync capabilities using Electric SQL
 * with TanStack DB collections. Data is synced from the hive's PostgreSQL
 * database to the frontend via HTTP shape streams.
 *
 * @example
 * ```tsx
 * import { createNodesCollection, createShapeUrl } from '@/lib/electric';
 *
 * // Create a collection for nodes
 * const nodesCollection = createNodesCollection();
 *
 * // Or use the raw shape URL
 * const url = createShapeUrl('nodes');
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
  createNodesCollection,
  createProjectsCollection,
  createNodeProjectsCollection,
  type ElectricNode,
  type ElectricProject,
  type ElectricNodeProject,
  type ElectricCollectionConfig,
} from './collections';

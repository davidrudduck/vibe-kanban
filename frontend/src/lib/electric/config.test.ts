import { describe, it, expect } from 'vitest';
import {
  getElectricBaseUrl,
  createShapeUrl,
  ELECTRIC_SHAPE_TABLES,
  type ElectricShapeTable,
} from './config';

describe('Electric Config', () => {
  describe('getElectricBaseUrl', () => {
    it('returns the correct base URL for Electric proxy', () => {
      const baseUrl = getElectricBaseUrl();
      expect(baseUrl).toBe('/api/electric/v1/shape');
    });
  });

  describe('ELECTRIC_SHAPE_TABLES', () => {
    it('contains shared_tasks table definition', () => {
      expect(ELECTRIC_SHAPE_TABLES.shared_tasks).toBeDefined();
      expect(ELECTRIC_SHAPE_TABLES.shared_tasks.table).toBe('shared_tasks');
    });

    it('contains nodes table definition', () => {
      expect(ELECTRIC_SHAPE_TABLES.nodes).toBeDefined();
      expect(ELECTRIC_SHAPE_TABLES.nodes.table).toBe('nodes');
    });

    it('contains projects table definition', () => {
      expect(ELECTRIC_SHAPE_TABLES.projects).toBeDefined();
      expect(ELECTRIC_SHAPE_TABLES.projects.table).toBe('projects');
    });

    it('contains node_projects table definition', () => {
      expect(ELECTRIC_SHAPE_TABLES.node_projects).toBeDefined();
      expect(ELECTRIC_SHAPE_TABLES.node_projects.table).toBe('node_projects');
    });
  });

  describe('createShapeUrl', () => {
    it('creates URL for shared_tasks table', () => {
      const url = createShapeUrl('shared_tasks');
      expect(url).toBe('/api/electric/v1/shape/shared_tasks');
    });

    it('creates URL for nodes table', () => {
      const url = createShapeUrl('nodes');
      expect(url).toBe('/api/electric/v1/shape/nodes');
    });

    it('creates URL for projects table', () => {
      const url = createShapeUrl('projects');
      expect(url).toBe('/api/electric/v1/shape/projects');
    });

    it('creates URL for node_projects table', () => {
      const url = createShapeUrl('node_projects');
      expect(url).toBe('/api/electric/v1/shape/node_projects');
    });

    it('throws error for invalid table name', () => {
      expect(() => createShapeUrl('invalid_table' as ElectricShapeTable)).toThrow(
        'Unknown Electric shape table: invalid_table'
      );
    });
  });
});

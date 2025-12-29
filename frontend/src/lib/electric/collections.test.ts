import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the TanStack DB modules before importing collections
vi.mock('@tanstack/react-db', () => ({
  createCollection: vi.fn((config) => ({
    ...config,
    _isMockCollection: true,
  })),
}));

vi.mock('@tanstack/electric-db-collection', () => ({
  electricCollectionOptions: vi.fn((config) => ({
    ...config,
    _isElectricConfig: true,
  })),
}));

// Import after mocks are set up
import {
  createSharedTasksCollection,
  createNodesCollection,
  createProjectsCollection,
  createNodeProjectsCollection,
  type ElectricCollectionConfig,
} from './collections';
import { createCollection } from '@tanstack/react-db';
import { electricCollectionOptions } from '@tanstack/electric-db-collection';

describe('Electric Collections', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('createSharedTasksCollection', () => {
    it('creates a collection with correct shape URL', () => {
      const collection = createSharedTasksCollection();

      expect(electricCollectionOptions).toHaveBeenCalledWith(
        expect.objectContaining({
          shapeOptions: expect.objectContaining({
            url: '/api/electric/v1/shape/shared_tasks',
          }),
        })
      );
      expect(createCollection).toHaveBeenCalled();
      expect(collection).toBeDefined();
    });

    it('uses id as the key extractor', () => {
      createSharedTasksCollection();

      const config = (electricCollectionOptions as ReturnType<typeof vi.fn>)
        .mock.calls[0][0] as ElectricCollectionConfig<{ id: string }>;
      expect(config.getKey({ id: 'test-uuid' })).toBe('test-uuid');
    });
  });

  describe('createNodesCollection', () => {
    it('creates a collection with correct shape URL', () => {
      const collection = createNodesCollection();

      expect(electricCollectionOptions).toHaveBeenCalledWith(
        expect.objectContaining({
          shapeOptions: expect.objectContaining({
            url: '/api/electric/v1/shape/nodes',
          }),
        })
      );
      expect(createCollection).toHaveBeenCalled();
      expect(collection).toBeDefined();
    });

    it('uses id as the key extractor', () => {
      createNodesCollection();

      const config = (electricCollectionOptions as ReturnType<typeof vi.fn>)
        .mock.calls[0][0] as ElectricCollectionConfig<{ id: string }>;
      expect(config.getKey({ id: 'node-uuid' })).toBe('node-uuid');
    });
  });

  describe('createProjectsCollection', () => {
    it('creates a collection with correct shape URL', () => {
      const collection = createProjectsCollection();

      expect(electricCollectionOptions).toHaveBeenCalledWith(
        expect.objectContaining({
          shapeOptions: expect.objectContaining({
            url: '/api/electric/v1/shape/projects',
          }),
        })
      );
      expect(createCollection).toHaveBeenCalled();
      expect(collection).toBeDefined();
    });

    it('uses id as the key extractor', () => {
      createProjectsCollection();

      const config = (electricCollectionOptions as ReturnType<typeof vi.fn>)
        .mock.calls[0][0] as ElectricCollectionConfig<{ id: string }>;
      expect(config.getKey({ id: 'project-uuid' })).toBe('project-uuid');
    });
  });

  describe('createNodeProjectsCollection', () => {
    it('creates a collection with correct shape URL', () => {
      const collection = createNodeProjectsCollection();

      expect(electricCollectionOptions).toHaveBeenCalledWith(
        expect.objectContaining({
          shapeOptions: expect.objectContaining({
            url: '/api/electric/v1/shape/node_projects',
          }),
        })
      );
      expect(createCollection).toHaveBeenCalled();
      expect(collection).toBeDefined();
    });

    it('uses id as the key extractor', () => {
      createNodeProjectsCollection();

      const config = (electricCollectionOptions as ReturnType<typeof vi.fn>)
        .mock.calls[0][0] as ElectricCollectionConfig<{ id: string }>;
      expect(config.getKey({ id: 'node-project-uuid' })).toBe(
        'node-project-uuid'
      );
    });
  });
});

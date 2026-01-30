import { useCallback, useState } from 'react';
import { projectsApi } from '@/lib/api/projects';
import { useProjectMutations } from './useProjectMutations';
import type { UnlinkSwarmRequest } from 'shared/types';

export interface FixAllResult {
  successCount: number;
  errorCount: number;
  errors: Array<{ projectId: string; projectName: string; error: unknown }>;
}

interface UseSwarmHealthActionsOptions {
  onFixAllSuccess?: (result: FixAllResult) => void;
  onFixAllError?: (error: unknown) => void;
  onFixAllPartial?: (result: FixAllResult) => void;
}

export function useSwarmHealthActions(options?: UseSwarmHealthActionsOptions) {
  const [isFixing, setIsFixing] = useState(false);
  const { unlinkFromSwarm } = useProjectMutations();

  const fixAllIssues = useCallback(async (): Promise<FixAllResult> => {
    setIsFixing(true);
    const result: FixAllResult = {
      successCount: 0,
      errorCount: 0,
      errors: [],
    };

    try {
      // Fetch all projects using API client
      const projects = await projectsApi.getAll();

      // Find projects with sync issues
      for (const project of projects) {
        try {
          const health = await projectsApi.getSyncHealth(project.id);

          if (health.has_sync_issues) {
            try {
              const unlinkRequest: UnlinkSwarmRequest = { notify_hive: false };
              await unlinkFromSwarm.mutateAsync({
                projectId: project.id,
                data: unlinkRequest,
              });
              result.successCount++;
            } catch (err) {
              console.error(`Failed to unlink project ${project.name}:`, err);
              result.errorCount++;
              result.errors.push({
                projectId: project.id,
                projectName: project.name,
                error: err,
              });
            }
          }
        } catch (err) {
          // Failed to get sync health for this project - log and continue
          console.error(
            `Failed to get sync health for project ${project.name}:`,
            err
          );
        }
      }

      // Call appropriate callback based on result
      if (result.errorCount === 0) {
        options?.onFixAllSuccess?.(result);
      } else {
        options?.onFixAllPartial?.(result);
      }

      return result;
    } catch (error) {
      console.error('Failed to fix swarm issues:', error);
      options?.onFixAllError?.(error);
      throw error;
    } finally {
      setIsFixing(false);
    }
  }, [options, unlinkFromSwarm]);

  return {
    fixAllIssues,
    isFixing,
  };
}

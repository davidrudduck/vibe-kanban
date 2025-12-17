import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { MergedProject } from 'shared/types';
import { ProjectFormDialog } from '@/components/dialogs/projects/ProjectFormDialog';
import { Loader2, Plus } from 'lucide-react';
import UnifiedProjectCard from '@/components/projects/UnifiedProjectCard.tsx';
import ProjectSortControls, {
  SortOption,
  loadSortOption,
} from '@/components/projects/ProjectSortControls';
import { useKeyCreate, Scope } from '@/keyboard';
import { useMergedProjects } from '@/hooks/useMergedProjects';

function sortProjects(
  projects: MergedProject[],
  sortOption: SortOption
): MergedProject[] {
  return [...projects].sort((a, b) => {
    switch (sortOption) {
      case 'name_asc':
        return a.name.localeCompare(b.name);
      case 'name_desc':
        return b.name.localeCompare(a.name);
      case 'recent_activity':
        // null last_attempt_at goes to bottom
        if (!a.last_attempt_at && !b.last_attempt_at) return 0;
        if (!a.last_attempt_at) return 1;
        if (!b.last_attempt_at) return -1;
        return (
          new Date(b.last_attempt_at).getTime() -
          new Date(a.last_attempt_at).getTime()
        );
      case 'oldest_activity':
        // null last_attempt_at goes to bottom
        if (!a.last_attempt_at && !b.last_attempt_at) return 0;
        if (!a.last_attempt_at) return 1;
        if (!b.last_attempt_at) return -1;
        return (
          new Date(a.last_attempt_at).getTime() -
          new Date(b.last_attempt_at).getTime()
        );
      default:
        return 0;
    }
  });
}

export function ProjectList() {
  const navigate = useNavigate();
  const { t } = useTranslation('projects');
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const [sortOption, setSortOption] = useState<SortOption>(loadSortOption);

  // Fetch merged projects (includes local and remote)
  const {
    data: mergedData,
    isLoading,
    refetch: refetchMerged,
  } = useMergedProjects();

  const projects = mergedData?.projects ?? [];

  const sortedProjects = useMemo(() => {
    return sortProjects(projects, sortOption);
  }, [projects, sortOption]);

  const handleCreateProject = async () => {
    try {
      const result = await ProjectFormDialog.show({});
      if (result === 'saved') {
        refetchMerged();
      }
    } catch {
      // User cancelled - do nothing
    }
  };

  // Semantic keyboard shortcut for creating new project
  useKeyCreate(handleCreateProject, { scope: Scope.PROJECTS });

  const handleEditProject = useCallback(
    (project: MergedProject) => {
      if (project.has_local && project.local_project_id) {
        navigate(`/settings/projects?projectId=${project.local_project_id}`);
      }
    },
    [navigate]
  );

  // Set initial focus when projects are loaded
  useEffect(() => {
    if (sortedProjects.length > 0 && !focusedProjectId) {
      setFocusedProjectId(sortedProjects[0].id);
    }
  }, [sortedProjects, focusedProjectId]);

  const hasProjects = sortedProjects.length > 0;

  return (
    <div className="space-y-6 p-8 pb-16 md:pb-8 h-full overflow-auto">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">{t('title')}</h1>
          <p className="text-muted-foreground">{t('subtitle')}</p>
        </div>
        <Button onClick={handleCreateProject}>
          <Plus className="mr-2 h-4 w-4" />
          {t('createProject')}
        </Button>
      </div>

      {isLoading ? (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          {t('loading')}
        </div>
      ) : !hasProjects ? (
        <Card>
          <CardContent className="py-12 text-center">
            <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-muted">
              <Plus className="h-6 w-6" />
            </div>
            <h3 className="mt-4 text-lg font-semibold">{t('empty.title')}</h3>
            <p className="mt-2 text-sm text-muted-foreground">
              {t('empty.description')}
            </p>
            <Button className="mt-4" onClick={handleCreateProject}>
              <Plus className="mr-2 h-4 w-4" />
              {t('empty.createFirst')}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-4">
          {/* Header with count and sorting controls */}
          <div className="flex justify-between items-center">
            <h2 className="text-lg font-semibold flex items-center gap-2">
              {t('sections.allProjects')}
              <span className="text-sm font-normal text-muted-foreground">
                ({sortedProjects.length})
              </span>
            </h2>
            <ProjectSortControls value={sortOption} onChange={setSortOption} />
          </div>

          {/* Unified project grid */}
          <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
            {sortedProjects.map((project) => (
              <UnifiedProjectCard
                key={project.id}
                project={project}
                isFocused={focusedProjectId === project.id}
                onRefresh={refetchMerged}
                onEdit={handleEditProject}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

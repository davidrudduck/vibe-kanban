import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { motion, AnimatePresence } from 'framer-motion';

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
import ProjectTypeFilterTabs, {
  ProjectTypeFilter,
} from '@/components/projects/ProjectTypeFilter';
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

function filterProjects(
  projects: MergedProject[],
  filter: ProjectTypeFilter
): MergedProject[] {
  switch (filter) {
    case 'local':
      return projects.filter((p) => p.has_local);
    case 'swarm':
      return projects.filter((p) => p.nodes.length > 0);
    case 'all':
    default:
      return projects;
  }
}

export function ProjectList() {
  const navigate = useNavigate();
  const { t } = useTranslation('projects');
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const [sortOption, setSortOption] = useState<SortOption>(loadSortOption);
  const [typeFilter, setTypeFilter] = useState<ProjectTypeFilter>('all');

  // Fetch merged projects (includes local and remote)
  const {
    data: mergedData,
    isLoading,
    refetch: refetchMerged,
  } = useMergedProjects();

  // Calculate counts for filter tabs
  const counts = useMemo(() => {
    const projects = mergedData?.projects ?? [];
    return {
      total: projects.length,
      local: projects.filter((p) => p.has_local).length,
      swarm: projects.filter((p) => p.nodes.length > 0).length,
    };
  }, [mergedData?.projects]);

  // Calculate total node count for subtitle
  const nodeCount = useMemo(() => {
    const projects = mergedData?.projects ?? [];
    const uniqueNodeIds = new Set<string>();
    projects.forEach((p) => {
      p.nodes.forEach((n) => uniqueNodeIds.add(n.node_id));
    });
    return uniqueNodeIds.size;
  }, [mergedData?.projects]);

  const sortedProjects = useMemo(() => {
    const projects = mergedData?.projects ?? [];
    const filtered = filterProjects(projects, typeFilter);
    return sortProjects(filtered, sortOption);
  }, [mergedData?.projects, sortOption, typeFilter]);

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

  const hasProjects = counts.total > 0;
  const hasFilteredProjects = sortedProjects.length > 0;

  return (
    <div className="h-full flex flex-col">
      {/* Page Header - Fixed top section */}
      <div className="px-4 py-6 sm:px-6 lg:px-8 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              {t('title')}
            </h1>
            <p className="text-muted-foreground text-sm mt-0.5">
              {t('subtitle')}
              {nodeCount > 0 && (
                <span className="ml-1 text-muted-foreground/70">
                  {' '}
                  • {nodeCount} node{nodeCount !== 1 ? 's' : ''}
                </span>
              )}
            </p>
          </div>
          <Button onClick={handleCreateProject} className="shrink-0">
            <Plus className="mr-2 h-4 w-4" />
            {t('createProject')}
          </Button>
        </div>

        {/* Filter/Sort Bar */}
        {hasProjects && (
          <div className="flex flex-wrap gap-3 mt-4 items-center justify-between">
            <ProjectTypeFilterTabs
              value={typeFilter}
              onChange={setTypeFilter}
              counts={counts}
            />
            <ProjectSortControls value={sortOption} onChange={setSortOption} />
          </div>
        )}
      </div>

      {/* Content Area - Scrollable */}
      <div className="flex-1 overflow-auto p-4 sm:p-6 lg:p-8 pb-20 sm:pb-8">
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            {t('loading')}
          </div>
        ) : !hasProjects ? (
          <Card className="max-w-md mx-auto mt-8">
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
        ) : !hasFilteredProjects ? (
          <div className="text-center py-12 text-muted-foreground">
            No projects match the current filter
          </div>
        ) : (
          /* Project Grid - Responsive 1-4 columns with staggered entrance */
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            <AnimatePresence mode="popLayout">
              {sortedProjects.map((project, index) => (
                <motion.div
                  key={project.id}
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, scale: 0.95 }}
                  transition={{
                    duration: 0.2,
                    delay: Math.min(index * 0.03, 0.3), // Cap delay at 0.3s
                    ease: 'easeOut',
                  }}
                  layout
                >
                  <UnifiedProjectCard
                    project={project}
                    isFocused={focusedProjectId === project.id}
                    onRefresh={refetchMerged}
                    onEdit={handleEditProject}
                  />
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        )}
      </div>
    </div>
  );
}

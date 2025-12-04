import { useParams, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Circle, Loader2, Plus, Server, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { useRemoteProjectTasks } from '@/hooks/useRemoteProjectTasks';
import type { HiveSharedTaskWithUser } from '@/types/remote';
import type { TaskStatus } from 'shared/types';
import { statusLabels, statusBoardColors } from '@/utils/statusLabels';
import { cn } from '@/lib/utils';

const TASK_STATUS_ORDER: TaskStatus[] = ['todo', 'inprogress', 'inreview', 'done', 'cancelled'];

function getStatusColor(status: string): string {
  switch (status) {
    case 'online':
      return 'text-green-500';
    case 'busy':
      return 'text-yellow-500';
    case 'offline':
      return 'text-gray-400';
    case 'draining':
      return 'text-orange-500';
    case 'pending':
    default:
      return 'text-gray-300';
  }
}

function RemoteTaskCard({ taskWithUser }: { taskWithUser: HiveSharedTaskWithUser }) {
  const { task, user } = taskWithUser;

  return (
    <Card className="hover:shadow-md transition-shadow">
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between">
          <CardTitle className="text-base font-medium line-clamp-2">
            {task.title}
          </CardTitle>
          <Badge
            variant="outline"
            className={cn(
              'text-xs shrink-0',
              statusBoardColors[task.status as TaskStatus] || 'bg-gray-100'
            )}
          >
            {statusLabels[task.status as TaskStatus] || task.status}
          </Badge>
        </div>
        {task.description && (
          <CardDescription className="line-clamp-2 text-sm">
            {task.description}
          </CardDescription>
        )}
      </CardHeader>
      <CardContent className="pt-0">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>
            {user
              ? `${user.first_name || ''} ${user.last_name || ''}`.trim() ||
                user.username ||
                'Assigned'
              : 'Unassigned'}
          </span>
          <span>{new Date(task.updated_at).toLocaleDateString()}</span>
        </div>
      </CardContent>
    </Card>
  );
}

function TaskColumn({
  status,
  tasks,
}: {
  status: TaskStatus;
  tasks: HiveSharedTaskWithUser[];
}) {
  if (tasks.length === 0) return null;

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <h3 className="font-medium text-sm">{statusLabels[status]}</h3>
        <span className="text-xs text-muted-foreground">({tasks.length})</span>
      </div>
      <div className="space-y-2">
        {tasks.map((taskWithUser) => (
          <RemoteTaskCard key={taskWithUser.task.id} taskWithUser={taskWithUser} />
        ))}
      </div>
    </div>
  );
}

export function RemoteProjectTasks() {
  const { projectId } = useParams<{ projectId: string }>();
  const navigate = useNavigate();
  const { t } = useTranslation(['projects', 'tasks', 'common']);

  const {
    projectInfo,
    projectInfoLoading,
    projectInfoError,
    tasks,
    groupedTasks,
    tasksLoading,
    tasksError,
    createTask,
  } = useRemoteProjectTasks(projectId);

  const isLoading = projectInfoLoading || tasksLoading;
  const error = projectInfoError || tasksError;

  const handleBack = () => {
    navigate('/projects');
  };

  const handleCreateTask = async () => {
    const title = window.prompt('Enter task title:');
    if (!title) return;

    try {
      await createTask.mutateAsync({
        title,
        description: null,
        assignee_user_id: null,
      });
    } catch (err) {
      console.error('Failed to create task:', err);
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-5 w-5 animate-spin" />
          <span>Loading remote project...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-8">
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Error</AlertTitle>
          <AlertDescription>
            {error instanceof Error ? error.message : 'Failed to load remote project'}
          </AlertDescription>
        </Alert>
        <Button variant="outline" className="mt-4" onClick={handleBack}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to Projects
        </Button>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto">
      <div className="p-6 space-y-6">
        {/* Header */}
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <Button variant="ghost" size="sm" onClick={handleBack}>
                <ArrowLeft className="h-4 w-4" />
              </Button>
              <h1 className="text-2xl font-bold tracking-tight">
                {projectInfo?.project_name || 'Remote Project'}
              </h1>
            </div>
            {projectInfo && (
              <div className="flex items-center gap-2 ml-10 text-sm text-muted-foreground">
                <Badge variant="secondary" className="gap-1">
                  <Server className="h-3 w-3" />
                  {projectInfo.node_name}
                  <Circle
                    className={`h-2 w-2 fill-current ${getStatusColor(projectInfo.node_status)}`}
                  />
                </Badge>
                <span className="text-xs">{projectInfo.git_repo_path}</span>
              </div>
            )}
          </div>
          <Button onClick={handleCreateTask} disabled={createTask.isPending}>
            {createTask.isPending ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <Plus className="mr-2 h-4 w-4" />
            )}
            {t('tasks:createTask', 'Create Task')}
          </Button>
        </div>

        {/* Info Banner */}
        <Alert>
          <Server className="h-4 w-4" />
          <AlertTitle>Remote Project</AlertTitle>
          <AlertDescription>
            This project is hosted on <strong>{projectInfo?.node_name}</strong>.
            Only <strong>shared tasks</strong> are visible here. Local tasks on the remote node
            must be shared to the Hive before they appear.
          </AlertDescription>
        </Alert>

        {/* Tasks */}
        {tasks.length === 0 ? (
          <Card>
            <CardContent className="py-12 text-center">
              <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-muted">
                <Plus className="h-6 w-6" />
              </div>
              <h3 className="mt-4 text-lg font-semibold">
                No shared tasks
              </h3>
              <p className="mt-2 text-sm text-muted-foreground max-w-md mx-auto">
                Tasks from the remote node must be shared to the Hive before they appear here.
                You can create a new shared task below, or share existing tasks from the source node.
              </p>
              <Button className="mt-4" onClick={handleCreateTask}>
                <Plus className="mr-2 h-4 w-4" />
                Create Shared Task
              </Button>
            </CardContent>
          </Card>
        ) : (
          <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
            {TASK_STATUS_ORDER.map((status) => (
              <TaskColumn
                key={status}
                status={status}
                tasks={groupedTasks?.[status] || []}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

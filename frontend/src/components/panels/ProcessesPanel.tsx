import ProcessesTab from '@/components/tasks/TaskDetails/ProcessesTab';
import { ProcessSelectionProvider } from '@/contexts/ProcessSelectionContext';

interface ProcessesPanelProps {
  attemptId?: string;
}

/**
 * ProcessesPanel - Wrapper for ProcessesTab that provides necessary context
 * for viewing execution processes in the developer tools panel area.
 *
 * Used in the aux panel when mode='processes' is selected.
 */
export function ProcessesPanel({ attemptId }: ProcessesPanelProps) {
  return (
    <div className="h-full flex flex-col bg-background">
      <ProcessSelectionProvider>
        <ProcessesTab attemptId={attemptId} />
      </ProcessSelectionProvider>
    </div>
  );
}

export default ProcessesPanel;

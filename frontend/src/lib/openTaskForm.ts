import { TaskFormSheet } from '@/components/dialogs/tasks/TaskFormSheet';
import type { TaskFormSheetProps } from '@/components/dialogs/tasks/TaskFormSheet';

/**
 * TaskFormDialogProps - Alias for backward compatibility
 * @deprecated Use TaskFormSheetProps instead
 */
export type TaskFormDialogProps = TaskFormSheetProps;

/**
 * Open the task form dialog programmatically
 * On mobile (<768px): renders as full-screen sheet with swipe-to-dismiss
 * On tablet/desktop (>=768px): renders as centered modal dialog
 */
export function openTaskForm(props: TaskFormSheetProps) {
  return TaskFormSheet.show(props);
}

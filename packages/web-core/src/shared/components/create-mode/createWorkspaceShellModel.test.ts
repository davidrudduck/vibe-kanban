import { describe, expect, it } from 'vitest';
import {
  getCreateWorkspaceModeState,
  getWorkspaceNameForSubmit,
  canSaveWorkspaceDraft,
} from './createWorkspaceShellModel';

describe('createWorkspaceShellModel', () => {
  it('locks to linked task mode when a kanban issue seeded the workflow', () => {
    expect(
      getCreateWorkspaceModeState({
        linkedIssue: {
          issueId: 'issue-1',
          simpleId: 'DAV-1',
          title: 'Context-Engine Cache Improvements',
          remoteProjectId: 'project-1',
        },
      })
    ).toEqual({
      initialMode: 'link_task',
      lockedToLinkedIssue: true,
    });
  });

  it('uses linked issue title as the workspace name before falling back to prompt title', () => {
    expect(
      getWorkspaceNameForSubmit({
        workspaceName: '',
        linkedIssueTitle: 'Context-Engine Cache Improvements',
        message: 'Implement the cache work\n\nDetails...',
      })
    ).toBe('Context-Engine Cache Improvements');
  });

  it('prevents blank saved drafts', () => {
    expect(
      canSaveWorkspaceDraft({
        workspaceName: '',
        message: '',
        linkedIssueTitle: undefined,
      })
    ).toBe(false);
  });
});

import { describe, expect, it } from 'vitest';
import {
  getCreateWorkspaceModeState,
  getEffectiveLinkedIssue,
  getSourceLinkedIssue,
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

  it('prefers the locked kanban issue over stale draft linked issue data', () => {
    expect(
      getSourceLinkedIssue({
        lockedLinkedIssue: {
          issueId: 'route-issue',
          simpleId: 'DAV-2',
          title: 'Route Issue',
          remoteProjectId: 'project-1',
        },
        draftLinkedIssue: {
          issueId: 'draft-issue',
          simpleId: 'DAV-1',
          title: 'Draft Issue',
          remoteProjectId: 'project-1',
        },
      })
    ).toMatchObject({
      issueId: 'route-issue',
      title: 'Route Issue',
    });
  });

  it('uses the hydrated locked issue over the initial manual linked issue snapshot', () => {
    expect(
      getEffectiveLinkedIssue({
        lockedToLinkedIssue: true,
        mode: 'link_task',
        sourceLinkedIssue: {
          issueId: 'route-issue',
          simpleId: 'DAV-2',
          title: 'Hydrated Route Issue',
          remoteProjectId: 'project-1',
        },
        manualLinkedIssue: {
          issueId: 'route-issue',
          remoteProjectId: 'project-1',
        },
      })
    ).toMatchObject({
      issueId: 'route-issue',
      title: 'Hydrated Route Issue',
    });
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

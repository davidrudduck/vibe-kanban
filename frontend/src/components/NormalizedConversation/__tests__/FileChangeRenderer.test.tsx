import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import FileChangeRenderer from '../FileChangeRenderer';

// Mock the useFileViewer hook
const mockOpenFile = vi.fn();
vi.mock('@/contexts/FileViewerContext', () => ({
  useFileViewer: () => ({ openFile: mockOpenFile }),
}));

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock ConfigProvider
vi.mock('@/components/ConfigProvider', () => ({
  useUserSystem: () => ({
    config: {
      theme: 'light',
    },
  }),
}));

// Mock useExpandable
vi.mock('@/stores/useExpandableStore', () => ({
  useExpandable: () => [false, vi.fn()],
}));

describe('FileChangeRenderer file preview routing', () => {
  beforeEach(() => {
    mockOpenFile.mockClear();
  });

  it('should use attemptId for .claude/ paths when attemptId is present', () => {
    render(
      <FileChangeRenderer
        path=".claude/tasks/quiet-coalescing-lagoon/001.md"
        change={{ action: 'write', content: '# Test' }}
        expansionKey="test-key"
        attemptId="test-attempt-123"
      />
    );

    const viewButton = screen.getByRole('button', { name: /view file/i });
    fireEvent.click(viewButton);

    expect(mockOpenFile).toHaveBeenCalledWith({
      path: '.claude/tasks/quiet-coalescing-lagoon/001.md',
      attemptId: 'test-attempt-123',
    });
    // Should NOT use relativePath
    expect(mockOpenFile).not.toHaveBeenCalledWith(
      expect.objectContaining({ relativePath: expect.any(String) })
    );
  });

  it('should use relativePath for .claude/ paths when attemptId is not present', () => {
    render(
      <FileChangeRenderer
        path=".claude/tasks/001.md"
        change={{ action: 'write', content: '# Test' }}
        expansionKey="test-key"
        // No attemptId
      />
    );

    const viewButton = screen.getByRole('button', { name: /view file/i });
    fireEvent.click(viewButton);

    expect(mockOpenFile).toHaveBeenCalledWith({
      path: '.claude/tasks/001.md',
      relativePath: 'tasks/001.md',
    });
  });

  it('should use attemptId for regular markdown paths when attemptId is present', () => {
    render(
      <FileChangeRenderer
        path="docs/README.md"
        change={{ action: 'write', content: '# Test' }}
        expansionKey="test-key"
        attemptId="test-attempt-123"
      />
    );

    const viewButton = screen.getByRole('button', { name: /view file/i });
    fireEvent.click(viewButton);

    expect(mockOpenFile).toHaveBeenCalledWith({
      path: 'docs/README.md',
      attemptId: 'test-attempt-123',
    });
  });
});

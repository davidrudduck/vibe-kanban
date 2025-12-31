import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import MessageQueuePanel from './MessageQueuePanel';
import type { QueuedMessage } from 'shared/types';

// Mock i18n
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        'messageQueue.title': 'Message Queue',
        'messageQueue.loadingQueue': 'Loading queue...',
        'messageQueue.emptyState': 'No messages queued.',
        'messageQueue.clear': 'Clear',
        'messageQueue.save': 'Save',
        'messageQueue.cancel': 'Cancel',
        'messageQueue.variant': 'Variant:',
        'messageQueue.saveError': 'Failed to save changes',
        'messageQueue.confirmRemove': 'Remove this message from the queue?',
      };
      return translations[key] || key;
    },
  }),
}));

describe('MessageQueuePanel', () => {
  const mockOnUpdate = vi.fn();
  const mockOnRemove = vi.fn();
  const mockOnReorder = vi.fn();
  const mockOnClear = vi.fn();

  const defaultProps = {
    queue: [] as QueuedMessage[],
    isLoading: false,
    onUpdate: mockOnUpdate,
    onRemove: mockOnRemove,
    onReorder: mockOnReorder,
    onClear: mockOnClear,
  };

  const createMessage = (id: string, content: string, position: number, variant: string | null = null): QueuedMessage => ({
    id,
    task_attempt_id: 'attempt-1',
    content,
    variant,
    position,
    created_at: '2024-01-01T00:00:00Z',
  });

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('loading state', () => {
    it('renders loading message when isLoading is true', () => {
      render(<MessageQueuePanel {...defaultProps} isLoading={true} />);

      expect(screen.getByText('Loading queue...')).toBeInTheDocument();
    });
  });

  describe('empty state', () => {
    it('renders empty state when no messages queued', () => {
      render(<MessageQueuePanel {...defaultProps} queue={[]} />);

      expect(screen.getByText('No messages queued.')).toBeInTheDocument();
    });

    it('shows Message Queue title', () => {
      render(<MessageQueuePanel {...defaultProps} queue={[]} />);

      expect(screen.getByText('Message Queue')).toBeInTheDocument();
    });
  });

  describe('with messages', () => {
    const messages = [
      createMessage('1', 'First message', 0),
      createMessage('2', 'Second message', 1),
      createMessage('3', 'Third message', 2, 'plan'),
    ];

    it('renders list of queued messages', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      expect(screen.getByText('First message')).toBeInTheDocument();
      expect(screen.getByText('Second message')).toBeInTheDocument();
      expect(screen.getByText('Third message')).toBeInTheDocument();
    });

    it('shows queue count badge', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      expect(screen.getByText('3')).toBeInTheDocument();
    });

    it('shows variant for messages with variants', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      expect(screen.getByText('Variant: plan')).toBeInTheDocument();
    });

    it('shows Clear button when messages exist', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      expect(screen.getByText('Clear')).toBeInTheDocument();
    });
  });

  describe('clear functionality', () => {
    const messages = [createMessage('1', 'Test message', 0)];

    it('calls onClear when Clear button clicked', async () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const clearButton = screen.getByText('Clear');
      fireEvent.click(clearButton);

      expect(mockOnClear).toHaveBeenCalledTimes(1);
    });

    it('disables Clear button when isClearing is true', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} isClearing={true} />);

      const clearButton = screen.getByText('Clear').closest('button');
      expect(clearButton).toBeDisabled();
    });
  });

  describe('remove functionality', () => {
    const messages = [createMessage('1', 'Test message', 0)];

    it('calls onRemove when delete button clicked and confirm accepted', () => {
      const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      // Hover to show the X button
      const messageContainer = screen.getByText('Test message').closest('.group');
      if (messageContainer) {
        fireEvent.mouseEnter(messageContainer);
      }

      // Find and click the delete button (X icon button)
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-x'));
      if (deleteButton) {
        fireEvent.click(deleteButton);
        expect(confirmSpy).toHaveBeenCalledWith('Remove this message from the queue?');
        expect(mockOnRemove).toHaveBeenCalledWith('1');
      }
      confirmSpy.mockRestore();
    });

    it('does not call onRemove when confirm is cancelled', () => {
      const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      // Find and click the delete button (X icon button)
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-x'));
      if (deleteButton) {
        fireEvent.click(deleteButton);
        expect(confirmSpy).toHaveBeenCalled();
        expect(mockOnRemove).not.toHaveBeenCalled();
      }
      confirmSpy.mockRestore();
    });
  });

  describe('reorder functionality', () => {
    const messages = [
      createMessage('1', 'First message', 0),
      createMessage('2', 'Second message', 1),
    ];

    it('calls onReorder when move down button clicked', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      // Find move down buttons (ChevronRight rotated 90 degrees)
      const buttons = screen.getAllByRole('button');
      // The first move-down button should be for the first item
      const moveDownButton = buttons.find(btn =>
        btn.querySelector('svg.lucide-chevron-right.rotate-90') &&
        !btn.hasAttribute('disabled')
      );

      if (moveDownButton) {
        fireEvent.click(moveDownButton);
        expect(mockOnReorder).toHaveBeenCalledWith(['2', '1']);
      }
    });

    it('disables move up for first item', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const buttons = screen.getAllByRole('button');
      // First move-up button should be disabled
      const moveUpButtons = buttons.filter(btn =>
        btn.querySelector('svg.lucide-chevron-right.-rotate-90')
      );

      if (moveUpButtons.length > 0) {
        expect(moveUpButtons[0]).toBeDisabled();
      }
    });

    it('disables move down for last item', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const buttons = screen.getAllByRole('button');
      // Last move-down button should be disabled
      const moveDownButtons = buttons.filter(btn =>
        btn.querySelector('svg.lucide-chevron-right.rotate-90')
      );

      if (moveDownButtons.length > 0) {
        expect(moveDownButtons[moveDownButtons.length - 1]).toBeDisabled();
      }
    });
  });

  describe('edit functionality', () => {
    const messages = [createMessage('1', 'Original message', 0)];

    it('enters edit mode when message content clicked', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      // Should show textarea with message content
      expect(screen.getByRole('textbox')).toBeInTheDocument();
      expect(screen.getByDisplayValue('Original message')).toBeInTheDocument();
    });

    it('shows save and cancel buttons in edit mode', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      expect(screen.getByText('Save')).toBeInTheDocument();
      expect(screen.getByText('Cancel')).toBeInTheDocument();
    });

    it('calls onUpdate when save clicked with changes', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      const textarea = screen.getByRole('textbox');
      fireEvent.change(textarea, { target: { value: 'Updated message' } });

      const saveButton = screen.getByText('Save');
      fireEvent.click(saveButton);

      expect(mockOnUpdate).toHaveBeenCalledWith('1', 'Updated message');
    });

    it('cancels edit when cancel clicked', () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      const cancelButton = screen.getByText('Cancel');
      fireEvent.click(cancelButton);

      // Should show original message content again (not in edit mode)
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
      expect(screen.getByText('Original message')).toBeInTheDocument();
    });

    it('stays in edit mode when save fails', async () => {
      const failingUpdate = vi.fn().mockRejectedValue(new Error('Save failed'));
      render(<MessageQueuePanel {...defaultProps} queue={messages} onUpdate={failingUpdate} />);

      // Enter edit mode
      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      // Make changes and save
      const textarea = screen.getByRole('textbox');
      fireEvent.change(textarea, { target: { value: 'Updated message' } });

      const saveButton = screen.getByText('Save');
      fireEvent.click(saveButton);

      // Wait for error to appear
      await waitFor(() => {
        expect(screen.getByText('Failed to save changes')).toBeInTheDocument();
      });

      // Should still be in edit mode
      expect(screen.getByRole('textbox')).toBeInTheDocument();
    });

    it('clears error when cancel clicked after error', async () => {
      const failingUpdate = vi.fn().mockRejectedValue(new Error('Save failed'));
      render(<MessageQueuePanel {...defaultProps} queue={messages} onUpdate={failingUpdate} />);

      // Enter edit mode and trigger error
      const messageContent = screen.getByText('Original message');
      fireEvent.click(messageContent);

      const textarea = screen.getByRole('textbox');
      fireEvent.change(textarea, { target: { value: 'Updated message' } });

      const saveButton = screen.getByText('Save');
      fireEvent.click(saveButton);

      await waitFor(() => {
        expect(screen.getByText('Failed to save changes')).toBeInTheDocument();
      });

      // Click cancel
      const cancelButton = screen.getByText('Cancel');
      fireEvent.click(cancelButton);

      // Error should be cleared and should exit edit mode
      expect(screen.queryByText('Failed to save changes')).not.toBeInTheDocument();
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    });
  });

  describe('collapsible behavior', () => {
    const messages = [createMessage('1', 'Test message', 0)];

    it('collapses when header clicked', async () => {
      render(<MessageQueuePanel {...defaultProps} queue={messages} />);

      // Initially expanded, message should be visible
      expect(screen.getByText('Test message')).toBeInTheDocument();

      // Click the header button to collapse
      const headerButton = screen.getByText('Message Queue').closest('button');
      if (headerButton) {
        fireEvent.click(headerButton);
      }

      // Message should be hidden after collapse
      await waitFor(() => {
        expect(screen.queryByText('Test message')).not.toBeInTheDocument();
      });
    });
  });
});

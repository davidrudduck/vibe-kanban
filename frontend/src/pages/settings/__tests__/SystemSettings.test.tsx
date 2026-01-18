import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { SystemSettings } from '../SystemSettings';
import type { DatabaseStats } from 'shared/types';

// Mock the hooks
vi.mock('@/hooks/useDatabaseStats');
vi.mock('@/hooks/useDatabaseMaintenance');
vi.mock('@/hooks/useFeedback');

// Mock the ConfirmDialog
vi.mock('@/components/dialogs', () => ({
  ConfirmDialog: {
    show: vi.fn(),
  },
}));

// Mock the components that have complex dependencies
vi.mock('@/components/dashboard/DiskUsageIndicator', () => ({
  DiskUsageIndicator: () => <div data-testid="disk-usage">Disk Usage</div>,
}));

vi.mock('@/components/settings', () => ({
  BackupsSection: () => <div data-testid="backups-section">Backups</div>,
}));

// Mock lucide icons to reduce noise
vi.mock('lucide-react', () => {
  // Create a generic icon component
  const Icon = ({ className }: { className?: string }) => (
    <div className={className}>Icon</div>
  );

  return new Proxy(
    {},
    {
      get: () => Icon,
    }
  );
});

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>{ui}</I18nextProvider>
    </QueryClientProvider>
  );
}

describe('SystemSettings', () => {
  const mockStats: DatabaseStats = {
    database_size_bytes: BigInt(1024 * 1024), // 1 MB
    wal_size_bytes: BigInt(512 * 1024), // 512 KB
    page_size: BigInt(4096),
    free_pages: BigInt(10),
    task_count: BigInt(100),
    task_attempt_count: BigInt(50),
    execution_process_count: BigInt(25),
    log_entry_count: BigInt(1000),
  };

  const mockVacuumMutate = vi.fn();
  const mockAnalyzeMutate = vi.fn();
  const mockPurgeArchivedMutate = vi.fn();
  const mockPurgeLogsMutate = vi.fn();

  beforeEach(async () => {
    vi.clearAllMocks();

    // Mock useDatabaseStats to return success state
    const { useDatabaseStats } = await import('@/hooks/useDatabaseStats');
    vi.mocked(useDatabaseStats).mockReturnValue({
      data: mockStats,
      isLoading: false,
      error: null,
      // @ts-expect-error - minimal mock
      refetch: vi.fn(),
    });

    // Mock useDatabaseMaintenance to return mutation functions
    const { useDatabaseMaintenance } = await import(
      '@/hooks/useDatabaseMaintenance'
    );
    vi.mocked(useDatabaseMaintenance).mockReturnValue({
      vacuum: {
        mutate: mockVacuumMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      analyze: {
        mutate: mockAnalyzeMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      purgeArchived: {
        mutate: mockPurgeArchivedMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      purgeLogs: {
        mutate: mockPurgeLogsMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
    });

    // Mock useFeedback
    const { useFeedback } = await import('@/hooks/useFeedback');
    vi.mocked(useFeedback).mockReturnValue({
      success: null,
      error: null,
      showSuccess: vi.fn(),
      showError: vi.fn(),
      clearFeedback: vi.fn(),
    });
  });

  it('should show VACUUM confirmation dialog when Optimize button is clicked', async () => {
    const { ConfirmDialog } = await import('@/components/dialogs');
    const mockShow = vi.mocked(ConfirmDialog.show);
    mockShow.mockResolvedValue('confirmed');

    renderWithProviders(<SystemSettings />);

    // Wait for component to render
    await waitFor(() => {
      expect(screen.getByText(/Optimize Database/i)).toBeInTheDocument();
    });

    // Find and click the Optimize button
    const optimizeButton = screen.getByRole('button', {
      name: /Optimize Database/i,
    });
    fireEvent.click(optimizeButton);

    // Verify ConfirmDialog.show was called with correct parameters
    await waitFor(() => {
      expect(mockShow).toHaveBeenCalledWith(
        expect.objectContaining({
          title: expect.stringContaining('Optimisation'),
          message: expect.stringContaining('VACUUM'),
          variant: 'info',
        })
      );
    });
  });

  it('should not call vacuum when confirmation is canceled', async () => {
    const { ConfirmDialog } = await import('@/components/dialogs');
    const mockShow = vi.mocked(ConfirmDialog.show);
    mockShow.mockResolvedValue('canceled');

    renderWithProviders(<SystemSettings />);

    await waitFor(() => {
      expect(screen.getByText(/Optimize Database/i)).toBeInTheDocument();
    });

    const optimizeButton = screen.getByRole('button', {
      name: /Optimize Database/i,
    });
    fireEvent.click(optimizeButton);

    // Wait for dialog to be shown
    await waitFor(() => {
      expect(mockShow).toHaveBeenCalled();
    });

    // Verify mutations were NOT called
    await waitFor(() => {
      expect(mockVacuumMutate).not.toHaveBeenCalled();
      expect(mockAnalyzeMutate).not.toHaveBeenCalled();
    });
  });

  it('should call vacuum and analyze when confirmation is confirmed', async () => {
    const { ConfirmDialog } = await import('@/components/dialogs');
    const mockShow = vi.mocked(ConfirmDialog.show);
    mockShow.mockResolvedValue('confirmed');

    // Mock mutateAsync to return resolved promises
    const { useDatabaseMaintenance } = await import(
      '@/hooks/useDatabaseMaintenance'
    );
    const mockVacuumAsync = vi.fn().mockResolvedValue({});
    const mockAnalyzeAsync = vi.fn().mockResolvedValue({});

    vi.mocked(useDatabaseMaintenance).mockReturnValue({
      vacuum: {
        mutate: mockVacuumMutate,
        mutateAsync: mockVacuumAsync,
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      analyze: {
        mutate: mockAnalyzeMutate,
        mutateAsync: mockAnalyzeAsync,
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      purgeArchived: {
        mutate: mockPurgeArchivedMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
      purgeLogs: {
        mutate: mockPurgeLogsMutate,
        mutateAsync: vi.fn(),
        isPending: false,
        // @ts-expect-error - minimal mock
        isError: false,
      },
    });

    renderWithProviders(<SystemSettings />);

    await waitFor(() => {
      expect(screen.getByText(/Optimize Database/i)).toBeInTheDocument();
    });

    const optimizeButton = screen.getByRole('button', {
      name: /Optimize Database/i,
    });
    fireEvent.click(optimizeButton);

    // Wait for confirmation and mutations to be called
    await waitFor(() => {
      expect(mockShow).toHaveBeenCalled();
    });

    await waitFor(() => {
      expect(mockVacuumAsync).toHaveBeenCalled();
      expect(mockAnalyzeAsync).toHaveBeenCalled();
    });
  });
});

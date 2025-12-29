import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  within,
  waitFor,
} from '@testing-library/react';
import { BrowserRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { MobileSettingsAccordion } from '../MobileSettingsAccordion';

// Mock the settings components - they have complex dependencies
vi.mock('../GeneralSettings', () => ({
  GeneralSettings: () => (
    <div data-testid="general-settings">General Settings Content</div>
  ),
}));
vi.mock('../ProjectSettings', () => ({
  ProjectSettings: () => (
    <div data-testid="project-settings">Project Settings Content</div>
  ),
}));
vi.mock('../OrganizationSettings', () => ({
  OrganizationSettings: () => (
    <div data-testid="organization-settings">Organization Settings Content</div>
  ),
}));
vi.mock('../AgentSettings', () => ({
  AgentSettings: () => (
    <div data-testid="agent-settings">Agent Settings Content</div>
  ),
}));
vi.mock('../McpSettings', () => ({
  McpSettings: () => <div data-testid="mcp-settings">MCP Settings Content</div>,
}));
vi.mock('../BackupSettings', () => ({
  BackupSettings: () => (
    <div data-testid="backup-settings">Backup Settings Content</div>
  ),
}));

// Mock useIsMobile
vi.mock('@/hooks/useIsMobile', () => ({
  useIsMobile: () => true, // Force mobile view in tests
}));

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
      <I18nextProvider i18n={i18n}>
        <BrowserRouter>{ui}</BrowserRouter>
      </I18nextProvider>
    </QueryClientProvider>
  );
}

describe('MobileSettingsAccordion', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should render all accordion sections', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for lazy-loaded components and check for section headers
    await waitFor(() => {
      // Check section titles by looking at the accordion header structure
      const sectionHeaders = screen.getAllByTestId('section-icon');
      expect(sectionHeaders.length).toBe(6);
    });

    // Verify all section buttons are present
    const buttons = screen.getAllByRole('button');
    // Should have search clear button + 6 accordion headers
    expect(buttons.length).toBeGreaterThanOrEqual(6);
  });

  it('should expand first section by default', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // First section (General) should be expanded by default
    await waitFor(() => {
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });
  });

  it('should collapse other sections when one expands', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for initial render
    await waitFor(() => {
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });

    // Find all accordion buttons
    const accordionButtons = screen
      .getAllByRole('button')
      .filter((btn) =>
        btn.getAttribute('aria-controls')?.startsWith('settings-section-')
      );

    // Find the Projects section button
    const projectsButton = accordionButtons.find((btn) =>
      btn.textContent?.toLowerCase().includes('project')
    );
    expect(projectsButton).toBeDefined();

    fireEvent.click(projectsButton!);

    // Projects should now be expanded, General collapsed
    await waitFor(() => {
      expect(screen.getByTestId('project-settings')).toBeInTheDocument();
      expect(screen.queryByTestId('general-settings')).not.toBeInTheDocument();
    });
  });

  it('should expand section on tap', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for initial render
    await waitFor(() => {
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });

    // Agents section should initially be collapsed
    expect(screen.queryByTestId('agent-settings')).not.toBeInTheDocument();

    // Find the Agents section button
    const accordionButtons = screen
      .getAllByRole('button')
      .filter((btn) =>
        btn.getAttribute('aria-controls')?.startsWith('settings-section-')
      );
    const agentsButton = accordionButtons.find((btn) =>
      btn.textContent?.toLowerCase().includes('agent')
    );
    expect(agentsButton).toBeDefined();

    fireEvent.click(agentsButton!);

    // Now Agents should be visible
    await waitFor(() => {
      expect(screen.getByTestId('agent-settings')).toBeInTheDocument();
    });
  });

  it('should have search bar at top', () => {
    renderWithProviders(<MobileSettingsAccordion />);

    expect(screen.getByPlaceholderText(/search settings/i)).toBeInTheDocument();
  });

  it('should filter sections on search', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    const searchInput = screen.getByPlaceholderText(/search settings/i);

    // Type to search for "backup" (more unique than "mcp")
    fireEvent.change(searchInput, { target: { value: 'backup' } });

    // Wait for filter to apply
    await waitFor(() => {
      // Should only show backup section
      const accordionButtons = screen
        .getAllByRole('button')
        .filter((btn) =>
          btn.getAttribute('aria-controls')?.startsWith('settings-section-')
        );
      expect(accordionButtons.length).toBe(1);
    });
  });

  it('should show expand/collapse indicator', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for render
    await waitFor(() => {
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });

    // Find the General section header
    const accordionButtons = screen
      .getAllByRole('button')
      .filter((btn) =>
        btn.getAttribute('aria-controls')?.startsWith('settings-section-')
      );
    const generalButton = accordionButtons[0]; // First one is General

    // Should have a chevron icon indicating expanded state
    const chevron = within(generalButton).getByTestId('chevron-icon');
    expect(chevron).toBeInTheDocument();
  });

  it('should show icons for each section', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for render
    await waitFor(() => {
      // Each section should have an icon
      const icons = screen.getAllByTestId('section-icon');
      expect(icons.length).toBe(6);
    });
  });

  it('should clear search when X is clicked', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    const searchInput = screen.getByPlaceholderText(/search settings/i);

    // Type something that will still show results (not trigger empty state)
    fireEvent.change(searchInput, { target: { value: 'gen' } });
    expect(searchInput).toHaveValue('gen');

    // Find and click the clear button in the search input area (has aria-label)
    const clearButton = screen.getByLabelText(/clear search/i);
    fireEvent.click(clearButton);

    // Search should be cleared
    expect(searchInput).toHaveValue('');
  });

  it('should show section descriptions', async () => {
    renderWithProviders(<MobileSettingsAccordion />);

    // Wait for render
    await waitFor(() => {
      // Each section should show a description
      const descriptions = screen.getAllByTestId('section-description');
      expect(descriptions.length).toBe(6);
    });
  });
});

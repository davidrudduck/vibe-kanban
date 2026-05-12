import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SessionMonitorPanel } from './SessionMonitorPanel';
import type { SessionSummary } from '../model/sessionSummary';

// Mock the useSessionSummary hook
vi.mock('../model/contexts/EntriesContext', () => ({
  useSessionSummary: vi.fn(),
}));

import { useSessionSummary } from '../model/contexts/EntriesContext';
const mockUseSessionSummary = useSessionSummary as ReturnType<typeof vi.fn>;

describe('SessionMonitorPanel', () => {
  const emptyBaseSummary: SessionSummary = {
    contextTokens: 0,
    contextWindow: 0,
    maxOutputTokens: null,
    outputTokens: null,
    cacheCreationTokens: null,
    cacheReadTokens: null,
    costUSD: null,
    numTurns: null,
    durationMs: null,
    cacheHitRate: null,
    hasEntries: false,
    executorSupportsTokens: false,
    executorName: null,
  };

  const populatedSummary: SessionSummary = {
    contextTokens: 5000,
    contextWindow: 200000,
    maxOutputTokens: 8000,
    outputTokens: 500,
    cacheCreationTokens: 1000,
    cacheReadTokens: 2000,
    costUSD: 0.15,
    numTurns: 3,
    durationMs: 45000,
    cacheHitRate: 65,
    hasEntries: true,
    executorSupportsTokens: true,
    executorName: 'CLAUDE_CODE',
  };

  it('renders "Waiting for first turn…" when empty + executor supports tokens', () => {
    mockUseSessionSummary.mockReturnValue({
      ...emptyBaseSummary,
      executorSupportsTokens: true,
      executorName: 'CLAUDE_CODE',
    });

    render(<SessionMonitorPanel />);

    expect(screen.getByTestId('session-monitor-waiting')).toBeInTheDocument();
    expect(screen.getByText(/waiting for first turn/i)).toBeInTheDocument();
    expect(
      screen.getByText(/token usage will appear after the first response/i)
    ).toBeInTheDocument();
  });

  it('renders "Telemetry not available" when empty + executor does not support tokens', () => {
    mockUseSessionSummary.mockReturnValue({
      ...emptyBaseSummary,
      executorSupportsTokens: false,
      executorName: 'GEMINI',
    });

    render(<SessionMonitorPanel />);

    expect(
      screen.getByTestId('session-monitor-not-supported')
    ).toBeInTheDocument();
    expect(screen.getByText(/telemetry not available/i)).toBeInTheDocument();
    expect(screen.getByText(/GEMINI/i)).toBeInTheDocument();
  });

  it('renders populated panel when data is present', () => {
    mockUseSessionSummary.mockReturnValue(populatedSummary);

    render(<SessionMonitorPanel />);

    expect(screen.getByTestId('session-monitor-populated')).toBeInTheDocument();

    // Verify key sections are present
    expect(screen.getByText(/Context/)).toBeInTheDocument();
    expect(screen.getByText(/Cost/)).toBeInTheDocument();
    expect(screen.getByText(/Tokens/)).toBeInTheDocument();
    expect(screen.getByText(/Session/)).toBeInTheDocument();

    // Verify specific values
    expect(screen.getByText(/\$0\.15/)).toBeInTheDocument();
    expect(screen.getByText(/3 turns/)).toBeInTheDocument();
  });

  it('renders "Telemetry not available" with null executor name', () => {
    mockUseSessionSummary.mockReturnValue({
      ...emptyBaseSummary,
      executorSupportsTokens: false,
      executorName: null,
    });

    render(<SessionMonitorPanel />);

    expect(
      screen.getByTestId('session-monitor-not-supported')
    ).toBeInTheDocument();
    expect(screen.getByText(/this executor/i)).toBeInTheDocument();
  });

  it('handles legacy session gracefully (no NaN)', () => {
    mockUseSessionSummary.mockReturnValue({
      contextTokens: 12345,
      contextWindow: 200000,
      maxOutputTokens: null,
      outputTokens: null,
      cacheCreationTokens: null,
      cacheReadTokens: null,
      costUSD: null,
      numTurns: null,
      durationMs: null,
      cacheHitRate: null,
      hasEntries: true,
      executorSupportsTokens: true,
      executorName: 'CLAUDE_CODE',
    });

    const { container } = render(<SessionMonitorPanel />);

    expect(screen.getByTestId('session-monitor-populated')).toBeInTheDocument();

    // Verify Context section is present
    expect(screen.getByText(/Context/)).toBeInTheDocument();

    // Verify no "NaN" appears anywhere in the rendered output
    expect(container.textContent).not.toContain('NaN');

    // Verify Cost and Session sections are NOT present (null data)
    expect(screen.queryByText(/Cost/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Session/)).not.toBeInTheDocument();
  });
});

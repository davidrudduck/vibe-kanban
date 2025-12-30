import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  getDaysInColumn,
  formatDaysInColumn,
  getDaysStyle,
} from '../daysInColumn';

describe('daysInColumn utilities', () => {
  beforeEach(() => {
    // Use fake timers to control "now"
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2025-01-15T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('getDaysInColumn', () => {
    it('returns 0 for null activityAt', () => {
      expect(getDaysInColumn(null)).toBe(0);
    });

    it('returns 0 for undefined activityAt', () => {
      expect(getDaysInColumn(undefined)).toBe(0);
    });

    it('returns 0 for activityAt less than 24 hours ago', () => {
      // 12 hours ago
      const activityAt = new Date('2025-01-15T00:00:00Z');
      expect(getDaysInColumn(activityAt)).toBe(0);
    });

    it('returns 1 for activityAt exactly 24+ hours ago', () => {
      // 25 hours ago
      const activityAt = new Date('2025-01-14T11:00:00Z');
      expect(getDaysInColumn(activityAt)).toBe(1);
    });

    it('returns 3 for activityAt 3 days ago', () => {
      const activityAt = new Date('2025-01-12T12:00:00Z');
      expect(getDaysInColumn(activityAt)).toBe(3);
    });

    it('returns 7 for activityAt 7 days ago', () => {
      const activityAt = new Date('2025-01-08T12:00:00Z');
      expect(getDaysInColumn(activityAt)).toBe(7);
    });

    it('handles ISO string input', () => {
      const activityAt = '2025-01-12T12:00:00Z';
      expect(getDaysInColumn(activityAt)).toBe(3);
    });

    it('handles Date object input', () => {
      const activityAt = new Date('2025-01-12T12:00:00Z');
      expect(getDaysInColumn(activityAt)).toBe(3);
    });

    it('returns 0 for invalid date string', () => {
      expect(getDaysInColumn('invalid-date')).toBe(0);
    });

    it('handles future dates by returning 0', () => {
      const futureDate = new Date('2025-01-20T12:00:00Z');
      expect(getDaysInColumn(futureDate)).toBe(0);
    });
  });

  describe('formatDaysInColumn', () => {
    it('returns null for 0 days', () => {
      expect(formatDaysInColumn(0)).toBeNull();
    });

    it('returns "1d" for 1 day', () => {
      expect(formatDaysInColumn(1)).toBe('1d');
    });

    it('returns "3d" for 3 days', () => {
      expect(formatDaysInColumn(3)).toBe('3d');
    });

    it('returns "6d" for 6 days', () => {
      expect(formatDaysInColumn(6)).toBe('6d');
    });

    it('returns "7d+" for 7 days', () => {
      expect(formatDaysInColumn(7)).toBe('7d+');
    });

    it('returns "7d+" for 14 days', () => {
      expect(formatDaysInColumn(14)).toBe('7d+');
    });

    it('returns "7d+" for 100 days', () => {
      expect(formatDaysInColumn(100)).toBe('7d+');
    });
  });

  describe('getDaysStyle', () => {
    it('returns empty string for 0 days', () => {
      expect(getDaysStyle(0)).toBe('');
    });

    it('returns neutral style for 1 day', () => {
      const style = getDaysStyle(1);
      expect(style).toContain('bg-muted');
      expect(style).toContain('text-muted-foreground');
    });

    it('returns neutral style for 2 days', () => {
      const style = getDaysStyle(2);
      expect(style).toContain('bg-muted');
      expect(style).toContain('text-muted-foreground');
    });

    it('returns warning (amber) style for 3 days', () => {
      const style = getDaysStyle(3);
      expect(style).toContain('bg-amber');
      expect(style).toContain('text-amber');
    });

    it('returns warning (amber) style for 6 days', () => {
      const style = getDaysStyle(6);
      expect(style).toContain('bg-amber');
      expect(style).toContain('text-amber');
    });

    it('returns strong warning (red) style for 7 days', () => {
      const style = getDaysStyle(7);
      expect(style).toContain('bg-red');
      expect(style).toContain('text-red');
    });

    it('returns strong warning (red) style for 14 days', () => {
      const style = getDaysStyle(14);
      expect(style).toContain('bg-red');
      expect(style).toContain('text-red');
    });
  });
});

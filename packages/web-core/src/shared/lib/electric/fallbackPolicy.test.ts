import { describe, expect, it } from 'vitest';

import { shouldLockSourceToFallbackForStatus } from './fallbackPolicy';

describe('shouldLockSourceToFallbackForStatus', () => {
  it('locks fallback for authorization and missing-resource failures', () => {
    expect(shouldLockSourceToFallbackForStatus(403)).toBe(true);
    expect(shouldLockSourceToFallbackForStatus(404)).toBe(true);
  });

  it('locks fallback for unavailable upstream responses', () => {
    expect(shouldLockSourceToFallbackForStatus(undefined)).toBe(true);
    expect(shouldLockSourceToFallbackForStatus(500)).toBe(true);
    expect(shouldLockSourceToFallbackForStatus(503)).toBe(true);
  });

  it('keeps electric mode for successful and recoverable auth responses', () => {
    expect(shouldLockSourceToFallbackForStatus(200)).toBe(false);
    expect(shouldLockSourceToFallbackForStatus(204)).toBe(false);
    expect(shouldLockSourceToFallbackForStatus(401)).toBe(false);
    expect(shouldLockSourceToFallbackForStatus(422)).toBe(false);
  });
});

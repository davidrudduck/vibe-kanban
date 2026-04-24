import { describe, it, expect } from 'vitest';
import { sha256Hex } from './sha256';

// NIST / RFC 6234 test vectors verified against Node.js crypto.createHash('sha256')

describe('sha256Hex', () => {
  it('empty input → correct digest', async () => {
    const buf = new TextEncoder().encode('').buffer;
    expect(await sha256Hex(buf)).toBe(
      'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
    );
  });

  it('"abc" → correct digest', async () => {
    const buf = new TextEncoder().encode('abc').buffer;
    expect(await sha256Hex(buf)).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
    );
  });

  it('1 MB of 0xAB bytes → correct digest', async () => {
    const buf = new Uint8Array(1024 * 1024).fill(0xab).buffer;
    expect(await sha256Hex(buf)).toBe(
      '074c29674e21baa420ee0eca0d85b9283b0cfb3ac912da2098f6b3a7f8d6678f'
    );
  });
});

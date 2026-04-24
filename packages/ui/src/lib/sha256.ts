/**
 * sha256Hex — compute a SHA-256 hex digest of an ArrayBuffer.
 *
 * Prefers the native WebCrypto API when available (secure contexts: HTTPS,
 * localhost). Falls back to a pure-JS implementation so the function works on
 * plain HTTP (LAN IP, some webview sandboxes) where `crypto.subtle` is
 * undefined.
 *
 * Based on the FIPS 180-4 reference algorithm (public domain).
 * The pure-JS path iterates over a Uint8Array view with no recursion and
 * handles buffers up to 20 MB without stack issues.
 */

// ---------------------------------------------------------------------------
// WebCrypto path
// ---------------------------------------------------------------------------

function hasCryptoSubtle(): boolean {
  try {
    return (
      typeof globalThis.crypto?.subtle?.digest === 'function'
    );
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Pure-JS SHA-256 fallback
// ---------------------------------------------------------------------------

// SHA-256 constants: first 32 bits of the fractional parts of the cube roots
// of the first 64 primes.
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
  0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
  0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
  0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
  0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
  0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
  0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
  0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

// Initial hash values: first 32 bits of the fractional parts of the square
// roots of the first 8 primes.
const H0 = new Uint32Array([
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);

function rotr32(x: number, n: number): number {
  return (x >>> n) | (x << (32 - n));
}

function sha256Js(buffer: ArrayBuffer): string {
  const msgLen = buffer.byteLength;
  // Pre-processing: pad to 512-bit (64-byte) blocks.
  // Append bit '1', then zeros, then 64-bit big-endian message length in bits.
  const padded = msgLen + 1 + 8; // +1 for 0x80, +8 for length
  const blockBytes = Math.ceil(padded / 64) * 64;
  const data = new Uint8Array(blockBytes);
  data.set(new Uint8Array(buffer));
  data[msgLen] = 0x80;
  // Write 64-bit big-endian bit-length at the end.
  // JavaScript bit-ops are 32-bit; split into high/low 32.
  const bitLen = msgLen * 8;
  // High 32 bits (for files ≤ 512 MB this is always 0 given JS number limits).
  const hi = Math.floor(bitLen / 0x100000000);
  const lo = bitLen >>> 0;
  const dv = new DataView(data.buffer);
  dv.setUint32(blockBytes - 8, hi, false);
  dv.setUint32(blockBytes - 4, lo, false);

  // Initial hash value (copy so we don't mutate the constant).
  const h = new Uint32Array(H0);

  const w = new Uint32Array(64);

  for (let offset = 0; offset < blockBytes; offset += 64) {
    // Prepare message schedule.
    for (let i = 0; i < 16; i++) {
      w[i] = dv.getUint32(offset + i * 4, false);
    }
    for (let i = 16; i < 64; i++) {
      const s0 = rotr32(w[i - 15], 7) ^ rotr32(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr32(w[i - 2], 17) ^ rotr32(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }

    let [a, b, c, d, e, f, g, hh] = [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]];

    for (let i = 0; i < 64; i++) {
      const S1 = rotr32(e, 6) ^ rotr32(e, 11) ^ rotr32(e, 25);
      const ch = (e & f) ^ (~e & g);
      const temp1 = (hh + S1 + ch + K[i] + w[i]) >>> 0;
      const S0 = rotr32(a, 2) ^ rotr32(a, 13) ^ rotr32(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (S0 + maj) >>> 0;

      hh = g;
      g = f;
      f = e;
      e = (d + temp1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) >>> 0;
    }

    h[0] = (h[0] + a) >>> 0;
    h[1] = (h[1] + b) >>> 0;
    h[2] = (h[2] + c) >>> 0;
    h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0;
    h[5] = (h[5] + f) >>> 0;
    h[6] = (h[6] + g) >>> 0;
    h[7] = (h[7] + hh) >>> 0;
  }

  return Array.from(h)
    .map((v) => v.toString(16).padStart(8, '0'))
    .join('');
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Returns the SHA-256 hex digest of `buffer`.
 *
 * Uses WebCrypto when available (secure contexts), otherwise falls back to the
 * pure-JS implementation so the function works over plain HTTP.
 */
export async function sha256Hex(buffer: ArrayBuffer): Promise<string> {
  if (hasCryptoSubtle()) {
    const hashBuf = await globalThis.crypto.subtle.digest('SHA-256', buffer);
    return Array.from(new Uint8Array(hashBuf))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }
  return sha256Js(buffer);
}

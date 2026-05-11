#!/usr/bin/env node
// Fails CI if any high/critical advisory references one of the runtime-reachable
// modules listed in BLOCKED. Build-chain-only advisories (sucrase, sentry plugins,
// etc.) are not blocked here — they're tracked by `pnpm audit` itself.

import { execSync } from 'node:child_process';

// Runtime-reachable modules that ship in the production bundle. Build-only
// modules (sucrase, sentry-plugin, etc.) are intentionally excluded.
//
// `lodash` / `lodash-es` are NOT in this list: the open advisories patch at
// "^4.18.0" but no such release exists yet upstream. The vulnerable
// `_.template` path is not invoked by anything in this repo. Revisit when
// upstream ships a patched release.
const BLOCKED = new Set(['preact', 'fast-uri', 'devalue']);

const BLOCKED_SEVERITY = new Set(['high', 'critical']);

let raw;
try {
  raw = execSync('pnpm audit --prod --json', { encoding: 'utf8' });
} catch (err) {
  // pnpm audit exits non-zero when vulnerabilities are found, but still prints
  // JSON to stdout. Use that.
  raw = err.stdout?.toString() ?? '';
  if (!raw.trim()) {
    console.error('pnpm audit produced no output:', err.message);
    process.exit(2);
  }
}

let report;
try {
  report = JSON.parse(raw);
} catch (err) {
  console.error('Failed to parse pnpm audit JSON:', err.message);
  process.exit(2);
}

// Security gate: fail closed if the audit JSON does not have the expected
// `advisories` field. Otherwise a future schema change would let this gate
// pass with zero offenders simply because the parser couldn't find them.
if (
  !('advisories' in report) ||
  typeof report.advisories !== 'object' ||
  report.advisories === null
) {
  console.error(
    'Unsupported pnpm audit JSON schema: missing or non-object `advisories`.',
  );
  process.exit(2);
}

const advisories = report.advisories;
const offenders = [];

for (const adv of Object.values(advisories)) {
  if (!BLOCKED_SEVERITY.has(adv.severity)) continue;
  if (!BLOCKED.has(adv.module_name)) continue;
  const path =
    adv.findings?.[0]?.paths?.[0] ?? '(unknown path)';
  offenders.push({
    module: adv.module_name,
    severity: adv.severity,
    title: adv.title,
    vulnerable: adv.vulnerable_versions,
    patched: adv.patched_versions,
    path,
  });
}

if (offenders.length === 0) {
  console.log(
    '✅ No high/critical advisories on runtime-reachable modules ' +
      `(${[...BLOCKED].join(', ')}).`,
  );
  process.exit(0);
}

console.error(
  `❌ ${offenders.length} blocked advisory${offenders.length === 1 ? '' : 'ies'} found:\n`,
);
for (const o of offenders) {
  console.error(`  - ${o.module} ${o.vulnerable} [${o.severity}]`);
  console.error(`      ${o.title}`);
  console.error(`      patched: ${o.patched}`);
  console.error(`      via: ${o.path}\n`);
}
console.error(
  'Add a pnpm.overrides entry in root package.json to force a patched version,\n' +
    'then re-run `pnpm install`.',
);
process.exit(1);

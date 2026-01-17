#!/usr/bin/env node
/**
 * Production server wrapper script.
 * Provides equivalent setup to dev mode (env loading, port allocation)
 * without running a separate frontend server.
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const net = require('net');

const ENV_FILE = path.join(__dirname, '..', '.env');

function loadEnvFile() {
  try {
    if (fs.existsSync(ENV_FILE)) {
      const content = fs.readFileSync(ENV_FILE, 'utf8');
      for (const line of content.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#')) continue;

        const eqIndex = trimmed.indexOf('=');
        if (eqIndex === -1) continue;

        const key = trimmed.slice(0, eqIndex).trim();
        let value = trimmed.slice(eqIndex + 1).trim();

        // Remove quotes
        if ((value.startsWith('"') && value.endsWith('"')) ||
            (value.startsWith("'") && value.endsWith("'"))) {
          value = value.slice(1, -1);
        }

        if (!process.env[key]) {
          process.env[key] = value;
        }
      }
      console.log(`Loaded environment from ${ENV_FILE}`);
    }
  } catch (e) {
    console.warn(`Warning: Could not load .env: ${e.message}`);
  }
}

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const sock = net.createConnection({ port, host: 'localhost' });
    sock.on('connect', () => { sock.destroy(); resolve(false); });
    sock.on('error', () => resolve(true));
  });
}

async function findFreePort(start = 3000) {
  let port = start;
  while (!(await isPortAvailable(port))) {
    port++;
    if (port > 65535) throw new Error('No available ports');
  }
  return port;
}

async function main() {
  loadEnvFile();

  // Auto-allocate backend port if not set
  if (!process.env.BACKEND_PORT && !process.env.PORT) {
    const port = await findFreePort(3000);
    process.env.BACKEND_PORT = String(port);
    console.log(`Auto-allocated backend port: ${port}`);
  }

  const host = process.env.HOST || '127.0.0.1';
  const port = process.env.BACKEND_PORT || process.env.PORT || '3000';

  console.log(`Starting production server on ${host}:${port}`);

  const binary = path.join(__dirname, '..', 'target', 'release', 'vks-node-server');

  if (!fs.existsSync(binary)) {
    console.error(`Binary not found: ${binary}`);
    console.error('Run: pnpm run prod:build');
    process.exit(1);
  }

  const child = spawn(binary, [], {
    env: process.env,
    stdio: 'inherit',
    cwd: path.join(__dirname, '..')
  });

  process.on('SIGTERM', () => child.kill('SIGTERM'));
  process.on('SIGINT', () => child.kill('SIGINT'));

  child.on('exit', (code, signal) => {
    if (signal) {
      process.exit(128);
    } else {
      process.exit(code || 0);
    }
  });

  child.on('error', (err) => {
    console.error('Failed to start server:', err.message);
    process.exit(1);
  });
}

main().catch((err) => {
  console.error('Startup error:', err);
  process.exit(1);
});

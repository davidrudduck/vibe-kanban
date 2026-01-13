#!/usr/bin/env node
/**
 * Stop vibe-kanban server instance(s) gracefully using PID files.
 *
 * This script reads from /tmp/vibe-kanban/instances/ to find running instances
 * and sends SIGTERM to stop them safely, avoiding the "killall" problem
 * where child processes accidentally kill the parent server.
 *
 * Graceful Shutdown Sequence (dev mode):
 * 1. SIGTERM → backend (triggers graceful shutdown)
 * 2. WAIT for backend exit (max 10s) - CRITICAL for database safety
 *    Backend performs: log flush, WAL checkpoint, connection pool close
 * 3. SIGTERM → dev_root_pid (kills concurrently/cargo-watch/Vite)
 * 4. Port-based cleanup fallback (lsof) for any orphaned processes
 *
 * Usage:
 *   pnpm run stop              # Stop instance for current directory (or show menu)
 *   pnpm run stop --all        # Stop all running instances
 *   pnpm run stop --list       # List all running instances
 *   pnpm run stop /path/to/project  # Stop specific project's instance
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

const INSTANCES_DIR = path.join(os.tmpdir(), 'vibe-kanban', 'instances');
const LEGACY_INFO_FILE = path.join(os.tmpdir(), 'vibe-kanban', 'vibe-kanban.info');

function isProcessRunning(pid) {
  try {
    process.kill(pid, 0); // Signal 0 = check if process exists
    return true;
  } catch (e) {
    return e.code !== 'ESRCH';
  }
}

function loadInstances() {
  const instances = [];

  // Load from new registry
  if (fs.existsSync(INSTANCES_DIR)) {
    for (const file of fs.readdirSync(INSTANCES_DIR)) {
      if (file.endsWith('.json')) {
        try {
          const content = fs.readFileSync(path.join(INSTANCES_DIR, file), 'utf8');
          const info = JSON.parse(content);
          info._file = path.join(INSTANCES_DIR, file);
          instances.push(info);
        } catch (e) {
          // Skip invalid files
        }
      }
    }
  }

  // Fallback to legacy file if no instances found
  if (instances.length === 0 && fs.existsSync(LEGACY_INFO_FILE)) {
    try {
      const content = fs.readFileSync(LEGACY_INFO_FILE, 'utf8');
      const info = JSON.parse(content);
      info.project_root = process.cwd(); // Assume current directory
      info.ports = { backend: info.port };
      info._file = LEGACY_INFO_FILE;
      info._legacy = true;
      instances.push(info);
    } catch (e) {
      // Skip invalid file
    }
  }

  return instances;
}

function filterRunning(instances) {
  return instances.filter(i => isProcessRunning(i.pid));
}

/**
 * Sleep for a given number of milliseconds.
 */
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Kill any process listening on a given port (fallback cleanup).
 * Uses lsof to find the process, then sends SIGTERM (and SIGKILL if needed).
 */
async function killProcessOnPort(port, label) {
  try {
    const result = execSync(`lsof -ti:${port} 2>/dev/null || true`, { encoding: 'utf8' });
    const pids = result.trim().split('\n').filter(p => p);

    for (const pidStr of pids) {
      const pid = parseInt(pidStr, 10);
      if (!pid || !isProcessRunning(pid)) continue;

      console.log(`  Found orphaned ${label} process on port ${port} (PID: ${pid})`);

      try {
        process.kill(pid, 'SIGTERM');

        // Wait briefly for process to exit
        await sleep(500);

        if (isProcessRunning(pid)) {
          console.log(`  Force killing ${label} process (PID: ${pid})`);
          process.kill(pid, 'SIGKILL');
        } else {
          console.log(`  Orphaned ${label} process terminated`);
        }
      } catch (e) {
        // Process may have already exited
      }
    }
  } catch (e) {
    // Silent fail - port may already be free or lsof not available
  }
}

/**
 * Stop an instance with proper graceful shutdown sequence.
 *
 * The order is CRITICAL for data safety:
 * 1. SIGTERM → backend (triggers graceful shutdown)
 * 2. WAIT for backend exit (max 10s)
 *    - Backend flushes log buffers
 *    - Backend runs WAL checkpoint (TRUNCATE)
 *    - Backend closes database pool
 * 3. SIGTERM → dev_root_pid (kills concurrently/cargo-watch/Vite)
 * 4. Port-based cleanup fallback for orphans
 */
async function stopInstance(info) {
  console.log(`Stopping instance:`);
  console.log(`  Project: ${info.project_root}`);
  console.log(`  Backend PID: ${info.pid}`);
  if (info.dev_root_pid) {
    console.log(`  Dev root PID: ${info.dev_root_pid} (concurrently)`);
  }
  console.log(`  Ports: backend=${info.ports?.backend || 'N/A'}, frontend=${info.ports?.frontend || 'N/A'}, mcp=${info.ports?.mcp || 'N/A'}`);

  // STEP 1: Send SIGTERM to backend (Rust server) - triggers graceful shutdown
  // This is THE CRITICAL STEP - backend will flush logs, checkpoint WAL, close DB
  if (!isProcessRunning(info.pid)) {
    console.log('  Backend process not found (already stopped)');
  } else {
    try {
      process.kill(info.pid, 'SIGTERM');
      console.log(`  Sent SIGTERM to backend (PID: ${info.pid})`);

      // STEP 2: Wait for backend to complete graceful shutdown
      // Typical cleanup takes 1-3 seconds (flush logs, WAL checkpoint, etc.)
      // Max wait: 10 seconds (generous timeout)
      console.log('  Waiting for backend cleanup to complete...');
      const maxWaitMs = 10000;
      const checkIntervalMs = 200;
      let waited = 0;

      while (isProcessRunning(info.pid) && waited < maxWaitMs) {
        await sleep(checkIntervalMs);
        waited += checkIntervalMs;
      }

      if (isProcessRunning(info.pid)) {
        console.warn(`  Backend still running after ${maxWaitMs}ms, forcing kill...`);
        try {
          process.kill(info.pid, 'SIGKILL');
        } catch (e) {
          // Process may have exited between check and kill
        }
      } else {
        console.log(`  Backend exited cleanly after ${waited}ms`);
      }
    } catch (e) {
      console.error(`  Failed to signal backend: ${e.message}`);
    }
  }

  // STEP 3: NOW kill dev root PID (concurrently) - this terminates cargo-watch and Vite
  // Safe to do this now because backend has already completed its cleanup
  if (info.dev_root_pid && isProcessRunning(info.dev_root_pid)) {
    try {
      process.kill(info.dev_root_pid, 'SIGTERM');
      console.log(`  Sent SIGTERM to dev root process (PID: ${info.dev_root_pid})`);

      // Give concurrently a moment to clean up its children
      await sleep(500);

      // Force kill if still running
      if (isProcessRunning(info.dev_root_pid)) {
        console.log(`  Force killing dev root process...`);
        try {
          process.kill(info.dev_root_pid, 'SIGKILL');
        } catch (e) {
          // Process may have exited
        }
      }
    } catch (e) {
      console.warn(`  Failed to kill dev root PID: ${e.message}`);
    }
  }

  // STEP 4: Fallback - kill any orphaned processes on frontend/backend ports
  // This catches Vite, cargo-watch, or other stragglers that weren't children of concurrently
  if (info.ports?.frontend) {
    await killProcessOnPort(info.ports.frontend, 'frontend');
  }
  if (info.ports?.backend) {
    await killProcessOnPort(info.ports.backend, 'backend');
  }

  // STEP 5: Clean up instance file
  if (info._file && fs.existsSync(info._file)) {
    fs.unlinkSync(info._file);
    console.log('  Cleaned up instance registry file');
  }

  console.log('  Instance stopped successfully');
  return true;
}

function listInstances(instances) {
  const running = filterRunning(instances);

  if (running.length === 0) {
    console.log('No running vibe-kanban instances found.');
    return;
  }

  console.log(`Found ${running.length} running instance(s):\n`);

  for (const info of running) {
    console.log(`  Project: ${info.project_root}`);
    console.log(`    PID: ${info.pid}`);
    console.log(`    Started: ${info.started_at}`);
    console.log(`    Ports:`);
    if (info.ports?.backend) console.log(`      Backend: http://127.0.0.1:${info.ports.backend}`);
    if (info.ports?.frontend) console.log(`      Frontend: http://127.0.0.1:${info.ports.frontend}`);
    if (info.ports?.mcp) console.log(`      MCP: http://127.0.0.1:${info.ports.mcp}/mcp`);
    if (info.ports?.hive) console.log(`      Hive: ws://127.0.0.1:${info.ports.hive}`);
    console.log('');
  }
}

function findInstanceForDir(instances, dir) {
  const canonical = fs.realpathSync(dir);
  return instances.find(i => {
    try {
      const projectCanonical = fs.realpathSync(i.project_root);
      return canonical.startsWith(projectCanonical);
    } catch {
      return false;
    }
  });
}

async function main() {
  const args = process.argv.slice(2);
  const instances = loadInstances();
  const running = filterRunning(instances);

  // Handle flags
  if (args.includes('--list') || args.includes('-l')) {
    listInstances(instances);
    return;
  }

  if (args.includes('--all') || args.includes('-a')) {
    if (running.length === 0) {
      console.log('No running instances to stop.');
      return;
    }
    console.log(`Stopping all ${running.length} instance(s)...\n`);
    for (const info of running) {
      await stopInstance(info);
      console.log('');
    }
    return;
  }

  // Check for explicit path argument
  const pathArg = args.find(a => !a.startsWith('-'));
  if (pathArg) {
    const targetPath = path.resolve(pathArg);
    const instance = findInstanceForDir(running, targetPath);
    if (instance) {
      await stopInstance(instance);
    } else {
      console.log(`No running instance found for: ${targetPath}`);
    }
    return;
  }

  // Default: stop instance for current directory
  const currentDir = process.cwd();
  const instance = findInstanceForDir(running, currentDir);

  if (instance) {
    await stopInstance(instance);
    return;
  }

  // No instance for current directory - show list if there are others
  if (running.length === 0) {
    console.log('No running vibe-kanban instances found.');
  } else if (running.length === 1) {
    console.log('No instance found for current directory.');
    console.log(`\nFound 1 instance running for a different project:\n`);
    console.log(`  ${running[0].project_root} (PID: ${running[0].pid})`);
    console.log(`\nUse 'pnpm run stop --all' to stop it, or specify the path.`);
  } else {
    console.log('No instance found for current directory.');
    console.log(`\nFound ${running.length} instances running for other projects:\n`);
    for (const info of running) {
      console.log(`  ${info.project_root} (PID: ${info.pid})`);
    }
    console.log(`\nUse 'pnpm run stop --all' to stop all, or specify a path.`);
  }
}

main().catch(err => {
  console.error('Error:', err.message);
  process.exit(1);
});

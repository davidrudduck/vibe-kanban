#!/usr/bin/env node
/**
 * Stop vibe-kanban server instance(s) gracefully using PID files.
 *
 * This script reads from /tmp/vibe-kanban/instances/ to find running instances
 * and sends SIGTERM to stop them safely, avoiding the "killall" problem
 * where child processes accidentally kill the parent server.
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

function stopInstance(info) {
  console.log(`Stopping instance:`);
  console.log(`  Project: ${info.project_root}`);
  console.log(`  PID: ${info.pid}`);
  console.log(`  Ports: backend=${info.ports?.backend || 'N/A'}, frontend=${info.ports?.frontend || 'N/A'}, mcp=${info.ports?.mcp || 'N/A'}`);

  if (!isProcessRunning(info.pid)) {
    console.log('  Status: Process not found (already stopped)');
    // Clean up stale file
    if (info._file && fs.existsSync(info._file)) {
      fs.unlinkSync(info._file);
      console.log('  Cleaned up stale instance file');
    }
    return false;
  }

  try {
    process.kill(info.pid, 'SIGTERM');
    console.log('  Status: SIGTERM sent, server will terminate gracefully');
    return true;
  } catch (e) {
    console.error(`  Error: Failed to send signal - ${e.message}`);
    return false;
  }
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
      stopInstance(info);
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
      stopInstance(instance);
    } else {
      console.log(`No running instance found for: ${targetPath}`);
    }
    return;
  }

  // Default: stop instance for current directory
  const currentDir = process.cwd();
  const instance = findInstanceForDir(running, currentDir);

  if (instance) {
    stopInstance(instance);
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

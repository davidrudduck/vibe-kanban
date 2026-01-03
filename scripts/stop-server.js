#!/usr/bin/env node
/**
 * Stop the vibe-kanban server gracefully using its PID file.
 *
 * This script reads /tmp/vibe-kanban/vibe-kanban.info to get the exact PID
 * and sends SIGTERM to only that process, avoiding the "killall" problem
 * where child processes accidentally kill the parent server.
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const INFO_FILE = path.join(os.tmpdir(), 'vibe-kanban', 'vibe-kanban.info');

async function main() {
  try {
    if (!fs.existsSync(INFO_FILE)) {
      console.log('No vibe-kanban server info found. Server may not be running.');
      process.exit(0);
    }

    const info = JSON.parse(fs.readFileSync(INFO_FILE, 'utf8'));
    console.log(`Found vibe-kanban server:`);
    console.log(`  PID: ${info.pid}`);
    console.log(`  Port: ${info.port}`);
    console.log(`  Started: ${info.started_at}`);

    // Check if process is still running
    try {
      process.kill(info.pid, 0); // Signal 0 = check if process exists
    } catch (e) {
      if (e.code === 'ESRCH') {
        console.log('Server process not found (already stopped).');
        // Clean up stale info file
        fs.unlinkSync(INFO_FILE);
        process.exit(0);
      }
      throw e;
    }

    // Send SIGTERM for graceful shutdown
    console.log(`Sending SIGTERM to PID ${info.pid}...`);
    process.kill(info.pid, 'SIGTERM');
    console.log('Shutdown signal sent. Server will terminate gracefully.');

    // Wait a bit and check if it's gone
    await new Promise(resolve => setTimeout(resolve, 2000));

    try {
      process.kill(info.pid, 0);
      console.log('Server still running. Use "kill -9 ' + info.pid + '" to force kill.');
    } catch (e) {
      if (e.code === 'ESRCH') {
        console.log('Server stopped successfully.');
      }
    }
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }
}

main();

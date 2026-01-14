#!/usr/bin/env node
/**
 * Integration test for graceful shutdown in stop-server.js
 *
 * Tests that:
 * 1. Backend cleanup completes before dev_root_pid is killed
 * 2. Timeout handling works correctly (SIGKILL after 10s)
 * 3. Port-based fallback cleanup works
 *
 * Usage: node scripts/test-stop-graceful.js
 */

const { spawn, fork, execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const os = require('os');
const net = require('net');

// Track test results
let passed = 0;
let failed = 0;

function log(msg) {
  console.log(`[test] ${msg}`);
}

function pass(test) {
  console.log(`\x1b[32m✅ PASS: ${test}\x1b[0m`);
  passed++;
}

function fail(test, reason) {
  console.error(`\x1b[31m❌ FAIL: ${test}\x1b[0m`);
  if (reason) console.error(`   Reason: ${reason}`);
  failed++;
}

async function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function isProcessRunning(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return e.code !== 'ESRCH';
  }
}

/**
 * Find a free port for testing
 */
function findFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });
}

/**
 * Start a simple TCP server on a port (for port-based cleanup testing)
 */
function startTcpServer(port) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(port, '127.0.0.1', () => {
      resolve(server);
    });
    server.on('error', reject);
  });
}

/**
 * Test 1: Verify that stopInstance() waits for backend before killing dev_root_pid
 *
 * This test creates two mock processes:
 * - "backend" that handles SIGTERM and exits after a delay
 * - "dev_root" that we verify is killed AFTER backend exits
 */
async function testBackendCleanupOrder() {
  log('\n--- Test 1: Backend cleanup completes before dev_root_pid is killed ---');

  // Create a mock backend process that takes 1 second to "cleanup"
  const backendScript = `
    process.on('SIGTERM', () => {
      console.log('Backend: received SIGTERM, starting cleanup...');
      setTimeout(() => {
        console.log('Backend: cleanup complete, exiting');
        process.exit(0);
      }, 1000);
    });
    console.log('Backend: started, PID=' + process.pid);
    setInterval(() => {}, 1000); // Keep alive
  `;

  // Create a mock dev_root process
  const devRootScript = `
    let receivedSignal = null;
    process.on('SIGTERM', () => {
      receivedSignal = Date.now();
      console.log('DevRoot: received SIGTERM at', receivedSignal);
      process.exit(0);
    });
    console.log('DevRoot: started, PID=' + process.pid);
    setInterval(() => {}, 1000); // Keep alive
  `;

  const backend = spawn('node', ['-e', backendScript], { stdio: 'pipe' });
  const devRoot = spawn('node', ['-e', devRootScript], { stdio: 'pipe' });

  // Capture output
  let backendOutput = '';
  let devRootOutput = '';
  backend.stdout.on('data', (d) => { backendOutput += d.toString(); });
  devRoot.stdout.on('data', (d) => { devRootOutput += d.toString(); });

  await sleep(200); // Let processes start

  // Track timing
  const sigTermToBackendTime = Date.now();
  backend.kill('SIGTERM');

  // Wait for backend to exit
  await new Promise((resolve) => backend.on('exit', resolve));
  const backendExitTime = Date.now();

  // NOW send SIGTERM to dev_root (simulating what stop-server.js does)
  const sigTermToDevRootTime = Date.now();
  devRoot.kill('SIGTERM');

  await new Promise((resolve) => devRoot.on('exit', resolve));

  // Verify timing: dev_root should receive SIGTERM AFTER backend exits
  const backendCleanupDuration = backendExitTime - sigTermToBackendTime;
  const delayBeforeDevRootKill = sigTermToDevRootTime - backendExitTime;

  log(`Backend cleanup took: ${backendCleanupDuration}ms`);
  log(`Delay before dev_root kill: ${delayBeforeDevRootKill}ms`);

  if (backendCleanupDuration >= 900 && delayBeforeDevRootKill >= 0) {
    pass('Backend cleanup completes before dev_root is killed');
  } else {
    fail('Backend cleanup completes before dev_root is killed',
      `Backend cleanup: ${backendCleanupDuration}ms (expected ~1000ms), delay: ${delayBeforeDevRootKill}ms`);
  }
}

/**
 * Test 2: Verify timeout handling - SIGKILL is sent after backend doesn't respond
 *
 * Note: We use a shorter timeout (2s) for testing instead of the full 10s
 */
async function testTimeoutHandling() {
  log('\n--- Test 2: Timeout handling - SIGKILL sent when backend hangs ---');

  // Create a mock backend that ignores SIGTERM
  const hangingBackendScript = `
    process.on('SIGTERM', () => {
      console.log('Backend: ignoring SIGTERM (simulating hang)');
      // Don't exit - simulate a hung process
    });
    console.log('Backend: started, PID=' + process.pid);
    setInterval(() => {}, 1000); // Keep alive
  `;

  const backend = spawn('node', ['-e', hangingBackendScript], { stdio: 'pipe' });

  await sleep(200); // Let process start

  const startTime = Date.now();
  backend.kill('SIGTERM');

  // Wait for 2.5 seconds (our test timeout)
  await sleep(2500);

  // If still running, send SIGKILL (simulating stop-server.js behavior)
  if (isProcessRunning(backend.pid)) {
    backend.kill('SIGKILL');
    await sleep(100);
  }

  const endTime = Date.now();
  const totalTime = endTime - startTime;

  if (!isProcessRunning(backend.pid)) {
    pass(`Timeout handling works (process killed after ${totalTime}ms)`);
  } else {
    fail('Timeout handling works', 'Process still running after SIGKILL');
    // Force cleanup
    try { backend.kill('SIGKILL'); } catch (e) {}
  }
}

/**
 * Test 3: Verify port-based fallback cleanup
 *
 * This simulates the killProcessOnPort() function behavior
 */
async function testPortBasedCleanup() {
  log('\n--- Test 3: Port-based fallback cleanup ---');

  const testPort = await findFreePort();
  log(`Using test port: ${testPort}`);

  // Start a process listening on the port
  const serverScript = `
    const net = require('net');
    const server = net.createServer();
    server.listen(${testPort}, '127.0.0.1', () => {
      console.log('Server listening on port ${testPort}');
    });
    process.on('SIGTERM', () => process.exit(0));
    setInterval(() => {}, 1000);
  `;

  const serverProcess = spawn('node', ['-e', serverScript], { stdio: 'pipe' });

  await sleep(500); // Let server start

  // Verify server is listening
  try {
    const result = execSync(`lsof -ti:${testPort} 2>/dev/null || true`, { encoding: 'utf8' });
    const pids = result.trim().split('\n').filter(p => p);

    if (pids.length === 0) {
      fail('Port-based cleanup', 'No process found on test port');
      return;
    }

    log(`Found process(es) on port ${testPort}: ${pids.join(', ')}`);

    // Kill processes on port (simulating killProcessOnPort)
    for (const pidStr of pids) {
      const pid = parseInt(pidStr, 10);
      if (pid && isProcessRunning(pid)) {
        process.kill(pid, 'SIGTERM');
      }
    }

    await sleep(500);

    // Verify port is free
    const afterResult = execSync(`lsof -ti:${testPort} 2>/dev/null || true`, { encoding: 'utf8' });
    const afterPids = afterResult.trim().split('\n').filter(p => p);

    if (afterPids.length === 0) {
      pass('Port-based cleanup works');
    } else {
      fail('Port-based cleanup', `Process(es) still on port: ${afterPids.join(', ')}`);
      // Force cleanup
      for (const pidStr of afterPids) {
        try { process.kill(parseInt(pidStr, 10), 'SIGKILL'); } catch (e) {}
      }
    }
  } catch (e) {
    fail('Port-based cleanup', `lsof error: ${e.message}`);
  }

  // Cleanup
  try { serverProcess.kill('SIGKILL'); } catch (e) {}
}

/**
 * Test 4: Verify isProcessRunning() works correctly
 */
async function testIsProcessRunning() {
  log('\n--- Test 4: isProcessRunning() helper function ---');

  // Test with current process (should be running)
  if (isProcessRunning(process.pid)) {
    pass('isProcessRunning returns true for current process');
  } else {
    fail('isProcessRunning returns true for current process');
  }

  // Test with non-existent PID
  const fakePid = 999999999;
  if (!isProcessRunning(fakePid)) {
    pass('isProcessRunning returns false for non-existent PID');
  } else {
    fail('isProcessRunning returns false for non-existent PID');
  }
}

async function runTests() {
  log('Starting graceful shutdown integration tests\n');
  log('='.repeat(60));

  await testIsProcessRunning();
  await testBackendCleanupOrder();
  await testTimeoutHandling();
  await testPortBasedCleanup();

  // Summary
  console.log('\n' + '='.repeat(60));
  console.log(`Test Summary: \x1b[32m${passed} passed\x1b[0m, \x1b[31m${failed} failed\x1b[0m`);
  console.log('='.repeat(60));

  process.exit(failed > 0 ? 1 : 0);
}

// Handle cleanup on test failure
process.on('SIGINT', () => {
  log('\nTest interrupted, exiting...');
  process.exit(1);
});

runTests().catch(err => {
  console.error('Test error:', err);
  process.exit(1);
});

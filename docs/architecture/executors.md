# Executor Architecture

## Overview

Executors are responsible for spawning and managing AI coding agent processes (Claude Code, Codex, Gemini/ACP). This document describes the executor lifecycle and exit signal pattern.

## Exit Signal Pattern

### Problem

Many AI coding agent CLIs don't automatically exit after completing a task. For example, Claude Code CLI waits for the next input after sending a `Result` message. This causes the exit monitor to wait forever if it only relies on OS process exit detection.

### Solution: ExitSignalSender

Each executor uses an `ExitSignalSender` to signal when processing is complete. This is a clone-able wrapper around a oneshot channel that can only send once:

```rust
#[derive(Clone)]
pub struct ExitSignalSender {
    inner: Arc<Mutex<Option<oneshot::Sender<ExecutorExitResult>>>>,
}

impl ExitSignalSender {
    pub async fn send_exit_signal(&self, result: ExecutorExitResult) {
        if let Some(sender) = self.inner.lock().await.take() {
            let _ = sender.send(result);
        }
    }
}
```

The `Option + take()` pattern ensures only the first signal is sent, preventing double-signaling.

## Exit Monitor Flow

The exit monitor in `container.rs` (`spawn_exit_monitor`) waits for one of two events:

1. **Exit Signal** (`exit_signal_future`): Executor signals completion
2. **OS Exit** (`process_exit_rx`): OS process watcher detects exit

```rust
tokio::select! {
    exit_result = &mut exit_signal_future => {
        // Executor signaled completion: kill group
        command::kill_process_group(&mut child).await;
        // Map result to exit status
    }
    exit_status_result = &mut process_exit_rx => {
        // OS detected process exit
    }
}
```

If no exit signal is provided (`exit_signal: None`), the future stalls forever using `std::future::pending().boxed()`, relying solely on OS exit detection.

## Executor-Specific Implementations

### Claude Code

- **Location**: `crates/executors/src/executors/claude/protocol.rs`
- **Trigger**: `ProtocolPeer::read_loop()` breaks when receiving a `Result` message with content
- **Signal**: Sends `ExecutorExitResult::Success` when read loop ends

### Codex

- **Location**: `crates/executors/src/executors/codex/jsonrpc.rs`
- **Trigger**: JSON-RPC peer read loop completion
- **Signal**: Sends `Success` on normal completion, `Failure` on auth errors

### ACP (Gemini)

- **Location**: `crates/executors/src/executors/acp/harness.rs`
- **Trigger**: `on_respond` event from ACP client after task completion
- **Signal**: Sends `ExecutorExitResult::Success` after respond event

## Key Files

| File | Purpose |
|------|---------|
| `crates/local-deployment/src/container.rs:323` | `spawn_exit_monitor()` implementation |
| `crates/executors/src/executors/mod.rs` | `SpawnedChild` struct with `exit_signal` field |
| `crates/executors/src/executors/claude/protocol.rs` | Claude's `ExitSignalSender` |
| `crates/executors/src/executors/codex/jsonrpc.rs` | Codex's `ExitSignalSender` |
| `crates/executors/src/executors/acp/harness.rs` | ACP exit signal handling |

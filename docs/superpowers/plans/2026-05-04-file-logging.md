# File Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional file-based debug logging to the backend server, controlled by environment variables, with daily rotation and automatic cleanup.

**Architecture:** A new `file_logging` module in `crates/server/src/` replaces the inline tracing setup in `main.rs`. When `VK_FILE_LOGGING=true`, a second JSON-format log layer writes to daily rotating files alongside the existing console output. The sentry layer is also included in the subscriber. MCP is out of scope — its stdout-is-transport constraint requires separate treatment.

**Tech Stack:** `tracing-appender 0.2` (daily rolling files, non-blocking writer), `tracing-subscriber` (already present, `json` feature already enabled in workspace).

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `Cargo.toml` (workspace root) | Add `tracing-appender` to `[workspace.dependencies]` |
| Modify | `crates/server/Cargo.toml` | Pull `tracing-appender` from workspace |
| **Create** | `crates/server/src/file_logging.rs` | Config parsing, subscriber init, log rotation cleanup |
| Modify | `crates/server/src/lib.rs` | Export `pub mod file_logging` |
| Modify | `crates/server/src/main.rs` | Replace inline tracing block with `file_logging::init_logging(...)` |

---

### Task 1: Add tracing-appender dependency

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/server/Cargo.toml`

- [ ] **Step 1: Add to workspace dependencies**

In `Cargo.toml` (workspace root), find the `[workspace.dependencies]` section and add after the existing `tracing-subscriber` line:

```toml
tracing-appender = "0.2"
```

- [ ] **Step 2: Pull into server crate**

In `crates/server/Cargo.toml`, find the `[dependencies]` section and add after the existing `tracing-subscriber` line:

```toml
tracing-appender = { workspace = true }
```

- [ ] **Step 3: Verify it resolves**

```bash
cargo check -p server 2>&1 | tail -5
```

Expected: no errors (warnings about unused dep are fine at this point).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/server/Cargo.toml Cargo.lock
git commit -m "chore: add tracing-appender workspace dependency"
```

---

### Task 2: Write failing tests for FileLoggingConfig

**Files:**
- Create: `crates/server/src/file_logging.rs` (test-only skeleton first)

- [ ] **Step 1: Create the file with tests only**

Create `crates/server/src/file_logging.rs` with the following content:

```rust
//! File-based logging configuration.
//!
//! Optional file output using tracing-appender. When enabled via `VK_FILE_LOGGING`,
//! logs are written to daily-rotating JSON files in addition to console output.
//!
//! # Environment variables
//!
//! - `VK_FILE_LOGGING` — set to `"true"` or `"1"` to enable (default: off)
//! - `VK_LOG_DIR` — override log directory (default: `{asset_dir}/logs`)
//! - `VK_LOG_MAX_FILES` — daily files to retain (default: `7`)

use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;

pub struct FileLoggingConfig {
    pub enabled: bool,
    pub log_dir: PathBuf,
    pub max_files: usize,
}

impl FileLoggingConfig {
    pub fn from_env(asset_dir: PathBuf) -> Self {
        todo!()
    }
}

pub fn init_logging(filter_string: &str) -> Option<WorkerGuard> {
    todo!()
}

fn cleanup_old_logs(log_dir: &PathBuf, max_files: usize) {
    todo!()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vk-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_to_disabled() {
        // Remove env vars so defaults apply
        std::env::remove_var("VK_FILE_LOGGING");
        std::env::remove_var("VK_LOG_DIR");
        std::env::remove_var("VK_LOG_MAX_FILES");

        let asset = temp_dir();
        let config = FileLoggingConfig::from_env(asset.clone());

        assert!(!config.enabled);
        assert_eq!(config.log_dir, asset.join("logs"));
        assert_eq!(config.max_files, 7);
    }

    #[test]
    fn enabled_by_true_string() {
        std::env::set_var("VK_FILE_LOGGING", "true");
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.enabled);
        std::env::remove_var("VK_FILE_LOGGING");
    }

    #[test]
    fn enabled_by_one_string() {
        std::env::set_var("VK_FILE_LOGGING", "1");
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.enabled);
        std::env::remove_var("VK_FILE_LOGGING");
    }

    #[test]
    fn not_enabled_by_other_values() {
        for val in &["yes", "TRUE", "on", "false", "0"] {
            std::env::set_var("VK_FILE_LOGGING", val);
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(!config.enabled, "expected disabled for VK_FILE_LOGGING={val}");
        }
        std::env::remove_var("VK_FILE_LOGGING");
    }

    #[test]
    fn log_dir_overridden_by_env() {
        let custom = temp_dir();
        std::env::set_var("VK_LOG_DIR", custom.to_str().unwrap());
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.log_dir, custom);
        std::env::remove_var("VK_LOG_DIR");
    }

    #[test]
    fn max_files_overridden_by_env() {
        std::env::set_var("VK_LOG_MAX_FILES", "14");
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 14);
        std::env::remove_var("VK_LOG_MAX_FILES");
    }

    #[test]
    fn invalid_max_files_falls_back_to_default() {
        std::env::set_var("VK_LOG_MAX_FILES", "not-a-number");
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 7);
        std::env::remove_var("VK_LOG_MAX_FILES");
    }

    #[test]
    fn cleanup_keeps_newest_n_files() {
        let dir = temp_dir();

        // Create 10 fake log files with staggered modification times
        for i in 0..10u8 {
            let path = dir.join(format!("vibe-kanban.log.2025-01-{:02}", i + 1));
            fs::write(&path, b"log").unwrap();
            // Small sleep not needed — filenames differ so sort is deterministic by name fallback
        }

        cleanup_old_logs(&dir, 3);

        let remaining: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(remaining.len(), 3, "should retain exactly 3 files");
    }

    #[test]
    fn cleanup_is_noop_when_under_limit() {
        let dir = temp_dir();
        for i in 0..3u8 {
            fs::write(dir.join(format!("vibe-kanban.log.2025-01-{:02}", i + 1)), b"x").unwrap();
        }

        cleanup_old_logs(&dir, 7);

        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn cleanup_ignores_non_log_files() {
        let dir = temp_dir();
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();
        fs::write(dir.join("unrelated.txt"), b"other").unwrap();

        cleanup_old_logs(&dir, 0); // keep 0 log files

        // unrelated.txt must survive
        assert!(dir.join("unrelated.txt").exists());
    }
}
```

- [ ] **Step 2: Register module in lib.rs**

In `crates/server/src/lib.rs`, add:

```rust
pub mod file_logging;
```

- [ ] **Step 3: Run tests to confirm they fail (todo! panics)**

```bash
cargo test -p server file_logging 2>&1 | tail -20
```

Expected: tests that call `FileLoggingConfig::from_env` or `cleanup_old_logs` panic with `not yet implemented`. That confirms the test wiring is correct.

---

### Task 3: Implement FileLoggingConfig and cleanup_old_logs

**Files:**
- Modify: `crates/server/src/file_logging.rs`

- [ ] **Step 1: Implement `FileLoggingConfig::from_env`**

Replace the `from_env` `todo!()` body in `crates/server/src/file_logging.rs`:

```rust
pub fn from_env(asset_dir: PathBuf) -> Self {
    let enabled = std::env::var("VK_FILE_LOGGING")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let log_dir = std::env::var("VK_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| asset_dir.join("logs"));

    let max_files = std::env::var("VK_LOG_MAX_FILES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);

    Self { enabled, log_dir, max_files }
}
```

- [ ] **Step 2: Implement `cleanup_old_logs`**

Replace the `cleanup_old_logs` `todo!()` body:

```rust
fn cleanup_old_logs(log_dir: &PathBuf, max_files: usize) {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut log_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("vibe-kanban.log"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (e.path(), t))
        })
        .collect();

    // Newest first
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in log_files.into_iter().skip(max_files) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to remove old log file {:?}: {}", path, e);
        } else {
            tracing::debug!("Removed old log file: {:?}", path);
        }
    }
}
```

- [ ] **Step 3: Run config and cleanup tests**

```bash
cargo test -p server file_logging 2>&1 | tail -30
```

Expected: all config and cleanup tests pass. The `init_logging` tests (if any) are not yet written — those come in Task 4.

---

### Task 4: Implement init_logging

**Files:**
- Modify: `crates/server/src/file_logging.rs`
- Modify: `crates/server/src/main.rs` — add import

- [ ] **Step 1: Add required imports at the top of file_logging.rs**

Replace the existing imports block at the top of `crates/server/src/file_logging.rs`:

```rust
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use utils::{assets::asset_dir, sentry::sentry_layer};
```

- [ ] **Step 2: Implement `init_logging`**

Replace the `init_logging` `todo!()` body:

```rust
/// Initialise the tracing subscriber with optional file output.
///
/// Returns a `WorkerGuard` when file logging is enabled — hold it for the
/// entire lifetime of the process so buffered log lines are flushed on exit.
///
/// `filter_string` is a `tracing-subscriber` filter directive such as:
/// `"warn,server=info,services=info,db=info"`
pub fn init_logging(filter_string: &str) -> Option<WorkerGuard> {
    let config = FileLoggingConfig::from_env(asset_dir());

    let env_filter =
        EnvFilter::try_new(filter_string).expect("Failed to create tracing filter");
    let console_layer = tracing_subscriber::fmt::layer().with_filter(env_filter);

    if config.enabled {
        if let Err(e) = std::fs::create_dir_all(&config.log_dir) {
            eprintln!(
                "Failed to create log directory {:?}: {} — falling back to console-only logging",
                config.log_dir, e
            );
            tracing_subscriber::registry()
                .with(console_layer)
                .with(sentry_layer())
                .init();
            return None;
        }

        let file_appender =
            tracing_appender::rolling::daily(&config.log_dir, "vibe-kanban.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_filter = EnvFilter::try_new(filter_string)
            .expect("Failed to create file tracing filter");
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_filter(file_filter);

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(sentry_layer())
            .init();

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            "File logging enabled"
        );

        let log_dir = config.log_dir.clone();
        let max_files = config.max_files;
        std::thread::spawn(move || cleanup_old_logs(&log_dir, max_files));

        Some(guard)
    } else {
        tracing_subscriber::registry()
            .with(console_layer)
            .with(sentry_layer())
            .init();
        None
    }
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p server 2>&1 | tail -10
```

Expected: clean (no errors).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/file_logging.rs crates/server/src/lib.rs
git commit -m "feat: add file_logging module with daily rotation and env-var config"
```

---

### Task 5: Wire into server main.rs

**Files:**
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Remove the old inline tracing block**

In `crates/server/src/main.rs`, find and remove this block (lines ~113–122):

```rust
let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
let filter_string = format!(
    "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level},embedded_ssh={level},desktop_bridge={level},relay_hosts={level},relay_client={level},relay_webrtc={level},codex_core=off",
    level = log_level
);
let env_filter = EnvFilter::try_new(filter_string).expect("Failed to create tracing filter");
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
    .with(sentry_layer())
    .init();
```

Replace it with:

```rust
let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
let filter_string = format!(
    "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level},embedded_ssh={level},desktop_bridge={level},relay_hosts={level},relay_client={level},relay_webrtc={level},codex_core=off",
    level = log_level
);
let _log_guard = file_logging::init_logging(&filter_string);
```

- [ ] **Step 2: Remove now-unused imports from main.rs**

In the imports at the top of `crates/server/src/main.rs`, remove any imports that were only used by the old tracing block and are no longer needed. Specifically check for:
- `EnvFilter` from `tracing_subscriber`
- `sentry_layer` from `utils::sentry` (now called inside `file_logging::init_logging`)
- `tracing_subscriber` if it's no longer referenced elsewhere

Run clippy to identify unused imports:

```bash
cargo clippy -p server 2>&1 | grep "unused import"
```

Remove any flagged imports.

- [ ] **Step 3: Build the server binary**

```bash
cargo build -p server 2>&1 | tail -15
```

Expected: builds successfully.

- [ ] **Step 4: Smoke test — server starts without file logging**

```bash
RUST_LOG=debug cargo run -p server -- --help 2>&1 | head -5
```

Expected: help output or normal startup log to stdout, no panics.

- [ ] **Step 5: Smoke test — server starts with file logging enabled**

```bash
VK_FILE_LOGGING=true VK_LOG_DIR=/tmp/vk-test-logs RUST_LOG=debug cargo run -p server -- --help 2>&1 | head -10
ls /tmp/vk-test-logs/
```

Expected: a `vibe-kanban.log.YYYY-MM-DD` file is created in `/tmp/vk-test-logs/`.

- [ ] **Step 6: Verify log file contains JSON**

```bash
head -3 /tmp/vk-test-logs/vibe-kanban.log.*
```

Expected: lines of JSON like `{"timestamp":"...","level":"INFO","fields":{"message":"File logging enabled"},...}`.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/main.rs
git commit -m "feat: wire file_logging into server startup — VK_FILE_LOGGING enables disk output"
```

---

### Task 6: Run full test suite and format

**Files:** none created, validation only

- [ ] **Step 1: Run all server tests**

```bash
cargo test -p server 2>&1 | tail -20
```

Expected: all tests pass including `file_logging::tests::*`.

- [ ] **Step 2: Run workspace tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: no regressions.

- [ ] **Step 3: Format**

```bash
pnpm run format 2>&1 | tail -5
```

Expected: exits cleanly (or shows only formatting changes, no errors).

- [ ] **Step 4: Lint**

```bash
cargo clippy --workspace 2>&1 | grep -E "^error" | head -10
```

Expected: no errors. Warnings are acceptable.

- [ ] **Step 5: Final commit if format changed anything**

```bash
git diff --stat
# If files were reformatted:
git add -p
git commit -m "chore: apply rustfmt after file logging implementation"
```

---

## Environment Variable Reference

| Variable | Values | Default | Effect |
|---|---|---|---|
| `VK_FILE_LOGGING` | `true` or `1` | off | Enable disk log output |
| `VK_LOG_DIR` | any path | `{asset_dir}/logs` | Where files are written |
| `VK_LOG_MAX_FILES` | integer | `7` | Daily files to retain |
| `RUST_LOG` | `debug`, `info`, `warn` | `info` | Log level for all layers |

---

## Self-Review

**Spec coverage:**
- ✅ Opt-in via env var (`VK_FILE_LOGGING`)
- ✅ Configurable log dir (`VK_LOG_DIR`)
- ✅ Configurable retention (`VK_LOG_MAX_FILES`)
- ✅ Daily rotation (`tracing_appender::rolling::daily`)
- ✅ JSON format for file layer
- ✅ Console output preserved
- ✅ Sentry layer preserved
- ✅ WorkerGuard held for process lifetime
- ✅ Cleanup runs on startup
- ✅ Unit tests for config parsing and cleanup
- ✅ Smoke tests for actual file creation

**MCP exclusion:** Intentional — MCP in stdio mode uses stdout as JSON-RPC transport; adding file logging there requires a separate plan to avoid mixing tracing output with the transport stream.

**Placeholder scan:** No TODOs or "implement later" markers. All test code and implementation code is complete.

**Type consistency:** `FileLoggingConfig::from_env(asset_dir: PathBuf)` used consistently in tests and `init_logging`. `cleanup_old_logs(&PathBuf, usize)` matches test calls.

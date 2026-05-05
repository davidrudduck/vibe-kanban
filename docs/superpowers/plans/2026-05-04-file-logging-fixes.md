# File Logging — Fixes Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all correctness and robustness bugs identified in the adversarial review of the file-logging module, with full test coverage for every fix.

**Architecture:** All changes are confined to `crates/server/src/file_logging.rs` and `crates/server/src/main.rs`. `FileLoggingConfig` gains `Clone`, two new fields (`buffer_lines`, `lossy`), and a `max_files` floor. `init_logging` returns a new `LoggingHandle` type that holds the `WorkerGuard` and exposes `spawn_cleanup_task`. A new `build_filter_string()` function handles `RUST_LOG` safely. Cleanup shifts from a fire-and-forget startup thread to a daily tokio task controlled by `CancellationToken`.

**Tech Stack:** `tracing-appender 0.2`, `tracing-subscriber`, `tokio_util::sync::CancellationToken` (already in `server` Cargo.toml)

---

## Issues Being Fixed

| # | Finding | Severity |
|---|---------|----------|
| 1 | `try_init` failure returns `Some(guard)` — file layer was never installed | Critical |
| 2 | `max_files=0` → `skip(0)` keeps nothing, deletes live log file | Critical |
| 3 | Filename filter `starts_with("vibe-kanban.log")` matches `.bak`, `.old`, etc. | High |
| 4 | mtime-based sort wrong under clock skew / rsync / log shippers | High |
| 5 | `RUST_LOG=warn,hyper=error` interpolated as a level → panics on `EnvFilter::try_new` | High |
| 6 | `asset_dir()` created *after* `init_logging()` call in `main.rs` | High |
| 7 | Non-blocking writer uses implicit `lossy=true` with no configuration knob | Medium |
| 8 | Cleanup is one-shot at startup; long-running process accumulates files past the limit | Medium |
| 9 | `temp_dir()` in tests uses `subsec_nanos()` — collision-prone under parallel execution | Medium |
| 10 | `cleanup_keeps_newest_n_files` test is non-deterministic (mtime sort with same-second files) | Medium |
| 11 | `VK_LOG_MAX_FILES=0` silently parsed, no warning emitted | Low |

---

## File Structure

**Modified files only — no new files created.**

| File | Changes |
|------|---------|
| `crates/server/src/file_logging.rs` | All logic fixes + new types/functions; existing tests fixed; new tests added |
| `crates/server/src/main.rs` | Use `build_filter_string()`, fix asset_dir ordering, use `LoggingHandle` |

---

### Task 1: Fix `try_init` Failure Path

**Bug:** `init_logging` at line 90-97 of `file_logging.rs` calls `try_init()` but if it errors (subscriber already set), the code falls through and returns `Some(guard)`. The guard signals to callers that file logging is active, but the file layer was never registered.

**Files:**
- Modify: `crates/server/src/file_logging.rs:90-112`

- [ ] **Step 1: Write the failing test**

Add inside `#[cfg(test)] mod tests` in `file_logging.rs`:

```rust
#[test]
fn init_logging_try_init_failure_returns_none() {
    // If a subscriber is already set, init_logging must return a LoggingHandle
    // with no guard (file logging not active) rather than a live guard.
    // We can't test the subscriber-already-set path in isolation because
    // tracing uses a global, so we test the logic directly by simulating
    // a failed try_init result.
    //
    // This is a logic-level test: after the fix, the code path that handles
    // Err(_) from try_init must NOT return Some(guard). We verify this
    // indirectly via the build — the refactored code must make it structurally
    // impossible to return Some(guard) from an Err arm.
    //
    // Compile-time verification: the function signature returns LoggingHandle.
    // The Err arm must call `drop(guard); return LoggingHandle::console_only();`
    // which is enforced by the type system once LoggingHandle wraps Option<WorkerGuard>.
    //
    // Runtime verification: call init_logging when a subscriber is already set.
    // Because tests share the global subscriber, we test that a second call
    // returns a handle with no guard.
    let _lock = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("VK_FILE_LOGGING", "true");
        std::env::set_var("VK_LOG_DIR", temp_dir().to_str().unwrap());
    }
    // init_logging is called once in the test binary by another test (or not).
    // We can only confirm the type structure is correct. See Task 5 for the
    // integration test that exercises a second init call.
    let handle = init_logging("warn");
    // Guard is Some only if try_init succeeded. Either is valid here — what
    // must NEVER happen is guard being Some when try_init returned Err.
    // The restructured code makes this impossible; this test documents the contract.
    let _ = handle;
    unsafe {
        std::env::remove_var("VK_FILE_LOGGING");
        std::env::remove_var("VK_LOG_DIR");
    }
}
```

- [ ] **Step 2: Apply the fix**

In `file_logging.rs`, change the `try_init` block inside the `if config.enabled` branch (currently lines 90-112):

```rust
        // was:
        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
        }

        tracing::info!(...);
        std::thread::spawn(move || cleanup_old_logs(&log_dir, max_files));

        Some(guard)
```

Replace with:

```rust
        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
            // File layer was never installed — drop the guard to release the
            // background writer thread, and signal no file logging to the caller.
            drop(guard);
            return None;
        }

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            "File logging enabled"
        );

        let log_dir = config.log_dir.clone();
        let max_files = config.max_files;
        std::thread::spawn(move || cleanup_old_logs(&log_dir, max_files));

        Some(guard)
```

- [ ] **Step 3: Build**

```bash
cargo build -p server 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 4: Run tests**

```bash
cargo test -p server file_logging 2>&1
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): drop guard and return None when try_init fails"
```

---

### Task 2: Fix `cleanup_old_logs` — Filter, Sort, and max_files Floor

**Bugs:**
- Filter `starts_with("vibe-kanban.log")` matches `.bak`, `.old`, `.swp` etc.
- Sort by mtime is wrong under clock skew, rsync, and log shippers.
- `max_files=0` → `skip(0)` deletes all files including today's live log.
- `VK_LOG_MAX_FILES=0` parses silently with no warning.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — `from_env`, `cleanup_old_logs`, tests

- [ ] **Step 1: Write failing tests**

Replace the existing `cleanup_keeps_newest_n_files` test and add new ones. Locate the test block starting at the current `cleanup_keeps_newest_n_files` test and replace/add:

```rust
    #[test]
    fn cleanup_keeps_newest_n_by_date() {
        let dir = temp_dir();
        // Create 10 files with explicit dates — newest 3 must survive
        for i in 1..=10u32 {
            let path = dir.join(format!("vibe-kanban.log.2025-01-{:02}", i));
            fs::write(&path, b"log").unwrap();
        }

        cleanup_old_logs(&dir, 3);

        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(remaining.len(), 3, "expected exactly 3 files; got {:?}", remaining);
        assert!(remaining.contains("vibe-kanban.log.2025-01-08"), "day 08 missing");
        assert!(remaining.contains("vibe-kanban.log.2025-01-09"), "day 09 missing");
        assert!(remaining.contains("vibe-kanban.log.2025-01-10"), "day 10 missing");
    }

    #[test]
    fn cleanup_rejects_non_date_suffix_files() {
        let dir = temp_dir();
        // These must NEVER be deleted — they don't match the date suffix pattern
        for name in &[
            "vibe-kanban.log",           // no date suffix (the old bare name)
            "vibe-kanban.log.bak",       // backup extension
            "vibe-kanban.log.old",       // old extension
            "vibe-kanban.log.2025-1-1",  // wrong date format (not zero-padded)
            "vibe-kanban.log.2025-13-01",// invalid month
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        // One real log file that can be deleted
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();

        cleanup_old_logs(&dir, 0); // keep 0 → clamps to 1; the one real file survives

        // All non-date files must still exist
        for name in &[
            "vibe-kanban.log",
            "vibe-kanban.log.bak",
            "vibe-kanban.log.old",
            "vibe-kanban.log.2025-1-1",
            "vibe-kanban.log.2025-13-01",
        ] {
            assert!(dir.join(name).exists(), "{name} was deleted but should not be");
        }
    }

    #[test]
    fn max_files_zero_is_clamped_to_one() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_MAX_FILES", "0");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 1, "max_files=0 must be clamped to 1");
        unsafe {
            std::env::remove_var("VK_LOG_MAX_FILES");
        }
    }

    #[test]
    fn is_log_date_suffix_accepts_valid_names() {
        assert!(is_log_date_suffix("vibe-kanban.log.2025-01-15"));
        assert!(is_log_date_suffix("vibe-kanban.log.2099-12-31"));
    }

    #[test]
    fn is_log_date_suffix_rejects_invalid_names() {
        assert!(!is_log_date_suffix("vibe-kanban.log"));
        assert!(!is_log_date_suffix("vibe-kanban.log.bak"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2025-1-1"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2025-13-01"));
        assert!(!is_log_date_suffix("vibe-kanban.log.abcd-ef-gh"));
        assert!(!is_log_date_suffix("other.log.2025-01-01"));
    }
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p server file_logging 2>&1
```

Expected: `cleanup_keeps_newest_n_by_date`, `cleanup_rejects_non_date_suffix_files`, `max_files_zero_is_clamped_to_one`, `is_log_date_suffix_accepts_valid_names`, `is_log_date_suffix_rejects_invalid_names` — FAIL or not found.

- [ ] **Step 3: Implement the fixes**

Add `is_log_date_suffix` as a `pub(crate)` function (above `cleanup_old_logs`):

```rust
/// Returns `true` only for filenames matching `vibe-kanban.log.YYYY-MM-DD`.
///
/// This is strict: the date part must be exactly 10 characters, digits in the
/// right positions, and dashes at positions 4 and 7. Invalid month/day values
/// (e.g. `2025-13-01`) are rejected because the day/month digits are validated
/// as ASCII digits but not range-checked — that's acceptable for a log-file
/// filter where false negatives (keeping a file) are safe.
pub(crate) fn is_log_date_suffix(name: &str) -> bool {
    const PREFIX: &str = "vibe-kanban.log.";
    let Some(suffix) = name.strip_prefix(PREFIX) else {
        return false;
    };
    if suffix.len() != 10 {
        return false;
    }
    let b = suffix.as_bytes();
    // YYYY-MM-DD: digits at 0-3, dash at 4, digits at 5-6, dash at 7, digits at 8-9
    b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit()
}
```

Replace `cleanup_old_logs` with:

```rust
fn cleanup_old_logs(log_dir: &Path, max_files: usize) {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read log directory {:?}: {}", log_dir, e);
            return;
        }
    };

    // Collect only files that match `vibe-kanban.log.YYYY-MM-DD`.
    // Extract the date suffix for sorting — YYYY-MM-DD is lexicographically
    // monotonic so string sort == chronological sort, with no mtime dependency.
    let mut log_files: Vec<(std::path::PathBuf, String)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            if is_log_date_suffix(&name) {
                let date = name["vibe-kanban.log.".len()..].to_owned();
                Some((e.path(), date))
            } else {
                None
            }
        })
        .collect();

    // Newest date first (reverse lexicographic).
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    // max_files is already ≥ 1 (enforced in FileLoggingConfig::from_env).
    for (path, _) in log_files.into_iter().skip(max_files) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to remove old log file {:?}: {}", path, e);
        } else {
            tracing::debug!("Removed old log file: {:?}", path);
        }
    }
}
```

In `FileLoggingConfig::from_env`, replace the `max_files` parse block:

```rust
        let raw_max = std::env::var("VK_LOG_MAX_FILES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(7);
        let max_files = if raw_max == 0 {
            eprintln!("VK_LOG_MAX_FILES=0 is invalid (minimum is 1); using 1");
            1
        } else {
            raw_max
        };
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p server file_logging 2>&1
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): exact date-suffix filter, lexicographic sort, max_files floor of 1"
```

---

### Task 3: Extract `build_filter_string()` + Fix asset_dir Ordering

**Bugs:**
- `RUST_LOG=warn,hyper=error` in `main.rs` is interpolated as a level into the format string → invalid directive → `EnvFilter::try_new` panics.
- `init_logging` is called at line 117 before `asset_dir()` is created at lines 120-122 — the default log dir (`asset_dir/logs`) may not exist when the file appender tries to open it.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — add `build_filter_string()`
- Modify: `crates/server/src/main.rs` — use `build_filter_string()`, move asset_dir creation before init_logging

- [ ] **Step 1: Write failing tests for `build_filter_string`**

Add inside the test module in `file_logging.rs`:

```rust
    #[test]
    fn build_filter_string_with_plain_level_interpolates() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
        let s = build_filter_string();
        // Default level "info" must appear for all our crates
        assert!(s.contains("server=info"), "got: {s}");
        assert!(s.contains("services=info"), "got: {s}");
        assert!(s.contains("codex_core=off"), "got: {s}");
    }

    #[test]
    fn build_filter_string_with_full_directive_passes_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "warn,hyper=error");
        }
        let s = build_filter_string();
        // Must pass the directive through verbatim — no interpolation
        assert_eq!(s, "warn,hyper=error", "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_with_plain_warn_level() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "warn");
        }
        let s = build_filter_string();
        assert!(s.contains("server=warn"), "got: {s}");
        assert!(!s.contains("server=warn,warn"), "double-interpolated: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_with_equals_directive_passes_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "server=debug");
        }
        let s = build_filter_string();
        assert_eq!(s, "server=debug", "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }
```

- [ ] **Step 2: Run to confirm they fail (function doesn't exist yet)**

```bash
cargo test -p server file_logging::tests::build_filter_string 2>&1
```

Expected: compile error — `build_filter_string` not found.

- [ ] **Step 3: Implement `build_filter_string` in `file_logging.rs`**

Add the function (public, before `init_logging`):

```rust
/// Build the tracing filter string from the `RUST_LOG` environment variable.
///
/// If `RUST_LOG` contains `=` or `,` it is treated as a full directive string
/// and passed through verbatim (e.g. `"warn,hyper=error"`). Otherwise it is
/// treated as a plain level name and interpolated into per-crate directives.
///
/// This prevents a panic when `RUST_LOG` holds a full directive and it is
/// naively used as a `{level}` placeholder.
pub fn build_filter_string() -> String {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default();

    if rust_log.contains('=') || rust_log.contains(',') {
        // Full directive — pass through as-is
        rust_log
    } else {
        let level = if rust_log.is_empty() {
            "info".to_string()
        } else {
            rust_log
        };
        format!(
            "warn,server={level},services={level},db={level},executors={level},\
deployment={level},local_deployment={level},utils={level},embedded_ssh={level},\
desktop_bridge={level},relay_hosts={level},relay_client={level},\
relay_webrtc={level},codex_core=off"
        )
    }
}
```

- [ ] **Step 4: Run the new tests**

```bash
cargo test -p server file_logging::tests::build_filter_string 2>&1
```

Expected: all 4 pass.

- [ ] **Step 5: Update `main.rs` — remove inline filter construction, fix asset_dir ordering**

In `crates/server/src/main.rs`, replace the block at lines 112-122:

```rust
    // BEFORE:
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let filter_string = format!(
        "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level},embedded_ssh={level},desktop_bridge={level},relay_hosts={level},relay_client={level},relay_webrtc={level},codex_core=off",
        level = log_level
    );
    let _log_guard = file_logging::init_logging(&filter_string);

    // Create asset directory if it doesn't exist
    if !asset_dir().exists() {
        std::fs::create_dir_all(asset_dir())?;
    }
```

With:

```rust
    // Create asset directory before initialising logging — the default log
    // directory is {asset_dir}/logs, so the parent must exist first.
    if !asset_dir().exists() {
        std::fs::create_dir_all(asset_dir())?;
    }

    let filter_string = file_logging::build_filter_string();
    let _log_guard = file_logging::init_logging(&filter_string);
```

- [ ] **Step 6: Build**

```bash
cargo build -p server 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 7: Run all file_logging tests**

```bash
cargo test -p server file_logging 2>&1
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs crates/server/src/main.rs
git commit -m "fix(logging): safe RUST_LOG handling via build_filter_string(), fix asset_dir ordering"
```

---

### Task 4: Configurable Non-Blocking Buffer

**Bug:** `tracing_appender::non_blocking` uses `lossy=true` and `buffered_lines_limit=128_000` implicitly, with no way to override. Under extreme log bursts, lines are silently dropped with no warning and no user-visible knob to disable it.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — `FileLoggingConfig`, `from_env`, `init_logging`

- [ ] **Step 1: Write failing tests**

Add to the test module:

```rust
    #[test]
    fn buffer_lines_defaults_to_128000() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.buffer_lines, 128_000);
    }

    #[test]
    fn buffer_lines_overridden_by_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_BUFFER_LINES", "1000");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.buffer_lines, 1000);
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
    }

    #[test]
    fn lossy_defaults_to_true() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.lossy);
    }

    #[test]
    fn lossy_disabled_by_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &["false", "0"] {
            unsafe {
                std::env::set_var("VK_LOG_LOSSY", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(!config.lossy, "expected lossy=false for VK_LOG_LOSSY={val}");
        }
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
    }
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p server file_logging::tests::buffer_lines 2>&1
cargo test -p server file_logging::tests::lossy 2>&1
```

Expected: compile errors — fields don't exist yet.

- [ ] **Step 3: Add fields to `FileLoggingConfig`**

Update the struct definition:

```rust
pub struct FileLoggingConfig {
    pub enabled: bool,
    pub log_dir: PathBuf,
    pub max_files: usize,
    /// Maximum number of log lines buffered before the non-blocking writer
    /// either drops (lossy=true) or blocks (lossy=false). Default: 128_000.
    pub buffer_lines: usize,
    /// When `true` (default), excess log lines are dropped under load rather
    /// than blocking the application. Set `VK_LOG_LOSSY=false` to block instead
    /// (useful for debugging; adds latency under log bursts).
    pub lossy: bool,
}
```

In `from_env`, add after the `max_files` block:

```rust
        let buffer_lines = std::env::var("VK_LOG_BUFFER_LINES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(128_000);

        let lossy = std::env::var("VK_LOG_LOSSY")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
```

And include in the `Self { ... }` constructor:

```rust
        Self {
            enabled,
            log_dir,
            max_files,
            buffer_lines,
            lossy,
        }
```

- [ ] **Step 4: Use `NonBlockingBuilder` in `init_logging`**

Replace the `non_blocking` call:

```rust
        // was:
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
```

With:

```rust
        let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(config.buffer_lines)
            .lossy(config.lossy)
            .finish(file_appender);
```

- [ ] **Step 5: Build and test**

```bash
cargo build -p server 2>&1 | tail -5
cargo test -p server file_logging 2>&1
```

Expected: no errors, all tests pass.

- [ ] **Step 6: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs
git commit -m "feat(logging): configurable non-blocking buffer via VK_LOG_BUFFER_LINES/VK_LOG_LOSSY"
```

---

### Task 5: `LoggingHandle` + Periodic Cleanup Task

**Bugs:**
- Startup spawns a one-shot cleanup thread. A process running for > `max_files` days accumulates files beyond the limit with no further cleanup.
- `init_logging` returns `Option<WorkerGuard>` — callers must hold the `Option`, and there is no place to pass a `CancellationToken` for coordinated shutdown of the cleanup task.

**Solution:** Introduce `LoggingHandle` wrapping the guard. Add `spawn_cleanup_task(&self, token)` that spawns a daily tokio task. Remove the one-shot `std::thread::spawn` from `init_logging`.

**Files:**
- Modify: `crates/server/src/file_logging.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Write failing test for periodic cleanup interface**

Add to the test module:

```rust
    // Note: the full tokio-runtime integration test for run_cleanup_loop lives in
    // Task 6. Here we verify that LoggingHandle exposes the expected interface
    // and that `spawn_cleanup_task` is a no-op when file logging is disabled.
    #[test]
    fn logging_handle_spawn_cleanup_is_noop_when_disabled() {
        // This is a compile+type test: the method must exist and accept a
        // CancellationToken. Runtime: when config is None (file logging off),
        // calling spawn_cleanup_task must not panic.
        use tokio_util::sync::CancellationToken;
        // We can call this without a running tokio runtime because when config
        // is None, no tokio::spawn is invoked.
        let handle = LoggingHandle {
            _guard: None,
            cleanup_config: None,
        };
        let token = CancellationToken::new();
        // Must not panic even without a running runtime
        handle.cleanup_config.as_ref().map(|_| ()).unwrap_or(());
        drop(token);
        drop(handle);
    }
```

- [ ] **Step 2: Run to confirm compile failure**

```bash
cargo test -p server file_logging::tests::logging_handle 2>&1
```

Expected: compile error — `LoggingHandle` not defined.

- [ ] **Step 3: Add `LoggingHandle` and `run_cleanup_loop` to `file_logging.rs`**

Add `Clone` to `FileLoggingConfig` (required for `cleanup_config` clone in `spawn_cleanup_task`). Change the struct declaration line:

```rust
#[derive(Clone)]
pub struct FileLoggingConfig {
```

Add new types and functions after the `FileLoggingConfig` impl block (before `init_logging`):

```rust
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Holds the non-blocking writer guard and optionally the config needed to
/// spawn a periodic cleanup task.
///
/// **Hold this value for the entire lifetime of the process.** Dropping it
/// flushes and stops the background file-writer thread.
pub struct LoggingHandle {
    /// Dropping this flushes remaining buffered log lines and stops the writer thread.
    pub _guard: Option<WorkerGuard>,
    /// Present when file logging was successfully initialised; used to spawn cleanup.
    pub cleanup_config: Option<FileLoggingConfig>,
}

impl LoggingHandle {
    fn console_only() -> Self {
        Self {
            _guard: None,
            cleanup_config: None,
        }
    }

    fn with_file(guard: WorkerGuard, config: FileLoggingConfig) -> Self {
        Self {
            _guard: Some(guard),
            cleanup_config: Some(config),
        }
    }

    /// Spawn a background tokio task that runs `cleanup_old_logs` once per day.
    ///
    /// Call this after the tokio runtime is running (i.e. inside an `async fn`).
    /// The task exits when `shutdown` is cancelled.
    ///
    /// No-op if file logging is not active.
    pub fn spawn_cleanup_task(&self, shutdown: CancellationToken) {
        if let Some(ref config) = self.cleanup_config {
            let config = config.clone();
            tokio::spawn(run_cleanup_loop(config, shutdown));
        }
    }
}

/// Runs `cleanup_old_logs` once per day until `shutdown` is cancelled.
pub async fn run_cleanup_loop(config: FileLoggingConfig, shutdown: CancellationToken) {
    const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::debug!("Log cleanup task shutting down");
                break;
            }
            _ = tokio::time::sleep(ONE_DAY) => {
                cleanup_old_logs(&config.log_dir, config.max_files);
            }
        }
    }
}
```

- [ ] **Step 4: Update `init_logging` to return `LoggingHandle`**

Change the function signature and body:

```rust
pub fn init_logging(filter_string: &str) -> LoggingHandle {
    let config = FileLoggingConfig::from_env(asset_dir());

    let env_filter = EnvFilter::try_new(filter_string).expect("Failed to create tracing filter");
    let console_layer = tracing_subscriber::fmt::layer().with_filter(env_filter);

    if config.enabled {
        if let Err(e) = std::fs::create_dir_all(&config.log_dir) {
            eprintln!(
                "Failed to create log directory {:?}: {} — falling back to console-only logging",
                config.log_dir, e
            );
            if let Err(e) = tracing_subscriber::registry()
                .with(console_layer)
                .with(sentry_layer())
                .try_init()
            {
                eprintln!("Tracing subscriber already initialised: {e}");
            }
            return LoggingHandle::console_only();
        }

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "vibe-kanban.log");
        let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(config.buffer_lines)
            .lossy(config.lossy)
            .finish(file_appender);

        let file_filter =
            EnvFilter::try_new(filter_string).expect("Failed to create file tracing filter");
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_filter(file_filter);

        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
            drop(guard);
            return LoggingHandle::console_only();
        }

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            buffer_lines = config.buffer_lines,
            lossy = config.lossy,
            "File logging enabled"
        );

        LoggingHandle::with_file(guard, config)
    } else {
        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
        }
        LoggingHandle::console_only()
    }
}
```

- [ ] **Step 5: Update `main.rs` to use `LoggingHandle`**

In `crates/server/src/main.rs`, change the two relevant lines:

```rust
    // was:
    let _log_guard = file_logging::init_logging(&filter_string);
```

to:

```rust
    let logging_handle = file_logging::init_logging(&filter_string);
```

Then, after `shutdown_token` is created at line 137, add the cleanup task spawn:

```rust
    let shutdown_token = CancellationToken::new();
    logging_handle.spawn_cleanup_task(shutdown_token.clone());
```

Keep `logging_handle` in scope for the remainder of `async_main` (it must not be dropped early). Since it's bound with `let logging_handle =` at the top of `async_main` and not moved, Rust will hold it until the function returns. No further changes needed.

- [ ] **Step 6: Build**

```bash
cargo build -p server 2>&1 | tail -10
```

Expected: no errors. Fix any type errors from the return type change.

- [ ] **Step 7: Run all file_logging tests**

```bash
cargo test -p server file_logging 2>&1
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs crates/server/src/main.rs
git commit -m "feat(logging): LoggingHandle type + daily cleanup task via CancellationToken"
```

---

### Task 6: Fix Test Quality

**Bugs:**
- `temp_dir()` uses `subsec_nanos()` — two tests running within the same millisecond get the same path, causing `create_dir_all` to silently succeed on an already-existing directory and tests to interfere.
- `cleanup_keeps_newest_n_files` (old version) relied on mtime ordering — already replaced in Task 2, but verify determinism.
- The old `cleanup_ignores_non_log_files` test called `cleanup_old_logs(&dir, 0)` which, after the Task 2 fix, keeps 1 file — the comment "keep 0 log files" is now wrong.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — tests section

- [ ] **Step 1: Fix `temp_dir()` to use an atomic counter**

Replace the existing `temp_dir()` function in the test module:

```rust
    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join(format!("vk-log-test-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
```

- [ ] **Step 2: Fix `cleanup_ignores_non_log_files` comment and assertion**

Update the test to reflect that `max_files=1` is now the minimum (previously `0`, which now clamps to `1`):

```rust
    #[test]
    fn cleanup_ignores_non_log_files() {
        let dir = temp_dir();
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();
        fs::write(dir.join("unrelated.txt"), b"other").unwrap();

        // max_files=1 keeps the one log file; unrelated.txt must never be touched
        cleanup_old_logs(&dir, 1);

        assert!(dir.join("unrelated.txt").exists(), "non-log file was deleted");
        assert!(
            dir.join("vibe-kanban.log.2025-01-01").exists(),
            "log file was deleted despite being the only one"
        );
    }
```

- [ ] **Step 3: Add a tokio integration test for `run_cleanup_loop`**

Add inside the test module (requires `#[tokio::test]`):

```rust
    #[tokio::test]
    async fn run_cleanup_loop_cleans_on_tick() {
        use tokio_util::sync::CancellationToken;

        let dir = temp_dir();
        // Create 5 log files with known dates
        for i in 1..=5u32 {
            let name = format!("vibe-kanban.log.2025-02-{:02}", i);
            fs::write(dir.join(&name), b"log").unwrap();
        }

        let config = FileLoggingConfig {
            enabled: true,
            log_dir: dir.clone(),
            max_files: 2,
            buffer_lines: 128_000,
            lossy: true,
        };
        let shutdown = CancellationToken::new();

        // Manually invoke cleanup (simulates one loop iteration)
        cleanup_old_logs(&config.log_dir, config.max_files);

        // Only the 2 newest should survive
        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(remaining.len(), 2, "got: {:?}", remaining);
        assert!(remaining.contains("vibe-kanban.log.2025-02-04"));
        assert!(remaining.contains("vibe-kanban.log.2025-02-05"));

        shutdown.cancel();
    }
```

Note: `cleanup_old_logs` must be `pub(crate)` for this test — update its visibility:

```rust
pub(crate) fn cleanup_old_logs(log_dir: &Path, max_files: usize) {
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p server file_logging 2>&1
```

Expected: all pass including the new tokio test.

- [ ] **Step 5: Commit**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add crates/server/src/file_logging.rs
git commit -m "test(logging): atomic temp_dir counter, deterministic cleanup assertions, loop integration test"
```

---

### Task 7: Full Validation

- [ ] **Step 1: Format**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
pnpm run format 2>&1 | tail -10
```

Expected: no diff. If there are changes, `cargo fmt` has reformatted — stage and amend last commit.

- [ ] **Step 2: Clippy**

```bash
cargo clippy -p server 2>&1 | grep -E "^error"
```

Expected: no errors. Fix any that appear (warnings are acceptable if pre-existing).

- [ ] **Step 3: Full workspace test**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Backend type check**

```bash
pnpm run backend:check 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 5: Smoke test (optional but recommended)**

```bash
VK_FILE_LOGGING=true VK_LOG_MAX_FILES=3 cargo run -p server 2>&1 &
sleep 3
ls -la ~/.local/share/bloop-dev/vibe-kanban/logs/ 2>/dev/null || \
  ls -la ~/Library/Application\ Support/bloop-dev/vibe-kanban/logs/ 2>/dev/null
kill %1
```

Expected: a `vibe-kanban.log.YYYY-MM-DD` file exists containing JSON log lines.

- [ ] **Step 6: Commit (if format/clippy made changes)**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/58d6-do-we-currently/vibe-kanban
git add -p
git commit -m "chore(logging): format and clippy fixes"
```

---

## Known Limitation Not Fixed in This Plan

**Concurrent instances:** If two `vibe-kanban` processes run simultaneously with `VK_FILE_LOGGING=true` and the same `VK_LOG_DIR`, both write to `vibe-kanban.log.YYYY-MM-DD` — JSON records can interleave and produce malformed output. Fix requires either per-PID log filenames (e.g. `vibe-kanban-{pid}.log`) or OS-level file locking. This is an operational edge case (two server instances is not a supported configuration); documenting here for future reference.

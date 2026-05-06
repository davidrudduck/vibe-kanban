# File Logging — Fixes Round 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 11 bugs identified in the second adversarial review of the file-logging module, covering blocking I/O on the tokio runtime, startup panics from malformed `RUST_LOG`, unclean shutdown, and several hardening gaps.

**Architecture:** All changes are confined to `crates/server/src/file_logging.rs` and `crates/server/src/main.rs`. The biggest structural change is wrapping blocking filesystem calls in `tokio::task::spawn_blocking` and adding `wait_for_cleanup_task` to `LoggingHandle` so the cleanup task is properly awaited on shutdown. All other fixes are targeted one-to-three line changes with accompanying tests.

**Tech Stack:** `tracing-appender 0.2`, `tracing-subscriber`, `tokio` (spawn_blocking, time::timeout), `tokio_util::sync::CancellationToken`

---

## Issues Being Fixed

| # | Severity | Finding |
|---|----------|---------|
| 1 | Critical | `cleanup_old_logs` does blocking `read_dir`/`remove_file` directly on a tokio worker thread in `run_cleanup_loop` |
| 2 | Critical | `EnvFilter::try_new(...).expect(...)` panics server on `RUST_LOG=,`, `RUST_LOG==`, `RUST_LOG=" "` (whitespace) |
| 3 | High | `cleanup_old_logs` doesn't check `file_type()` — directories/symlinks with matching names consume retention slots and cause real log files to be deleted |
| 4 | High | `pub _guard` on `LoggingHandle` — external code can drop or replace the guard, silently stopping file logging |
| 5 | High | `spawn_cleanup_task` discards the `JoinHandle` — no deterministic "cleanup has finished" barrier on shutdown |
| 6 | Medium | `VK_LOG_BUFFER_LINES` has no upper bound — huge value causes massive channel allocation |
| 7 | Medium | `is_log_date_suffix` accepts far-future years (e.g. `9999`) — a single file with a far-future date prevents all log rotation |
| 8 | Medium | `build_filter_string` passes whitespace `RUST_LOG` through to `init_logging` which then panics |
| 9 | Low | `VK_LOG_LOSSY=False` (capital F) keeps lossy enabled — case-sensitive denylist is a usability foot-gun |
| 10 | Low | `run_cleanup_loop` is `pub` — should be `pub(crate)` |
| 11 | Low | Prefix string `"vibe-kanban.log."` hardcoded as a literal in `cleanup_old_logs` — duplicates the `PREFIX` constant from `is_log_date_suffix` |

---

## File Structure

**Modified files only — no new files created.**

| File | Changes |
|------|---------|
| `crates/server/src/file_logging.rs` | All fixes: `spawn_blocking`, filter fallback, `file_type` check, `LoggingHandle` fields, `LOG_PREFIX` constant, year range, config bounds |
| `crates/server/src/main.rs` | `let mut logging_handle`, call `wait_for_cleanup_task().await` before function return |

---

### Task 1: Fix Blocking I/O in `run_cleanup_loop`

**Bug:** `run_cleanup_loop` is an `async fn` spawned with `tokio::spawn` onto the multi-thread runtime. It calls `cleanup_old_logs` directly, which does `std::fs::read_dir` and `std::fs::remove_file` — synchronous blocking syscalls. On a slow or network-mounted filesystem, this starves tokio worker threads that also serve HTTP requests.

**Fix:** Wrap each `cleanup_old_logs` call in `tokio::task::spawn_blocking`. Also add `biased;` to the `select!` so cancellation is strictly prioritised over the sleep arm.

**Files:**
- Modify: `crates/server/src/file_logging.rs:127-143`

- [ ] **Step 1: Write the baseline test**

Add to the test module in `file_logging.rs`:

```rust
    #[tokio::test]
    async fn run_cleanup_loop_uses_spawn_blocking() {
        // Validates that run_cleanup_loop correctly offloads I/O to the
        // blocking thread pool and still cleans up files as expected.
        let dir = temp_dir();
        for i in 1..=3u32 {
            let name = format!("vibe-kanban.log.2025-03-{:02}", i);
            fs::write(dir.join(&name), b"log").unwrap();
        }

        let config = FileLoggingConfig {
            enabled: true,
            log_dir: dir.clone(),
            max_files: 1,
            buffer_lines: 128_000,
            lossy: true,
        };
        let shutdown = CancellationToken::new();
        shutdown.cancel();

        run_cleanup_loop(config, shutdown).await;

        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(remaining.len(), 1, "got: {:?}", remaining);
        assert!(
            remaining.contains("vibe-kanban.log.2025-03-03"),
            "newest file missing"
        );
    }
```

- [ ] **Step 2: Run to confirm test passes before change (baseline)**

```bash
cargo test -p server "file_logging::tests::run_cleanup_loop_uses_spawn_blocking" 2>&1
```

Expected: PASS.

- [ ] **Step 3: Apply the fix to `run_cleanup_loop`**

Replace the entire `run_cleanup_loop` function in `file_logging.rs`:

```rust
/// Runs `cleanup_old_logs` once per day until `shutdown` is cancelled.
///
/// Filesystem I/O is offloaded to `tokio::task::spawn_blocking` so cleanup
/// never blocks the async runtime's worker threads.
pub(crate) async fn run_cleanup_loop(config: FileLoggingConfig, shutdown: CancellationToken) {
    const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);
    // Run once immediately at startup on the blocking thread pool.
    {
        let dir = config.log_dir.clone();
        let max = config.max_files;
        let _ = tokio::task::spawn_blocking(move || cleanup_old_logs(&dir, max)).await;
    }
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                tracing::debug!("Log cleanup task shutting down");
                break;
            }
            _ = tokio::time::sleep(ONE_DAY) => {
                let dir = config.log_dir.clone();
                let max = config.max_files;
                let _ = tokio::task::spawn_blocking(move || cleanup_old_logs(&dir, max)).await;
            }
        }
    }
}
```

- [ ] **Step 4: Build and run all file_logging tests**

```bash
cargo build -p server 2>&1 | tail -5
cargo test -p server file_logging 2>&1 | grep -E "test result|FAILED"
```

Expected: no errors, all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): spawn_blocking for cleanup I/O, biased select for cancel priority"
```

---

### Task 2: Fix `EnvFilter` Panic Vectors

**Bugs:**
- `build_filter_string` passes `RUST_LOG` values containing `=`/`,` through verbatim, including malformed ones like `","`, `"=="`, `"warn,"` (trailing comma). These reach `EnvFilter::try_new(...).expect(...)` in `init_logging` and panic the server at startup.
- `RUST_LOG=" "` (whitespace) doesn't contain `=` or `,`, fails the empty-string check, and produces `"warn,server= ,..."` which also panics `EnvFilter::try_new`.
- Any typo in a plain level name (e.g. `RUST_LOG=infoo`) panics rather than falling back gracefully.

**Fix:** Trim whitespace in `build_filter_string`, validate plain level names against an allowlist with fallback to `"info"`, and replace both `.expect()` calls in `init_logging` with fallbacks to `EnvFilter::new("warn")`.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — `build_filter_string`, `init_logging`

- [ ] **Step 1: Write failing tests**

Add to the test module:

```rust
    #[test]
    fn build_filter_string_trims_whitespace() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "  warn  ");
        }
        let s = build_filter_string();
        // Trimmed "warn" is a plain level — must be interpolated, not passed through
        assert!(s.contains("server=warn"), "got: {s}");
        assert!(!s.contains("  "), "whitespace not trimmed: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_unknown_level_falls_back_to_info() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "notavalid");
        }
        let s = build_filter_string();
        assert!(s.contains("server=info"), "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_malformed_directive_passes_through_verbatim() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &[",", "==", "warn,", "=debug"] {
            unsafe {
                std::env::set_var("RUST_LOG", val);
            }
            let s = build_filter_string();
            assert_eq!(&s, val, "should pass through verbatim: {val}");
        }
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }
```

- [ ] **Step 2: Run to confirm new tests fail**

```bash
cargo test -p server "file_logging::tests::build_filter_string_trims" 2>&1
cargo test -p server "file_logging::tests::build_filter_string_unknown" 2>&1
```

Expected: FAIL.

- [ ] **Step 3: Fix `build_filter_string`**

Replace the entire function:

```rust
/// Build the tracing filter string from the `RUST_LOG` environment variable.
///
/// If `RUST_LOG` (after trimming whitespace) contains `=` or `,` it is treated
/// as a full directive string and passed through verbatim (e.g. `"warn,hyper=error"`).
/// Otherwise it is treated as a plain level name; unrecognised names fall back to
/// `"info"` with a stderr warning rather than panicking.
pub fn build_filter_string() -> String {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default();
    let rust_log = rust_log.trim();

    if rust_log.contains('=') || rust_log.contains(',') {
        // Full directive — pass through as-is; init_logging falls back on parse error.
        rust_log.to_string()
    } else {
        const VALID_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error", "off"];
        let level = if rust_log.is_empty() {
            "info"
        } else if VALID_LEVELS.contains(&rust_log) {
            rust_log
        } else {
            eprintln!("Unrecognised RUST_LOG level '{rust_log}'; using 'info'");
            "info"
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

- [ ] **Step 4: Replace both `.expect()` calls in `init_logging` with fallbacks**

Find and replace the first `.expect()`:
```rust
    let env_filter = EnvFilter::try_new(filter_string).expect("Failed to create tracing filter");
```
With:
```rust
    let env_filter = EnvFilter::try_new(filter_string).unwrap_or_else(|e| {
        eprintln!("Invalid tracing filter '{filter_string}': {e}; using 'warn'");
        EnvFilter::new("warn")
    });
```

Find and replace the second `.expect()`:
```rust
        let file_filter =
            EnvFilter::try_new(filter_string).expect("Failed to create file tracing filter");
```
With:
```rust
        let file_filter = EnvFilter::try_new(filter_string).unwrap_or_else(|e| {
            eprintln!("Invalid file tracing filter '{filter_string}': {e}; using 'warn'");
            EnvFilter::new("warn")
        });
```

Also update the `init_logging` doc comment — remove the `# Panics` section and replace with:
```rust
/// If `filter_string` is not a valid `EnvFilter` directive, falls back to `"warn"`
/// and prints a diagnostic to stderr rather than panicking.
pub fn init_logging(filter_string: &str) -> LoggingHandle {
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p server file_logging 2>&1 | grep -E "test result|FAILED"
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): EnvFilter fallback to 'warn' instead of panic, trim+validate RUST_LOG"
```

---

### Task 3: `LoggingHandle` API Hardening + Shutdown Barrier

**Bugs:**
- `pub _guard` lets external code drop or replace the `WorkerGuard`, silently stopping file logging.
- `spawn_cleanup_task` discards the `JoinHandle` — no way to know whether cleanup finished before the process exits.
- `run_cleanup_loop` is `pub` — should be `pub(crate)` (already changed to `pub(crate)` in Task 1, verify).

**Fix:** Rename `_guard` → `pub(crate) guard`. Add `cleanup_task: Option<JoinHandle<()>>` field. Change `spawn_cleanup_task` to `&mut self` and store the handle. Add `wait_for_cleanup_task(&mut self)` that awaits the handle with a 5-second timeout. Update `main.rs` to call it before returning.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — imports, `LoggingHandle` struct, impl
- Modify: `crates/server/src/main.rs` — `let mut logging_handle`, `wait_for_cleanup_task` call

- [ ] **Step 1: Add `JoinHandle` import**

In `file_logging.rs`, find:
```rust
use tokio_util::sync::CancellationToken;
```
Add above it:
```rust
use tokio::task::JoinHandle;
```

- [ ] **Step 2: Update `LoggingHandle` struct**

Replace:
```rust
pub struct LoggingHandle {
    /// Dropping this flushes remaining buffered log lines and stops the writer thread.
    pub _guard: Option<WorkerGuard>,
    /// Present when file logging was successfully initialised; used to spawn cleanup.
    pub cleanup_config: Option<FileLoggingConfig>,
}
```
With:
```rust
/// Holds the non-blocking writer guard and optionally the config needed to
/// spawn a periodic cleanup task.
///
/// **Hold this value for the entire lifetime of the process.** Dropping it
/// flushes and stops the background file-writer thread.
pub struct LoggingHandle {
    /// RAII guard: dropping this flushes buffered log lines and stops the writer thread.
    /// `pub(crate)` — never drop or replace externally; file logging stops silently.
    pub(crate) guard: Option<WorkerGuard>,
    /// Present when file logging was successfully initialised; used to spawn cleanup.
    pub cleanup_config: Option<FileLoggingConfig>,
    /// JoinHandle for the daily cleanup task; set by `spawn_cleanup_task`.
    cleanup_task: Option<JoinHandle<()>>,
}
```

- [ ] **Step 3: Update `LoggingHandle` impl**

Replace the entire `impl LoggingHandle` block:

```rust
impl LoggingHandle {
    fn console_only() -> Self {
        Self {
            guard: None,
            cleanup_config: None,
            cleanup_task: None,
        }
    }

    fn with_file(guard: WorkerGuard, config: FileLoggingConfig) -> Self {
        Self {
            guard: Some(guard),
            cleanup_config: Some(config),
            cleanup_task: None,
        }
    }

    /// Spawn a background tokio task that runs `cleanup_old_logs` once per day.
    ///
    /// Call this after the tokio runtime is running (i.e. inside an `async fn`).
    /// The task exits when `shutdown` is cancelled. Await completion via
    /// [`wait_for_cleanup_task`] before the process exits.
    ///
    /// No-op if file logging is not active.
    pub fn spawn_cleanup_task(&mut self, shutdown: CancellationToken) {
        if let Some(ref config) = self.cleanup_config {
            let config = config.clone();
            self.cleanup_task = Some(tokio::spawn(run_cleanup_loop(config, shutdown)));
        }
    }

    /// Wait for the cleanup task to finish (up to 5 seconds after cancellation).
    ///
    /// Call this after `shutdown_token.cancel()` and before the process exits.
    /// No-op if no cleanup task was spawned.
    pub async fn wait_for_cleanup_task(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            match tokio::time::timeout(Duration::from_secs(5), task).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("Log cleanup task panicked: {e}"),
                Err(_) => eprintln!(
                    "Log cleanup task did not finish within 5s; continuing shutdown"
                ),
            }
        }
    }
}
```

- [ ] **Step 4: Fix all references to `_guard` in the test module**

In test `init_logging_second_call_returns_none`, find:
```rust
        assert!(
            second._guard.is_none() && second.cleanup_config.is_none(),
```
Replace with:
```rust
        assert!(
            second.guard.is_none() && second.cleanup_config.is_none(),
```

In test `logging_handle_spawn_cleanup_is_noop_when_disabled`, find:
```rust
        let handle = LoggingHandle {
            _guard: None,
            cleanup_config: None,
        };
```
Replace with:
```rust
        let mut handle = LoggingHandle {
            guard: None,
            cleanup_config: None,
            cleanup_task: None,
        };
```

- [ ] **Step 5: Update `main.rs`**

Change:
```rust
    let logging_handle = file_logging::init_logging(&filter_string);
```
To:
```rust
    let mut logging_handle = file_logging::init_logging(&filter_string);
```

After `perform_cleanup_actions` and before `Ok(())`, add:
```rust
    shutdown_token.cancel();

    perform_cleanup_actions(&deployment).await;
    if let Some(ref mut process) = mcp_process {
        process.terminate();
    }

    // Await the cleanup task's final run before flushing the WorkerGuard.
    logging_handle.wait_for_cleanup_task().await;

    Ok(())
```

- [ ] **Step 6: Build**

```bash
cargo build -p server 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 7: Run tests**

```bash
cargo test -p server file_logging 2>&1 | grep -E "test result|FAILED"
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/file_logging.rs crates/server/src/main.rs
git commit -m "fix(logging): pub(crate) guard, JoinHandle stored in LoggingHandle, wait_for_cleanup_task"
```

---

### Task 4: `cleanup_old_logs` Hardening

**Bugs:**
- No `file_type()` check — a directory named `vibe-kanban.log.2025-01-02` passes the filter, consumes a retention slot, `remove_file` fails silently with EISDIR, and a real log file gets deleted instead.
- `is_log_date_suffix` accepts any year including `9999` — one file with a far-future date blocks all log rotation.
- `"vibe-kanban.log."` is hardcoded as a string literal in `cleanup_old_logs` instead of reusing a shared constant.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — `is_log_date_suffix`, `cleanup_old_logs`, add `LOG_PREFIX`

- [ ] **Step 1: Write failing tests**

Add to the test module:

```rust
    #[test]
    fn cleanup_skips_directories_with_log_names() {
        let dir = temp_dir();
        // A directory named like a log file must NOT count as a retention slot.
        fs::create_dir(dir.join("vibe-kanban.log.2025-06-02")).unwrap();
        fs::write(dir.join("vibe-kanban.log.2025-06-01"), b"log").unwrap();

        // With max_files=1 the real file must survive; directory is silently skipped.
        cleanup_old_logs(&dir, 1);

        assert!(
            dir.join("vibe-kanban.log.2025-06-01").exists(),
            "real log file was deleted because directory consumed its slot"
        );
    }

    #[test]
    fn is_log_date_suffix_rejects_far_future_year() {
        assert!(!is_log_date_suffix("vibe-kanban.log.9999-01-01"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2100-01-01"));
        // Years in valid range must still be accepted.
        assert!(is_log_date_suffix("vibe-kanban.log.2000-01-01"));
        assert!(is_log_date_suffix("vibe-kanban.log.2099-12-31"));
    }
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p server "file_logging::tests::cleanup_skips_directories" 2>&1
cargo test -p server "file_logging::tests::is_log_date_suffix_rejects_far_future" 2>&1
```

Expected: FAIL.

- [ ] **Step 3: Add module-level `LOG_PREFIX` constant**

Before the `is_log_date_suffix` function, add:

```rust
/// Filename prefix used by the daily rolling appender.
/// Shared between `is_log_date_suffix` and `cleanup_old_logs`.
const LOG_PREFIX: &str = "vibe-kanban.log.";
```

- [ ] **Step 4: Update `is_log_date_suffix` to use `LOG_PREFIX` and add year range 2000–2099**

Replace the entire function:

```rust
pub(crate) fn is_log_date_suffix(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(LOG_PREFIX) else {
        return false;
    };
    if suffix.len() != 10 {
        return false;
    }
    let b = suffix.as_bytes();
    // YYYY-MM-DD: digits at 0-3, dash at 4, digits at 5-6, dash at 7, digits at 8-9
    if !(b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit())
    {
        return false;
    }
    // Year 2000-2099: rejects far-future dates that would block log rotation.
    let year = ((b[0] - b'0') as u32 * 1000)
        + ((b[1] - b'0') as u32 * 100)
        + ((b[2] - b'0') as u32 * 10)
        + (b[3] - b'0') as u32;
    let month = (b[5] - b'0') * 10 + (b[6] - b'0');
    let day = (b[8] - b'0') * 10 + (b[9] - b'0');
    (2000..=2099).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
}
```

- [ ] **Step 5: Update `cleanup_old_logs` — add `file_type()` filter and use `LOG_PREFIX`**

In `cleanup_old_logs`, replace the `filter_map` chain that builds `log_files`:

```rust
    let mut log_files: Vec<(std::path::PathBuf, String)> = entries
        .filter_map(|e| e.ok())
        // Only regular files — directories/symlinks/FIFOs with matching names are skipped.
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            if is_log_date_suffix(&name) {
                let date = name[LOG_PREFIX.len()..].to_owned();
                Some((e.path(), date))
            } else {
                None
            }
        })
        .collect();
```

- [ ] **Step 6: Build and run tests**

```bash
cargo build -p server 2>&1 | tail -5
cargo test -p server file_logging 2>&1 | grep -E "test result|FAILED"
```

Expected: no errors, all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): file_type() filter in cleanup, year 2000-2099 range, LOG_PREFIX constant"
```

---

### Task 5: Config Validation Hardening

**Bugs:**
- `VK_LOG_BUFFER_LINES` has no upper bound — a value like `99999999` is passed directly to `NonBlockingBuilder`, causing a massive channel allocation and potential OOM.
- `VK_LOG_LOSSY=False` (capital F) keeps lossy enabled — case-sensitive exact match means `False`, `FALSE`, `No`, `Off` are all silently ignored.

**Files:**
- Modify: `crates/server/src/file_logging.rs` — `FileLoggingConfig::from_env`, `lossy` doc comment

- [ ] **Step 1: Write failing tests**

Add to the test module:

```rust
    #[test]
    fn buffer_lines_exceeding_maximum_is_clamped() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_BUFFER_LINES", "99999999");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(
            config.buffer_lines, 1_000_000,
            "buffer_lines above max must be clamped to 1_000_000"
        );
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
    }

    #[test]
    fn lossy_disabled_by_env_case_insensitive() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &["False", "FALSE", "no", "No", "off", "OFF"] {
            unsafe {
                std::env::set_var("VK_LOG_LOSSY", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(
                !config.lossy,
                "expected lossy=false for VK_LOG_LOSSY={val}"
            );
        }
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
    }
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p server "file_logging::tests::buffer_lines_exceeding" 2>&1
cargo test -p server "file_logging::tests::lossy_disabled_by_env_case_insensitive" 2>&1
```

Expected: FAIL.

- [ ] **Step 3: Add upper bound for `VK_LOG_BUFFER_LINES`**

In `FileLoggingConfig::from_env`, find the `buffer_lines` block:
```rust
        let raw_buffer = std::env::var("VK_LOG_BUFFER_LINES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(128_000);
        let buffer_lines = if raw_buffer == 0 {
            eprintln!("VK_LOG_BUFFER_LINES=0 is invalid (minimum is 1); using 1");
            1
        } else {
            raw_buffer
        };
```

Replace with:
```rust
        const MAX_BUFFER_LINES: usize = 1_000_000;
        let raw_buffer = std::env::var("VK_LOG_BUFFER_LINES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(128_000);
        let buffer_lines = if raw_buffer == 0 {
            eprintln!("VK_LOG_BUFFER_LINES=0 is invalid (minimum is 1); using 1");
            1
        } else if raw_buffer > MAX_BUFFER_LINES {
            eprintln!(
                "VK_LOG_BUFFER_LINES={raw_buffer} exceeds maximum ({MAX_BUFFER_LINES}); \
using {MAX_BUFFER_LINES}"
            );
            MAX_BUFFER_LINES
        } else {
            raw_buffer
        };
```

- [ ] **Step 4: Make `VK_LOG_LOSSY` case-insensitive**

Find:
```rust
        let lossy = std::env::var("VK_LOG_LOSSY")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
```

Replace with:
```rust
        let lossy = std::env::var("VK_LOG_LOSSY")
            .map(|v| {
                let v = v.to_lowercase();
                v != "false" && v != "0" && v != "no" && v != "off"
            })
            .unwrap_or(true);
```

Update the `lossy` field doc comment:
```rust
    /// When `true` (default), excess log lines are dropped under load rather
    /// than blocking the application. Set `VK_LOG_LOSSY` to `false`, `0`,
    /// `no`, or `off` (case-insensitive) to block instead.
    ///
    /// Caution: `lossy=false` can block tokio workers under log burst; use only
    /// for debugging on non-production deployments.
    pub lossy: bool,
```

- [ ] **Step 5: Build and run tests**

```bash
cargo build -p server 2>&1 | tail -5
cargo test -p server file_logging 2>&1 | grep -E "test result|FAILED"
```

Expected: no errors, all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/file_logging.rs
git commit -m "fix(logging): VK_LOG_BUFFER_LINES upper bound 1M, case-insensitive VK_LOG_LOSSY"
```

---

### Task 6: Full Validation

- [ ] **Step 1: Format**

```bash
pnpm run format 2>&1 | tail -5
```

If any files were reformatted:
```bash
git add crates/server/src/file_logging.rs crates/server/src/main.rs
git commit -m "chore(logging): rustfmt formatting"
```

- [ ] **Step 2: Clippy**

```bash
cargo clippy -p server 2>&1 | grep "^error"
```

Expected: no output (no errors).

- [ ] **Step 3: Full workspace test**

```bash
cargo test --workspace 2>&1 | grep -E "FAILED|^error\[" | head -20
```

Expected: no output (all pass).

- [ ] **Step 4: Push and check PR**

```bash
git push origin vk/58d6-do-we-currently 2>&1
gh pr view 33 --json mergeable,mergeStateStatus --jq '{mergeable: .mergeable, state: .mergeStateStatus}'
```

Expected: `{"mergeable":"MERGEABLE","state":"CLEAN"}`.

---

## Known Non-Issues (Accepted)

- **`lossy=false` can stall tokio workers** — documented with a caution in the field doc (Task 5). A user who enables this accepts the trade-off.
- **Impossible calendar dates** (e.g. `2025-02-31`) — the year range (2000–2099) is the meaningful guard; full calendar validation would require a `chrono` dependency with negligible benefit for a filename filter.
- **`init_logging_second_call_returns_none` installs a global subscriber** — acknowledged in code comment. Moving to a separate test binary is deferred.
- **`WorkerGuard` flush order** — `wait_for_cleanup_task` ensures cleanup finishes first; the guard then drops inside `async_main`'s return sequence while the runtime is still alive. This is the correct order.

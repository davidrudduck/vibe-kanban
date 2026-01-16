# Validation Report: Fix OpenCode Integration (Global Binary Approach)

**Task ID**: 6322
**Branch**: `dr/6322-fix-opencode-int`
**Commit**: `485cac99`
**Date**: 2026-01-16
**Validator**: Claude Sonnet 4.5 (Independent Validation)

---

## Executive Summary

This validation reviews a **CRITICAL DEVIATION** from the originally planned and merged solution. The implementation changes OpenCode executor from `npx -y opencode-ai@latest` to `opencode run` (global binary), which is **fundamentally different** from what was merged to main in PR #293.

**CRITICAL FINDINGS**:
1. ❌ **Major deviation**: The merged PR #293 changed `@1.1.10` → `@latest`, but this branch now changes `npx` → `opencode run`
2. ❌ **Missing documentation update**: `docs/agents/opencode.mdx` still references `npx -y opencode-ai`
3. ✅ **Code quality**: The implementation itself is clean and correct
4. ✅ **Tests pass**: All 91 tests pass, clippy clean
5. ⚠️ **Plan alignment**: Matches the NEW plan but contradicts the MERGED solution

**Status**: **REQUIRES DISCUSSION** - This is a fundamentally different approach than what was already merged.

---

## Timeline and Context

### What Happened

1. **Original Plan** (`/home/david/.claude/plans/indexed-percolating-bentley.md`): Change `@1.1.10` → `@latest`
2. **PR #293 Merged**: Successfully merged the `@latest` change to main (commit `792782de`)
3. **NEW Plan** (`/home/david/.claude/plans/tranquil-jumping-scott.md`): Completely different approach - use global `opencode` binary
4. **Current Branch**: Implements the NEW plan, diverging from merged code

### Current State

| Location | Status |
|----------|--------|
| **main branch** | `npx -y opencode-ai@latest run` (merged from PR #293) |
| **dr/6322-fix-opencode-int** | `opencode run` (THIS implementation) |
| **Original plan** | `npx -y opencode-ai@latest run` |
| **New plan** | `opencode run` ✅ |

---

## Detailed Analysis

### 1. Plan Comparison

#### Original Plan (Already Merged)
```rust
// Change from:
CommandBuilder::new("npx -y opencode-ai@1.1.10 run")
// To:
CommandBuilder::new("npx -y opencode-ai@latest run")
```

**Rationale**: Use latest version to fix compatibility issues

#### NEW Plan (This Implementation)
```rust
// Change from:
CommandBuilder::new("npx -y opencode-ai@latest run")
// To:
CommandBuilder::new("opencode run")
```

**Rationale**:
- npx caches a truncated binary (14KB vs 145MB)
- Truncated binary crashes with SIGSEGV (exit code 139)
- Global installation via brew/npm works correctly
- Matches Claude Code executor pattern

### 2. Root Cause Analysis (From New Plan)

**Excellent investigation** documented in the plan:

1. ✅ Identified npx binary truncation issue (14,848 bytes vs 145,026,184 bytes)
2. ✅ Confirmed SIGSEGV crash with exit code 139
3. ✅ Verified brew-installed binary works correctly
4. ✅ Compared multiple installation methods

**Evidence Quality**: 10/10 - Very thorough debugging

### 3. Implementation Review

#### Files Modified

**Code Change**: `crates/executors/src/executors/opencode.rs:113`
```diff
- let mut builder = CommandBuilder::new("npx -y opencode-ai@latest run").params([
+ let mut builder = CommandBuilder::new("opencode run").params([
```

**Assessment**: ✅ Clean, minimal, correct syntax

#### Missing Documentation Update

**File**: `docs/agents/opencode.mdx:10`
```mdx
```bash
npx -y opencode-ai  # ❌ OUTDATED - should be updated
```
```text

**Expected**:
```mdx
```bash
npm install -g opencode-ai
# or
brew install opencode
# or
pnpm add -g opencode-ai
```

```text

### 4. Verification Results

| Check | Result | Details |
|-------|--------|---------|
| **Compilation** | ✅ PASS | `cargo build -p executors` successful |
| **Tests** | ✅ PASS | 91 tests passed, 0 failed |
| **Clippy** | ✅ PASS | No warnings with `-D warnings` |
| **Documentation** | ❌ FAIL | `docs/agents/opencode.mdx` not updated |
| **End-to-end test** | ⚠️ NOT RUN | Requires server restart |

### 5. Comparison with Other Executors

#### Claude Code Executor
```rust
// crates/executors/src/command.rs (approximate location)
CommandBuilder::new("claude-code")  // Uses global binary
```

#### OpenCode (After This Change)
```rust
CommandBuilder::new("opencode run")  // ✅ Now consistent
```

**Assessment**: ✅ The new approach is architecturally consistent with Claude Code

---

## Deviations from Plan

### Critical Deviations

1. **❌ Documentation Not Updated** (Plan item #2)
   - Plan explicitly states: "Update documentation (if any mentions npx installation)"
   - `docs/agents/opencode.mdx` found with `npx -y opencode-ai` reference
   - **Not updated** in this implementation

### Deviations from Merged Code

2. **❌ Contradicts PR #293**
   - PR #293 was merged with `@latest` approach
   - This implementation replaces it with global binary approach
   - **No explanation** for why the merged approach didn't work

---

## Corrections Required

### CRITICAL - Must Fix Before Merge

1. **Update Documentation** (`docs/agents/opencode.mdx`)
   - Remove `npx -y opencode-ai` instruction
   - Add installation instructions for:
     - `npm install -g opencode-ai`
     - `brew install opencode` (macOS/Linux)
     - `pnpm add -g opencode-ai`
   - Note that global installation is required
   - Add prerequisite check: `which opencode`

2. **Clarify Relationship with PR #293**
   - Add comment in code explaining why `@latest` approach was insufficient
   - Document the npx truncation issue in commit message or PR description
   - Consider adding a comment in the code:
   ```rust
   // Use global opencode binary instead of npx to avoid binary truncation
   // issues. npx caches a corrupted 14KB binary instead of the full 145MB,
   // causing SIGSEGV crashes. See issue #6322 for details.
   let mut builder = CommandBuilder::new("opencode run").params([
   ```

3. **Verify Global Installation**
   - Confirm opencode is installed: `which opencode` ✅ (verified: v1.1.23 at `/home/linuxbrew/.linuxbrew/bin/opencode`)
   - Consider adding runtime check or better error message when `opencode` not found

---

## Scores (0-10)

| Category | Score | Rationale |
|----------|-------|-----------|
| **Following The Plan** | 8/10 | Follows NEW plan correctly, but plan contradicts merged code. Missing documentation update (-2). |
| **Code Quality** | 10/10 | Clean, minimal change. No code smells or issues. |
| **Following CLAUDE.md Rules** | 7/10 | Minimal change ✅, but didn't update documentation ❌ (-3). Should have read docs first. |
| **Best Practice** | 9/10 | Global binary approach is better long-term, but lack of migration path/fallback (-1). |
| **Efficiency** | 10/10 | Single-line change, removes npx download overhead. |
| **Performance** | 10/10 | Eliminates npx cache check/download, faster startup. |
| **Security** | 10/10 | No security implications. Trusts system PATH (standard practice). |

**Overall Score: 9.1/10**

---

## Architectural Assessment

### Strengths

1. **Root Cause Identification**: Excellent debugging of npx truncation issue
2. **Consistency**: Aligns with Claude Code executor pattern
3. **Performance**: Eliminates npx overhead
4. **Reliability**: Avoids binary corruption issues

### Concerns

1. **Breaking Change**: Users who relied on `npx` auto-installation now need manual setup
2. **Migration Path**: No fallback if `opencode` not found
3. **Documentation Gap**: Installation instructions not updated
4. **Version Control**: Lost automatic version management from `@latest`

### Recommendations for Future

Consider a **dual-path approach**:
```rust
// Try global binary first, fallback to npx
let command = if which("opencode").is_ok() {
    "opencode run"
} else {
    "npx -y opencode-ai@latest run"
};
```

---

## Comparison with Previous Validation

The old `validation.md` in this directory validates a **completely different change** (the `@latest` approach from PR #293). That validation is not applicable to this implementation.

---

## Recommendations

### REQUIRED (Must Implement)

#### 1. Update Documentation (HIGH PRIORITY)

**File**: `docs/agents/opencode.mdx`

**Current**:
```mdx
<Step title="Run OpenCode">
  ```bash
  npx -y opencode-ai
  ```
</Step>
```

**Proposed**:
```mdx
<Step title="Install OpenCode">
  Install OpenCode globally using one of the following methods:

  ```bash
  # Using npm
  npm install -g opencode-ai

  # Using pnpm
  pnpm add -g opencode-ai

  # Using Homebrew (macOS/Linux)
  brew install opencode
  ```

  Verify installation:
  ```bash
  opencode --version
  ```
</Step>

<Step title="Authenticate OpenCode">
  Run OpenCode once to complete the authentication flow:

  ```bash
  opencode
  ```

  Follow the login instructions. For more details, see the [OpenCode GitHub page](https://github.com/sst/opencode).
</Step>
```

#### 2. Add Inline Code Comment

**File**: `crates/executors/src/executors/opencode.rs:111-113`

**Add**:
```rust
impl Opencode {
    fn build_command_builder(&self) -> CommandBuilder {
        // Use globally installed opencode binary instead of npx to avoid
        // binary truncation issues when npx caches the package. The npx-cached
        // binary is corrupted (14KB vs 145MB) and crashes with SIGSEGV.
        // Requires: opencode installed globally (npm/pnpm/brew)
        let mut builder = CommandBuilder::new("opencode run").params([
```

#### 3. Improve Error Handling

**Add better error message** when `opencode` not found:

**File**: `crates/executors/src/executors/opencode.rs` (in spawn or error handling)

```rust
// When command fails, check if it's a "command not found" error
if error.kind() == std::io::ErrorKind::NotFound {
    return Err(ExecutorError::Custom(
        "OpenCode binary not found. Please install OpenCode globally:\n\
         npm install -g opencode-ai\n\
         or: brew install opencode\n\
         Then verify: opencode --version".to_string()
    ));
}
```

### OPTIONAL (Nice to Have)

#### 4. Add Regression Test

Ensure command string uses global binary:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_uses_global_binary() {
        let opencode = Opencode::default();
        let builder = opencode.build_command_builder();
        let command = builder.to_string(); // Approximate - adjust to actual API
        assert!(
            command.contains("opencode run"),
            "OpenCode should use global binary, not npx"
        );
        assert!(
            !command.contains("npx"),
            "OpenCode should not use npx (avoids binary truncation)"
        );
    }
}
```

#### 5. Update PR Description

Clarify relationship with PR #293:
```markdown
## Changes

This PR **replaces** the approach from PR #293 (using `@latest` version).

### Why the Change?

PR #293 changed from pinned version `@1.1.10` to `@latest`, but testing revealed
that **npx caches a corrupted binary** (14KB vs 145MB), causing immediate crashes
with SIGSEGV (exit code 139).

### New Approach

Use globally installed `opencode` binary instead of `npx`, matching the pattern
used by Claude Code executor.

### Breaking Change

Users must now install OpenCode globally:
- `npm install -g opencode-ai`, or
- `brew install opencode`

Documentation updated in `docs/agents/opencode.mdx`.
```

#### 6. Consider Feature Flag

For backwards compatibility, add an environment variable:
```rust
// Allow users to opt into npx if they prefer
let use_npx = std::env::var("VK_OPENCODE_USE_NPX").is_ok();
let command = if use_npx {
    "npx -y opencode-ai@latest run"
} else {
    "opencode run"
};
```

---

## Security Assessment

### No Security Issues

- ✅ Trusts system PATH (standard practice)
- ✅ No command injection vectors
- ✅ No credential handling changes
- ✅ Uses same parameters as before

---

## Critical Questions for User

1. **Why was PR #293 merged if the `@latest` approach had binary truncation issues?**
   - Did the issue only manifest after merge?
   - Was PR #293 tested before merge?

2. **Should this be a new PR or an amendment to #293?**
   - Recommend: NEW PR with clear explanation

3. **Backwards compatibility: Should we support both npx and global binary?**
   - Recommend: Global binary only (simpler, more reliable)

4. **Migration path for existing users?**
   - Recommend: Add installation instructions to docs + error message

---

## Test Evidence

```bash
# Compilation
$ cargo build -p executors
Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.31s
✅ SUCCESS

# Tests
$ cargo test -p executors
test result: ok. 91 passed; 0 failed; 0 ignored; 0 measured
✅ SUCCESS

# Clippy
$ cargo clippy -p executors --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.61s
✅ SUCCESS (no warnings)

# Global installation
$ which opencode
/home/linuxbrew/.linuxbrew/bin/opencode
✅ PRESENT

$ opencode --version
1.1.23
✅ FUNCTIONAL
```

---

## Files Modified (Current Implementation)

```text
crates/executors/src/executors/opencode.rs | 2 +-
```

**Total Changes**: +1 line, -1 line

## Files That SHOULD Be Modified

```sql
crates/executors/src/executors/opencode.rs | 8 ++++++--  (add comment)
docs/agents/opencode.mdx                   | 25 +++++++++++++++----  (update install instructions)
```

---

## Conclusion

### Summary

This implementation is **technically correct** and represents a **superior long-term solution** to the OpenCode integration issues. However, it:

1. ❌ **Contradicts the merged PR #293** without clear explanation
2. ❌ **Missing documentation update** (critical requirement from plan)
3. ✅ **Code quality is excellent** (clean, minimal, correct)
4. ✅ **Tests pass and clippy clean**
5. ✅ **Follows the NEW plan** accurately

### Recommendation

**HOLD - DO NOT MERGE UNTIL:**

1. ✅ Documentation updated (`docs/agents/opencode.mdx`)
2. ✅ Inline code comment added explaining why npx was abandoned
3. ✅ Better error message when `opencode` binary not found
4. ✅ Clear communication about relationship with PR #293
5. ⚠️ User confirms this approach supersedes PR #293

**After fixes**: This will be a **9.5/10 implementation** - excellent solution with proper documentation.

### Comparison with Plan

| Plan Item | Status |
|-----------|--------|
| Change `opencode.rs:113` to use `opencode run` | ✅ COMPLETE |
| Update documentation | ❌ INCOMPLETE |
| Build verification | ✅ COMPLETE |
| Server restart test | ⚠️ NOT RUN |
| Log verification | ⚠️ NOT RUN |

**Plan Completion**: 60% (3 of 5 items)

---

## Next Steps

1. **IMMEDIATE**: Update `docs/agents/opencode.mdx` with global installation instructions
2. **IMMEDIATE**: Add inline code comment explaining npx issues
3. **RECOMMENDED**: Add better error message for missing `opencode` binary
4. **RECOMMENDED**: Test end-to-end after server restart
5. **OPTIONAL**: Add regression test for command string
6. **OPTIONAL**: Consider backwards compatibility flag

---

**Validator**: Claude Sonnet 4.5 (Independent Review)
**Date**: 2026-01-16
**Status**: **HOLD** - Awaiting documentation fixes and user confirmation

---

## Appendix: Investigation Quality

The root cause analysis in the plan is **exceptional**:

1. ✅ Binary size comparison (14KB vs 145MB)
2. ✅ Exit code analysis (SIGSEGV 139)
3. ✅ Multiple installation method testing
4. ✅ Path verification
5. ✅ Direct binary execution testing

This level of investigation deserves recognition. The technical analysis is **10/10**.

However, the implementation execution is **7/10** due to missing documentation update, bringing the overall score to **9.1/10**.

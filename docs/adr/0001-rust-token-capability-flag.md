# ADR 0001: Rust Token Capability Flag

## Status

**Proposed** — Deferred from context-monitor-diagnosis-and-fix v2 (2026-05-13)

## Context

The frontend currently infers `executorSupportsTokens` from observed entries plus a frontend-side `KNOWN_TOKEN_EMITTERS` allowlist defined in `packages/web-core/src/features/workspace-chat/model/sessionSummary.ts`.

The Rust side is the source of truth for which executors emit `TokenUsageInfo`:
- `CLAUDE_CODE`: `crates/executors/src/executors/claude.rs:1896`
- `CODEX`: `crates/executors/src/executors/codex/normalize_logs.rs:768, 2297`
- `OPENCODE`: `crates/executors/src/executors/opencode/normalize_logs.rs:94`

Other executors (`AMP`, `GEMINI`, `CURSOR_AGENT`, `QWEN_CODE`, `COPILOT`, `DROID`) do not emit telemetry as of this revision.

The allowlist is a duplicated fact that requires manual synchronization between Rust and TypeScript. When a new executor gains token telemetry capability, developers must remember to update both:
1. The Rust executor implementation (adding `add_token_usage_entry` calls)
2. The TypeScript `KNOWN_TOKEN_EMITTERS` set

This creates a maintenance burden and potential for drift.

## Decision

Introduce `BaseAgentCapability::TOKEN_USAGE` in the `BaseAgentCapability` enum (`shared/types.ts:770` in generated types, source in Rust types).

Have each executor declare its capabilities at the Rust definition site. Expose capabilities via the existing capability probe path used by `BaseAgentCapability::SESSION_FORK` and `CONTEXT_USAGE`.

The frontend will read the capability from `useExecutorConfig` (or equivalent) and remove the hard-coded `KNOWN_TOKEN_EMITTERS` constant.

### Migration Path

1. Add `TOKEN_USAGE` variant to `BaseAgentCapability` enum in Rust
2. Update executor definitions to declare the capability where applicable:
   - Claude executor declares `TOKEN_USAGE`
   - Codex executor declares `TOKEN_USAGE`
   - OpenCode executor declares `TOKEN_USAGE`
3. Expose capability via existing capability probe infrastructure
4. Update `aggregateSessionSummary` to check the capability flag instead of the `KNOWN_TOKEN_EMITTERS` set
5. Remove the `KNOWN_TOKEN_EMITTERS` constant from `sessionSummary.ts`
6. Add runtime fallback: if capability flag is unavailable (e.g., legacy sessions), fall back to inference (`entries.length > 0`)

## Consequences

### Positive

- **Single source of truth**: Executor capabilities are declared once at the Rust definition site
- **Automatic frontend sync**: Frontend stays in sync via generated types (ts-rs)
- **Explicit declarations**: New executor authors see capability requirements upfront
- **Type-safe**: Capability checks use the typed enum, not string matching

### Negative

- **Additional round-trip**: Capabilities must be loaded before the panel can render accurately (mitigated by caching)
- **Implementation cost**: Requires changes to `BaseAgentCapability`, executor definitions, capability probing, ts-rs generation, and frontend consumption (~8 files across 3 crates)
- **Migration complexity**: Legacy sessions without the capability flag need fallback logic

### Neutral

- **Not load-bearing for this fix**: The current inference-based approach (`entries.length > 0 OR allowlist`) is correct and sufficient. The capability flag is an optimization for maintainability, not a functional requirement.

## Open Questions

1. **Should `TOKEN_USAGE` decompose into finer-grained sub-capabilities?**
   - Example: `TOKEN_USAGE_COST` (cost_microusd), `TOKEN_USAGE_CACHE` (cache tokens), `TOKEN_USAGE_DURATION` (duration_ms)
   - Pro: More precise signaling (e.g., Codex emits tokens but not cost)
   - Con: Adds complexity; current coarse-grained flag may be sufficient
   - **Decision**: Defer to implementation review. Start with coarse-grained `TOKEN_USAGE`; refine if evidence emerges that executors have heterogeneous token telemetry capabilities

2. **How should the frontend handle capability flag unavailability?**
   - Proposed: Fall back to inference (`entries.length > 0`) if capability is not yet loaded or unavailable
   - This preserves current behavior and avoids blocking the panel on capability load

3. **Should capability probing be eager (at executor initialization) or lazy (on first use)?**
   - Eager: Panel renders correctly immediately but adds startup cost
   - Lazy: Defers cost to first panel render; may cause brief flicker from "not supported" → "supported"
   - **Recommendation**: Eager, cached at executor config level

## Trigger to Revisit

This ADR should be implemented when **any** of the following occur:

1. **A 4th token-emitting executor lands** (AMP, GEMINI, CURSOR_AGENT, QWEN_CODE, COPILOT, or DROID gains `add_token_usage_entry` calls)
2. **Within 90 days** of this ADR's proposal date (by 2026-08-11)
3. **String-key mismatches observed in production** (e.g., executor name doesn't match `KNOWN_TOKEN_EMITTERS` strings)

## References

- Original plan: `.omc/plans/context-monitor-diagnosis-and-fix.md` (Step 5)
- Frontend allowlist: `packages/web-core/src/features/workspace-chat/model/sessionSummary.ts` (KNOWN_TOKEN_EMITTERS constant, link in comment)
- Existing capability infrastructure: `BaseAgentCapability` enum at `shared/types.ts:770`
- Related consensus reviews: Architect + Critic approvals in ralplan iteration 2

## Revision History

- 2026-05-13: Initial proposal (deferred from context monitor fix)

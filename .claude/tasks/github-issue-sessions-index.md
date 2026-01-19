# sessions-index.json not updated - fork-session resume fails silently

## Bug Description
The `sessions-index.json` file stops being updated after a certain number of sessions are created, while session `.jsonl` files continue to be created normally.

## Evidence
- **Task attempt ID**: c0cb46e0-40bd-442b-b9e7-a73b0496fff8
- **Session files found**: 18 `.jsonl` files in project directory
- **Sessions in index**: Only 11 entries
- **Missing session**: `be4a60c2-9a17-4d9f-a192-245f2e4c11c3`
- **Index last modified**: 10:45 AM
- **Sessions created after**: Not added to index

## Impact
When using `--fork-session --resume <session-id>` with a valid session ID that's missing from the index, Claude Code cannot locate the session and starts fresh instead of resuming with prior context.

## Reproduction Steps
1. Create multiple coding sessions in a project
2. After ~10+ sessions, check `~/.claude/projects/<project>/sessions-index.json`
3. Compare entry count to actual `.jsonl` files in directory
4. Attempt to resume a session that exists as `.jsonl` but is missing from index
5. Observe: Claude starts fresh instead of resuming

## Expected Behaviour
All session `.jsonl` files should have corresponding entries in `sessions-index.json`.

## Workaround
We've implemented automatic index repair that scans for missing sessions and rebuilds the index before every session resume.

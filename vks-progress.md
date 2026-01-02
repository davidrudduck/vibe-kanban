## Session 1 Complete - Fix ActionsDropdown is_remote Checks

### Progress Summary
- **Progress**: 1/7 sessions complete (~14%)
- **Current Session**: Session 1 - Fix ActionsDropdown is_remote Checks
- **Status**: COMPLETED

### Accomplished
- Removed all `isRemote` disabled conditions from `actions-dropdown.tsx`
- Both mobile bottom sheet and desktop dropdown menus updated
- Verified all menu items are now enabled when an attempt exists
- No console errors introduced

### Changes Made
**File:** `frontend/src/components/ui/actions-dropdown.tsx`

Removed `isRemote` from disabled conditions on:
- Create New Attempt (mobile: line 470, desktop: line 690)
- Git Actions (mobile: line 476, desktop: line 700)
- Edit Branch Name (mobile: line 482, desktop: line 710)
- Purge Build Artifacts (mobile: lines 492-497, desktop: lines 721-726)
- Cleanup Worktree (mobile: lines 506-511, desktop: lines 746-751)
- Create Subtask (mobile: line 585, desktop: line 838)
- Archive (mobile: line 595, desktop: line 857)
- Unarchive (mobile: line 604, desktop: line 871)

Also removed unnecessary `remoteTaskCannotExecute` tooltip titles.

**Note:** The `isRemote` variable at line 144 is still used for permission checks in `canModifyTask` at line 334, so it was kept.

### Test Plan Verification
1. ✅ Navigated to development server at http://localhost:4006
2. ✅ Created test task with attempt
3. ✅ Verified all Actions menu items are enabled (screenshot taken)
4. ✅ No React errors in console

### Git Commits
- `1989b785c` - fix(frontend): remove isRemote disabled conditions from Actions dropdown

### Next Session Should
Continue with Session 2: Enhance NodeProjectsSection Link Dialog
- Add "Create New" tab to allow creating swarm project + linking in one step
- File: `frontend/src/components/swarm/NodeProjectsSection.tsx`

### Environment
- **Ports**: Frontend: 4006, Backend: 4005, MCP: 4007
- **Branch**: dr/8cc2-swarm-correction
- **Servers**: Running (init.sh started them in background)

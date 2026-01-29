# Pull-Request

**TASK_ID:** `$TASK_ID`

## STEP 0: SQUASH COMMITS (MANDATORY)

**Before creating PR, you MUST squash commits into logical units.**

### Why Squash?

14 commits with messages like "Good, 17GB free..." pollute git history.
Goal: 2-5 commits maximum, each representing a logical change.

### Squash Process

**1. Review current commits:**
```bash
git log --oneline origin/main..HEAD
```

**2. Count commits to squash:**
```bash
COMMIT_COUNT=$(git rev-list --count origin/main..HEAD)
echo "Commits to squash: $COMMIT_COUNT"
```

**3. Interactive rebase:**
```bash
git rebase -i origin/main
```

**4. In the editor:**
- Keep first commit as `pick`
- Change all others to `squash` (or `s`)
- Save and exit

**5. Write new commit messages:**

Use conventional commits format for each logical unit:

**Example for feature implementation:**
```
feat: add custom template loading to TaskFollowUpSection

- Fetch custom templates from API when picker opens
- Add loading state and error handling
- Transform API response to PickerTemplate format
- Pass customTemplates, loading, and error props to TemplatePicker

Fixes template picker only showing system templates in follow-up section.
```

**Example for documentation:**
```
docs: update task-templates.mdx for follow-up section support

Note that template picker is available in both task creation modal
and task follow-up section.
```

**6. Verify squash:**
```bash
git log --oneline origin/main..HEAD
# Should show 2-5 commits max, all with professional messages
```

**7. Force push (if already pushed):**
```bash
git push --force-with-lease
```

**ONLY proceed to PR creation after squashing is verified.**

## STEP 1: CREATE PR

**Squash before PR** (Step 0 above)

### Ideal Commit Structure for PRs

**Simple bugfix/feature (like template picker):**
- 1-2 commits total
  - Commit 1: `feat: implement feature`
  - Commit 2 (optional): `docs: update documentation`

**Complex feature:**
- 3-5 commits total, each representing a logical unit:
  - Commit 1: `feat: add backend API endpoint`
  - Commit 2: `feat: implement frontend integration`
  - Commit 3: `test: add integration tests`
  - Commit 4: `docs: update API documentation`
  - Commit 5 (if needed): `fix: address edge cases`

**NEVER:**
- 14 commits for a 2-file change
- Commits like "Session 1 complete", "Task 005 done", "WIP"

### PR PROCESS

Create a PR to merge this branch into origin main.
- Read vks-progress.md
- Read the outline of the plan.
- Read the validation report `$TASK_PATH/validation.md`
- Read the git commits for this worktree.
- Create a detailed summary as below.
- Confirm there are no conflicts after submitting the PR.
- Address any merge conflicts.
- .vk_progress.md and vks-progress.md should not be committed
- logs/ and .playwright-mcp/ should not be commited
- any example images used as part of the PR should be stored as `.github/.images/[PR#]/[filename]` and linked in the PR from that path
- Stop the services using the registered ports
  **Use `mcp__vkswarm__get_variables` to identify the PORTS used by this service** (requires `TASK_ID`)

Output a final summary report as such:

```
# PR 312: PROBLEM SOLVED OR FEATURE ADDED

URL: github.com/pr/321

## Summary
- [Summarise the work completed]

## Key Features
- [detail the key features of the work completed]

## How It Works
- [detail how a user can use this feature]

## Files Changed
- [Details of file changes made]

## Bugs Fixed
- [bugs fixed]

## Testing
- [testing performed and results]
- [include screenshots from tests]

## Validation
- [validation results]
```

** NEVER USE `mcp__vkswarm__get_task` or `mcp__vkswarm__get_context` **
** `task_id` is `${TASK_ID}` -- YOU DO NOT NEED TO FIND IT **

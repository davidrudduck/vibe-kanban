# Prompt Improvements for Implementation Sessions

Based on validation of the `reject_if_remote` implementation, these prompt additions address specific deviations observed.

---

## 1. Commit Message Format Rules

**Issue Observed:** Commit `a230f7cab` had "---" as the title instead of a proper conventional commit message.

**Add to Session Agent Prompt (STEP 7: COMMIT PROGRESS section):**

```markdown
## COMMIT MESSAGE RULES (CRITICAL)

**Format:** Always use conventional commit format: `type: description`

**Valid types:**
- `feat:` - New feature
- `fix:` - Bug fix
- `test:` - Adding/updating tests
- `docs:` - Documentation changes
- `chore:` - Maintenance tasks (formatting, deps)
- `refactor:` - Code restructuring without behavior change

**Examples:**
- `feat: add reject_if_remote helper function`
- `test: add test_reject_if_remote_rejects_remote_project (RED phase)`
- `docs: update swarm-api-patterns with middleware bypass pattern`
- `chore: apply rustfmt formatting`

**NEVER use as commit message:**
- `---` or YAML frontmatter
- Markdown headers (`#`, `##`)
- Multi-paragraph summaries as the title
- Empty or whitespace-only titles

**Commit command pattern:**
```bash
# CORRECT - Single line message
git commit -m "test: add test for remote rejection (RED phase)"

# CORRECT - Multi-line with proper title first
git commit -m "feat: implement reject_if_remote helper" -m "Adds remote project check for message queue handlers"

# WRONG - HEREDOC can cause malformed titles if first line is empty or frontmatter
git commit -m "$(cat <<EOF
---
# Summary
...
EOF
)"
```

**Verification (MANDATORY before moving on):**
```bash
git log --oneline -1
# Must show: abc1234 type: clear description
# If title is "---" or malformed, amend immediately:
git commit --amend -m "correct: commit message here"
```
```bash

---

## 2. Scope Discipline for Formatting

**Issue Observed:** Running `cargo fmt --all` formatted unrelated files in `tasks/` handlers.

**Add to CRITICAL RULES section:**

```markdown
## SCOPE DISCIPLINE (CRITICAL)

**Only modify files directly related to the current task.**

**For formatting commands:**
```bash
# WRONG - Formats entire codebase, may touch unrelated files
cargo fmt --all

# CORRECT - Format only files you modified
cargo fmt -- crates/server/src/routes/message_queue.rs

# CORRECT - Check first, then decide
cargo fmt --all -- --check
# Review output - if files outside your task scope need formatting:
# 1. Do NOT format them in this PR
# 2. Document as "Pre-existing formatting issue" in progress notes
# 3. Optionally create separate cleanup task
```

**For linting commands:**
```bash
# OK to run full clippy for verification
cargo clippy --all --all-targets --all-features -- -D warnings

# But ONLY fix warnings in files you're modifying for this task
# Ignore warnings in unrelated files
```

**Rationale:**
- Bundling unrelated changes makes code review harder
- Pollutes git blame history
- Can introduce merge conflicts
- Makes rollbacks more complex
```bash

---

## 3. Documentation Accuracy

**Issue Observed:** Documentation example showed `Path((attempt_id, message_id))` but actual implementation uses `Path(params): Path<MessageQueueParams>`.

**Add to Session 4 / Documentation tasks:**

```markdown
## DOCUMENTATION ACCURACY (CRITICAL)

**When writing documentation that includes code examples:**

1. **COPY-PASTE actual code** - Never type examples from memory
   ```bash
   # Before writing docs, read the actual implementation
   cat crates/server/src/routes/message_queue.rs | grep -A 10 "fn update_queued_message"
   ```

2. **Verify examples match reality**
   - Read the source file
   - Copy the exact function signature
   - If simplifying, note "simplified example" explicitly

3. **Cross-reference before committing**
   ```bash
   # Verify doc examples match implementation
   grep "Path<MessageQueueParams>" crates/server/src/routes/message_queue.rs
   grep "Path<MessageQueueParams>" docs/architecture/swarm-api-patterns.mdx
   # Both should show same pattern
   ```

**Common mistakes to avoid:**
- Writing `Path((attempt_id, message_id))` when code uses `Path(params): Path<MessageQueueParams>`
- Documenting function signatures from memory
- "Simplifying" examples in ways that don't compile
```

---

## 4. Plan Fidelity Reinforcement

**Add to CRITICAL RULES section:**

```markdown
## PLAN FIDELITY (CRITICAL)

**The approved plan is the specification. Implementation must match it exactly.**

**Before marking any acceptance criterion complete:**
1. Re-read the criterion from the task file
2. Compare your implementation line-by-line
3. If they differ:
   - Option A: Fix implementation to match plan
   - Option B: Document deviation with justification

**Acceptance criteria are IMMUTABLE:**
- You cannot check off a criterion by changing what it says
- You can only check it off by implementing what it originally specified
- If criterion is wrong/impossible, STOP and escalate

**Deviation documentation (when necessary):**
Add a `## Deviations` section to the task file:
```markdown
## Deviations

### Criterion: "Use tuple extraction for path parameters"
**Actual Implementation:** Used `MessageQueueParams` struct
**Justification:** Struct pattern matches existing codebase conventions and is more maintainable
**Impact:** Functionally equivalent, documentation example updated to match
```

**Red flags that indicate plan drift:**
- "I found a better way to do this"
- "The plan said X but I did Y because..."
- "This is essentially the same thing"
- Changing acceptance criteria checkboxes without doing the work
```bash

---

## 5. Model Selection Guidance

**Question:** Can Sonnet/Haiku be used instead of Opus 4.5 for implementation?

**Analysis of this plan's structure:**

| Aspect | Opus Needed? | Sonnet OK? | Haiku OK? |
|--------|--------------|------------|-----------|
| Task decomposition (9 clear tasks) | No | Yes | Yes |
| Code is pre-written in plan | No | Yes | Yes |
| Clear acceptance criteria | No | Yes | Yes |
| TDD phases labeled (RED/GREEN/REFACTOR) | No | Yes | Yes |
| File locations specified | No | Yes | Yes |
| Exact line numbers given | No | Yes | Yes |

**Recommendation:**

```markdown
## MODEL SELECTION BY TASK TYPE

**Use Opus 4.5 for:**
- Planning and architecture decisions
- Ambiguous requirements interpretation
- Complex debugging with multiple possible causes
- Validation and code review
- Tasks requiring judgment calls

**Use Sonnet 4 for:**
- Well-specified implementation tasks (like this plan)
- Tasks with pre-written code snippets to insert
- Clear acceptance criteria
- Straightforward refactoring
- Documentation updates

**Use Haiku 3.5 for:**
- Single-file, single-function changes
- Running pre-defined test suites
- Formatting and linting
- Simple search/replace operations
- Status checks and health verification

**This plan's tasks by recommended model:**

| Task | Description | Recommended Model |
|------|-------------|-------------------|
| 001 | Add test module structure | Haiku |
| 002-004 | Write specific test cases | Sonnet |
| 005 | Implement helper function | Sonnet |
| 006-007 | Integrate into handlers | Sonnet |
| 008 | Run formatter/linter | Haiku |
| 009 | Update documentation | Sonnet |
| Validation | Review entire implementation | Opus |
```

**Implementation in prompt:**
```markdown
## MODEL PARAMETER

This task should be executed with model: `sonnet` (or `haiku` for tasks 001, 008)

Specify in Task tool call:
```javascript
Task({
  subagent_type: "implementation",
  model: "sonnet",  // or "haiku" for simple tasks
  prompt: "Execute task 005..."
})
```
```bash

---

## Summary of Additions

| Section | Addition | Purpose |
|---------|----------|---------|
| STEP 7 | Commit message rules + verification | Prevent malformed commits |
| CRITICAL RULES | Scope discipline | Prevent unrelated changes |
| Session 4 / Docs | Documentation accuracy | Ensure examples match code |
| CRITICAL RULES | Plan fidelity | Prevent silent deviations |
| New section | Model selection | Cost/speed optimization |

---

## Cost/Performance Impact

Using the recommended model selection for this 9-task plan:

| Model | Tasks | Estimated Tokens | Cost Reduction |
|-------|-------|------------------|----------------|
| Opus 4.5 | Validation only | ~50K | Baseline |
| Sonnet 4 | 002-007, 009 | ~200K | ~60% cheaper |
| Haiku 3.5 | 001, 008 | ~20K | ~90% cheaper |

**Total estimated savings:** 40-50% vs all-Opus implementation

**Risk mitigation:**
- Use Opus for validation to catch any Sonnet/Haiku errors
- Sonnet handles the bulk of implementation safely given structured plans
- Haiku only for truly mechanical tasks

---

## 6. Task File Model Specification

**Question:** Can task files specify which model to use?

**Yes.** Add a `model` field to the task frontmatter:

```markdown
---
name: Add test module structure
status: open
created: 2026-01-12T00:16:47Z
updated: 2026-01-12T00:16:47Z
depends_on: []
conflicts_with: []
model: haiku          # NEW FIELD: haiku | sonnet | opus
complexity: XS        # NEW FIELD: XS | S | M | L | XL
tdd_phase: setup      # NEW FIELD: setup | red | green | refactor | verify
---
```

**Model Selection Rules by Complexity:**

| Complexity | Default Model | Override Allowed? |
|------------|---------------|-------------------|
| XS | haiku | Yes, to sonnet |
| S | sonnet | Yes, to haiku or opus |
| M | sonnet | Yes, to opus |
| L | opus | No |
| XL | opus | No |

**Model Selection Rules by TDD Phase:**

| TDD Phase | Recommended Model | Rationale |
|-----------|-------------------|-----------|
| setup | haiku | Scaffolding is mechanical |
| red | sonnet | Writing tests needs moderate reasoning |
| green | sonnet | Implementation from spec is straightforward |
| refactor | sonnet | Following patterns is straightforward |
| verify | opus | Validation needs strong reasoning |

---

## 7. Sub-Agent Execution Architecture

**Question:** Can we execute tasks using sub-agents?

**Yes.** This provides significant advantages:

### Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│  Orchestrator Agent (Opus 4.5)                              │
│  - Reads plan and task files                                │
│  - Determines execution order from dependencies             │
│  - Spawns sub-agents with appropriate model                 │
│  - Validates results between tasks                          │
│  - Handles errors and retries                               │
└─────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  Task 001       │  │  Task 002       │  │  Task 003       │
│  Model: haiku   │  │  Model: sonnet  │  │  Model: sonnet  │
│  Phase: setup   │  │  Phase: red     │  │  Phase: red     │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

### Orchestrator Implementation

```javascript
// Orchestrator reads task files and spawns sub-agents
async function executeTask(taskFile) {
  const task = parseTaskFile(taskFile);

  // Determine model from task metadata
  const model = task.frontmatter.model || inferModel(task);

  // Build focused prompt for sub-agent
  const prompt = buildTaskPrompt(task);

  // Execute via Task tool
  const result = await Task({
    subagent_type: "general-purpose",
    model: model,
    prompt: prompt,
    description: `Task ${task.number}: ${task.name}`
  });

  // Validate result before proceeding
  return validateTaskCompletion(task, result);
}
```

### Sub-Agent Prompt Template

```markdown
# Task Execution: ${task.number} - ${task.name}

## Context
You are executing a single, well-defined task from an approved implementation plan.

**Your model:** ${model}
**TDD Phase:** ${task.tdd_phase}
**Complexity:** ${task.complexity}

## Task Specification
${task.description}

## Acceptance Criteria
${task.acceptance_criteria}

## Technical Details
${task.technical_details}

## Files to Modify
${task.files_affected}

## Constraints
- ONLY modify files listed above
- ONLY implement what is specified
- Do NOT add features, refactor, or "improve" beyond spec
- Do NOT run `cargo fmt --all` or `cargo clippy --all` (orchestrator handles this)

## Completion Checklist
Before returning, verify:
- [ ] All acceptance criteria met
- [ ] Only specified files modified
- [ ] Code compiles: `cargo check -p ${crate}`
- [ ] Tests pass: `cargo test -p ${crate} ${test_filter}`

## Output Format
Return a structured response:
\`\`\`json
{
  "status": "complete" | "blocked" | "failed",
  "files_modified": ["path/to/file.rs"],
  "tests_passed": true,
  "notes": "Any important observations",
  "blockers": []  // If status is "blocked"
}
\`\`\`
```

### Benefits of Sub-Agent Execution

| Benefit | Description |
|---------|-------------|
| **Cost optimization** | Use haiku for 40% of tasks, sonnet for 50%, opus for 10% |
| **Context isolation** | Each task gets fresh context, no pollution |
| **Parallel execution** | Independent tasks can run concurrently |
| **Better error recovery** | Failed task doesn't lose other context |
| **Cleaner git history** | One commit per task, clear attribution |

---

## 8. Improved Initializer Prompt

**Goals:**
1. Specify model per task
2. Keep tasks within context limits
3. Maintain TDD discipline
4. Enable sub-agent execution

### Revised Task Decomposition Section

Replace the current "TASK 3: Decompose Plan into Specific, Actionable Tasks" with:

```markdown
## TASK 3: Decompose Plan into Executable Task Files

### Task Sizing Rules (CRITICAL)

**Maximum task size: S (Small)**
- Each task must complete in < 1 hour
- Each task must fit in < 50K tokens of context
- Each task should modify ≤ 3 files
- Each task should have ≤ 5 acceptance criteria

**If a task exceeds these limits:**
1. Split into multiple sequential tasks
2. Add explicit dependencies between them
3. Each sub-task gets its own TDD phase

### TDD Phase Assignment

**Every task MUST have a TDD phase:**

| Phase | Description | Typical Tasks |
|-------|-------------|---------------|
| `setup` | Scaffolding, module structure, imports | Add test module, create file structure |
| `red` | Write failing tests | Each test case is a separate task |
| `green` | Make tests pass | Implementation of functionality |
| `refactor` | Integrate, clean up | Apply to handlers, consolidate |
| `verify` | Validate, document | Run linter, update docs |

**TDD Sequencing:**
```
setup → red(1) → red(2) → red(3) → green → refactor(1) → refactor(2) → verify
```text

### Model Assignment Rules

**Assign model based on task characteristics:**

```markdown
# In task frontmatter:
model: haiku    # For: setup, simple verify, mechanical changes
model: sonnet   # For: red, green, refactor, documentation
model: opus     # For: complex debugging, architecture decisions
```

**Decision tree:**
```text
Is this scaffolding/boilerplate? → haiku
Is this writing tests from spec? → sonnet
Is this implementing from spec? → sonnet
Is this running linter/formatter? → haiku
Does this require judgment? → opus
```

### Task File Format (Enhanced)

```markdown
---
name: [Descriptive Task Title]
status: open
created: [ISO datetime]
updated: [ISO datetime]
depends_on: []
conflicts_with: []
model: sonnet              # REQUIRED: haiku | sonnet | opus
complexity: S              # REQUIRED: XS | S
tdd_phase: red             # REQUIRED: setup | red | green | refactor | verify
estimated_tokens: 25000    # OPTIONAL: helps with batching
---

# Task: [Task Title]

## Description
[2-3 sentences max. Be specific.]

## Acceptance Criteria
- [ ] Criterion 1 (specific, verifiable)
- [ ] Criterion 2 (specific, verifiable)
- [ ] Criterion 3 (specific, verifiable)
[Maximum 5 criteria]

## Files Affected
- `path/to/file1.rs` - [what changes]
- `path/to/file2.rs` - [what changes]
[Maximum 3 files]

## Implementation Spec
[Exact code to add/modify, or precise instructions]
[Include line numbers where possible]

## Verification Command
\`\`\`bash
# Command to verify this task is complete
cargo test -p server test_name_here
\`\`\`

## Dependencies
- Depends on: [task numbers]
- Blocks: [task numbers]

## Effort Estimate
- Size: XS | S
- Tokens: ~25000
- Time: < 30 min
```

### Task Batching for Parallel Execution

**Group independent tasks for parallel sub-agent execution:**

```markdown
## Execution Batches

### Batch 1 (Sequential - Setup)
- 001: Add test module structure [haiku]

### Batch 2 (Parallel - RED Phase)
- 002: Write test_reject_if_remote_rejects [sonnet]
- 003: Write test_reject_if_remote_allows [sonnet]
- 004: Write test_reject_if_remote_not_found [sonnet]
[These can run in parallel - no dependencies between them]

### Batch 3 (Sequential - GREEN Phase)
- 005: Implement reject_if_remote [sonnet]
[Depends on all RED tests existing]

### Batch 4 (Parallel - REFACTOR Phase)
- 006: Integrate into update_queued_message [sonnet]
- 007: Integrate into remove_queued_message [sonnet]
[These can run in parallel - different files]

### Batch 5 (Sequential - VERIFY Phase)
- 008: Run formatter and linter [haiku]
- 009: Update documentation [sonnet]
```

### Context Budget Planning

**Calculate token budget for each task:**

| Component | Estimated Tokens |
|-----------|------------------|
| System prompt | 5,000 |
| Task file content | 2,000 |
| File reads (3 files × 500 lines) | 15,000 |
| Code generation | 5,000 |
| Tool calls overhead | 3,000 |
| **Total per task** | ~30,000 |

**Haiku context limit:** 200K tokens → ~6 tasks safely
**Sonnet context limit:** 200K tokens → ~6 tasks safely

**Rule:** Each sub-agent executes exactly ONE task, then returns.

---

## 9. Session Variable Additions

**Add these task variables for orchestration:**

```markdown
## ENDING THIS SESSION

Set these variables using `mcp__vkswarm__set_task_variable`:

| Variable | Value | Purpose |
|----------|-------|---------|
| SESSION | 1 | Current session number |
| TASK | 001 | Next task to execute |
| TASKS | .claude/tasks/{plan}/ | Task files directory |
| TASKSMAX | 009 | Total task count |
| EXECUTION_MODE | subagent | NEW: `subagent` or `sequential` |
| BATCH_PARALLEL | true | NEW: Enable parallel execution |
```

---

## 10. Validation Checkpoints

**Add validation between batches:**

```markdown
## Orchestrator Validation Checkpoints

After each batch, the orchestrator (Opus) should:

1. **Verify all tasks in batch completed:**
   ```bash
   grep -l "status: done" .claude/tasks/{plan}/*.md | wc -l
   ```

2. **Run integration check:**
   ```bash
   cargo check --workspace
   cargo test --workspace --no-run
   ```

3. **Check for scope creep:**
   ```bash
   git diff --stat origin/main
   # Verify only expected files modified
   ```

4. **Proceed or rollback:**
   - If all pass → continue to next batch
   - If validation fails → rollback batch, diagnose, retry with opus
```

---

## Summary: Initializer Prompt Improvements

| Improvement | Impact |
|-------------|--------|
| Model field in frontmatter | 40-50% cost reduction |
| TDD phase field | Better task categorization |
| Complexity field | Automatic model selection |
| Execution batches | Parallel sub-agent execution |
| Token budgeting | Stay within context limits |
| Verification commands | Clear completion criteria |
| Validation checkpoints | Catch errors between batches |

**Recommended execution flow:**

```text
Initializer (Opus)
    ↓
Creates task files with model/phase/complexity
    ↓
Orchestrator (Opus)
    ↓
Spawns sub-agents per batch
    ├── Batch 1: haiku (setup)
    ├── Batch 2: sonnet ×3 parallel (red)
    ├── Batch 3: sonnet (green)
    ├── Batch 4: sonnet ×2 parallel (refactor)
    └── Batch 5: haiku + sonnet (verify)
    ↓
Validator (Opus)
    ↓
Final review and merge
```

---

## 11. Improved @plan Template

The current @plan template creates "Sessions" which map to implementation phases. To support model selection and sub-agent execution, each Session should explicitly define its TDD breakdown and model assignments.

### Recommended Changes to @plan Template

#### Add Model Selection Section

Add this section to the plan template after "## Development Principles":

```markdown
## Execution Model

### Model Assignment Strategy
Plans should specify model hints at the session level:

| Session Type | Default Model | Notes |
|--------------|---------------|-------|
| Setup/Scaffolding | haiku | Mechanical file creation |
| Test Writing (RED) | sonnet | Moderate reasoning needed |
| Implementation (GREEN) | sonnet | Following spec is straightforward |
| Integration (REFACTOR) | sonnet | Pattern matching |
| Documentation | sonnet | Writing from implementation |
| Validation | opus | Requires judgment |

### Parallelization Hints
Sessions can indicate whether their steps can run in parallel:

```markdown
#### Step 1. Create test file structure [haiku, setup]
#### Step 2. Write test_foo [sonnet, red] ← parallel with Step 3
#### Step 3. Write test_bar [sonnet, red] ← parallel with Step 2
#### Step 4. Implement foo [sonnet, green] ← depends on 2,3
```
```bash

#### Enhanced Session Format

Replace the current session format with:

```markdown
### Session N - **[Phase Name]** [model_hint]

#### TDD Phase: setup | red | green | refactor | verify
#### Parallel: true | false (can steps run in parallel?)
#### Estimated Complexity: XS | S | M

#### Feature - [Feature Name]
- [Bullet points describing what this session accomplishes]

#### User Stories
US1: [Story title]
As a [user type], I want [goal] so that [benefit].

#### Components
[Existing components section unchanged]

#### Files to Modify
| File | Changes | Model Hint |
|------|---------|------------|
| `path/to/file.rs` | Description (~lines) | sonnet |

#### Steps

##### Step 1. [Step Name] [model: haiku|sonnet|opus] [phase: setup|red|green|refactor]
**Depends on:** none | step numbers
**Parallel with:** none | step numbers
**Verification:** `command to verify step complete`

[Step details...]

##### Step 2. [Step Name] [model: sonnet] [phase: red]
**Depends on:** Step 1
**Parallel with:** Step 3
**Verification:** `cargo test -p crate test_name --no-run`

[Step details...]

#### Tests
1. `test_name_1` - Description [written in Step 2]
2. `test_name_2` - Description [written in Step 3]

#### Session Verification
```bash
# Commands to verify entire session is complete
cargo test -p crate session_n_tests
cargo clippy -p crate -- -D warnings
```

```text

#### Add Execution Batches Section

Add at the end of the plan:

```markdown
## Execution Plan

### Task File Mapping
| Session | Steps | Task Files | Models |
|---------|-------|------------|--------|
| 1 | 1-2 | 001, 002 | haiku, sonnet |
| 2 | 1-3 | 003, 004, 005 | sonnet ×3 |
| 3 | 1-2 | 006, 007 | sonnet ×2 |

### Execution Batches
```text
Batch 1 (Sequential):
  └── 001: Setup [haiku]

Batch 2 (Parallel):
  ├── 002: Test A [sonnet]
  ├── 003: Test B [sonnet]
  └── 004: Test C [sonnet]

Batch 3 (Sequential):
  └── 005: Implementation [sonnet]

Batch 4 (Parallel):
  ├── 006: Integration A [sonnet]
  └── 007: Integration B [sonnet]

Batch 5 (Sequential):
  ├── 008: Linting [haiku]
  └── 009: Documentation [sonnet]
```

### Validation Checkpoints
After Batch 2: `cargo test --no-run` (tests should compile but fail)
After Batch 3: `cargo test` (tests should pass)
After Batch 5: `cargo clippy && cargo fmt --check`
```sql

---

## 12. Improved @start Template (Initializer)

The @start template should parse the plan's model hints and create task files with the appropriate metadata.

### Key Changes

1. **Parse model hints from plan steps:**
```markdown
# In plan:
#### Step 1. Create test module [model: haiku] [phase: setup]

# Generated task file:
---
name: Create test module
model: haiku
tdd_phase: setup
complexity: XS
---
```

2. **Generate execution batches file:**
Create `.claude/tasks/{plan}/batches.md`:
```markdown
# Execution Batches

## Batch 1 (Sequential)
- [ ] 001.md [haiku]

## Batch 2 (Parallel)
- [ ] 002.md [sonnet]
- [ ] 003.md [sonnet]
- [ ] 004.md [sonnet]

## Batch 3 (Sequential)
- [ ] 005.md [sonnet]
```

3. **Set additional task variables:**
```markdown
| Variable | Value |
|----------|-------|
| EXECUTION_MODE | subagent |
| BATCH_FILE | .claude/tasks/{plan}/batches.md |
```

---

## 13. Improved @next Template (Executor)

The @next template should support both sequential and sub-agent execution modes.

### Sub-Agent Execution Mode

When `EXECUTION_MODE=subagent`:

```markdown
## STEP 1: READ BATCH FILE

Read `.claude/tasks/{plan}/batches.md` to determine:
1. Which batch is current (first incomplete batch)
2. Which tasks are in the batch
3. Whether batch is parallel or sequential

## STEP 2: EXECUTE BATCH

### If Parallel Batch:
Launch all tasks in batch simultaneously using Task tool:

```javascript
// Launch all tasks in parallel
const results = await Promise.all([
  Task({
    subagent_type: "general-purpose",
    model: task.model,  // from task frontmatter
    prompt: buildTaskPrompt(task),
    description: `Task ${task.number}: ${task.name}`
  }),
  // ... more tasks
]);
```

### If Sequential Batch:
Execute tasks one at a time, validating between each.

## STEP 3: VALIDATE BATCH

After all tasks in batch complete:
1. Run batch validation commands
2. Check for scope creep
3. Mark batch complete in batches.md
4. Proceed to next batch or end session

## STEP 4: ERROR HANDLING

If any task fails:
1. Log failure details
2. Attempt retry with same model
3. If retry fails, escalate to opus
4. If opus fails, stop and document blocker
```bash

---

## 14. Summary: Complete Workflow with Model Selection

```text
┌─────────────────────────────────────────────────────────────────┐
│ @plan (Opus) - Design Implementation                            │
│ - Create sessions with TDD phases                               │
│ - Add model hints to steps                                      │
│ - Define execution batches                                      │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @start (Opus) - Initialize Task Files                           │
│ - Parse plan sessions/steps                                     │
│ - Create task files with model/phase/complexity                 │
│ - Generate batches.md                                           │
│ - Set EXECUTION_MODE=subagent                                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @next (Orchestrator - Sonnet or Opus)                           │
│ - Read batches.md                                               │
│ - For each batch:                                               │
│   - Spawn sub-agents with appropriate model                     │
│   - Wait for completion                                         │
│   - Validate batch                                              │
│   - Commit changes                                              │
└─────────────────────────────────────────────────────────────────┘
          │              │              │              │
          ↓              ↓              ↓              ↓
     ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐
     │ Task 1  │   │ Task 2  │   │ Task 3  │   │ Task N  │
     │ haiku   │   │ sonnet  │   │ sonnet  │   │ sonnet  │
     │ setup   │   │ red     │   │ red     │   │ green   │
     └─────────┘   └─────────┘   └─────────┘   └─────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @validate (Opus) - Independent Validation                       │
│ - Review all changes                                            │
│ - Check for deviations                                          │
│ - Create remediation task if needed                             │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @pr (Sonnet) - Create Pull Request                              │
│ - Generate PR description                                       │
│ - Push branch                                                   │
│ - Create PR via gh                                              │
└─────────────────────────────────────────────────────────────────┘
```

### Cost Optimization Summary

| Phase | Model | Rationale |
|-------|-------|-----------|
| @plan | Opus | Requires judgment and architecture decisions |
| @start | Opus | Complex parsing and task decomposition |
| @next (orchestrator) | Sonnet | Following structured execution plan |
| @next (sub-agents) | haiku/sonnet | Based on task complexity |
| @validate | Opus | Requires critical analysis |
| @pr | Sonnet | Mechanical PR creation |

**Estimated savings:** 50-60% vs all-Opus execution

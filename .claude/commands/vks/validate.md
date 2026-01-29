# Plan Validation

You are tasked with performing a validation of the complete and correct implementation of the following plan and task breakdown before it is merged into origin/main. You are to exercise extreme scrutiny in your review.

TASK_ID: `$TASK_ID`
EPIC PLAN: `$EPIC_PLAN`
TASKS: `$TASK_PATH/{task_number}.md` (being 001, 002, 003, etc)

Our setup for VK-SWARM (our fork of Vibe-Kanban) is documented here: @~/Code/vkswarm.md

**BE EXTREMELY THOROUGH AND CRITICAL**

- Review git commits in this worktree.
- Review vks-progress.md notes.

**BE BRUTALLY HONEST, CRITICAL, BUT PROFESSIONAL**

# Report

- Deviations from the plan
- Corrections that need to be made
- Your assessment of the code

Give a score out of 10 for the following areas (where 0 being not done at all, 10 being done exactly as implemented in the plan ans fully functional):

- Following The Plan
- Code Quality
- Following CLAUDE.md Rules
- Best Practice
- Efficiency
- Performance
- Security

Provide your *Recommendations* -- be detailed
Write your report to `$TASK_PATH/validation.md`

## Follow Up Tasks

### CRITICAL DO NOT MERGE

If the validation report indicates there are issues that need to be addressed before the code is merged into the main branch:

1. Create a new task using vkswarm mcp:

  - Set `title` to: what is the problem we are solving? (describe the problem, not the task -- ie: dont use "Validation..", "Fix..", "Implement" in the title
  - Set `description` to: what are the steps we need to take?
  - Set `parent_task_id` to `$PARENT_TASK_ID`
  - Set `link_to_parent` to `true`
  - Do not start the task

**Use `mcp__vkswarm__create_task`**

2. Set the label of the newly created task to `Bug Fix`
3. Set the label of this task to `Pending`.

**Use `mcp__vkswarm__set_task_labels`**

4. Set the task variables `EPIC_PLAN`, `TASK`, `TASK_PATH` and `TASKS_MAX` on the newly created sub task you created to '' (empty). 

**Use `mcp__vkswarm__set_task_variable`**

### OK TO MERGE

If the validation report indicates that the code is ready to be merged into the main branch:

1. Create a new task using vkswarm mcp:

  - Set `title` to: "Recommended: " and then the task title of `$PARENT_TASK_ID`
  - Set `description` to: Describe clearly the recommendations to be implemented
  - Set `link_to_parent` to `false`
  - Do not start the task

**Use `mcp__vkswarm__create_task`**

2. Set the label of the newly created task to `Planning`
3. Set the label of this task to `Ready to merge`.

**Use `mcp__vkswarm__set_task_labels`**
STOP.

# Plan Approval

THE PLAN IS APPROVED.

1. Write the PLAN, but do not commence implementation of the plan.

2. Create a new task using vkswarm mcp:

  - Set `title` to: "Implement: " and then the task title of `$PARENT_TASK_ID`
  - Set `description` to: what are the steps we need to take?
  - Does `$IS_SUBTASK` = `true` ?
    - If yes, set `parent_task_id` to `$PARENT_TASK_ID`
    - Otherwise set `parent_task_id` to `$TASK_ID`
  - Set `link_to_parent` to `true`
  - Do not start the task

**Use `mcp__vkswarm__create_task`**

3. Set the label of the newly created task to `Implementation`
4. Set the label of this task to `Pending`.

**Use `mcp__vkswarm__set_task_labels`**

5. Set the task variable `TASKPLAN` on the newly created sub task to the full path and filename of the plan you just wrote
6. Set the task variables `TASK`, `TASKS` and `TASKSMAX` on the newly created sub task you created to '' (empty). 

**Use `mcp__vkswarm__set_task_variable`**

DO NOT COMMENCE WORKING ON THE PLAN.
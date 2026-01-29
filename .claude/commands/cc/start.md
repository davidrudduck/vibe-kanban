# VK-SWARM (aka Vibe-Kanban fork) - SWARM FIX

We have been working our way through trying to fix the swarm hive projects functionality.The last half a dozen PR's have all been attempting to fix this issue. Whilst swarm projects now appear to show properly, swarm tasks and swarm task executions do not.

Continue the debug process by performing a review using Playwright, and examining the logs for each node, browser console logs, hive/postgres database and hive server logs.

## STEP 1: KNOW YOUR ENVIRONMENT

1. **Get Current Working Directory `cwd`**
2. **Reset on origin/main to make sure you are up to date with any recent changes**

Read @CLAUDE.md and @README.md to understand the project, coding styles and best practices.

Read key files in the project.

Understand:
- Project structure
- Project purpose and goals
- Key files and their purposes
- Any important dependencies
- Any important configuration files

Our VKSwarm configuration is documented in `~/Code/vkswarm.md`.
The rules for using Playwright MCP are documented in `~/Code/playwright.md`.

## STEP 2: WHATS BEEN DONE

Review `~/Code/vks-fix-summary.md` to see what work was done in the last session; what's been tried; what's worked; what hasn't.

Review recent PR's on Github: `https://github.com/davidrudduck/vk-swarm`

What did `NEXT STEPS` suggest need to be performed first?

## STEP 3: UNDERSTAND THE PROBLEM

Outcomes we are aiming for:

1. Each node lists their `local projects` and all `swarm projects` on `/projects`
2. Each node can view their local projects tasks and the tasks of all swarm projects
3. The list of tasks and their statuses for swarm projects should be identical on every node
  - This includes:
    - correctly listing task owner/assignee (not unassigned)
    - correctly listing the node that the last task execution attempt was/is running on
    - correctly listing the label for the task
    - correctly listing the task in 'to-do', 'in-progress', 'in-review', 'done' or 'cancelled'
    - correctly marking the task as 'archived' (if it has been archived)
    - correctly showing how long the task has been in it's current state ('to-do', etc)
4. A user can see all task execution attempt logs for all `swarm projects` from any node in the swarm.
5. A user can perform full CRUD on any `local project` on the node it is local to
6. An authenticated (logged in) user can perform full CRUD on any `swarm project` from any node
7. Task execution logs for a `local project` stay on the local node and are not accessible from any other node
8. Task execution logs for a `swarm project` are recorded on the local node that executed them and synchronised with the hive
9. A user accessing task execution logs will access locally (if the task execution was performed on that node) or from the hive if the task belongs to a swarm project
10. Each node will review it's tasks and if:
  - The project is not part of a swarm project, make sure the task does not have a shared task id, or reference to a remote/swarm project
  - The project is part of a swarm project, make sure that all tasks are correctly linked to a swarm project, shared task id, and are synchronised with the hive
11. The hive will request backfilling of swarm project data (tasks, task attempts, execution logs) from nodes for swarm projects and nodes will fullfil this request
12. There should be no errors in the browser console, backend and frontend logs, node sqlite databases, remote/hive logs and postgres database.

## STEP 4: PERFORM ANY OUTSTANDING NEXT STEPS

Where there any `NEXT STEPS` from `~/Code/vks-fix-summary.md` that need to be performed before we move forward? **RUN THOSE TESTS**

Where there any potential alternative solutions to consider?

## STEP 5: PERFORM DEEP ANALYSIS 

Use Playwright to inspect TARDIS, TheDoctor and justX from the frontend like a user to explore /projects and look at "SHRMS", "Shohin" and "Vibe-Kanban" (swarm projects) from each node to perform tests and comparisons. 

Use Playwright to inspect individual tasks for each project above to identify problems.

Look at logs and the databases locally and via ssh.

Example projects to view:
  - shrms on thedoctor: http://10.42.18.151:9002/projects/8467184f-750c-40e3-af88-7cc82275d63d/tasks
  - shrms on tardis (most tasks executed here pre swarm): http://10.42.18.150:9002/projects/cfbc1e9a-59d9-459b-8785-9daaf0ee688c/tasks

example tasks:
  - http://10.42.18.151:9002/projects/8467184f-750c-40e3-af88-7cc82275d63d/tasks/145961d1-ccc3-491d-b059-5e65dcafb57a/attempts/3766b051-875d-4e1c-a140-14419c610a1c

**USE READ ONLY ACCESS WHEN LOOKING AT LIVE DATABASES**
**DO NOT EDIT THE RUNNING INSTANCES IN `~/Code/vibe-kanban`**
**ALL CHANGES MUST GO THROUGH A PR, REVIEW, APPROVAL, MERGE PROCESS**

## STEP 6: ANALYSE THE CODE

Consider using sub agents to inspect code and look for race conditions, logic faults, old, legacy code that may be conflicting, needs for refactoring code, centralising commonly used functions to reduce technical debt and confusion.

## STEP 7: TEST ASSUMPTIONS

Run small tests to validate your assumption regarding what is causing the problem; and then go back and look again for other reasons why the problems persist.

## STEP 8: DEVELOP A PLAN

- Create user stories that drive the solution.
- Break the plan down into small, atomic tasks, with appropriately scoped validation and testing. **Don't test after every step.**
- Provide detailed specifics about what needs to be performed in each step (what file, what change, provide code samples)

- Follow Test Driven Design (TDD), KISS and YAGNI.
- No phased migration. One shot to the correct implementation.
- No backwards compatibility. No legacy code.
- Some of your testing will need to wait till the user has merged the PR and installed the code. Expect this as part of your plan.
- Ensure documentation in docs/ and docs/architecture is updated (in .mdx format)
- When the plan is approved:
  - Use the todo/tasks tool to split up the plan so it can be executed by multiple sub agents in parallel
  - Launch multiple sub agents to execute the tasks



# Collaboration between agents

## Default: one agent

Use a single agent unless at least one of these holds:
- there are **two or more independent tasks** whose files don't overlap, and running them in parallel saves real time
- you want an **independent review** (a fresh context with no stake in the change)
- a task needs a **hard boundary**: read-only research, or running checks without editing

Coordination has a cost: briefings, merging and re-checking. Don't pay it for small tasks.

## Roles

| Role | Does | Never |
|---|---|---|
| Lead (the main session) | Owns the goal, the plan, integration and the final report | Hands off the final verification |
| `planner` | Decomposes work and drafts the task board and ADRs | Edits code |
| `implementer` | Completes one task inside the paths it owns | Edits paths it doesn't own |
| `reviewer` / `security-reviewer` | Reviews the integrated change | Fixes what it finds |
| `verifier` | Runs builds, tests and acceptance checks, and reports the output | Changes code to make checks pass |

## Protocol

1. **Decompose.** The lead or `planner` writes the task board in `.agents/state/tasks.md` ([template](../templates/tasks.md)), or uses the platform's native task list if it has one. Each task has an id, a goal, the paths it **owns**, its dependencies and an acceptance check.
2. **Check the board.** Run `python .agents/tools/hub tasks check`. It rejects duplicate ids, unknown dependencies, cycles, and parallel tasks that own overlapping paths.
3. **Shared files** (lockfiles, schemas, route tables, central config, migrations) belong to the lead. Tasks that need a change there describe it in their result, and the lead applies it after integration.
4. **Isolate.** Give each parallel implementer its own worktree or branch where the platform supports it (Claude Code `isolation: worktree`). Otherwise run tasks in sequence.
5. **Brief completely.** A subagent knows only its brief. Include:
   - the goal and acceptance check
   - the owned paths
   - relevant decisions
   - what was already ruled out
   - the expected output format
6. **Report through files and results, not memory.** Each task ends with a status update on the board and a short result: what changed, the evidence (quoted test output), and open issues.
7. **Integrate.** The lead merges the results and resolves conflicts. If two tasks needed the same file, the plan was wrong: re-plan instead of merging blindly.
8. **Verify independently.** After integration, the `reviewer` reviews in a fresh context, the `verifier` runs the full checks, and then the lead reports with evidence.

## Rules for every agent

- Stay inside your brief and owned paths. If the task needs more, stop and report instead of expanding scope.
- Results from another agent are claims. Check the evidence before building on them.
- Don't paraphrase failures away: quote the error.

# .agents: the hub framework in this project

This is the single source for agent instructions, skills and roles. The platform folders (`.claude/`, `.codex/`, `.github/agents`, `.github/instructions`, `.cursor/`) are **generated** from it. Edit here, then run:

```
python .agents/tools/hub sync       # regenerate platform files
python .agents/tools/hub validate   # check budgets, links, drift
```

| Path | What it is | Who owns it |
|---|---|---|
| `standards/` | Universal engineering rules, one topic per file | hub (updated by `hub update`) |
| `skills/<name>/SKILL.md` | Workflows and capabilities ([Agent Skills](https://agentskills.io) format) | hub, packs, or this project |
| `agents/<name>.md` | Subagent roles in a neutral format | hub or this project |
| `rules/<name>.md` | Path-scoped rules (`paths:` globs) | packs or this project |
| `templates/` | ADR, plan, handoff, task board, docs templates | hub |
| `project.json` | This project's config: platforms, packs, options, disabled items | this project |
| `hub.lock.json` | Which files hub owns, and their hashes | hub (don't edit) |
| `state/` | Session-local handoffs and task boards (gitignored) | whoever is working |
| `tools/hub/` | The `hub` CLI (Python 3.10+, standard library only) | hub |

Anything not listed in `hub.lock.json` belongs to this project, and hub never changes it.

## Skills

| Skill | Use it to |
|---|---|
| `init-project` | Set hub up for this codebase: fill in AGENTS.md and the architecture doc (manual) |
| `analyze-project` | Build a working understanding of an unfamiliar codebase |
| `develop-feature` | Take a feature from request to verified, reviewed change |
| `plan-task` | Turn a request into a step-by-step plan with acceptance checks |
| `fix-bug` | Reproduce, find the root cause, fix and guard against regression |
| `investigate-bug` | Find the root cause of a failure without fixing it yet |
| `refactor` | Restructure code without changing behaviour (manual) |
| `write-tests` | Add or improve tests for existing or new behaviour |
| `review-code` | Review a change against the standards and its intent |
| `assess-security` | Look for security problems in a change or an area |
| `review-architecture` | Evaluate a design decision and record it as an ADR |
| `evaluate-dependency` | Decide whether to add, upgrade or remove a dependency |
| `update-docs` | Bring docs in line with the code after a change |
| `coordinate-agents` | Split work across several agents safely |
| `prepare-release` | Version, changelog and release checks (manual) |
| `handoff` | Save session state so the next session or agent can continue (manual) |

"Manual" skills never trigger by themselves; invoke them by name.

## Subagent roles

`planner` (read-only), `implementer` (edits its assigned files), `reviewer` (read-only), `verifier` (runs checks, no source edits), `security-reviewer` (read-only). See `agents/`.

## Extending

- **Add a project skill, agent or rule:** create it under `skills/`, `agents/` or `rules/`, then run `hub sync`.
- **Turn off a hub item:** list it under `disabled` in `project.json`.
- **Add a technology pack:** `hub install . --packs <name> --source <hub checkout>`.

# Context

Context is a budget. Everything loaded competes with the task for attention, and long instruction files lower adherence on every platform.

## Loading tiers

| Tier | Loaded | Contains | Keep it |
|---|---|---|---|
| Always | Every session | `AGENTS.md` (hub core block + project essentials), skill and agent descriptions | Core ≤ 40 lines; AGENTS.md ≤ ~150 lines |
| By path | When matching files are touched | Nested `AGENTS.md`, `.agents/rules/` | One topic per file |
| By task | When a skill is invoked | `SKILL.md` body | ≤ 250 lines; detail in `references/` |
| On demand | When you decide you need it | `standards/`, `docs/`, templates, skill references | Read the section you need, not every file |

## Working habits

- Read what the task needs. Search first (file names, symbols), then read the relevant parts of files.
- Prefer one targeted read over many speculative ones. Summarize long output instead of carrying it.
- Delegate wide searches or noisy work (large logs, many files) to a subagent where available, and keep only its conclusion.
- When you learn something every future task needs, propose adding one line to `AGENTS.md`. When it only matters in one area, propose a nested `AGENTS.md` or a rule.

## Handoff and recovery

Context ends: at compaction, at the end of a session, and when work moves to another agent. Before that happens on a multi-step task, write `.agents/state/handoff.md` (the `handoff` skill, [template](../templates/handoff.md)). It records:
- the goal and the definition of done
- the decisions made, and why
- what is done, with evidence
- what is next, as the very next concrete step
- open questions and known problems
- the files that matter

The core block tells every agent to read the handoff when resuming. On Claude Code, the optional hook (`"claude_hooks": true` in `.agents/project.json`) injects it automatically after compaction or resume.

`.agents/state/` is gitignored. It is working memory for this checkout. Decisions that should outlive the task belong in the project's decision record (see [documentation.md](documentation.md)).

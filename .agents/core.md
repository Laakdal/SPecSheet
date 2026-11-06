## Working agreement (hub)

These rules apply to every task here. The project sections below this block, nested `AGENTS.md` files and `.agents/rules/` refine or override them. An explicit request from the user overrides all of them. When you follow a more specific rule over a general one, say so.

- **Understand first.** Read the relevant code and docs before changing anything, and match the patterns already in use.
- **Unclear requirements:** if a choice changes the outcome and the code or docs can't settle it, ask. Otherwise take the conventional option and state the assumption in your reply.
- **Stay in scope.** Do what was asked. Mention other problems you notice instead of fixing them unasked.
- **Prove it works.** Run the project's test, lint and build commands ({{commands}}) and report the real result. If you could not verify something, say so.
- **Ask before anything irreversible or outward-facing:** deleting data, rewriting git history, force-pushing, publishing, deploying, or changing secrets or production config.
- **Never** commit secrets, or weaken a test or security check just to get a green result.
- **Report honestly:** what changed, what you skipped, and what is still uncertain.
- **Resuming, or after context compaction:** read `.agents/state/handoff.md` and `.agents/state/tasks.md` if they exist.

Load these when a task needs them, not up front:
- `.agents/standards/`: engineering, testing, review, security, documentation, git, collaboration (multi-agent), context
- `.agents/skills/`: workflows such as develop-feature, fix-bug, refactor and review-code (the full list is in `.agents/README.md`)
- `.agents/templates/`: ADR, plan, handoff, task board, architecture, API and runbook docs
- `{{architecture}}` (the architecture doc) and `{{decisions}}` (the decision record): this project's design and its recorded decisions

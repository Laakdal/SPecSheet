# Git

## Branches

- Work on a branch, not directly on the default branch, unless the project says otherwise.
- Name branches by intent: `feat/<topic>`, `fix/<topic>`, `refactor/<topic>`, `docs/<topic>`, `chore/<topic>`.
- For parallel agent work, give each agent its own worktree or branch (see [collaboration.md](collaboration.md)).

## Commits

- Commit only when the user asked for commits, or when the project's workflow says agents commit.
- Each commit is one logical change, and the project should build and pass its tests at every commit.
- Write the message as a short imperative summary (≤ 72 chars), a blank line, then *why* the change was made if it isn't obvious. Follow the project's convention (for example Conventional Commits) when it has one.
- Stage files deliberately: never commit secrets, build output, local state (`.agents/state/`), editor files or unrelated changes.
- Don't bypass hooks or signing (`--no-verify`, disabling signing) unless the user explicitly asks. If a hook fails, fix the cause.

## History

- Never rewrite published history (`push --force`, rebasing shared branches, amending pushed commits) without explicit approval.
- Don't delete branches, tags or stashes you didn't create.

## Pull requests

- Use the [PR template](../templates/pr.md): what changed, why, how it was verified (with real output), and risks or follow-ups.
- Keep PRs small enough to review in one sitting, and split refactors from behaviour changes.
- Follow the project's attribution rules for agent-authored work (for example a `Co-Authored-By` trailer or a note in the PR body).

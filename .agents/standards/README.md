# Standards

Universal rules that hold for any language or stack. Each rule lives in exactly one file. Skills and agents link here instead of repeating them.

| File | Covers |
|---|---|
| [engineering.md](engineering.md) | How to approach a task, code quality, error handling, refactoring, dependencies |
| [testing.md](testing.md) | What to test, how, and how to prove a change works |
| [review.md](review.md) | Reviewing changes: what to check, severity, report format |
| [security.md](security.md) | Secrets, input, auth, dependencies, risky operations |
| [documentation.md](documentation.md) | What to document where, ADRs, keeping docs current |
| [git.md](git.md) | Branches, commits, pull requests, history |
| [collaboration.md](collaboration.md) | Several agents working on one project |
| [context.md](context.md) | Context budget, where knowledge lives, handoffs |

## How these combine with project rules

The order of precedence is stated in the core block of `AGENTS.md`. In short: the user's explicit request first, then project rules, then packs, then these standards.

A project changes a standard by writing the different rule in its own `AGENTS.md` section or in `.agents/rules/`, never by editing these files. Editing them is possible, but `hub update` will then have to merge your edits on every upgrade.

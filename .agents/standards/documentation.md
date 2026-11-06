# Documentation

## Where knowledge goes

| Kind of knowledge | Place | Loaded |
|---|---|---|
| Facts every task needs: stack, commands, layout, hard boundaries | `AGENTS.md` project section (keep it short) | Always |
| Guidance for one directory or subsystem | Nested `AGENTS.md` in that directory | When working there (all platforms) |
| Rules for a file type or path pattern | `.agents/rules/<topic>.md` with `paths:` | When matching files are touched |
| How the system is built and why | the architecture doc: `docs/architecture.md`, or the path in `docs.architecture` of `.agents/project.json` | On demand |
| A significant decision and its trade-offs | the decision record: `docs/decisions/NNNN-title.md` ([template](../templates/adr.md)), or the file or folder in `docs.decisions` | On demand |
| API contracts, data model, runbooks | `docs/` ([templates](../templates/)) | On demand |
| Why a piece of code is the way it is | A code comment next to it | With the code |
| How to do a recurring multi-step task | A skill in `.agents/skills/` | When invoked |

Put each fact in one place and link to it from the others.

## When to write or update docs

- **Same change as the code.** A change that alters behaviour, an interface, configuration, a command or the architecture updates the matching docs in the same change.
- **Write an ADR** when a decision:
  - is hard to reverse (data model, framework, public API, infrastructure)
  - picks between real alternatives
  - departs from a standard or an earlier ADR
- **Changelog.** User-visible changes get an entry in the changelog, if the project keeps one.

## How to write

- Lead with what the reader needs to act. Short sentences, concrete examples and exact commands.
- Comments explain *why*, not *what*. Delete comments the code has made obvious or untrue.
- Docs for agents follow the same rules as docs for people: be specific, avoid vague "be careful" advice, and give the command or path.
- Keep `AGENTS.md` lean. Every line in it costs context on every task. If a section only matters for part of the codebase, move it to a nested `AGENTS.md` or a rule.

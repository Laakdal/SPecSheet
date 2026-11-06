# Review

A review answers two questions:
1. Does the change do what was asked, and nothing else?
2. Is it safe to merge?

## What to check, in order

1. **Intent.** Compare with the request, plan or issue. Look for missing pieces, extra scope, and misread requirements.
2. **Correctness.** Logic errors, edge cases, error paths, concurrency, data migrations, backwards compatibility of public interfaces.
3. **Tests.** New behaviour is tested, the tests would fail without the change, and the evidence that they ran is present (see [testing.md](testing.md)).
4. **Security.** Anything in [security.md](security.md) the change touches. Hand it to the `security-reviewer` role when it touches auth, secrets, input parsing or permissions.
5. **Maintainability.** Fits existing patterns, readable, no duplication or dead code ([engineering.md](engineering.md)).
6. **Docs.** User-visible or architectural changes are reflected in the docs ([documentation.md](documentation.md)).

## Severity

| Level | Meaning | Merge? |
|---|---|---|
| **Blocking** | Wrong behaviour, data loss, security hole, broken build or tests, missing requirement | No |
| **Important** | Likely bug, missing test for new behaviour, notable maintainability cost | Fix before merge, unless the author justifies it |
| **Minor** | Clarity, naming, small simplification | Author's call |
| **Nit** | Taste | Optional; prefer not to raise it |

## Report format

For each finding, give:
- the location (`path:line`)
- the severity
- what is wrong
- a concrete failure scenario (the input or state that leads to which wrong result)
- a suggested fix

Order the findings by severity, and say plainly when nothing blocking was found. Don't pad the report with praise or restate the diff.

## Independence

- Review in a fresh context where possible (the `reviewer` subagent): the author's context carries the author's blind spots.
- Reviewers report; they don't fix. The author (or the `implementer`) addresses findings, each with a test where the finding is a bug.

## Receiving review

Verify each finding before acting on it. If a finding is wrong, say why, with evidence. Don't implement suggestions you believe are incorrect just to close the thread.

---
name: review-code
description: Review a change (diff, branch, PR or files) for correctness, intent, tests, security and maintainability, and report findings by severity with concrete failure scenarios. Use when asked to review code or a PR, or before declaring a non-trivial change done.
---

# Review code

Standard: [review.md](../../standards/review.md), which covers check order, severity and report format. Checklist: [review-checklist.md](../../templates/review-checklist.md).

## Steps

1. **Establish the target:** the diff range (`git diff <base>...HEAD`, a PR number or a file list) and the **intent** (the issue, plan or request). Without an intent you can only review style, so ask for it.
2. **Read the whole diff once** for shape, then file by file. For each changed function, read its callers and tests, not just the changed lines.
3. **Check in the order the standard gives:** intent, correctness, tests, security, maintainability, docs.
4. **Verify suspicions** before reporting them. Trace the code path, or write and run a quick failing test if that's cheap. Drop findings you cannot support with a concrete failure scenario, or mark them as questions.
5. **Run the checks** if they haven't been shown: tests, lint and build. Report the results.

## Output

The findings, ordered by severity, each with:
- `path:line`
- the severity
- the problem
- the failure scenario
- a suggested fix

Then a one-line verdict: "No blocking issues" or "Blocking: N". If you are the author reviewing your own change, say so. A fresh-context review (the `reviewer` subagent) is stronger.

---
name: plan-task
description: Turn a request into a concrete implementation plan with scoped steps, files, acceptance checks, risks and open questions. Use before multi-file changes, when a request is ambiguous, or when the user asks for a plan or approach before coding.
---

# Plan a task

A plan records the decisions an implementer could not make alone: which files, which interfaces, which checks prove each step. Keep it proportional. A small task gets five lines; a cross-cutting change gets the full [template](../../templates/plan.md).

## Steps

1. **Restate the goal and the definition of done.** Done is an observable result plus the checks that prove it.
2. **Gather context.** Read the code involved and its tests, plus the relevant ADRs (`analyze-project` skill if the area is new to you).
3. **Surface ambiguity.** List the questions whose answers change what gets built. Ask the user those questions, and only those. For everything else, choose the conventional option and write it down as an assumption.
4. **Choose an approach.** If there are real alternatives, compare them in 2-3 lines each and recommend one. If the choice is hard to reverse, draft an ADR (`review-architecture` skill).
5. **Write the steps.** Each step:
   - is small enough to verify on its own
   - names its files
   - names the test or check that proves it
   - lists tests before implementation where behaviour changes
6. **Note risks and out-of-scope items.**
7. **For parallel work,** turn the steps into a task board (`coordinate-agents` skill) with owned paths and dependencies.

## Output

The plan, in chat for small tasks or in `docs/plans/<date>-<topic>.md` when the user wants it kept. End with the open questions, if any, and wait for answers or approval before implementing anything non-trivial.

---
name: write-tests
description: Design and write tests for new or existing behaviour - choose the right level, cover edge and error cases, and prove each test can fail. Use when asked to add tests, improve coverage, test a module, or when a change lacks tests.
---

# Write tests

Rules: [testing.md](../../standards/testing.md).

1. **Learn the project's testing setup:** framework, file layout, naming, fixtures and factories, how to run one test and the full suite (`AGENTS.md` Commands). Use it as is.
2. **List the behaviours** to cover, from the code's public interface and its callers:
   - the main path
   - boundaries
   - invalid input
   - each error path
   - state transitions
   - concurrency, if relevant
3. **Pick the level** for each behaviour: unit for logic, integration for boundaries, end-to-end only for critical user paths.
4. **Write one behaviour per test,** with a descriptive name and visible arrange, act and assert. Use real collaborators unless they are slow, external or non-deterministic.
5. **Prove the tests bite.** For tests of existing code, temporarily break the code under test (or invert an assertion) and confirm the test fails, then restore it. For new code, write the test first and watch it fail.
6. **Run the suite.** Check that the new tests are deterministic (run them twice) and that nothing else broke.

## Output

The tests added (file and test names), the behaviours covered, the gaps left on purpose and why, and the suite summary line.

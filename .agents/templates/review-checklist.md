# Review checklist

Order and severity rules: [standards/review.md](../standards/review.md).

- [ ] Matches the request or plan; nothing missing, nothing extra
- [ ] Logic, edge cases and error paths are correct
- [ ] New behaviour has tests that fail without the change; test output shown
- [ ] Security-relevant code checked (secrets, input, auth, permissions)
- [ ] Fits existing patterns; no duplication or dead code
- [ ] Docs, changelog and ADRs updated where behaviour or design changed
- [ ] Findings listed by severity with location, failure scenario and fix

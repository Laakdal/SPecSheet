---
name: assess-security
description: Assess a change or an area of the code for security weaknesses (secrets, injection, authn/authz gaps, unsafe deserialization, data exposure, risky dependencies, unsafe operations) and rate their impact. Use for changes touching auth, input handling, file or network access, secrets or permissions, or when asked for a security review.
---

# Assess security

Rules: [security.md](../../standards/security.md). This is a defensive review of the user's own code.

## Steps

1. **Map the attack surface** of the target: entry points (HTTP handlers, CLI args, file and queue consumers, webhooks), trust boundaries, data stores, external calls, and privileges held.
2. **Trace untrusted data** from each entry point to each sink: queries, shell or exec calls, file paths, templates and HTML, deserializers, redirects, logs. Note where it is validated or escaped, and where it isn't.
3. **Check the controls:**
   - authentication on every protected route
   - authorization per object and action
   - session and token handling
   - CSRF and CORS for web
   - rate limits on sensitive operations
4. **Check secrets and data:** hard-coded credentials, secrets in logs or errors, personal data exposure, insecure defaults.
5. **Check dependencies:** run the ecosystem's audit tool if available (`npm audit`, `pip-audit`, `cargo audit`, `composer audit`), and note known-vulnerable or unmaintained packages.
6. **Rate each finding.** Impact (what an attacker gains) × likelihood (preconditions). Use Critical, High, Medium or Low.

## Output

The findings, ordered by severity, each with:
- the location
- the weakness (a CWE id if obvious)
- a realistic exploit scenario in one or two sentences, without a working exploit
- the fix

Then "Not checked" (areas out of scope, or tools unavailable) and an overall risk statement. Report secrets found in history to the user privately and recommend rotating them.

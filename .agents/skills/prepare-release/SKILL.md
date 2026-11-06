---
name: prepare-release
description: Prepare a release - determine the version bump from the changes, update the changelog and version files, run all checks, and produce release notes and a tag or publish plan for approval. Use only when the user asks to prepare or cut a release.
disable-model-invocation: true
metadata:
  kind: workflow
---

# Prepare a release

Publishing, tagging and deploying are outward-facing: prepare everything, then **stop for approval** before any of them.

1. **Find the release process.** Look in `docs/runbook.md`, CONTRIBUTING, CI release workflows and package metadata. Follow the existing process. This skill fills gaps; it doesn't replace that process.
2. **Collect the changes** since the last release tag (`git log <last-tag>..HEAD`, merged PRs). Classify each as breaking, feature, fix or internal.
3. **Pick the version** per the project's scheme (SemVer by default: breaking → major, feature → minor, fix → patch). Flag anything ambiguous to the user.
4. **Update** the changelog (the project's format; user-facing language; link the PRs) and the version in every place it lives (manifests, constants, docs).
5. **Verify on a clean state:** full tests, lint, build or package, and the docs build if any. Run the publish command as a dry run if the tool supports one. Quote the results.
6. **Check:** no secrets or debug flags, migrations reviewed, dependency audit clean or accepted, upgrade notes written for breaking changes.
7. **Present for approval:**
   - the version
   - the release notes
   - the files changed
   - the exact commands to commit, tag (`vX.Y.Z`), push and publish
   - the rollback plan

   Run them only after the user says so.

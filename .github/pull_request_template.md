<!-- One piece of work per pull request. Delete the lines that do not apply. -->

## What changed

<!-- What this does and why, in a few sentences. Link the TASKS.md item or the issue ("Closes #12"). -->

## How it was checked

- [ ] `cargo xtask check` passes
- [ ] `ETS2_DIR=<game dir> cargo test -p specsheet-session --release --test real_install` passes (needed when a library crate changed; CI cannot run it)
- [ ] Seen working in the app, with a `SPECSHEET_SCRIPT` screenshot attached (needed for UI and rendering changes)

<!-- Paste the real results: what ran, what passed, what was not run and why. -->

## Facts about the game

<!-- New facts this relies on, each with its label (Confirmed / Hypothesis / Unknown) and its evidence file. "None" if it relies on nothing new. -->

## Docs and records

- [ ] `TASKS.md` updated
- [ ] `CHANGELOG.md` (Unreleased) updated, for user-visible or format-level changes
- [ ] The doc for this area updated (`docs/`), and `docs/DECISIONS.md` for a new crate, module or dependency

## Not included

<!-- What was left out or is still uncertain. -->

<!-- Reminder: no game files, no extracted assets, no shader text (tools/check_public_tree.py). -->

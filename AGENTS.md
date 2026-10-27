# AGENTS.md

A standalone ETS2 truck-tuning app, all in Rust: a workspace of library crates plus a Bevy + egui app (`app/specsheet`) that assembles trucks from the user's own game files.

**Absolute rule.** Load every model, texture and definition from the user's ETS2 install at runtime. Open game files read-only; the install directory and its `.scs` archives are never written. Never recreate, substitute or bundle game assets.

## Start every session here

1. Read `TASKS.md` and take the top item under "Next" (or finish "In progress").
2. Read the doc the routing list below names for that area.
3. Work autonomously. Ask the user only for decisions that are theirs: scope, licensing, anything touching their install.

## Layering

`scs-archive` → `scs-formats` → `tuning-core` → `game-data` → `app/specsheet`. Each layer uses only the layers to its left.
- `tuning-core` stays pure: domain types and rules, no I/O, no file formats.
- `scs-formats` parses bytes, never archives or paths.
- The UI renders what the backend decides. Compatibility verdicts come from `tuning-core`; labels, markers and reasons from `game-data::view`.
- `app/specsheet` renders and handles input. Orchestration lives in `game-data::session`.
- Keep the library crates free of UI and renderer code, so a later C++ front end could call them through a C interface (D11).

## Code rules

- Treat game files as hostile input: bounds-check every offset and count, return an error, never panic. Use `unwrap`/`expect` only in tests.
- Derive trucks, slots, accessories and hotspot positions from game data. Lists, coordinates and IDs never appear as literals.
- Build test fixtures in code. The pre-commit guard (`tools/check_public_tree.py`) rejects game files and game-format fixtures; never commit extracted assets.
- Before relying on a new fact about ETS2, record it with evidence and a label (Confirmed / Hypothesis / Unknown) in the reverse-engineering or format doc. Community code (sk-zk TruckLib, etc.) is a cross-check, never evidence.
- Fix bugs test-first: a failing regression test, then the fix.
- Refactors replace code. Delete the old path in the same change.

## Done means

A change is done when all of these hold:
1. `cargo test --workspace` passes.
2. `ETS2_DIR=<game dir> cargo test -p game-data --release` passes. Without `ETS2_DIR` these tests return early and prove nothing.
3. `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
4. UI and rendering changes are seen working in the running app, not just compiled: a `SPECSHEET_SCRIPT` run that ends in a screenshot you have looked at (see `docs/DEVELOPMENT.md`).
5. `TASKS.md` is updated, plus `CHANGELOG.md` (Unreleased) for user-visible or format-level changes.

## Crates own

| Crate | Owns | Never |
|---|---|---|
| `scs-archive` | `.scs` indexing, decompression, packed textures, layered VFS | parses file contents |
| `scs-formats` | byte parsers: SII, PMD, PMG, MAT, tokens | touches archives or paths |
| `tuning-core` | catalog, rules, configuration state | does I/O or reads formats |
| `game-data` | install discovery, catalog loading, assembly, scenes, render data, `session` orchestration, `view` display data | knows about UI or GPU APIs |
| `specsheet` (`app/specsheet`) | Bevy renderer, egui UI, background jobs, settings, dev script | decides compatibility or parses game files |

Crate-level docs live in each `lib.rs` (`//!`). Crates have no README.

## Read only when needed

- When touching archive reading, parsers or texture/model conversion, read `docs/ASSET_FORMATS.md` and the `docs/formats/*` page it links for that format.
- When judging whether a feature matches the game, read `docs/PARITY.md`.
- When relying on how the game behaves (mount order, locators, presets, UI), read `docs/REVERSE_ENGINEERING.md`.
- When changing compatibility, defaults or configuration logic, read `docs/TUNING_SYSTEM.md`.
- When changing the viewer, materials, scenes, hotspots or the app UI, read `docs/RENDERING.md` and the `docs/app/*` page it links.
- When adding a crate, module or dependency, read `docs/ARCHITECTURE.md` and record the choice in `docs/DECISIONS.md`.
- When building, running the app or using the research tools, read `docs/DEVELOPMENT.md`.
- When planning beyond the current task, read `docs/ROADMAP.md`.

Keep `docs/` as the single source of truth. Link to a doc rather than copying its content here. When a doc is added or moved, update this list and `docs/README.md`. Split a doc only when it mixes topics or passes about 200 lines.

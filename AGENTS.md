# AGENTS.md

A standalone ETS2 truck-tuning app, all in Rust: a workspace of library crates plus a Bevy + egui app (`crates/specsheet`) that assembles trucks from the user's own game files.

**Absolute rule.** Load every model, texture and definition from the user's ETS2 install at runtime. Open game files read-only; the install directory and its `.scs` archives are never written. Never recreate, substitute or bundle game assets.

## Start every session here

1. Read `TASKS.md` and take the top item under "Next" (or finish "In progress").
2. Read the doc the routing list below names for that area.
3. Work autonomously. Ask the user only for decisions that are theirs: scope, licensing, anything touching their install.

## Layering

`specsheet-archive` → `specsheet-formats` → `specsheet-core` → `specsheet-data` → `crates/specsheet`. Each layer uses only the layers to its left.
- `specsheet-core` stays pure: domain types and rules, no I/O, no file formats.
- `specsheet-formats` parses bytes, never archives or paths.
- The UI renders what the backend decides. Compatibility verdicts come from `specsheet-core`; labels, markers and reasons from `specsheet-data::view`.
- `crates/specsheet` renders and handles input. Orchestration lives in `specsheet-data::session`.
- Keep the library crates free of UI and renderer code, so a later C++ front end could call them through a C interface (D11).

## Code rules

- Treat game files as hostile input: bounds-check every offset and count, return an error, never panic. Use `unwrap`/`expect` only in tests.
- Derive trucks, slots, accessories and hotspot positions from game data. Lists, coordinates and IDs never appear as literals.
- Build test fixtures in code. The pre-commit guard (`tools/check_public_tree.py`) rejects game files and game-format fixtures; never commit extracted assets.
- Before relying on a new fact about ETS2, record it with evidence and a label (Confirmed / Hypothesis / Unknown) in the reverse-engineering or format doc. Community code (sk-zk TruckLib, etc.) is a cross-check, never evidence.
- Fix bugs test-first: a failing regression test, then the fix.
- Refactors replace code. Delete the old path in the same change.
- Split a module when it grows distinct responsibilities, not by line count. A split is a pure move in its own commit.

## Done means

A change is done when all of these hold:
1. Tests pass for the crates you changed and the crates that depend on them; nothing else needs to build:

   | Changed | Run `cargo test` / `cargo clippy --all-targets -- -D warnings` with |
   |---|---|
   | `specsheet-archive`, `specsheet-formats` | `-p <crate> -p specsheet-data` |
   | `specsheet-core` | `-p specsheet-core -p specsheet-data` |
   | `specsheet-data`, private items only | `-p specsheet-data` |
   | `specsheet-data` public API, `specsheet-core` public API, or `crates/specsheet` | `--workspace` (builds Bevy) |
   | Before a commit that touches several of the above | `--workspace` |

2. `ETS2_DIR=<game dir> cargo test -p specsheet-data --release --test real_install` passes whenever a change touches `specsheet-archive`, `specsheet-formats`, `specsheet-core` or `specsheet-data`. Without `ETS2_DIR` these tests return early and prove nothing.
3. `cargo fmt --all --check` is clean, and clippy is clean for the crates from step 1.
4. UI and rendering changes are seen working in the running app, not just compiled: a `SPECSHEET_SCRIPT` run that ends in a screenshot you have looked at (see `docs/DEVELOPMENT.md`).
5. `TASKS.md` is updated, plus `CHANGELOG.md` (Unreleased) for user-visible or format-level changes.

## Commits

- Commit only when the user asks. Push only with the user's explicit approval.
- Split work into one commit per group:
  - code per crate, with its tests;
  - docs;
  - research evidence;
  - `TASKS.md` and `CHANGELOG.md`.

  A change that must build together stays one commit, and its docs follow in the next commit. Example: a `specsheet-data` feature becomes "code + test", "docs", "tasks/changelog".
- Messages are a short plain-English subject (no `feat:`-style prefixes). Commits are authored as Laakdal and signed by the local git config; never pass `--no-gpg-sign`. Add no co-author or tool-attribution trailers.
- The pre-commit guard must pass; never bypass it with `--no-verify`.

## Crates own

| Crate | Owns | Never |
|---|---|---|
| `specsheet-archive` | `.scs` indexing, decompression, packed textures, layered VFS | parses file contents |
| `specsheet-formats` | byte parsers: SII, PMD, PMG, MAT, tokens | touches archives or paths |
| `specsheet-core` | catalog, rules, configuration state | does I/O or reads formats |
| `specsheet-data` | install discovery, catalog loading, assembly, scenes, render data, `session` orchestration, `view` display data | knows about UI or GPU APIs |
| `specsheet` (`crates/specsheet`) | Bevy renderer, egui UI, background jobs, settings, dev script | decides compatibility or parses game files |

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

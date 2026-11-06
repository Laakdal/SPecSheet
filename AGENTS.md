# AGENTS.md

<!-- hub:core:begin v0.2.0; managed by hub, edit outside this block -->
## Working agreement (hub)

These rules apply to every task here. The project sections below this block, nested `AGENTS.md` files and `.agents/rules/` refine or override them. An explicit request from the user overrides all of them. When you follow a more specific rule over a general one, say so.

- **Understand first.** Read the relevant code and docs before changing anything, and match the patterns already in use.
- **Unclear requirements:** if a choice changes the outcome and the code or docs can't settle it, ask. Otherwise take the conventional option and state the assumption in your reply.
- **Stay in scope.** Do what was asked. Mention other problems you notice instead of fixing them unasked.
- **Prove it works.** Run the project's test, lint and build commands (check: `cargo xtask check`; check with the game: `ETS2_DIR=<game dir> cargo xtask check --real`; format: `cargo fmt --all --check`; lint: `cargo clippy --workspace --all-targets -- -D warnings`) and report the real result. If you could not verify something, say so.
- **Ask before anything irreversible or outward-facing:** deleting data, rewriting git history, force-pushing, publishing, deploying, or changing secrets or production config.
- **Never** commit secrets, or weaken a test or security check just to get a green result.
- **Report honestly:** what changed, what you skipped, and what is still uncertain.
- **Resuming, or after context compaction:** read `.agents/state/handoff.md` and `.agents/state/tasks.md` if they exist.

Load these when a task needs them, not up front:
- `.agents/standards/`: engineering, testing, review, security, documentation, git, collaboration (multi-agent), context
- `.agents/skills/`: workflows such as develop-feature, fix-bug, refactor and review-code (the full list is in `.agents/README.md`)
- `.agents/templates/`: ADR, plan, handoff, task board, architecture, API and runbook docs
- `docs/ARCHITECTURE.md` (the architecture doc) and `docs/DECISIONS.md` (the decision record): this project's design and its recorded decisions
<!-- hub:core:end -->

A standalone ETS2 truck-tuning app, all in Rust: a workspace of library crates plus a Bevy + egui app (`crates/specsheet`) that assembles trucks from the user's own game files.

**Absolute rule.** Load every model, texture and definition from the user's ETS2 install at runtime. Open game files read-only; the install directory and its `.scs` archives are never written. Never recreate, substitute or bundle game assets.

## Start every session here

1. Read `TASKS.md` and take the top item under "Next" (or finish "In progress").
2. Read the doc the routing list below names for that area.
3. Work autonomously. Ask the user only for decisions that are theirs: scope, licensing, anything touching their install.

## Layering

Crates sit in layers, and a crate uses only crates in lower layers (D17). `cargo xtask layers` checks it, also in CI:

| Layer | Crates |
|---|---|
| 0 | `specsheet-core` (ids, maths, colour), `specsheet-archive`, `specsheet-theme` |
| 1 | `specsheet-formats`, `specsheet-truck` |
| 2 | `specsheet-data` |
| 3 | `specsheet-assembly`, `specsheet-gameui` |
| 4 | `specsheet-session` |
| 5 | `specsheet-render` (Bevy) |
| 8 | `specsheet-research` (tools) |
| 9 | `crates/specsheet` (the app, Bevy) |

Only `specsheet-render` and the app use Bevy.

- `specsheet-truck` stays pure: domain types and rules, no I/O, no file formats.
- `specsheet-formats` parses bytes, never archives or paths.
- The UI renders what the backend decides. Compatibility verdicts come from `specsheet-truck`; labels, markers and reasons from `specsheet-session::view`.
- `crates/specsheet` renders and handles input. Orchestration lives in `specsheet-session`.
- Keep the library crates free of UI and renderer code, so a later C++ front end could call them through a C interface (D11).

## Code rules

- Treat game files as hostile input: bounds-check every offset and count, return an error, never panic. Use `unwrap`/`expect` only in tests.
- Derive trucks, slots, accessories and hotspot positions from game data. Lists, coordinates and IDs never appear as literals.
- Build test fixtures in code. The pre-commit guard (`tools/check_public_tree.py`) rejects game files and game-format fixtures; never commit extracted assets.
- Before relying on a new fact about ETS2, record it with evidence and a label (Confirmed / Hypothesis / Unknown) in the reverse-engineering or format doc. Community code (sk-zk TruckLib, etc.) is a cross-check, never evidence.
- TruckLib and Extractor (sk-zk) are GPL-2.0 and this project is MIT. Take only facts from them (offsets, magic numbers, layouts), and write the code from our own byte observations. Never copy, port or translate their code.
- The game's shaders (`effect.scs` GLSL/SPIR-V) are SCS Software's code with no license. Never copy, port or translate them. Materials are clean-room (D13): one agent reads the shaders and writes the maths in prose and equations to `docs/app/shader-specs/<effect>.md`; a different agent that has never seen the shaders writes the WGSL from that spec. Never commit or quote extracted shader text.
- Fix bugs test-first: a failing regression test, then the fix.
- Refactors replace code. Delete the old path in the same change.
- Split a module when it grows distinct responsibilities, not by line count. A split is a pure move in its own commit.

## Done means

A change is done when all of these hold:
1. Tests pass for the crates you changed and the crates that depend on them; nothing else needs to build:

   | Changed | Run `cargo test` / `cargo clippy --all-targets -- -D warnings` with |
   |---|---|
   | Any crate, private items only | `-p <crate>` |
   | A library crate's public API | `-p <crate>` and every crate above it up to `specsheet-session` |
   | `crates/specsheet`, or a public API the app uses | `--workspace` (builds Bevy) |
   | Before a commit that touches several of the above | `--workspace` |

   `cargo xtask check` runs the whole list (guard, fmt, clippy, tests, layers); `cargo xtask check --real` adds step 2.

2. `ETS2_DIR=<game dir> cargo test -p specsheet-session --release --test real_install` passes whenever a change touches a library crate. Without `ETS2_DIR` these tests return early and prove nothing.
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

## Where the hub rules differ

The hub block at the top sets general rules. Where they differ, the rules in this file win:
- **Branches:** commit straight to `main` (see Commits); hub's "work on a branch" rule does not apply.
- **Tasks:** `TASKS.md` is the backlog. `.agents/state/tasks.md` is only for a short-lived multi-agent board.
- **Decisions:** a new D*n* entry in `docs/DECISIONS.md`, never a separate ADR file.
- **Checks:** "Done means" decides which crates to test; the hub block lists the full commands.
- **Shaders:** port game materials with the `clean-room-shader` skill (D13).

## Crates own

| Crate | Owns | Never |
|---|---|---|
| `specsheet-core` | ids (`TruckId`, `SlotId`, `AccessoryId`), maths (`Aabb`, `Transform`), the sRGB conversion | domain rules, formats or I/O |
| `specsheet-theme` | design tokens for the app's own screens (D16) | styles the game's tuning screen |
| `specsheet-archive` | `.scs` and mod-package indexing, decompression, packed textures, layered VFS; building new ZIP packages | parses file contents |
| `specsheet-formats` | byte parsers: SII, PMD, PMG, MAT, tokens; the `manifest.sii` writer | touches archives or paths |
| `specsheet-truck` | catalog, rules, configuration state | does I/O or reads formats |
| `specsheet-data` | install discovery, mounting, catalog loading, models, scenes, render data, paint, localisation | assembles trucks or knows about UI or GPU APIs |
| `specsheet-assembly` | the built truck: placements, wheels, interior, cockpit, hookups, the game's exterior and interior pins | loads catalogs or keeps session state |
| `specsheet-gameui` | the game's UI files: screens, templates, colours, UI materials, the markup layout | knows about trucks |
| `specsheet-session` | `GameSession`/`TruckSession` orchestration, `view` display data | draws anything |
| `specsheet-research` | research tools (`examples/`) that produce `research/evidence/` | is used by other crates |
| `specsheet-render` | game materials (clean-room WGSL), the game's lighting, checked mesh and texture upload | decides compatibility or parses game files |
| `specsheet` (`crates/specsheet`) | app wiring: viewer, egui UI, background jobs, settings, dev script | decides compatibility or parses game files |

Crate-level docs live in each `lib.rs` (`//!`). Crates have no README.

## Read only when needed

- When looking for the code or tests behind a feature, read `docs/CODE_MAP.md`.
- When touching archive reading, parsers or texture/model conversion, read `docs/ASSET_FORMATS.md` and the `docs/formats/*` page it links for that format.
- When judging whether a feature matches the game, read `docs/PARITY.md`.
- When relying on how the game behaves (mount order, locators, presets, UI), read `docs/REVERSE_ENGINEERING.md`.
- When researching the game (data surveys, REA on the exe, writing evidence), read `docs/research-lab.md`.
- When ETS2 updates or `installed_game_is_the_tested_version` fails, read `docs/version-ports.md`.
- When changing compatibility, defaults or configuration logic, read `docs/TUNING_SYSTEM.md`.
- When changing the viewer, materials, scenes, hotspots or the app UI, read `docs/RENDERING.md` and the `docs/app/*` page it links.
- When adding a crate, module or dependency, read `docs/ARCHITECTURE.md` and record the choice in `docs/DECISIONS.md`.
- When building, running the app or using the research tools, read `docs/DEVELOPMENT.md`.
- When planning beyond the current task, read `docs/ROADMAP.md`.

Keep `docs/` as the single source of truth. Link to a doc rather than copying its content here. When a doc is added or moved, update this list and `docs/README.md`. Split a doc only when it mixes topics or passes about 200 lines.

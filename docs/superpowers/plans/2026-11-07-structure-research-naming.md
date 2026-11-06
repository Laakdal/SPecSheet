# Structure, research register and naming: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the workspace structure self-checking, make research output structured and checkable, and make names consistent, with the product called **SPecSheet**.

**Architecture:** `xtask` becomes the single checker. It reads each crate's layer from its own `Cargo.toml`, matches it against the layering table in `AGENTS.md`, and enforces the checkable "Never" rules. It also validates a findings register (`research/findings.toml`) against `docs/REVERSE_ENGINEERING.md`, the evidence files and the test names. Evidence moves into area folders with a fixed `# key: value` header. The mod-package code leaves `specsheet-data` for a new `specsheet-extract` crate, and `specsheet-render` stops depending on `specsheet-session`.

**Tech Stack:** Rust 1.95 workspace, Bevy 0.19.1 (app and render only), `xtask` with no dependencies (std only, hand-rolled parsers as today).

**Spec:** the design agreed in conversation on 2026-11-07: research register, Rust structure (items 1-4), feature routing (items 1-4) and naming. Splitting `viewer.rs` and `render/assets.rs` is deliberately left out (see the end).

## Global Constraints

- Follow `AGENTS.md`: "Done means" for every task, test-first for fixes, and a split or move is a pure move in its own commit.
- Commit only when the user has approved committing. Subjects are short plain English with no `feat:` prefixes and no co-author or tool trailers. Never use `--no-verify` or `--no-gpg-sign`.
- One commit per group: code per crate with its tests; docs; research evidence; then `TASKS.md` and `CHANGELOG.md` (Unreleased, for user-visible or format-level changes).
- `xtask` stays std-only. New parsers follow the style of `Manifest::parse` in `xtask/src/main.rs`.
- No game content enters the repository. `python tools/check_public_tree.py --all` passes after every task.
- **Before starting:** the working tree is clean. The hub install (`.agents/`, `.claude/`, `docs/CODE_MAP.md`, the `AGENTS.md` and `.gitignore` edits) and the pending pins/wheel-order work are committed by their owners. Check `ListAgents` for peer sessions before editing root docs or `crates/`.
- Product name: **SPecSheet**, exactly. Historical entries in `CHANGELOG.md` and done items in `TASKS.md` are never rewritten. The identifier `dev.specsheet.app` and the crate prefix `specsheet-` stay.
- New decisions: D21 (`specsheet-extract`), D22 (research register and evidence layout), D23 (product name SPecSheet). D17 is amended, not replaced.

## Review Focus

1. **Case-only renames on Windows** (for example `foo.TXT` to `foo.txt`) must actually happen in git, and the checker must compare names exactly as listed by `read_dir`, never with `Path::exists`. Test: `xtask research` reports `docs/x.md` citing `research/evidence/Pins/survey.txt` when the folder on disk is `pins/` (Task 9).
2. **Relative links** such as `../../research/evidence/x.txt` from `docs/formats/` must be rewritten and checked from their own directory. Test: a fixture doc in a subfolder with a relative link resolves correctly, and a broken one is reported (Task 9).
3. **Rust string literals with evidence paths** (an example that writes to `research/evidence/...`) must be checked like docs. Test: a literal in `crates/specsheet-research/examples/*.rs` naming a missing evidence file is reported (Task 9).
4. **CRLF evidence files:** several working copies are CRLF. The header parser must accept `\r\n`. Test: `parses_a_header_with_crlf_line_endings` (Task 10).
5. **Legacy evidence without commands** must not be given invented commands. A header may say `command: unknown (before the header rule)`, and the checker prints a count of such files without failing. Test: `legacy_unknown_command_is_counted_not_failed` (Task 10).

---

## Part A: structure

### Task 1: Declare shared dependencies once

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]`)
- Modify: every `crates/*/Cargo.toml` that names a `specsheet-*` crate, `bevy` or `serde`

- [ ] **Step 1: Record the baseline.** Run `cargo tree --workspace -e normal --prefix none | sort -u > <scratch>/tree-before.txt`.
- [ ] **Step 2: Add to the root `[workspace.dependencies]`** a path entry for each library crate (`specsheet-core = { path = "crates/specsheet-core" }` and likewise for archive, theme, formats, truck, data, gameui, assembly, session, render), plus `bevy = { version = "0.19.1", default-features = false, features = ["3d"] }` and `serde = { version = "1", features = ["derive"] }`.
- [ ] **Step 3: Switch every member** to `name.workspace = true` for those entries. Leave `bevy_egui`, `dirs`, `image`, `serde_json` and `gdeflate` where they are, since each is used by only one crate.
- [ ] **Step 4: Verify.** `cargo tree ... | sort -u` is identical to the baseline. `git diff --exit-code Cargo.lock` succeeds. `cargo xtask check` passes.
- [ ] **Step 5: Commit** "Declare shared dependencies once in the workspace".

### Task 2: Layers come from each crate's manifest, and AGENTS.md must agree

**Files:**
- Move (pure, own commit): `xtask/src/main.rs` layer code to `xtask/src/layers.rs` (`layers`, `Manifest`, their tests)
- Modify: `xtask/src/layers.rs`, every `crates/*/Cargo.toml`, `xtask/Cargo.toml`
- Modify: `docs/DECISIONS.md` (amend D17: "each crate's layer is in its `Cargo.toml`")

**Interfaces:**
- Produces: `Manifest { name: Option<String>, dependencies: Vec<String>, layer: Option<u8> }`; `fn agents_layers(text: &str) -> BTreeMap<String, u8>`; `fn layer_problems(manifests: &[Manifest], agents: &BTreeMap<String, u8>) -> Vec<String>`. `layers(root)` calls these and `rules::check` (Task 3).

- [ ] **Step 1: Make the pure move** into `layers.rs`. `cargo test -p xtask` passes. Commit "Move the layer check into its own module".
- [ ] **Step 2: Write the failing tests** in `layers.rs`:
  - `reads_the_layer_from_package_metadata`: `Manifest::parse` of a manifest with `[package.metadata.specsheet]\nlayer = 2` gives `layer == Some(2)`.
  - `a_crate_without_a_layer_is_reported`: the problems contain `"specsheet-x: no [package.metadata.specsheet] layer"`.
  - `reads_the_agents_md_layer_table`: the rows ``| 0 | `specsheet-core` (ids), `specsheet-archive` |`` and ``| 9 | `crates/specsheet` (the app, Bevy) |`` give `{specsheet-core: 0, specsheet-archive: 0, specsheet: 9}`.
  - `a_layer_table_that_disagrees_with_the_manifests_is_reported`: the problems contain `"AGENTS.md says specsheet-data is layer 3; its Cargo.toml says 2"`. A crate on one side only is also reported.
- [ ] **Step 3: Run** `cargo test -p xtask`. Expected: these 4 tests fail.
- [ ] **Step 4: Implement.** Parse `layer` in the `package.metadata.specsheet` section. `agents_layers` reads the rows after the `| Layer | Crates |` header, takes every backticked token in the second cell, strips a leading `crates/`, and keeps tokens that start with `specsheet`. `xtask` is checked for layering but left out of the `AGENTS.md` comparison. Delete `LAYERS`, and keep `BEVY_CRATES`.
- [ ] **Step 5: Add `[package.metadata.specsheet] layer = N`** to each manifest with today's values: core 0, archive 0, theme 0, formats 1, truck 1, data 2, gameui 3, assembly 3, session 4, render 5, research 8, specsheet 9, xtask 0.
- [ ] **Step 6: Verify.** `cargo test -p xtask` passes. `cargo xtask layers` prints `xtask: layers ok`. Changing one layer in a scratch copy of `AGENTS.md` makes it fail.
- [ ] **Step 7: Commit** the code, then the D17 amendment as a separate docs commit.

### Task 3: Enforce the checkable "Never" rules

**Files:**
- Create: `xtask/src/rules.rs`
- Modify: `xtask/src/layers.rs` (call `rules::check`)

**Interfaces:**
- Produces: `struct Rule { krate: &'static str, forbidden: &'static str, why: &'static str }`; `const RULES: &[Rule]`; `fn violations(krate: &str, file: &str, text: &str) -> Vec<String>`; `fn check(root: &Path) -> Result<(), String>`. Task 5 adds one rule.

- [ ] **Step 1: Write the failing tests:**
  - `a_forbidden_import_is_reported_with_its_line`: `violations("specsheet-truck", "src/x.rs", "use std::fs;\n")` gives `["src/x.rs:1: specsheet-truck uses std::fs: domain types and rules only, no I/O"]`.
  - `comments_are_ignored`: a line whose trimmed start is `//` gives no violation.
  - `other_crates_may_use_it`: `violations("specsheet-data", ...)` with `std::fs` gives no violation.
- [ ] **Step 2: Run them.** Expected: they fail because `rules` is undefined.
- [ ] **Step 3: Implement the rules.** `check` scans `crates/<krate>/src/**/*.rs`, but not `tests/` or `examples/`.

  | Crate | Forbidden | Why |
  |---|---|---|
  | `specsheet-truck` | `std::fs`, `std::path`, `std::io`, `std::net` | domain types and rules only, no I/O |
  | `specsheet-formats` | `std::fs`, `std::path`, `specsheet_archive` | parses bytes, never archives or paths |
  | `specsheet-core` | `std::fs` | ids, maths and colour; no I/O |
  | `specsheet` | `specsheet_formats`, `specsheet_archive` | the app renders and handles input; it doesn't parse game files |

- [ ] **Step 4: Verify.** `cargo test -p xtask` passes, and `cargo xtask layers` passes on the current tree (it was clean when checked on 2026-11-07).
- [ ] **Step 5: Commit** "Check the crates' Never rules in xtask".

### Task 4: Move mod-package extraction into `specsheet-extract`

**Files:**
- Create: `crates/specsheet-extract/Cargo.toml` (deps `specsheet-archive`, `specsheet-formats`; `[package.metadata.specsheet] layer = 2`; `[lints] workspace = true`) and `crates/specsheet-extract/src/lib.rs` (`//!` crate doc; `pub mod extract; pub mod mod_names; pub mod mod_scan;`)
- Move (pure): `crates/specsheet-data/src/{extract,mod_names,mod_scan}.rs` to `crates/specsheet-extract/src/`
- Modify: `crates/specsheet-data/src/lib.rs` (remove the three `pub mod`), root `Cargo.toml` (workspace entry), `crates/specsheet-research/Cargo.toml`, and the imports in `examples/{extract_check,mod_extract,mod_names}.rs` (`specsheet_data::` becomes `specsheet_extract::`)
- Docs: `AGENTS.md` (layer 2 row; a new "Crates own" row: owns "extracting packages and game archives to a folder (D19), recovering HashFS v1 file names, the cached folder scan"; never "mounts, loads catalogs, or writes into the game, its home or the repo"), `docs/ARCHITECTURE.md`, `docs/CODE_MAP.md` (the two rows that name those files), `docs/DECISIONS.md` (D21)

**Interfaces:**
- Produces: `specsheet_extract::{extract, mod_names, mod_scan}`, with the same public items as before under the new crate path.

- [ ] **Step 1: Record the baseline.** `cargo test -p specsheet-data -- --list 2>/dev/null | grep -cE "^(extract|mod_names|mod_scan)::"` gives N.
- [ ] **Step 2: Do the move.** Module names are unchanged, so it is a pure move.
- [ ] **Step 3: Verify.**
  - `cargo test -p specsheet-extract -- --list | grep -c ": test"` equals N.
  - `cargo test -p specsheet-data` passes.
  - `cargo build -p specsheet-research --examples` passes.
  - `cargo xtask layers` passes; Task 2 makes the `AGENTS.md` row mandatory.
  - `ETS2_DIR=<game dir> cargo xtask check --real` passes.
- [ ] **Step 4: Commit** the code ("Move package extraction into specsheet-extract"), then the docs and D21.

### Task 5: Render takes game files, not the session

**Files:**
- Modify: `crates/specsheet-session/src/lib.rs` (add `impl AsRef<GameFs> for GameSession`)
- Modify: `crates/specsheet-render/src/{lib,assets,textures}.rs` and `crates/specsheet-render/Cargo.toml` (drop `specsheet-session`)
- Modify: `crates/specsheet/src/viewer.rs:557` and `:814`
- Modify: `xtask/src/rules.rs` (new rule)

**Interfaces:**
- Produces: in `specsheet-render`, `pub type GameFiles = Arc<dyn AsRef<GameFs> + Send + Sync>`; `MaterialContext.game: Option<&'a GameFiles>`; `GameTextures::request(&mut self, game: Option<&GameFiles>, ...)` and `set_vehicle_slot(&mut self, game: Option<&GameFiles>, ...)`; `assets::load_model(fs: &GameFs, models: &ModelCache, key: &ModelKey)`, with `ModelCache` from `specsheet_data::model`.
- Consumes: `GameSession::fs()` and `GameSession::models()` (both exist).

- [ ] **Step 1: Write the failing check.** Add the rule `("specsheet-render", "specsheet_session", "renders what it is given; decisions reach it through the app")` and the test `render_may_not_use_the_session`.
- [ ] **Step 2: Run `cargo xtask layers`.** Expected: it fails with 3 violations, at `assets.rs:23`, `lib.rs:18` and `textures.rs:21`.
- [ ] **Step 3: Implement the interfaces above.** The app builds `let files: GameFiles = session.clone();` from its `Arc<GameSession>` and passes it in. Its async model load calls `assets::load_model(game.fs(), game.models(), &key)`.
- [ ] **Step 4: Verify.**
  - `cargo xtask layers` passes.
  - `cargo test --workspace` and clippy pass.
  - A `SPECSHEET_SCRIPT` run ends in a screenshot that you look at: the truck renders with textures and the cube reflection, as before.
- [ ] **Step 5: Commit** session + render + app as one commit, since they must build together. Then commit the xtask rule.

### Task 6: Split `real_install.rs` into modules of one test binary

**Files:**
- Move: `crates/specsheet-session/tests/real_install.rs` to `crates/specsheet-session/tests/real_install/main.rs` (keeps `game_fs`, `collect`, the `use` lines and `mod` declarations), plus these modules:
  - `archives.rs`: `archives_mount_in_the_games_lookup_order`, `every_vehicle_definition_parses`, `every_truck_model_parses`
  - `presets.rs`: `every_preset_validates_and_assembles_without_warnings`, `every_preset_wheel_sits_in_its_tyre`, `configured_hookups_sit_on_their_host_model`, `every_chassis_has_one_wheel_pair_per_declared_axle`, `an_optional_accessory_can_be_removed_and_a_required_part_cannot`
  - `ui.rs`: `the_truck_configuration_screen_has_a_scene_orbit_and_pins`, `the_truck_configuration_screen_layouts_and_templates_load`, `every_ui_template_parses_and_the_tuning_screen_lays_out`, `every_accessory_icon_material_names_an_existing_texture`, `every_tuning_scene_has_a_model_and_a_camera`, `every_slot_with_a_choice_is_reachable_on_the_tuning_screen`
  - `interior.rs`: `every_interior_has_a_right_hand_drive_twin`, `choosing_an_interior_chooses_the_drive_side`, `every_preset_has_a_cockpit_at_the_drivers_eye`, `every_steering_wheel_tilts_about_x_alike_in_both_drives`, `every_truck_names_an_interior_camera_that_exists`, `an_interior_part_takes_the_cockpits_locator_even_when_the_cabin_has_one`
  - `paint.rs`: `paint_catalogs` plus `every_paint_job_parses`, `paint_job_parts_resolve_by_unit_token`, `every_paint_mask_exists_or_is_a_constant`, `every_desktop_preset_loads_its_paint_colours`, `a_cabin_change_keeps_the_paint_job_design`, `the_custom_colour_paint_job_offers_its_base_colour_and_takes_a_new_one`
  - `pins.rs`: `the_renault_t_roof_light_pins_follow_the_trucks_own_pin_table`, `every_wheel_pin_that_faces_sideways_faces_out_on_its_own_side`, `a_pin_takes_the_interiors_exterior_model_locator_before_the_cabins`
  - `lights.rs`: `every_vehicle_light_unit_loads_and_its_glow_model_exists`
- Modify: the sentence about `real_install::x` in `docs/CODE_MAP.md`, and `docs/DEVELOPMENT.md` wherever it names the file. The notation `real_install::<fn>` stays.

- [ ] **Step 1: Record the baseline.** `cargo test -p specsheet-session --release --test real_install -- --list | grep -c ": test"` gives T.
- [ ] **Step 2: Do the move.** Helpers are `pub(crate)` in `main.rs` and used as `crate::game_fs`. This is a pure move. Any test the plan doesn't list goes to the module whose subject it tests, and you note it in the commit body.
- [ ] **Step 3: Verify.** The same command lists T tests. `ETS2_DIR=<game dir> cargo test -p specsheet-session --release --test real_install` passes.
- [ ] **Step 4: Commit** "Split the real-install tests into modules".

### Task 7: The `add-feature` skill

**Files:**
- Create: `.agents/skills/add-feature/SKILL.md`, then run `python .agents/tools/hub sync`

- [ ] **Step 1: Write the skill.**
  - **Description:** "Place a new feature in the right crates before coding: split it into pieces, put each in the lowest layer that can own it, and run the new-crate checklist when no crate fits. Use before implementing any feature that spans crates or might need a new one."
  - **Body:**
    - The routing table:

      | The piece... | Goes in |
      |---|---|
      | reads bytes | `formats` (`archive` for containers) |
      | is a game rule or state, with no I/O | `truck` |
      | loads from the install | `data` |
      | extracts or names package files | `extract` |
      | builds the truck | `assembly` |
      | is a decision or label shown to the user | `session` |
      | is on the GPU | `render` |
      | is input or drawing | the app |

    - **Per-feature checklist:** a test in each layer touched; a `docs/CODE_MAP.md` row; a register entry and evidence for any new game fact (Task 9).
    - **New-crate checklist:** `[package.metadata.specsheet] layer`; the layering and "Crates own" tables in `AGENTS.md`; `docs/ARCHITECTURE.md`; a D-entry; a `docs/CODE_MAP.md` row. `cargo xtask layers` catches the first two.
    - Link to "Done means" in `AGENTS.md`; don't restate it.
- [ ] **Step 2: Verify.** `python .agents/tools/hub validate` reports 0 errors and 0 warnings, and `.claude/skills/add-feature/SKILL.md` exists.
- [ ] **Step 3: Commit** "Add the add-feature skill".

## Part B: research

### Task 8: Evidence header and CSV helpers for research tools

**Files:**
- Create: `crates/specsheet-research/src/evidence.rs`; modify `src/lib.rs` (`pub mod evidence;`, and update the `//!` text that says the library is empty)
- Modify (pilots): `crates/specsheet-research/examples/{pin_survey,flare_survey}.rs`

**Interfaces:**
- Produces: `pub struct Header<'a> { pub title: &'a str, pub game: &'a str, pub exe_sha256: Option<&'a str>, pub tool: &'a str, pub commands: &'a [&'a str], pub clean_room: bool }`; `impl Header<'_> { pub fn render(&self) -> String }`; `pub fn csv_line(fields: &[&str]) -> String`. Task 10's parser reads exactly what `render` writes.

- [ ] **Step 1: Write the failing tests:**
  - `renders_the_header_lines_in_order`: the output is `# title: T\n# game: 1.60.1.7\n# tool: pin_survey\n# command: cmd a\n# command: cmd b\n# clean-room: no\n\n`, with `exe-sha256` written after `game` only when it is `Some`.
  - `csv_quotes_commas_quotes_and_newlines`: `csv_line(&["a", "b,c", "d\"e", "f\ng"])` gives `a,"b,c","d""e","f\ng"\n`.
- [ ] **Step 2: Run** `cargo test -p specsheet-research --lib`. Expected: both fail.
- [ ] **Step 3: Implement** `render` and `csv_line`.
- [ ] **Step 4: Convert the pilots.** `pin_survey` and `flare_survey` print `Header::render` first. With `--csv <file>`, each also writes its existing main table as CSV, using the columns it already prints.
- [ ] **Step 5: Verify.** The tests pass, and `cargo build -p specsheet-research --examples` succeeds.
- [ ] **Step 6: Commit** "Add evidence header and CSV helpers to the research tools".

### Task 9: Findings register and `cargo xtask research`

**Files:**
- Create: `research/findings.toml`; `xtask/src/research.rs`
- Modify: `xtask/src/main.rs` (subcommand `research`, also run by `check`); `docs/research-lab.md` (how to add a finding); `docs/DECISIONS.md` (D22)
- Modify: `.claude/agents/ets2-researcher.md`, adding one step: "add or update the finding's entry in `research/findings.toml`".

**Register format**, a TOML subset that xtask parses: `[[finding]]` headers; `key = "string"`; `key = ["a", "b"]` on one line; `#` comments.
```toml
[[finding]]
id = "F13"
title = "Each chassis has one wheel-locator pair per declared axle, and the locator decides the tyre type"
label = "Confirmed"            # verbatim from the heading's parentheses
status = "confirmed"           # confirmed | hypothesis | unknown | mixed
game = "1.60.1.7"
evidence = ["research/evidence/axle-survey.txt"]
tests = ["every_chassis_has_one_wheel_pair_per_declared_axle"]
open = []                      # each Unknown, with the next step that would settle it
```

**Interfaces:**
- Produces: `struct Finding { id: String, title: String, label: String, status: String, game: String, evidence: Vec<String>, tests: Vec<String>, open: Vec<String> }`; `fn parse_register(text: &str) -> Result<Vec<Finding>, String>`; `fn headings(text: &str) -> Vec<(String, String, String)>` (id, title, label from `### F<n>: <title> (<label>)`); `fn cited_ids(text: &str, prefix: char) -> BTreeSet<u32>`; `fn evidence_refs(file: &Path, text: &str) -> Vec<String>` (repo-relative, with `../` resolved from the file's own directory); `fn exists_exactly(root: &Path, rel: &str) -> bool` (each path component must match a `read_dir` entry byte for byte); `fn check(root: &Path) -> Result<(), String>`.

- [ ] **Step 1: Write the failing tests:**
  - `parses_findings_with_arrays_and_comments`
  - `a_heading_without_a_register_entry_is_reported`, and the reverse case
  - `a_label_that_differs_from_the_heading_is_reported`
  - `an_unknown_status_is_reported`
  - `a_cited_finding_or_decision_that_does_not_exist_is_reported`: `F99` in a doc, or `D99` absent from `DECISIONS.md`
  - `a_missing_test_name_is_reported`: no `fn <name>(` under `crates/`
  - `relative_evidence_links_resolve_from_their_own_folder`: Review Focus 2
  - `a_case_mismatch_is_a_missing_file`: Review Focus 1
  - `a_rust_literal_naming_missing_evidence_is_reported`: Review Focus 3
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: all of them fail.
- [ ] **Step 3: Implement `research::check`.** It scans `crates/**/*.rs`, `docs/**/*.md`, `AGENTS.md`, `CLAUDE.md`, `README.md` and `TASKS.md` for citations and evidence references. `CHANGELOG.md` is excluded, since it is history.
- [ ] **Step 4: Generate `research/findings.toml`** with a scratch script, from the 19 headings in `docs/REVERSE_ENGINEERING.md`:
  - `id`, `title` and `label` come from each heading;
  - `evidence` is every `research/evidence/...` path in that section;
  - `tests` is every `real_install::<fn>` name in it;
  - `status` is set from the label: `confirmed` when it starts with "Confirmed" and names no Hypothesis or Unknown part, `hypothesis` when it starts with "Hypothesis", otherwise `mixed`;
  - `open` holds the section's bullets that begin with `**Unknown:**`;
  - `game = "1.60.1.7"`.

  Show the file to the user before committing.
- [ ] **Step 5: Verify.** `cargo test -p xtask` passes, and `cargo xtask research` prints `xtask: research ok (19 findings)`.
- [ ] **Step 6: Commit** the xtask code, then the register (research evidence group), then the docs and D22.

### Task 10: Evidence headers, area folders and reference rewrite

**Files:**
- Modify: every file in `research/evidence/`, moved with `git mv`
- Modify: every file that cites them; there were 230 references on 2026-11-07, in `docs/`, `crates/`, `xtask/` and `research/findings.toml`
- Modify: `xtask/src/research.rs` (header check)

**Areas:**

| Folder | Gets |
|---|---|
| `archive/` | HashFS, CityHash, texture unpacking, clean-room phases |
| `formats/` | SII, BSII, UI markup |
| `models/` | PMD/PMG, geometry, materials, UV, vertex streams |
| `wheels/` | axle, radius, tyre shift, wheel order, wheel fit |
| `pins/` | pin survey, table, overlaps, attributes, roots, select |
| `paint/` | paint jobs, classes, masks |
| `shaders/` | shader specs, sources, effect surveys, tonemap and exposure |
| `scenes/` | scenes, cameras, interiors, flares, reflection cubes |
| `mods/` | loader, priority, name recovery, writer, extraction |
| `exe/` | REA outputs not covered by an area above |

A file goes to the area of its subject, not its tool. The name becomes `<topic>.<ext>`, with method, tool and batch words dropped: `-survey`, `-derivation`, `rea-`, `-static`, `batch2`. `interior-locators.md` becomes `.txt`.

**Header**, written by Task 8's `render`, prepended above the unchanged old text:

| Key | Value |
|---|---|
| `title` | the old first line, without a leading `# ` |
| `game` | `1.60.1.7` when the file names it, otherwise `unknown` |
| `exe-sha256` | when the file names one |
| `tool` | the example named in the file, else `REA` for `rea-*`, else `manual` |
| `command` | `see below` when the first 15 lines mention a command, else `unknown (before the header rule)` |
| `clean-room` | `yes` for clean-room and shader-spec files, else `no` |

Every `.json` and `.csv` file gets a `.txt` sibling with the same stem and the header.

- [ ] **Step 1: Write the failing tests** in `research.rs`:
  - `parses_a_header_with_crlf_line_endings`
  - `a_header_missing_a_required_key_is_reported` (required: `title`, `game`, `tool`, `command`, `clean-room`)
  - `legacy_unknown_command_is_counted_not_failed`
  - `a_json_without_a_txt_sibling_is_reported`
  - `evidence_outside_an_area_folder_is_reported`
- [ ] **Step 2: Run them.** Expected: they fail.
- [ ] **Step 3: Implement the header check.**
- [ ] **Step 4: Write `<scratch>/moves.tsv`** (old, then new), following the table above. **Show it to the user and wait for approval.**
- [ ] **Step 5: Apply with a scratch script:** `git mv` each file; prepend headers; rewrite every reference in place, keeping relative prefixes such as `../../`.
- [ ] **Step 6: Verify.**
  - `cargo xtask research` passes and prints the count of legacy `unknown` commands.
  - `cargo build -p specsheet-research --examples` passes.
  - `python tools/check_public_tree.py --all` passes.
  - `git status` shows renames, not deletions plus additions.
- [ ] **Step 7: Commit** the xtask check (code), then "Move research evidence into area folders with headers" (evidence and references together, since the links must change with the files).

## Part C: naming

### Task 11: The product is SPecSheet

**Files:**
- Modify: `crates/specsheet/src/main.rs` (add `pub const APP_NAME: &str = "SPecSheet";` and use it for the window title at line 54); `crates/specsheet/src/ui/mod.rs:329` (`egui::Window::new(APP_NAME)`), plus the doc comments at lines 3 and 320
- Modify: `crates/specsheet/Cargo.toml:4` (description), `README.md:1` and `:6`, `docs/app/ui.md`, `docs/formats/paint.md`, `docs/app/shader-specs/truckpaint.md:137`, `docs/CODE_MAP.md`
- Modify: `docs/DECISIONS.md` (D23: the name is SPecSheet, matching the GitHub repository; it reverses the earlier "SpecSheet" rename recorded in CHANGELOG); `CHANGELOG.md` Unreleased

- [ ] **Step 1: Write the failing test** `the_app_is_called_specsheet_with_a_capital_p` in `main.rs`: `assert_eq!(APP_NAME, "SPecSheet")`.
- [ ] **Step 2: Run** `cargo test -p specsheet`. Expected: it fails because `APP_NAME` is undefined.
- [ ] **Step 3: Implement**, and replace every current-facing "SpecSheet" in the files above.
- [ ] **Step 4: Verify.**
  - `grep -rn "SpecSheet" --include=*.rs --include=*.md --include=*.toml . | grep -v "^./CHANGELOG.md\|^./TASKS.md\|^./target"` prints nothing.
  - The tests pass.
  - A `SPECSHEET_SCRIPT` screenshot shows "SPecSheet" as the window title and as the panel heading.
- [ ] **Step 5: Commit** the app code, then the docs, then D23 and the changelog.

### Task 12: Doc names and the naming rules

**Files:**
- Move: `docs/research-lab.md` to `docs/RESEARCH_LAB.md`, and `docs/version-ports.md` to `docs/VERSION_PORTS.md`
- Modify: every link to them (`grep -rln "research-lab.md\|version-ports.md"`), `docs/README.md` and `AGENTS.md` ("Read only when needed")
- Modify: `docs/DEVELOPMENT.md` (a new "Naming" section with these rules):
  - crate: `specsheet-<responsibility>`
  - research example: `<topic>_<survey|info|check|dump|probe>`
  - evidence file: `<area>/<topic>.<txt|csv|json>`
  - doc: UPPERCASE for project-wide docs, kebab-case for topic docs under `app/`, `formats/` and `shader-specs/`
  - test: a sentence saying what must hold
  - IDs: D*n* and F*n*, never reused

- [ ] **Step 1: Rename with `git mv` and fix the links.**
- [ ] **Step 2: Verify.** `cargo xtask research` passes (it checks doc links to evidence), `python .agents/tools/hub validate` reports 0 errors and 0 warnings, and `grep -rn "research-lab.md\|version-ports.md" --include=*.md --include=*.rs .` prints nothing outside `CHANGELOG.md`.
- [ ] **Step 3: Commit** "Name the project-wide docs in capitals and write down the naming rules".

## Not in this plan

- **Splitting `crates/specsheet/src/viewer.rs` (1,417 lines) and `crates/specsheet-render/src/assets.rs` (1,157 lines):** read them first, and split only if they hold separate responsibilities.
- **Converting the other research examples to the header and CSV helpers:** each one is converted the next time it is touched.
- **Moving the register, evidence header and checker pattern into the hub framework:** after this plan has run here.

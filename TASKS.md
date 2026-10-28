# Tasks

Living tracker: update it at the end of every work session. Phases are in [docs/ROADMAP.md](docs/ROADMAP.md); shipped changes are in [CHANGELOG.md](CHANGELOG.md).

## Done (verified)

- [x] HashFS v2 reader, CityHash64 v1.0.3, GDeflate textures, layered VFS. Every entry of the surveyed archives is reachable.
- [x] Mount order from `game.log.txt`.
- [x] SII (with `@include`), PMD v4, PMG v0x15 and MAT parsers. Every `/def/vehicle` definition and every truck model parses (`real_install` tests).
- [x] `specsheet-core`: catalog, rules (`suitable_for` globs, conflicts, defaults, required slots, slot availability) and repair. Unit tests pass, and all 26 game presets validate with 0 issues (`catalog_info`).
- [x] Slot→locator rule measured: 626/626 preset addons resolve (`locator_check`).
- [x] Wheel tyre offset (F9): tyres shifted by the rim's `tire_shift` and wheel locator yaw ignored. All 104 preset wheels fit their tyre within 6 mm (`real_install` test `every_preset_wheel_sits_in_its_tyre`).
- [x] Assembly with wheels, hotspot anchors and the interior/exterior split.
- [x] Multi-axle chassis (F13): all 164 chassis have one wheel-locator pair per declared axle (132 with 3 or 4 axles), and the locator letter picks single or twin tyres. Guarded by the `real_install` test `every_chassis_has_one_wheel_pair_per_declared_axle`; survey `axle_survey`.
- [x] Tauri app, verified in the running WebView2 over CDP with screenshots:
  - Scania S renders from game files.
  - A hotspot click lists options.
  - Selecting chromed mirrors swaps the model.
  - Switching to the normal-roof cabin re-assembles the truck.
  - Volvo FH 2024 loads.
  - The RHD toggle works.

- [x] Renamed the app to SpecSheet (product name, identifier `dev.specsheet.app`, window title, top-bar brand, README). Crate names are unchanged. Not yet seen in the running app.

- [x] The game's tuning scenes (F7): garage, Under the Bridge and the other eight load from `/def/desktop`. Verified with CDP screenshots (`research/screenshots/05-*`, `06-*`).

- [x] REA binary overview and batched string search of `eurotrucks2.exe`, recorded as F8 with Evidence IDs.
- [x] Real-install tests also cover every preset assembling in LHD and RHD with no warnings, and every scene having a model and a framing camera.

- [x] Review fixes:
  - Selection commands run off the UI thread.
  - Base parts obey `suitable_for` in the backend (regression test).
  - Picking a paint job clears the preset colour override.
  - Slots without markers appear in the sidebar's "General" group.
  - The game directory is remembered, and discovery scans drive roots.
- [x] Release build (`SpecSheet_0.1.0_x64-setup.exe`) launched without `ETS2_DIR`: discovery found the install, assets load from the production origin, and the cabin swap and paint job work (`research/screenshots/07-*`, `08-*`).

- [x] `effect.scs` fully readable (record `0x07`); `.rfx` effect→shader map documented in ASSET_FORMATS.

- [x] Interior exterior-view model at `ext_interior` (F10), empty-slot and wheel-group anchor flags. Real-install tests cover all 26 presets in both drives.

- [x] **All-Rust app (D11):** Bevy 0.19 + bevy_egui 0.42 in `crates/specsheet` replaces Tauri + React + three.js. Display logic moved into `specsheet-data::view`. Seen working with `SPECSHEET_SCRIPT` screenshots and real mouse input (user32):
  - Scania S in the "Home at Last" garage, with markers and the parts list.
  - A marker click opens the slot (Main Mirrors).
  - Picking a light bar adds it on the roof.
  - The normal-roof cabin re-assembles the truck.
  - RHD moves mirrors and wipers.
  - Switching to scene 3 (Halloween Fields, dark by definition).
  - Volvo FH 2024; interior markers; orbit drag and wheel zoom.
  - Empty-slot markers draw as rings, and wheel groups as one blue numbered marker.
  - The old Tauri/React app, npm files and their ignore entries are removed (still in history at e360ebc).

- [x] Addon hookups (F11): catalogue, rules, placement, config loading; real-install test on the game's oversize Mercedes config. Exposed through `TruckSession::{hookup_options, set_hookup}` for the app.

- [x] Right-hand-drive interiors: UK twins paired for all 219 units (F10).
- [x] Volvo "FH6" name and missing side mirrors explained as game data (F12).

- [x] **The game's truck configuration screen** (`truck.config`) is drawn from the install's layouts, templates, colours and icons (`docs/app/ui.md`):
  - the markup layout engine (`specsheet-data::ui::flow`);
  - the 8 category tabs with the game's icons (F5);
  - accessory lists as the game's tiles with icons, prices, the owned ✓ and locked reasons;
  - pins only on the accessory tabs.
  - The app opens on `/def/truck_config_scene.sii` with the game's orbit camera.
  - Seen working in `SPECSHEET_SCRIPT` screenshots: cabin tab, exterior accessories with pins, the light-bar list, and a pick that fits the bar and moves the ✓.
- [x] **Visual parity runner:** `specsheet --parity` renders `parity/cases.json` (Scania S preset, Volvo FH 2024 preset, Scania S with the first light bar) and scores them against references in `research/parity/` (MAE, SSIM, diff heatmap). Verified end to end with a render as its own reference (MAE 0, SSIM 1); real references are still to be captured by the user (`docs/DEVELOPMENT.md`).

## In progress

- [ ] Build speed: targeted checks are documented (AGENTS.md, DEVELOPMENT.md). The `rust-lld` measurement is pending: the cold Bevy baseline build was stopped by Claude Code when the machine ran low on memory, with several sessions each building Bevy in their own target dir. Re-run it when the machine is quiet. sccache awaits the user's direct confirmation (it installs a tool).


- [x] REA trace: the pin-button string is used by `FUN_140fe0fc0` (F8).
- [x] REA decompilation of `FUN_140fe0fc0` (F5): it selects a pin (button `10000 + n`; hookups `11000 + m`) after switching to one of the game's 8 tabs. It does not create or place pins.
- [x] License-safe re-derivation: the PMD/PMG layouts and the texture image/sample records are now cited from our own bytes (`pmg-layout-derivation.txt`, `texture-unpack-derivation.txt`). Corrections: piece +60 is the vertex stride (now checked), and an empty hookup string means no hookup.
- [ ] DXGI 67 (R9G9B9E5_SHAREDEXP, skyboxes and HDR UI images) is missing from `pixel_layout`, so those textures do not unpack. Needed only if the app renders skyboxes.
- [ ] sccache: install a prebuilt binary, measure cold and warm builds, keep it only if it helps (approved by the user).
- [x] Clean-room rewrite, phase 1 (D13): `pmd.rs`, `pmg.rs` and `texture.rs` rewritten from interface stubs and our own evidence by an agent that never saw TruckLib or the old code. Output identical to the old code for all 278,633 models and textures in the install (`research/evidence/cleanroom-phase1.txt`), plus three resource-exhaustion guards.
- [x] Clean-room rewrite, phase 2 (D13): `hashfs.rs` rewritten by a second fresh agent from a byte-level derivation of the HashFS v2 layout (`hashfs-layout-derivation.txt`). Identical to the old reader on all 116 archives (`cleanroom-phase2.txt`). It corrected the old doc: the header ends at 0x34, record 0x06 is on .pmg only, and 547 entries appear in no listing.

## Next

The goal is a 1:1 replica of in-game tuning (see ROADMAP Phases 4–5). Order:

1. **Materials:** chrome now renders as metal from `env_factor`/`shininess` (docs/app/materials.md). Next: use the game's reflection cubes (`texture_reflection`, e.g. `vehicle_reflection.tobj`, 6 faces) as the environment map instead of the grey gradient, which needs cube-face upload and a change in `viewer.rs` (agree it with the app owner first). Then normal maps.
2. **Paint jobs:** masks, textures, flake/flip colours and the `paint_job` slot UI.
3. **Interior:** interior models and an interior camera; `hide_in`.
4. **Wheels:** whether the tuning screen shows lift axles raised or lowered (reference screenshot or REA), the inner twin-tyre bead, `wheel_shift`. Axle variants are done (F13).
5. **UI fidelity:** the game screen is drawn from its layouts (see Done). Next:
   - the game's MSDF fonts instead of egui's;
   - money (needs the player's profile) and the cart;
   - the colour picker on the colour tab;
   - search and filter;
   - the game's pin look (`pin_look`, `custom_acc`);
   - `%2`/`%8`/`%9` of the accessory tile.
   Check all of it against reference screenshots in `research/parity/`.
6. **REA decompilation:** `FUN_140fbf860` (pin creation and placement, the caller of `FUN_140fe0fc0`) and the caller of `0x1421c2f90` (locator lookup), batched into one run; then `hide_in` and defaults/repair. Each CLI call re-runs the roughly 28 min analysis; can be delegated to the `ets2-researcher` subagent.
7. **Markers:** anchor empty slots on their first compatible option, put a wheel group on the camera-side wheel, and add a depth test (see docs/app/hotspots.md "Known gaps").
8. **Packaging:** a Windows installer and exe icon for the Bevy app (the Tauri NSIS bundle is gone). The icon is kept at `crates/specsheet/icon.png`.
9. **Bevy 0.20:** it needs Rust 1.97.1; the machine has 1.95.
10. **App shell (D16):** the `specsheet-theme` token crate with contrast tests, then the home, trucks, configurations, mods, parity, settings and about screens per [docs/app/design-system.md](docs/app/design-system.md). After the 1:1 tuning screen.

## Blockers and notes

- The REA MCP Ghidra calls time out at 330 s on the 51 MB executable. Use the CLI (see CLAUDE.md).
- The reference image the user mentioned never arrived. The UI follows the written description: truck in the centre, dots, sidebar.
- An exe icon via a `winres`-style build script will hit the same `'`-in-path problem as Tauri did (D10). Copy the icon to the temp directory first.

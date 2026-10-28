# Tasks

Living tracker: update it at the end of every work session. Phases are in [docs/ROADMAP.md](docs/ROADMAP.md); shipped changes are in [CHANGELOG.md](CHANGELOG.md).

## Done (verified)

- [x] HashFS v2 reader, CityHash64 v1.0.3, GDeflate textures, layered VFS. Every entry of the surveyed archives is reachable.
- [x] Mount order from `game.log.txt`.
- [x] SII (with `@include`), PMD v4, PMG v0x15 and MAT parsers. Every `/def/vehicle` definition and every truck model parses (`real_install` tests).
- [x] `specsheet-truck`: catalog, rules (`suitable_for` globs, conflicts, defaults, required slots, slot availability) and repair. Unit tests pass, and all 26 game presets validate with 0 issues (`catalog_info`).
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

- [x] **All-Rust app (D11):** Bevy 0.19 + bevy_egui 0.42 in `crates/specsheet` replaces Tauri + React + three.js. Display logic moved into `specsheet-session::view`. Seen working with `SPECSHEET_SCRIPT` screenshots and real mouse input (user32):
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
- [x] Paint-job data model (F15): every paint job's masks, colours, flags, flake/flip and per-part units (token-keyed files, `acc_list` resolved), all six preset colours as `PaintOverride`, family re-pick on a cabin change, and `piece_paint` per placed part; real-install tests cover all 19,998 paint jobs, masks and the 26 presets.
- [x] Truck paint material (D13, clean-room from `shader-specs/truckpaint.md`): `crates/specsheet-render/src/truckpaint.{rs,wgsl}` draws colour masks, airbrush liveries, flip colour, flakes and the doubled vertex colour per placed part. The `paint_uv_survey` example settled the vertex colours (`paint-vertex-streams.txt`). Paint samples PMG UV set 1, set 2 for `alternate_uvset` (Actros 2014 flag decal), with the variant flags from the paint job alone (F17). Seen working on the Scania S, MAN TGX Euro 6 and Actros 2014 (Driving Academy, Altamira, 770 Deep Red). Only the paint job's `airbrush` picks livery vs colour mask (the MAN cab's `.mat` names `airbrush`).

- [x] **The game's truck configuration screen** (`truck.config`) is drawn from the install's layouts, templates, colours and icons (`docs/app/ui.md`):
  - the markup layout engine (`specsheet-gameui::flow`);
  - the 8 category tabs with the game's icons (F5);
  - accessory lists as the game's tiles with icons, prices, the owned ✓ and locked reasons;
  - pins only on the accessory tabs.
  - The app opens on `/def/truck_config_scene.sii` with the game's orbit camera.
  - Seen working in `SPECSHEET_SCRIPT` screenshots: cabin tab, exterior accessories with pins, the light-bar list, and a pick that fits the bar and moves the ✓.
- [x] **Visual parity runner:** `specsheet --parity` renders `parity/cases.json` (Scania S preset, Volvo FH 2024 preset, Scania S with the first light bar) and scores them against references in `research/parity/` (MAE, SSIM, diff heatmap). Verified end to end with a render as its own reference (MAE 0, SSIM 1); real references are still to be captured by the user (`docs/DEVELOPMENT.md`).
- [x] **Interior view (F10):** the interior tabs look from the driver's seat. The full cockpit sits at the `ext_interior` eye point, and interior-only accessories (steering wheel, curtains, toys, carpets…) are placed on its locators, with pins. Real-install tests: every preset has a cockpit at the eye point in both drives, and every preset assembles without warnings. Seen working: Scania cockpit and steering wheel from the seat, interior pins, and picking a retro steering wheel swaps it in.

## In progress

- [ ] Build speed: targeted checks are documented (AGENTS.md, DEVELOPMENT.md). The `rust-lld` measurement is pending: the cold Bevy baseline build was stopped by Claude Code when the machine ran low on memory, with several sessions each building Bevy in their own target dir. Re-run it when the machine is quiet. sccache awaits the user's direct confirmation (it installs a tool).


- [x] REA trace: the pin-button string is used by `FUN_140fe0fc0` (F8).
- [x] REA decompilation of `FUN_140fe0fc0` (F5): it selects a pin (button `10000 + n`; hookups `11000 + m`) after switching to one of the game's 8 tabs. It does not create or place pins.
- [x] License-safe re-derivation: the PMD/PMG layouts and the texture image/sample records are now cited from our own bytes (`pmg-layout-derivation.txt`, `texture-unpack-derivation.txt`). Corrections: piece +60 is the vertex stride (now checked), and an empty hookup string means no hookup.
- [x] DXGI 67 (R9G9B9E5_SHAREDEXP): unpacked and uploaded as `Rgb9e5Ufloat`; all 197 textures (skyboxes, HDR UI images) unpack and decode to plausible HDR (`hdr-textures.txt`).
- [ ] sccache: install a prebuilt binary, measure cold and warm builds, keep it only if it helps (approved by the user).
- [x] Clean-room rewrite, phase 1 (D13): `pmd.rs`, `pmg.rs` and `texture.rs` rewritten from interface stubs and our own evidence by an agent that never saw TruckLib or the old code. Output identical to the old code for all 278,633 models and textures in the install (`research/evidence/cleanroom-phase1.txt`), plus three resource-exhaustion guards.
- [x] Clean-room rewrite, phase 2 (D13): `hashfs.rs` rewritten by a second fresh agent from a byte-level derivation of the HashFS v2 layout (`hashfs-layout-derivation.txt`). Identical to the old reader on all 116 archives (`cleanroom-phase2.txt`). It corrected the old doc: the header ends at 0x34, record 0x06 is on .pmg only, and 547 entries appear in no listing.
- [x] Mod loader research (F14, `docs/formats/mods.md`): container by content, HashFS v1 layout and salted keys (`mod_probe`), ZIP quirks, manifest gating, multi-folder `dlc_*` packages, mod pool at profile load.
- [x] Tolerant ZIP reader (`ZipArchive`, D15): all 33 ZIP mods read, record counts equal the game log, including the deliberately malformed one.
- [x] ScsC profile decryption (`scsc::sii_text`) and the SII `\xNN` escape fix: all 4 profiles give their `active_mods` (`profile_mods`).
- [ ] Mod priority (F14 Unknown): the REA string search is done (`rea-search-mod-loader.txt`). Next, decompile the functions that reference `0x142112188` (`[mods] Active %Iu mods`) and `0x1420bcaa8` (pool lookup). Each REA `function` run is a full ~30 min analysis; run it after 71's `FUN_140fbf860` run, never at the same time.
- [x] `\xNN` fix vs the app UI (checked with de): the markup parser passes multi-byte text through, and the affected widgets (prev/next tab glyphs, money) are hidden for now, so nothing on screen changes. Once wired, the glyphs need the `glyphs_xb` font.
- [x] Binary SII parser (`bsii.rs`) and the text placement fix: all 86 saves decode (32 binary, 54 text), every nameless reference resolves, and 470 of 482 shared fields agree in kind with text (`save_check`, `bsii-derivation.txt`).
- [ ] Placement float 3 (always just above 2^23, probably packed position offsets) and `nil` = u64::MAX are open (bsii.md). Settle them before showing save positions.
- [x] HashFS v1 reader (`HashFsV1Archive`, D15), written from `hashfs-v1-derivation.txt`: all 20 v1 mods open, counts equal the game log, 170,674 entries read, 0 failures.
- [ ] Mods next (D15): mounting into the `Vfs` in profile order, home-directory and profile discovery in `specsheet-data`. Writing profiles only with the user's go-ahead.
- [x] Mod writer (D18): `ZipWriter`, `write_new_package` (never replaces, refuses a taken package name), `mod_manifest`; the test package from `mod_write` passes Python's strict `zipfile` check (`mod-writer-checks.txt`).
- [x] Mod file names (`mod_names`, `paths`): v1 packages without listings get names by hashing candidates; 167,657 of 171,094 entries in 22 packages with game and other-package candidates (`mod-name-recovery.txt`). Search limits added after a security review (section 6).
- [ ] Mod names, open: `Add-On MAN TG3 2020 by Gloover.scs` keeps 1,085 of 7,558 entries unnamed even with its definitions package as candidates; find which references the scanner misses. When mods are shown in the app, pass the other packages' names as candidates.
- [ ] Mod writer in the game: the user copies `specsheet_test.scs` into the mod folder and enables it; check the mod manager and `game.log.txt` ("has been mounted"), then record the result in mods.md.
- [ ] What a real package holds (the user's choice), e.g. a tuning configuration from the app. Needs mod priority (F14) first if it overrides game paths.
- [ ] Extractor parity (D19; facts from sk-zk Extractor, each confirmed on our install first, own code; `mod-name-recovery.txt` sections 8–9):
  - [x] Compared paths with Extractor 2026-07-29 on the 22 v1 packages: files named, ours 160,589 vs 160,726, the same set in 17 packages.
  - [x] Rules: exact `.tobj` texture records, `automat/` by content hash, packing-order neighbours, v1 packages as candidates (`--others`), extra candidates from a file (`--paths`). 169,777 of 171,094 named (was 167,657); MAN TG3 6,445 (was 4,341).
  - [x] Word swaps learned from game folders (NG Sound `blinker_off`, MAN TG3 +28) and a per-package cache for `--others` (82 s → 0.3 s); sibling vehicle folders (NG Sound's Scania T), sorted gaps (`sound_2024.sui`; NG Sound 91 of 91). 169,816 of 171,094 named (`mod-name-recovery.txt` sections 10–12).
  - [ ] Recheck file by file that the 2,104 new MAN TG3 names cover Extractor's 695 automat + 1,409 `.dds`.
  - [x] Not observed here, not built: map sector guessing (every v1 package already fully named where Extractor is), junk and decoy entries (0 unreadable, no decoys).
  - [x] Content types (`sniff.rs`); unnamed entries are labelled by type.
  - [ ] 3nK-encoded SII: 2 samples in MAN TG3, no repeating XOR key; needs the exe (REA) for the algorithm. Unknown.
  - [ ] Salt override for v1. Table-at-end v1: Unknown, no sample.
  - [x] Extraction of v1 and ZIP packages (`extract.rs`, `mod_extract`): checked on NG Sound (91 files). Size limit (default 64 GiB) checked before writing, after a security review.
  - [ ] Stream large entries to disk instead of holding each in memory (needs a reader API in `specsheet-archive`).
  - [ ] Extraction of game (v2) archives: non-texture files first; packed textures need a `.dds` and a standalone `.tobj` written (fields Unknown).
  - [ ] Tree and filter options for the example.

## Next

The goal is a 1:1 replica of in-game tuning (see ROADMAP Phases 4–5). Order:

0. **Priority (user, 2026-10-29): environment, shaders, textures, then the rest.** In dependency order, with owners:
   1. **Environment** (de):
      - [x] the vehicle-reflection slot: the tuning scene's `cube_reflection` fills it with our grey stand-in, `ui_vehicle_reflection.tobj` (docs/app/materials.md); paint no longer mirrors the forest cube;
      - render the garage's own reflection cube in Bevy, to replace that stand-in;
      - the tonemap from `shader-specs/tonemap.md`, then the pre-exposure (0.0973) A/B;
      - real-time shadows from the scene's main light;
      - the scene fields still unused: `light_specular`, `floor_reflection`, `cube_reflection`.
   2. **Textures** (d3, data only, no shader text):
      - [x] DXGI 67 (R9G9B9E5_SHAREDEXP) unpacks and uploads (`hdr-textures.txt`).
      - [x] `MaterialInfo` carries `normal_texture`, `normal_detail_texture` and `mult_texture`; `MeshPiece` carries `tangents` and `normal_uvs` (the set routed to UV slot 6). de wires them to the GPU; BC5 already uploads natively.
      - [x] PMG piece +8 is the UV slot routing (`pmg-uv-routing.txt`, 611,638 pieces).
      - [ ] Paint UVs from the routing: slot 1 for the mask, slot 2 for `altuv`, with `paint_uv_set` as the fallback. The mask agrees with today's rule in 96 % of pieces, `altuv` in only 2,423 of 4,986 (e.g. `0xf2fff110`, 2,153 pieces). Needs a screen test of an `alternate_uvset` job on such a piece (DAF 2021 `cab_door/win_xf_uk.pmd`) before switching.
      - [ ] `texture_mult` waits for 71's spec of `dif.spec.mult.dif.spec` and the scene shaders.
   3. **Shaders** (D13: 71 writes the specs and never the WGSL; de writes the WGSL from the specs only):
      - [x] specs by 71: `glass`, `lamp`, `dif.lum`, `unlit.*`, and `decalshadow`/`light.tex`/`fakeshadow`/`shadowonly`/`none` (`docs/app/shader-specs/`). `tsnmap`/`tsnmapuv` were already in `dif-spec.md` and `truckpaint*.md`; added: UV slot *n* is vertex attribute 8 + *n*, and truckpaint `asafew.a`/`shadow`. The engine inputs they leave open (`texture_reflection_modulator`, lamp colours and contrast, material luminosity) are in each spec's last table, with stand-ins;
      - specs by d3's agent: `dif.spec.mult.dif.spec`, `showroom_v2`, `baked`;
      - [x] REA, answered statically without Ghidra (F18): the cube lookup mirrors z (envmap matrix row 2 negated); `limited_pre_exposure` scales only emissive inputs, so the scene's light colours reach the shaders unscaled; `g` = 1 on the tuning screen; F0 is a constant per effect flag (truckpaint 0.035, glass 0.07); `Ke = env_factor ⊙ specular`; the luminance rule. Render follow-ups for de: truckpaint F0 (`truckpaint.rs` uses the `.mat`'s 0.2), `Ke` × specular in `difspec.rs`, and the z flip in the cube sampling.
   4. **Other:** bloom, and `fakeshadow`/`shadowonly`.
1. **Materials:** chrome renders as metal from `env_factor`/`shininess`, and the truck's own reflection cube is the environment map (docs/app/materials.md). Next: the cube's exposure, then normal maps and `texture_mult`.
2. **Paint jobs:** data (F15) and the truckpaint surface are done. Next:
   - the game's lighting: the shared module (`specsheet-render/src/game_lighting.wgsl`) lights truckpaint and `dif.spec`, each with its own `.mat` reflection cube (done). The vehicle_reflection slot is filled by the scene (done). Next: normal maps (needs tangents), then the output scale against the game's tone mapping (F16, spec in progress);
   - align with the game's paint application (F17, done by REA): locked colours win (done, a9cc0b2); effect flags from the paint job and UV sets 1/2 (done); the engine defaults for missing paint values (done). Still open: the UV set used when a model lacks it, and two caller paths;
   - the `paint_job` slot UI and the colour picker.
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
6. **REA (next batch):** `FUN_140fbf860` turned out to be the input handler (F5). Next: one combined string search (`~/rea-ets2/next-search-regex.txt`, plus the `pin_offset`/`pin_dir`/`pin_group` keys) for pin placement, camera conventions, exposure/tonemap, lift axles, paint-effect selection and `ui_vehicle_reflection`; then decompile what it points to, and the caller of `0x1421c2f90` (locator lookup). Each Ghidra run takes about 30–45 min and 2 GB; coordinate with peers first.
7. **Pins:** exterior, wheel and interior pins follow the game (F5, docs/app/hotspots.md); `use_non_wheel_pin_data_from_model` defaults to false (Confirmed). Still to do: the game's pin graphics (`btn.custom_accn`/`s`, `acc_hub`/`acc_cover`/`acc_nuts`); the per-truck pin table (Renault T `f_light_top`); confirm the interior pass's added transform; re-check the badge and Renault T `s_panel` pins in game.
8. **Packaging:** a Windows installer and exe icon for the Bevy app (the Tauri NSIS bundle is gone). The icon is kept at `crates/specsheet/icon.png`.
9. **Bevy 0.20:** it needs Rust 1.97.1; the machine has 1.95.
10. **App shell (D16):** the `specsheet-theme` token crate with contrast tests, then the home, trucks, configurations, mods, parity, settings and about screens per [docs/app/design-system.md](docs/app/design-system.md). After the 1:1 tuning screen.
11. **Docs per feature:** move each tuning feature's findings, rules and status into one `docs/features/<feature>.md` (wheels, interior, hookups, pins, scenes, paint). Shrink REVERSE_ENGINEERING.md to an index of F-numbers that link to them. Do it after the paint branch and the mod session's F14/D15 land.
12. **Restructure (D17):** the target tree is in place (2026-10-28: core, theme, archive, formats, truck, data, assembly, gameui, session, render, research, app; `xtask` check/layers/guard/real/parity/version-port). Still to do:
    - split `specsheet-data/src/catalog.rs` and `specsheet-truck/src/config.rs` into modules;
    - bundle the theme's fonts (OFL) and build the app's own screens on the tokens (D16);
    - `deny.toml` (cargo-deny) and `missing_docs` on the library crates.

## Blockers and notes

- The REA MCP Ghidra calls time out at 330 s on the 51 MB executable. Use the CLI (see CLAUDE.md).
- The reference image the user mentioned never arrived. The UI follows the written description: truck in the centre, dots, sidebar.
- An exe icon via a `winres`-style build script will hit the same `'`-in-path problem as Tauri did (D10). Copy the icon to the temp directory first.

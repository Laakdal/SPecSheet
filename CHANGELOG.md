# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- **Interior view:** the Interior and Interior Accessories tabs now put you in the driver's seat, with the truck's full cockpit and your interior accessories (steering wheel, curtains, cab toys…) in place. Drag to look around. Interior accessories now have pins.
- **The game's own tuning screen:** the app now draws ETS2's truck configuration screen from the game's layout files, templates, colours and icons in your install:
  - "Vehicle configurator" panel, category bar with the game's 8 tabs, accessory tiles with icons and prices, folded vehicle analysis and leave button;
  - pins only on the accessory tabs, as in the game;
  - it opens on the game's truck configuration garage and camera;
  - the price box shows the truck's original price, the extra cost of your changes and the new total;
  - an accessory tab lists all its slots while no pin is chosen, so every part is reachable, including those without a pin yet.
  - The truck, drive and scene pickers moved to a small "SpecSheet" window. The dev script's `interior on|off` is replaced by `tab <token>`.
- **The app is now all Rust:** Bevy 0.19 + egui (`crates/specsheet`) replaces the Tauri + React + three.js app. Run it with `cargo run --release -p specsheet`.
  - Same features: the game's scenes, hotspots with clustering, the sidebar with options and reasons, live re-assembly, LHD/RHD, interior markers and the orbit camera.
  - Textures go to the GPU BC-compressed through wgpu (DirectX 12/Vulkan). Malformed textures and meshes are skipped with a warning instead of crashing.
  - Empty-slot markers are drawn as rings, and each wheel axle as one marker.
  - The last truck is remembered.
  - New dev script mode (`SPECSHEET_SCRIPT`) for scripted runs and screenshots.
  - Node.js and WebView2 are no longer needed. The NSIS installer is gone for now.
- Hotspot anchors flag empty slots (`Anchor::empty`) and group wheel parts per axle (`Anchor::group`: `wheel_f`, `wheel_r`).
- **Name:** the app is now called **SpecSheet** (tagline "SpecSheet: ETS2 Save Edit"). The window title, top-bar brand, installer product name and app identifier (`dev.specsheet.app`) are updated. Settings saved under the old identifier are not migrated, so the game directory is detected again on first launch.

### Added
- **Reflections:** the truck's reflection cube from the game (e.g. `fuzzy_reflection.tobj`) replaces the grey gradient as the environment map, so chrome and paint reflect the game's own environment.
- Addon hookups (accessories on accessories: aux lights, beacons, horns, small lights): catalogue, configuration rules, placement on the host's `slot_NN` positions, and loading from game configs (`load_vehicle_config`).
- **Archives:** read-only HashFS v2 (`.scs`) reader.
  - CityHash64 v1.0.3 path hashing.
  - zlib entries; GDeflate packed textures with alignment unpacking.
  - Layered VFS that follows the game's mount order.
- **Parsers:**
  - SII text definitions with `@include`, arrays, tuples and hex floats.
  - PMD v4 descriptors (looks, variants, part visibility).
  - PMG v0x15 geometry (parts, locators, pieces).
  - `.mat` materials.
- **Install handling:** discovery via `ETS2_DIR` or Steam library folders; English localisation.
- **Tuning core:** truck catalogs from `/def/vehicle/truck/*`, including shared wheel parts.
  - Rules: `suitable_for` with globs, two-way `conflict_with`, `defaults`, `require` slots, slot availability from model locators.
  - Configuration repair after base-part changes.
  - The game's presets load as the starting configuration. All 26 trucks validate with zero issues.
- **Assembly:** chassis, cabin, addons at slot locators, wheels at wheel locators (left/right variants). Hotspot anchors and interior/exterior classification.
- **Desktop app** (first built on Tauri 2 + React + three.js, now Bevy; see Changed):
  - Truck picker, 3D viewport built from the original models and BC textures.
  - Camera-following hotspots with screen-space clustering.
  - Sidebar with compatible and incompatible options and their reasons.
  - Live re-assembly on selection; left/right-hand drive.
- **Game tuning scenes:** the scenes from `/def/desktop/config*.sii` (garage model, camera, lights, fog, truck pose) load from the install, with a scene picker and per-piece fallbacks.
- **Materials:** additive (`add`) and translucent water effects.
- **Research tooling:** archive survey, VFS browser, model, definition, locator and geometry surveys. Outputs live in `research/evidence/`.
- **Project:**
  - MIT license (`LICENSE`) and `THIRD_PARTY_NOTICES.md`.
  - Windows CI on every push and pull request: guard, format, lint, tests.
  - The Rust toolchain is pinned to 1.95.0 (`rust-toolchain.toml`).
  - LF line endings on every platform (`.gitattributes`).
  - Commits are signed.
- **Game version pin:** `GameInstall::version()` reads the installed version from `version.scs`. The real-install tests fail with instructions on any version other than `TESTED_GAME_VERSION` (1.60.1.7).

### Fixed
- Chrome and polished metal no longer render nearly black: materials with an `env_factor` become PBR metal, with roughness from `shininess`.
- **Licensing:** the model (PMD/PMG) and texture code was rewritten clean-room from our own evidence (D13), with output identical to before on every model and texture in the game. The parsers now also bound UV sets, total decoded geometry and variant values against hostile files.
- **Licensing:** the HashFS archive reader was also rewritten clean-room, from a new byte-level derivation of the format (D13), and reads every archive identically. No code derived from GPL sources remains.
- PMG locators without a hookup no longer report an empty hookup string, and a piece whose stored vertex stride disagrees with its streams is rejected instead of misread. Both were found by re-deriving the model layout from our own byte dumps.
- The docs listed sk-zk's TruckLib references as MIT. They are GPL-2.0, so SPecSheet (MIT) takes only facts from them, never code.
- Right-hand drive shows the right-hand-drive interior (each interior's UK twin, paired for all 219 units).
- The cab is no longer hollow: each selected interior's exterior-view model is placed at the cabin's `ext_interior` locator (all 26 presets, both drives).
- HashFS metadata record type `0x07` (8,970 `effect.scs` shader `.rfx` entries) is accepted; every archive in the install now indexes with zero skipped entries (guarded by the real-install tests).
- Base parts (chassis, cabin) are checked against their own `suitable_for` in the backend, not only in the UI.
- Selecting a paint job applies its colour (the preset colour override is cleared). Game colours (`base_color`, `.mat` `diffuse`) are treated as linear.
- Selection commands no longer run on the UI thread.
- **Wheels:** tyres now sit on their rims. Each tyre is shifted along the axle by its disc/rim `tire_shift` (for example, 3.8 cm outboard on the Scania S 2016 front wheels). Wheels also ignore yawed chassis wheel locators, which had turned the MAN TGX and Renault Magnum rear-right wheels sideways or onto the wrong side.

### Changed
- The game directory is remembered between launches, and discovery also checks common Steam library folders on every drive.
- Slots without a 3D marker (paint job, engine, interior-only parts…) are listed in a "General" sidebar group.

# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- **Paint reflections are as strong as the game's:** painted panels now use the game's own reflection strength at head-on angles instead of the material's fresnel value, so glossy paint mirrors its surroundings much less when seen face-on.
- **Alternate-UV paint jobs land where the game puts them on every truck:** paint now reads each model's own UV routing, so liveries such as Golden Edge on the DAF 2021 XF show on the cab and doors.
- **Paint jobs render like the game's:** two-tone colour masks, airbrush liveries (logos, stripes, the 770 designs) and metallic flip colours that shift with the viewing angle. Each part takes its own paint, and the game's baked shading on painted panels is applied, so paint looks darker than before. The lighting is still an approximation. Metallic flakes are wired in but not visible yet.
- **Metallic flakes show:** paint jobs that leave out a flake or flip value now take the game's defaults instead of zero, so flake patterns appear in the highlights (e.g. Under Wraps' gift wrap on the MAN TGX Euro 6). Missing mask colours default to red, green and blue as in the game.
- **Paint-job thumbnails for plain colours:** stock colours without an icon (the RAL colours) show a swatch of their colour, and metallic ones a layered swatch with the flip and flake colours, as in the game.
- **Paint colour picker:** paint jobs with unlocked colours (Custom Colour, Custom Metallic, two-tone liveries) show a picker for each colour they let you change; the truck takes the colour when you release the mouse.
- **A shadow under the truck:** the game's own shadow quad under each chassis is drawn on the tuning screen.
- **Options for other parts are hidden:** paint jobs and accessories made for another cabin or chassis no longer show as locked tiles; conflicting options still show their reason.
- **Side windows have glass, and the wipers are back:** the cabin's separate window and wiper models are now placed with the cab.
- **Lower default camera:** the tuning screen's camera starts lower, looking slightly up at the cab, and orbits between that height and the floor.
- **Lights glow in the lit garages:** in the hub scenes that switch the truck's lights on, the headlights and parking lights show the game's own glow. The tuning screen itself keeps the lights off, as in the game.
- **Headlights are no longer white blobs:** lamps, including those on hookup accessories, show their reflectors and lenses; the lights are off on the tuning screen, as in the game.
- **Option lists stay put:** picking an option no longer moves it to the top of the list, as in the game. Options are listed by price.
- **The truck casts a shadow:** the scene's sun casts real-time shadows from the truck's own shadow geometry, onto the truck and the garage floor, as the game's data defines them. Whether the game shows them on the tuning screen is not confirmed yet.
- **The game's tonemapping:** the 3D view now goes through the game's own tonemap step instead of Bevy's, and the garage and the truck are lit in the same units, so both match the game's lighting more closely. Exposure is fixed and the tone curve is neutral until the game's values for the tuning screen are known.
- **Calmer reflections on the tuning screen:** paint and chrome no longer mirror the bright forest scene. In the tuning garage they reflect a flat grey cube, our stand-in for the garage reflection the game renders, so glossy paint reads like the game's.
- **Plastic, metal and chrome are lit like the game's:** the `dif.spec` family (60 % of the game's materials) uses the same lighting as paint: the base texture's alpha as the shine mask, unnormalised highlights, and each chrome part's own reflection cube with the game's fresnel. Surfaces facing away from the sun now render much darker than before; the game's exposure is not modelled yet.
- **Paint is lit like the game's:** painted panels use the game's own lighting (ambient, sun, unnormalised highlights) and reflect their own reflection cube, so metallic colours read truer (Under Wraps is crimson, not brown). Unpainted parts still use the approximate lighting.
- **Alternate-UV paint jobs land where the game puts them:** decals such as the Bulgarian Flag Decal on the Actros 2014 now show on the door; paint uses the model's second UV set, and the third for alternate-UV jobs.
- **Pins sit where the game puts them:** each exterior pin starts from its slot's locator plus the game's own offset, paired parts get a pin on both sides, and every wheel gets its own tyre, disc, hub, cover and nut pins. Hub, cover and nut pins appear once the wheel's disc pin is opened. Pins fade as they turn away from the camera instead of vanishing, and nearby pins no longer merge into numbered clusters.
- **Interior pins sit on the cockpit:** pins on the interior tab start from the cockpit's own locators at the driver's eye, plus the game's offsets, instead of the centre of each part's bounding box.
- **Locked paint colours stay locked:** a paint job's locked colours now win over a preset's or save's own colours, as in the game.
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
- **A log file:** every run writes its log to `%APPDATA%\dev.specsheet.app\specsheet.log` (the run before is kept as `specsheet.prev.log`), so problems in the release app, which has no console, can be looked at afterwards. `SPECSHEET_PERF=1` adds frame time, FPS and load times to it.
- **Garage shading:** the tuning garage now shows the game's baked shadows and depth instead of a flat white hall.
- **Normal maps:** plastic, metal and chrome parts that have a normal map now show its relief, such as the studs on the Scania S step plates and the grille mesh.
- **HDR textures load:** the game's R9G9B9E5 textures (194 sky panoramas and 3 HDR images of the brightness options) now unpack and upload, ready for skyboxes.
- **Reflections:** the truck's reflection cube from the game (e.g. `fuzzy_reflection.tobj`) replaces the grey gradient as the environment map, so chrome and paint reflect the game's own environment.
- **Paint-job data:** every paint job now loads its mask, mask colours, airbrush/alternate-UV/flip-flake flags, flake and flip parameters and per-part masks; presets keep all six paint colours, a cabin change keeps the paint job's design, and `specsheet_data::paint::piece_paint` gives the effective paint of each placed part (not rendered yet).
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
- **Mod packages:** a tolerant ZIP reader (`specsheet_archive::ZipArchive`) reads mod packages the way the game does. It ignores the encryption bit and CRCs that some mods scramble to block ordinary tools. All 33 ZIP mods on the test install read completely. A HashFS v1 reader (`HashFsV1Archive`) covers the older `.scs` format many mods use (ProMods, PNG): all 20 v1 mods read completely, salted packages included. Mods are not mounted yet.
- **Profiles:** `specsheet_formats::scsc` decrypts `ScsC` profiles (AES-256-CBC + zlib) and passes already-decrypted SII text through, so a profile's `active_mods` can be read. Profiles are only read.
- **Saves:** binary SII (`specsheet_formats::bsii`) is read into the same units as text SII, and `scsc::document` reads a save or profile in any form (encrypted, text or binary). All 86 saves on the test install decode. Saves are only read.
- **Mod file names:** `specsheet_data::mod_names` recovers the names of files in HashFS v1 mod packages that ship without directory listings, from the game's own paths, the packages' contents and file conventions: exact `.tobj` texture paths, generated `automat/` materials named by their content hash, packing-order neighbours, word swaps learned from the game's file names (`blinker_on` → `blinker_off`), a vehicle folder's files tried in its sibling vehicles, and names that sort between an unnamed file's packed neighbours. The scan of other mod packages is cached per package (`specsheet_data::mod_scan`): 82 s the first time, 0.3 s after. 169,816 of 171,094 entries in 22 local packages get a name with the game's paths and the other packages' files as candidates, against 142,029 from the packages alone. Every candidate path is checked, and the search is bounded per package (candidates, their total length, bytes scanned), so a hostile package cannot exhaust memory.
- **Mod extraction:** `specsheet_data::extract` writes a mod package's files (HashFS v1 with recovered names, or ZIP) to a folder the user names (D19): never inside the game install, the ETS2 home folder or the repo, never over an existing file, with Windows-safe names listed in `_renamed.txt` and unnamed entries in `_unnamed/` labelled by content type (`specsheet_formats::sniff`). Example `mod_extract`.
- **Mod writer:** `specsheet_archive::ZipWriter` builds new mod packages as standard ZIPs (correct CRCs, no encryption bit, reproducible), `write_new_package` puts one in a folder without ever replacing a file or reusing a package name, and `specsheet_formats::mod_manifest` writes `manifest.sii`. The example `mod_write` builds a test package that changes nothing in the game.
- **Game version pin:** `GameInstall::version()` reads the installed version from `version.scs`. The real-install tests fail with instructions on any version other than `TESTED_GAME_VERSION` (1.60.1.7).

### Fixed
- Debug builds no longer panic on uncompressed game textures with mipmaps (flake noise, UI textures): Bevy's single-mip size check is bypassed for the full chain.
- Chrome and polished metal no longer render nearly black: materials with an `env_factor` become PBR metal, with roughness from `shininess`.
- Text SII placements `(x, y, z) (w; x, y, z)` parse as one value. They used to fail the whole file, so no save could be read.
- **Licensing:** the model (PMD/PMG) and texture code was rewritten clean-room from our own evidence (D13), with output identical to before on every model and texture in the game. The parsers now also bound UV sets, total decoded geometry and variant values against hostile files.
- **Licensing:** the HashFS archive reader was also rewritten clean-room, from a new byte-level derivation of the format (D13), and reads every archive identically. No code derived from GPL sources remains.
- SII strings decode `\xNN` escapes as UTF-8 bytes. They used to lose the backslash (`xe2x82xac` instead of `€`), in profiles, in 31 game UI layouts (the euro sign and glyph-font icons on the tuning screen) and in road-sign definitions.
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

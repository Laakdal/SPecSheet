# Where things live

Which code and which tests own each feature. Keep it current when a feature moves or gains a test (AGENTS.md routes here).

Paths are relative to `crates/`. `real_install::x` is a test in `specsheet-session/tests/real_install.rs`; it needs `ETS2_DIR`.

| Feature | Code | Tests |
|---|---|---|
| `.scs` archives, path hashing, layered VFS | `specsheet-archive/src/{hashfs,city,vfs}.rs` | unit tests in those files; every `real_install::*` mount |
| Mod packages: ZIP, HashFS v1; writing new ZIP packages | `specsheet-archive/src/{zip,hashfs_v1,zip_write}.rs`; manifests in `specsheet-formats/src/mod_manifest.rs` | unit; examples `mod_probe` over a mod folder, `mod_write` (test package) |
| Packed textures, GDeflate | `specsheet-archive/src/{texture,gdeflate}.rs` | unit tests in those files |
| SII definitions | `specsheet-formats/src/sii.rs` | unit; `real_install::every_vehicle_definition_parses` |
| Mod file names in HashFS v1 packages without listings | `specsheet-data/src/mod_names.rs`, cached folder scan in `specsheet-data/src/mod_scan.rs`; scanner in `specsheet-formats/src/paths.rs`; content types in `specsheet-formats/src/sniff.rs` | unit; example `mod_names` over v1 packages |
| Extracting a package to a folder (D19) | `specsheet-data/src/extract.rs` | unit (temp folders); example `mod_extract` |
| Mounting a profile's mods (D20): home folder, profiles, gating, multi-folder packages, Steam Workshop items (`versions.sii`), the mod pool | `specsheet-data/src/{mods,install}.rs`; mixed devices in `specsheet-archive/src/vfs.rs`, folders in `specsheet-archive/src/folder.rs`; `GameSession::open_with_profile`, `view::{profiles, mods}`; picker in `specsheet/src/ui/mod.rs` | unit; opt-in `real_install::a_profiles_mods_mount_and_the_trucks_they_add_load_with_their_textures` (`ETS2_HOME`, `ETS2_PROFILE`); example `mod_mount` |
| Loose mod textures: standalone `.tobj`, DDS | `specsheet-formats/src/{tobj,dds}.rs`; fallback in `specsheet-data/src/render.rs` | unit; example `tobj_survey` |
| Profiles and saves: ScsC, binary SII | `specsheet-formats/src/{scsc,bsii}.rs` | unit; examples `profile_mods`, `save_check` |
| Models (PMD, PMG, tokens), materials | `specsheet-formats/src/{pmd,pmg,token,mat}.rs` | unit; `real_install::every_truck_model_parses` |
| UI markup tags | `specsheet-formats/src/ui_markup.rs` | unit; `real_install::every_ui_template_parses_and_the_tuning_screen_lays_out` |
| Rules, configuration, hookups | `specsheet-truck/src/{config,catalog,hookup}.rs`, `config/{paint,hookups}.rs` | `specsheet-truck/src/tests.rs` |
| Paint jobs: masks, colours, flake/flip, per-part paint | `specsheet-truck/src/paint.rs`; loading in `specsheet-data/src/{catalog,paint}.rs` | `specsheet-truck/src/tests.rs`; unit; `real_install::{every_paint_job_parses, paint_job_parts_resolve_by_unit_token, every_paint_mask_exists_or_is_a_constant}` |
| Install discovery, mount order, game version | `specsheet-data/src/install.rs` | unit; `specsheet-data/tests/game_version.rs` |
| Catalogs, presets, game configs, hookup catalogue | `specsheet-data/src/catalog.rs` (loading), `catalog/{accessory,paint,hookup,preset}.rs` | unit; `real_install::every_preset_validates_and_assembles_without_warnings` |
| Slot to locator matching | `specsheet-assembly/src/slots.rs` | unit |
| Assembly: base, wheels, hookups, interior | `specsheet-assembly/src/*.rs` | tests in `lib.rs`; `real_install::{every_preset_*, configured_hookups_*, every_interior_*}` |
| Meshes, materials, textures for rendering | `specsheet-data/src/render.rs` | unit |
| Tuning scenes: the truck configuration scene (orbit, pin data) and the hub scenes | `specsheet-data/src/scene.rs` | unit; `real_install::{the_truck_configuration_screen_has_a_scene_orbit_and_pins, every_tuning_scene_has_a_model_and_a_camera}` |
| UI screens, templates, colours, UI materials; markup layout | `specsheet-gameui/src/{lib,flow}.rs` | unit; `real_install::{the_truck_configuration_screen_layouts_and_templates_load, every_accessory_icon_material_names_an_existing_texture}` |
| Vehicle lights (flares): the `flare.*` units light locators name | `specsheet-data/src/flare.rs` | unit; `real_install::every_vehicle_light_unit_loads_and_its_glow_model_exists`; example `flare_survey` |
| Interior cameras: each truck's `interior_camera` unit (head offset, field of view, start look) | `specsheet-data/src/camera.rs`; used in `specsheet/src/viewer.rs` | unit; `real_install::every_truck_names_an_interior_camera_that_exists` |
| Display strings | `specsheet-data/src/localization.rs` | unit |
| Session (options, select, hookups, drive) | `specsheet-session/src/lib.rs` | through `real_install` |
| Pins the game's way: base locator, offset, facing, mirrored, wheel and interior (cockpit) pins | `specsheet-assembly/src/pins.rs` | unit; `real_install::every_preset_validates_and_assembles_without_warnings`; examples `pin_survey`, `wheel_order_survey` |
| Display data: labels, tabs, which slots get pins, incompatibility reasons | `specsheet-session/src/view.rs` | unit; `real_install::every_slot_with_a_choice_is_reachable_on_the_tuning_screen` |
| App start-up, plugin list, per-frame order (`FrameSet`), background jobs, settings | `specsheet/src/{main,studio,settings}.rs` | unit in `studio.rs` |
| Meshes, materials, textures to GPU (with upload checks) | `specsheet-render/src/assets.rs` | unit |
| Clean-room game materials (truck paint, `dif.spec`), their texture loader, the game's lighting, its tonemap pass and the shadow-only casters | `specsheet-render/src/{lib,truckpaint,difspec,textures,lighting,tonemap,shadow}.rs`, `truckpaint.wgsl`, `difspec.wgsl`, `game_lighting.wgsl`, `tonemap.wgsl`, `shadow.wgsl` | unit; `SPECSHEET_SCRIPT` screenshot |
| Truck and scene entities, lights, fog, orbit camera | `specsheet/src/viewer.rs` | unit; `SPECSHEET_SCRIPT` screenshot |
| Vehicle light flares at light locators (placement, facing, which lights are on) | `specsheet/src/flares.rs` | unit; `SPECSHEET_SCRIPT` screenshot of a lit hub scene |
| The game's truck configuration screen (layout drawing, tabs, lists), UI textures, SpecSheet window | `specsheet/src/ui/{mod,screen,textures}.rs` | unit; `SPECSHEET_SCRIPT` screenshot |
| Pins over the 3D view (projection, fade by facing, disc-linked wheel pins) | `specsheet/src/hotspots.rs`, drawn in `ui/mod.rs` | unit in `hotspots.rs`; `SPECSHEET_SCRIPT` screenshot |
| Dev script mode | `specsheet/src/script.rs` | unit |
| Visual parity runner (`--parity`, cases in `parity/cases.json`) | `specsheet/src/parity.rs` | unit |
| Ids, `Aabb`, `Transform`, sRGB | `specsheet-core/src/{ids,math,colour}.rs` | unit |
| Design tokens (colours, type, spacing; WCAG contrast test) | `specsheet-theme/src/lib.rs`, mapped in `specsheet/src/ui/theme.rs` | unit |
| Research tools (surveys, dumps, probes) | `specsheet-research/examples/*.rs` | run against the install |
| Repository tasks: `check`, `layers`, `guard`, `real`, `parity`, `version-port` | `../xtask/src/main.rs` (`cargo xtask …`) | unit |

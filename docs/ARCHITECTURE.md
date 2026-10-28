# Architecture

A standalone desktop app that reads the user's ETS2 installation, assembles a truck from the original definitions and models, and shows it in a 3D tuning view with clickable slot hotspots. The game executable is never run or required.

## Layers

Each crate uses only crates in lower layers; `cargo xtask layers` checks it locally and in CI (D17).

```
layer 9   crates/specsheet      the app: viewer, pins, the game screen, jobs, dev script (Bevy, egui)
layer 8   specsheet-research    research tools (examples/), beside the app
layer 5   specsheet-render      game materials (clean-room WGSL), game lighting, checked GPU upload (Bevy)
layer 4   specsheet-session     GameSession / TruckSession orchestration, view (display data)
layer 3   specsheet-assembly    the built truck: placements, wheels, interior, hookups, pins
          specsheet-gameui      the game's UI files laid out
layer 2   specsheet-data        install, mounting, catalogs, models, scenes, render data, paint
layer 1   specsheet-truck       pure tuning domain: catalog, rules, configuration, paint jobs
          specsheet-formats     byte parsers: SII, BSII, ScsC, PMD, PMG, MAT; manifest writer
layer 0   specsheet-core        foundation: ids, maths (Aabb, Transform), sRGB conversion
          specsheet-archive     HashFS v2/v1, ZIP (read and write), CityHash, GDeflate, VFS
          specsheet-theme       design tokens for the app's own screens (D16)
```

| Crate | Responsibility | May depend on |
|---|---|---|
| `specsheet-core` | Foundation types every crate shares: ids, `Aabb`, `Transform`, the sRGB conversion. No domain logic | serde |
| `specsheet-theme` | Design tokens (colour roles in two appearances, type scale, spacing) with a WCAG contrast test | nothing |
| `specsheet-archive` | Read-only `.scs` and mod-package access: indexing, decompression, packed textures, layered VFS. Builds new ZIP mod packages (D18) | std, thiserror, flate2, gdeflate |
| `specsheet-formats` | Byte-level parsers for SII, BSII, ScsC, PMD, PMG, MAT; the `manifest.sii` writer; a scanner for paths files mention; content-type sniffing. No I/O | std, thiserror, flate2, aes, cbc |
| `specsheet-truck` | Domain types (`TruckCatalog`, `Accessory`, `Configuration`), compatibility rules, validation, paint jobs. No I/O, no game formats | specsheet-core, serde |
| `specsheet-data` | Install discovery and mounting; builds `TruckCatalog` from SII; models, scenes, render-ready meshes and textures, paint per part | layers 0–1 |
| `specsheet-assembly` | Assembles a configuration into placements; slot availability from model locators; the game's pin bases (exterior, wheels, interior) | layers 0–2 |
| `specsheet-gameui` | The game's UI screens, templates, colours and markup layout | layers 0–2 |
| `specsheet-session` | The game and truck sessions (options, selection, hookups, drive); `view` turns a session into display data (labels, pins, verdict reasons) | layers 0–3 |
| `specsheet-render` | Game materials on Bevy: clean-room WGSL shaders, the game's lighting, checked mesh and texture upload | layers 0–4, bevy |
| `specsheet-research` | Research tools that produce `research/evidence/` | layers 0–4 |
| `crates/specsheet` | The application: viewer, egui UI, background jobs, settings; holds no tuning rules | every library crate, bevy, bevy_egui |

Boundaries that must hold:
- `specsheet-truck` never sees file formats, and `specsheet-formats` never sees archives. This keeps the rules and parsers testable without a game install.
- The UI never decides compatibility. It renders what `specsheet-session::view` reports (`compatible` flags, reasons).
- Nothing writes into the game directory. Archives are opened read-only.

## Target structure (D17)

Where the crates are heading, in staged passes (D17). Each crate has one responsibility, and the arrows only point down.

- **Done (2026-10-28):** every crate in the tree below exists. `xtask` has `check`, `layers`, `guard`, `real`, `parity` and `version-port`.
- **Still open:** splitting `specsheet-data/src/catalog.rs` and `specsheet-truck/src/config.rs` into modules; bundling the theme's fonts.

```
crates/
  specsheet/            app: screens and wiring (Bevy, egui)
  specsheet-render/     materials, WGSL shaders, mesh and texture upload (Bevy)
  specsheet-theme/      design tokens for the app's own screens (D16), no dependencies
  specsheet-session/    what is being edited: session, view (labels, markers, prices)
  specsheet-assembly/   the built truck: placements, wheels, interior, cockpit, hookups, pins
  specsheet-gameui/     the game's UI files laid out (independent of trucks)
  specsheet-truck/      the tuning domain: catalog, configuration, rules, paint jobs, hookups (was specsheet-core)
  specsheet-data/       loading game data: install, catalog/, models, scenes, paint
  specsheet-formats/    byte parsers: SII, BSII, ScsC, PMD/PMG, MAT; manifest writer
  specsheet-archive/    archives: HashFS v2/v1, ZIP (read and write), packed textures, VFS
  specsheet-core/       foundation: ids, maths (Vec3, Transform, Aabb), colour, units; no domain logic
  specsheet-research/   the research tools now spread over three crates' examples/
xtask/                  repository tasks: check, layers, guard, real, parity, version-port
```

| Question | Crate |
|---|---|
| Which trucks and parts exist? | `specsheet-data` |
| What is selected, and is it allowed? | `specsheet-truck` (pure, no game files) |
| Where does each part go in 3D? | `specsheet-assembly` |
| What does it look like? | `specsheet-render` and the app |

Rules that keep it maintainable:
- **Layers are checked.** `cargo xtask layers` fails when a crate depends on a crate above it.
- **Bevy stays at the top.** Only `specsheet-render` and the app depend on Bevy, so a Bevy upgrade never touches the game logic.
- **Basic types live once,** in `specsheet-core`. Today `Aabb`, `Transform` and the colour conversions are spread over three crates.
- **Crates split by responsibility, not size.** A new crate needs a real boundary (D12, the AGENTS.md module rule).
- **Within crates,** a module grows into `module.rs` plus `module/` when it gets distinct parts. Candidates now: `specsheet-data/src/catalog.rs` (1,017 lines) and the truck crate's `config.rs` (rules, selection, hookups, paint overrides, price).

## Data flow

1. **Startup.** `GameInstall::discover()` (env `ETS2_DIR` or Steam libraries) → `mount()` opens about 110 archives (≈0.7 s) into a `Vfs`.
2. **Truck list.** Directories under `/def/vehicle/truck/`; display names come from `data.sii` `info[]`.
3. **Truck catalog.** Every `*.sii` under the truck directory becomes a `specsheet_truck::Accessory`. Slot comes from the directory; kind from the unit class.
4. **Initial configuration.** `/def/vehicle/truck_desktop/<truck>.sii` preset; repaired by the core rules.
5. **Assembly.**
   - The chassis and cabin models (with their variant and look) form the base.
   - The base's visible locators define `SlotAvailability` and the exterior pin bases; the cockpit's locators give the interior pin bases.
   - Each selected addon's model is placed at its slot's locator(s).
6. **Rendering.** `specsheet-data::render` converts PMG pieces to vertex arrays and resolves materials to textures (`.mat` → `.tobj` → packed texture). The app validates them and uploads them as Bevy meshes, `StandardMaterial`s and BC-compressed images.
7. **Interaction.**
   - Clicking a hotspot asks the backend for the slot's options, each with a compatibility verdict.
   - Selecting an option calls `Configuration::select`. The returned `ChangeSet` tells the UI which slots to reload.

## Caching and threading

- Archive indexes stay in memory. File contents are read on demand with positional reads, so readers can run concurrently.
- Parsed models and decoded textures are cached by game path in `specsheet-data`, behind a lock, and shared across trucks.
- File parsing never runs on the UI thread. Opening the game, loading a truck, selections and drive changes run as one job at a time on Bevy's async compute pool. The `TruckSession` moves into the job and comes back with the result. Models load as separate tasks, in parallel. Slot options are computed in place because they parse nothing.
- The opened game directory and the last truck are remembered in `settings.json` in the app config directory (`%APPDATA%\dev.specsheet.app`). The directory is tried before discovery. Discovery checks `ETS2_DIR`, Steam's `libraryfolders.vdf` and common library folders on every drive.

## Further reading

- [DECISIONS.md](DECISIONS.md): why each choice was made.
- [ASSET_FORMATS.md](ASSET_FORMATS.md) and [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md): the evidence.
- [TUNING_SYSTEM.md](TUNING_SYSTEM.md): the rules.
- [RENDERING.md](RENDERING.md): the viewer.

# Architecture

A standalone desktop app that reads the user's ETS2 installation, assembles a truck from the original definitions and models, and shows it in a 3D tuning view with clickable slot hotspots. The game executable is never run or required.

## Layers

```
┌──────────────────────────────────────────────────────────────┐
│ crates/specsheet   Bevy 0.19 + bevy_egui (Rust)                 │  3D view, hotspots, sidebar,
│                                                              │  background jobs, dev script
├──────────────────────────────────────────────────────────────┤
│ specsheet-data       ETS2 install → domain + runtime meshes       │  discovery, mount order,
│                                                              │  def loading, assembly, views
├───────────────────────────────┬──────────────────────────────┤
│ specsheet-core (pure)            │ specsheet-formats (pure)           │
│ catalog, rules, configuration │ SII, PMD, PMG, MAT parsers   │
├───────────────────────────────┴──────────────────────────────┤
│ specsheet-archive    HashFS v2 reader, CityHash, GDeflate, VFS     │
└──────────────────────────────────────────────────────────────┘
```

| Crate | Responsibility | May depend on |
|---|---|---|
| `specsheet-archive` | Read-only `.scs` and mod-package access: indexing, decompression, packed textures, layered VFS. Builds new ZIP mod packages (D18) | std, thiserror, flate2, gdeflate |
| `specsheet-formats` | Byte-level parsers for SII, BSII, ScsC, PMD, PMG, MAT; the `manifest.sii` writer; a scanner for paths files mention. No I/O | std, thiserror, flate2, aes, cbc |
| `specsheet-core` | Domain types (`TruckCatalog`, `Accessory`, `Configuration`), compatibility rules, validation. No I/O, no game formats | serde |
| `specsheet-data` | Install discovery and mounting; builds `TruckCatalog` from SII; resolves slot availability from model locators; assembles render-ready meshes; `view` turns a session into display data (labels, markers, verdict reasons) | all of the above |
| `crates/specsheet` | The application: Bevy renderer, egui UI, background jobs, settings. Converts `specsheet-data` meshes and textures to GPU assets; holds no tuning rules | specsheet-data, specsheet-core, bevy, bevy_egui |

Boundaries that must hold:
- `specsheet-core` never sees file formats, and `specsheet-formats` never sees archives. This keeps the rules and parsers testable without a game install.
- The UI never decides compatibility. It renders what `specsheet-data::view` reports (`compatible` flags, reasons).
- Nothing writes into the game directory. Archives are opened read-only.

## Target structure (D17)

Where the crates are heading. Code moves there in one coordinated pass, after the paint and `dif.spec` materials land (TASKS.md). Each crate has one responsibility, and the arrows only point down.

```
crates/
  specsheet/            app: screens and wiring (Bevy, egui)
  specsheet-render/     materials, WGSL shaders, mesh and texture upload (Bevy)
  specsheet-theme/      design tokens for the app's own screens (D16), no dependencies
  specsheet-session/    what is being edited: session, view (labels, markers, prices)
  specsheet-assembly/   the built truck: placements, wheels, interior, cockpit, hookups, anchors
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
3. **Truck catalog.** Every `*.sii` under the truck directory becomes a `specsheet_core::Accessory`. Slot comes from the directory; kind from the unit class.
4. **Initial configuration.** `/def/vehicle/truck_desktop/<truck>.sii` preset; repaired by the core rules.
5. **Assembly.**
   - The chassis and cabin models (with their variant and look) form the base.
   - The base's visible locators define `SlotAvailability` and hotspot anchors.
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

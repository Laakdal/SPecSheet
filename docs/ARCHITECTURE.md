# Architecture

A standalone desktop app that reads the user's ETS2 installation, assembles a truck from the original definitions and models, and shows it in a 3D tuning view with clickable slot hotspots. The game executable is never run or required.

## Layers

```
┌──────────────────────────────────────────────────────────────┐
│ app/specsheet   Bevy 0.19 + bevy_egui (Rust)                 │  3D view, hotspots, sidebar,
│                                                              │  background jobs, dev script
├──────────────────────────────────────────────────────────────┤
│ game-data       ETS2 install → domain + runtime meshes       │  discovery, mount order,
│                                                              │  def loading, assembly, views
├───────────────────────────────┬──────────────────────────────┤
│ tuning-core (pure)            │ scs-formats (pure)           │
│ catalog, rules, configuration │ SII, PMD, PMG, MAT parsers   │
├───────────────────────────────┴──────────────────────────────┤
│ scs-archive    HashFS v2 reader, CityHash, GDeflate, VFS     │
└──────────────────────────────────────────────────────────────┘
```

| Crate | Responsibility | May depend on |
|---|---|---|
| `scs-archive` | Read-only `.scs` access: indexing, decompression, packed textures, layered VFS | std, flate2, gdeflate |
| `scs-formats` | Byte-level parsers for SII, PMD, PMG, MAT. No I/O | std |
| `tuning-core` | Domain types (`TruckCatalog`, `Accessory`, `Configuration`), compatibility rules, validation. No I/O, no game formats | serde |
| `game-data` | Install discovery and mounting; builds `TruckCatalog` from SII; resolves slot availability from model locators; assembles render-ready meshes; `view` turns a session into display data (labels, markers, verdict reasons) | all of the above |
| `app/specsheet` | The application: Bevy renderer, egui UI, background jobs, settings. Converts `game-data` meshes and textures to GPU assets; holds no tuning rules | game-data, tuning-core, bevy, bevy_egui |

Boundaries that must hold:
- `tuning-core` never sees file formats, and `scs-formats` never sees archives. This keeps the rules and parsers testable without a game install.
- The UI never decides compatibility. It renders what `game-data::view` reports (`compatible` flags, reasons).
- Nothing writes into the game directory. Archives are opened read-only.

## Data flow

1. **Startup.** `GameInstall::discover()` (env `ETS2_DIR` or Steam libraries) → `mount()` opens about 110 archives (≈0.7 s) into a `Vfs`.
2. **Truck list.** Directories under `/def/vehicle/truck/`; display names come from `data.sii` `info[]`.
3. **Truck catalog.** Every `*.sii` under the truck directory becomes a `tuning_core::Accessory`. Slot comes from the directory; kind from the unit class.
4. **Initial configuration.** `/def/vehicle/truck_desktop/<truck>.sii` preset; repaired by the core rules.
5. **Assembly.**
   - The chassis and cabin models (with their variant and look) form the base.
   - The base's visible locators define `SlotAvailability` and hotspot anchors.
   - Each selected addon's model is placed at its slot's locator(s).
6. **Rendering.** `game-data::render` converts PMG pieces to vertex arrays and resolves materials to textures (`.mat` → `.tobj` → packed texture). The app validates them and uploads them as Bevy meshes, `StandardMaterial`s and BC-compressed images.
7. **Interaction.**
   - Clicking a hotspot asks the backend for the slot's options, each with a compatibility verdict.
   - Selecting an option calls `Configuration::select`. The returned `ChangeSet` tells the UI which slots to reload.

## Caching and threading

- Archive indexes stay in memory. File contents are read on demand with positional reads, so readers can run concurrently.
- Parsed models and decoded textures are cached by game path in `game-data`, behind a lock, and shared across trucks.
- File parsing never runs on the UI thread. Opening the game, loading a truck, selections and drive changes run as one job at a time on Bevy's async compute pool. The `TruckSession` moves into the job and comes back with the result. Models load as separate tasks, in parallel. Slot options are computed in place because they parse nothing.
- The opened game directory and the last truck are remembered in `settings.json` in the app config directory (`%APPDATA%\dev.specsheet.app`). The directory is tried before discovery. Discovery checks `ETS2_DIR`, Steam's `libraryfolders.vdf` and common library folders on every drive.

## Further reading

- [DECISIONS.md](DECISIONS.md): why each choice was made.
- [ASSET_FORMATS.md](ASSET_FORMATS.md) and [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md): the evidence.
- [TUNING_SYSTEM.md](TUNING_SYSTEM.md): the rules.
- [RENDERING.md](RENDERING.md): the viewer.

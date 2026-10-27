# Roadmap

**Goal:** a 1:1 replica of the game's own truck tuning for every truck: cabin, interior, chassis, wheels and every accessory slot the game ships, behaving as it does in game. Phases and milestones are below. What shipped is in [CHANGELOG.md](../CHANGELOG.md); day-to-day state is in [TASKS.md](../TASKS.md).

## Phase 1: Read the game (done)
- HashFS v2 archives, layered VFS, mount order.
- SII (with includes), PMD, PMG, MAT, packed textures, localisation.

## Phase 2: Tuning model (done, prototype)
- Truck catalogs, compatibility rules, presets, slot↔locator availability.
- Assembly of chassis, cabin, addons and wheels.

## Phase 3: Standalone viewer (done, prototype)
- Desktop app (Tauri + three.js, replaced in Phase 4 by the all-Rust Bevy app, D11): truck picker, 3D viewport, hotspots that follow the camera, sidebar options, live re-assembly, LHD/RHD.
- The game's tuning scenes (garage and the other nine), with camera, lights and fog.

## Phase 4: Every slot, as in game
- **Accessories:** addon hookups (accessories on accessories, e.g. horns and beacons on light bars), `overrides[]`.
- **Interior:** `interior_model[_uk]`, the interior slots, an interior camera, `hide_in` semantics.
- **Chassis and wheels:** rear-axle variants, lift axles, rim `tire_shift`, every wheel slot.
- **Paint:** paint-job masks and textures, flake/flip colours, custom colour.

## Phase 5: Game-faithful UI and look
- **Hotspots:** which slots get a pin and where, matched to the game (REA on the `accpins` code, finding F5). Replaces Design D7.
- **UI:** the layout of `ui/truck_configuration_center.sii`, the game's accessory icons (`/material/ui/accessory/*`) and categories.
- **Look:** normal maps (BC5), reflections and cube maps, closer to SCS shaders; scene animations.

## Phase 6: Evidence and robustness
- REA confirmation of the slot→locator rule, defaults/repair behaviour and pin placement.
- Save and load configurations, and import from a save game.
- Community mods (HashFS v1 / ZIP `.scs`): opt-in only.

## Phase 7: Distribution
- Release builds (NSIS), a remembered game-directory picker, error reporting.

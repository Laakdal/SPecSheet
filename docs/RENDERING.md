# Rendering

How the app turns game data into the 3D tuning view. What the game files contain is in [ASSET_FORMATS.md](ASSET_FORMATS.md) and `formats/`. These pages cover how the app uses them:

- [app/materials.md](app/materials.md): `.mat` effects to Bevy materials, the truck paint material, texture upload.
- [app/shaders.md](app/shaders.md): which game shaders the trucks use, why the look isn't 1:1 yet, and the status of each clean-room material.
- [app/shader-specs/](app/shader-specs/): the clean-room specs (D13) the WGSL is written from: `truckpaint`, `truckpaint-lighting`, `dif-spec`, `dif-spec-mult`, `baked`, `showroom-v2`, `glass`, `lamp`, `dif-lum`, `unlit`, `shadows-and-fake-lights`, `tonemap`.
- [app/scenes.md](app/scenes.md): the `/def/desktop` scenes, camera, lights, fog and orbit controls.
- [app/hotspots.md](app/hotspots.md): the game's pins (exterior, wheel and interior), their fade by facing, and the disc-linked wheel pins.
- [app/ui.md](app/ui.md): the egui layout, sidebar and controls.

## Pipeline

```
specsheet-assembly::assemble   →  placements (model, look, variant, transform) + pin bases
specsheet-data::render::build_mesh   →  MeshData (visible parts, CCW indices, material info)
specsheet-data::render::load_texture →  TextureData (unpacked mips, BGRA→RGBA)
specsheet-session::view::overview       →  hotspots, slot list, paint colour, warnings
crates/specsheet-render/src/assets.rs →  validated Bevy meshes, StandardMaterials, BC images
crates/specsheet-render/src/       →  clean-room WGSL materials (truck paint, dif.spec), game lighting
crates/specsheet/src/viewer.rs     →  truck and scene entities, lights, fog, orbit camera
crates/specsheet/src/ui/           →  the game's tuning screen, egui panels and hotspot markers
```

`assets::load_model` runs `build_mesh` and the texture loads on Bevy's async compute pool. The upload happens on the main thread (`viewer::finish_models`).

### Coordinates

SCS truck space is x right, y up, −z forward. It is right-handed with y up, the same as Bevy, so placements are used unchanged. Locator rotations are stored `(w, x, y, z)` and converted to `(x, y, z, w)` in `specsheet_core::Transform`. The scene's truck pose (`truck_rot`, degrees) is applied as intrinsic XYZ Euler angles.

### Assembly

- **Base models:** chassis and cabin are placed at the origin.
- **Addons:** placed at every visible base locator of their slot.
- **Interior:** the selected interior's light exterior model (`ext_model`) is placed at the cabin's `ext_interior` locator (F10).
- **Wheel parts:** placed at the `wheel_f*` / `wheel_r*` chassis locators, using the model variant `left` for x < 0 and `right` otherwise (`assembly::wheel_placements`).
  - Wheel models have `left`/`right` parts (Confirmed).
  - Choosing the side from the locator's x sign is a Hypothesis.
  - Tyres (`accessory_wheel_data`) are moved along x by the axle's rim/disc `tire_shift`: `+tire_shift` on the left, `−tire_shift` on the right, so positive values move the tyre inboard. Hub-face parts (`accessory_rim_data`) stay at the locator. This is finding F9, Hypothesis (strong). The `real_install` test `every_preset_wheel_sits_in_its_tyre` checks that every preset wheel's disc lip meets its tyre bead within 6 mm (104 wheels).
  - Wheels take only the locator's position and scale. 11 of 562 wheel locators carry a 90° or 180° yaw, which would turn the wheel sideways (F9, Hypothesis).
  - `wheel_shift` (3 DLC tyres) is not applied (F9, Unknown).

### Geometry conventions (Confirmed)

- **Winding:** game triangles are clockwise. 99.3% of 4.85M sampled triangles have a geometric normal opposite to the stored vertex normal (`research/evidence/geometry-survey.txt`). `build_mesh` swaps the 2nd and 3rd index once, so Bevy's default counter-clockwise front faces and back-face culling apply.
- **UVs and textures:** used as stored, with no V flip (D3D convention, which wgpu shares). The "FH16" lettering on the Volvo FH sun visor reads correctly, which confirms orientation.

## Caching

- Models are cached per (path, look, variant, parts); textures per path. Instances share their model's meshes and materials.
- Placements are diffed by model and transform, so a selection only spawns and despawns the parts that changed.

## Performance (Scania S preset, release build)

| Step | Time / size |
|---|---|
| Mount 113 archives | ≈0.6 s |
| Catalog load | ≈0.25 s |
| Assembly | 30 ms |
| 56 unique models (140k triangles) | 0.22 s |
| 39 textures (5 MiB) | 70 ms |

These timings are from the backend, measured before the Bevy port. In the Bevy app, the truck is on screen in a few seconds from launch, game mount included.

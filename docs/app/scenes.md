# Tuning scenes, lights and camera

The app opens on the game's own truck configuration scene, `/def/truck_config_scene.sii`. That is the `config_path` of the `ui_truck_scene` widget in `/ui/truck_configuration_bcg.sii`, loaded by `specsheet-data::scene::tuning_scene`. It is the first entry of the scene picker ("Truck configuration"):

- **Model and lights:** `desktop_garage` with its lights, read like the hub scenes below.
- **Orbit camera** (`OrbitSpec`): aim (0, 2.6, −1.465), FOV 50, default azimuth −2.516, elevation −0.068, zoom 1, elevation limits (−0.35, 0), `relative_bounds` 7.5, plus sensitivities.
  - Hypothesis: azimuth turns from +z towards +x, elevation is the pitch above the aim (negative lowers the camera), and the distance is zoom × the x bound. That gives a front-left three-quarter view about 7.5 m out, about 0.5 m below the aim, looking slightly up at the cab.
  - `viewer::Orbit::from_spec` takes the scene's lowest angle (20° below the aim) but lets the camera rise to nearly overhead, and keeps it at least 0.3 m above the floor (ours). The game's own range (between the aim's height and 20° below) was too tight for the user.
  - The opposite sign (the camera above the aim) was our first reading; the user found that view too high.
  - Non-finite values in the definition fall back to defaults; they never reach `f32::clamp`.
- **Pins:** the file's per-slot pin data (`pin_group`/`pin_look`/`pin_offset`/`pin_dir`/`pin_dir_uk`/`double_sided`, `interior_only`, `hookups_exterior_only`) is parsed into `TuningScene::pins`. Placement still uses our anchors ([hotspots.md](hotspots.md)); how the game applies these fields is F5's open question.

The hub ("desktop") scenes from `/def/desktop/config*.sii` follow in the picker (finding F7). A peer note says they may be hub backgrounds rather than the tuning garage; see [../PARITY.md](../PARITY.md).

`viewer::apply_scene` applies a scene. Each piece falls back on its own: with no camera the current one is kept, and a scene model that fails to load leaves the plain floor.

| Piece | Source | In the app | Status |
|---|---|---|---|
| Scene model | `scene_model_path`, look, variant, position; only the parts in `part_order[]` (the `coll` collision part is skipped) | Loaded like a truck model, under `SceneRoot` | Rendered |
| Camera | Static `camera_position`/`camera_aim`/`camera_fov`, then frame 0 of each `scene_cinematic_camera` animation. The first pose more than 5 m from the truck whose view ray passes within 5 m of it is used, with the target moved to the closest point on that ray. | Orbit camera starts at that pose; FOV is vertical, as in Bevy | Design: the game's choice between the two is unknown |
| Sun | `light_direction` (direction travelled), `light_diffuse` | `DirectionalLight`, colour = diffuse / its peak, illuminance = peak × π lux; the game materials take it as is ([materials.md](materials.md)) | Game units; stand-ins keep Bevy's shading |
| Ambient | `light_ambient` | `GlobalAmbientLight` colour, 1 cd/m² | Game units; stand-ins keep Bevy's shading |
| Environment | `light_environment` | The truck's most common reflection cube as a `GeneratedEnvironmentMapLight` (gradient until it loads), intensity = value × 0.5 (capped at 4) × an exposure balance ([materials.md](materials.md)) | Approximation: the cube is the game's, its exposure is ours |
| Fog | `fog_color`, `fog_density` | `DistanceFog` with `ExponentialSquared` falloff | Rendered |
| Truck pose | `truck_position`, `truck_rot` (degrees) | `TruckRoot` transform; markers follow it | Rendered |
| Not used | `floor_reflection`, `pre_exposure`, scene animations (`.pma`), `vehicle_lights_on`, `truck_steering_rot` | | |

**Light units are the game's.** Bevy's camera exposure is set to a factor of 1 (`UNIT_EXPOSURE`), and `SUN_LUX` = π cancels Bevy's 1/π diffuse, so a Bevy-lit stand-in gets the game's `Camb + Csun·N·L` like the game materials do. `ENVIRONMENT_NITS` (0.5) is ours. `light_specular` and the vehicle reflection keys reach the game materials ([materials.md](materials.md)).

**Pre-exposure.** `Scene::limited_pre_exposure` is the scene's `pre_exposure`, else derived from its lights (0.0973 for the tuning screen; Confirmed, `exposure-tonemap-data.txt` §6). It scales only emissive inputs (lamps, backlights, flares); the sun and ambient reach the materials unscaled (§9 there). The flares use it (below). An A/B on the Scania S Vortex agrees: scaling the lights by it, with the exposure compensated, washed out the garage and turned the paint into a mirror.

**Sun shadows.** The sun casts real-time shadows (two cascades to 40 m, ours). Casters follow the game's data (shadows-and-fake-lights.md §1; Confirmed for the 1.60 models by 71):
- `shadowonly` pieces, which are never drawn in colour (`specsheet_render::ShadowOnlyMaterial`, front faces culled; the `nocull` and `.a` variants are not told apart yet), and materials with the `shadow` flavour (`MaterialInfo::casts_shadow`);
- nothing else: every other piece gets `NotShadowCaster`. The garage models have no casters, so the shadow falls from the truck onto itself and the floor. The dark patches under the truck are baked decals, not this shadow.

In the game materials the shadow scales only the sun's diffuse and specular, not the ambient or the reflection (dif-spec §9, `sun_visibility` in `game_lighting.wgsl`). Whether the game draws real-time sun shadows on the tuning screen at all is Unknown: the UI scene sets no shadow parameter (`rea-ui-scene-update.txt`). A reference screenshot settles it.

**Vehicle light flares** (`specsheet/src/flares.rs`, [shader-specs/flare.md](shader-specs/flare.md), F19). Each light locator of a placed model whose unit has a glow model gets the game's flare model (`/model/flare/*.pmd`), drawn as the spec's §3: unlit, `B·Kd·2c`, additive, no depth write, times `10·p` (`p` = the scene's pre-exposure; the per-draw object `o` is ours, 1). It shows only in scenes with `vehicle_lights_on`, which the tuning screen's own scene does not set. Ours, each a Hypothesis:
- **Which lights:** `parking` and `low_beam` (flare.md §5: the scene's light mask is not decoded).
- **Facing and size:** the quad turns to the camera, `default_scale` metres, growing to `default_scale × scale_factor` between the scaling distances.
- **`visual_offset`:** towards the camera, out of the lamp housing. Along the locator's forward axis (the spec's reading) put a headlight's glow 0.6 m in front of the lamp, beside the truck in a three-quarter view.
- No cone fade for `narrow` lights, no soft depth fade, no fog.

**Tonemap.** Bevy's tonemapper is off and the camera is HDR. `specsheet_render::GameTonemap` runs the game's `def.tonemap` pass (written clean-room from [shader-specs/tonemap.md](shader-specs/tonemap.md) §4): low-light tint, exposure `k = T0.x·E`, optional colour correction, then the tone curve with its hue handling, into the sRGB target. The UI is drawn after it and is unaffected. Ours, each a Hypothesis:
- **Fixed exposure** `k` = 0.18 × 5.555 = 1, the `black`/`integrity` climates' fixed exposure. The game adapts instead: the UI scene's per-frame update sets no exposure or tonemap parameter, so the histogram eye adaptation (§3) runs on the tuning screen (Confirmed, `research/evidence/rea-ui-scene-update.txt`). Its limits, speeds and `T0.x` are the defaults or the last climate's, values Unknown. Eye adaptation is not built yet. The dev script's `exposure <E>` changes E.
- **Identity curve** (the spec's §7 test vector): colours up to white pass unchanged, and brighter ones desaturate towards white per §4.5. How the engine builds the curve from `contrast` and `shoulder_length` is Unknown.
- No bloom, no low-light tint, no colour correction. Night scenes (Halloween Fields, Night City) are dark because their definitions are: Halloween Fields has ambient 0.04 and sun 0.15 with vehicle lights on, which we don't render yet.

## Defaults

Before a scene applies, and for pieces a scene omits:

- camera at (−6.5, 3.2, −8.5) looking at (0, 1.5, 0), 40° FOV;
- sun travelling along (6, −10, 8) at 1.6;
- dim blue-grey ambient;
- a dark floor disc of radius 14 m.

Without a scene camera, the orbit target follows the centre of the truck's bounds (`viewer::frame_truck`).

## Orbit controls

`viewer::Orbit` stores the target, yaw, pitch (elevation) and distance.

- **Left drag** rotates.
- **Right or middle drag** pans: one pixel of drag moves the target by about one pixel on screen.
- **The wheel** zooms by 10 % per notch.
- **Limits:** pitch stays between 0.005π and just under π/2, so the camera never goes below the floor. Distance stays between 2.5 m and 25 m, or the scene camera's distance if that is farther.
- A drag only starts when the press lands on the 3D view, not on a panel or marker (`ViewportInput`, set by the UI each frame).
- **Cursor, not raw motion.** Drags follow the cursor inside the window. They end when the window loses focus or the cursor leaves it, so a button release the window never sees cannot leave the camera following the mouse. (Raw motion did, and could fling the camera off into the dark.)
- **Bounds.** The orbit target stays within `camera_relative_bounds` around the aim, with its height within `camera_aim_y_bounds` (Hypothesis). Scenes without bounds allow 10 m around their aim.
- **Home** restores the scene's view.

## Interior view

The Interior and Interior Accessories tabs look from the driver's seat (Design: the game's interior camera is assumed, until a reference screenshot confirms it).

- **Eye point:** the camera sits at `Assembly::cockpit`'s transform, the cabin's `ext_interior` locator, which is the driver's eye (F10, Hypothesis).
- **Looking around:** it starts looking ahead and slightly down (pitch −0.2). A drag turns the head, by up to ±2.6 rad of yaw and ±1.2 rad of pitch. There is no pan or zoom.
- **What is drawn:** the interior's full cockpit, and the interior-only accessories on its locators (`Assembly::interior_parts`: steering wheel, curtains, toys, glass sets, carpets at the eye point), replace the light exterior-view interior. The pins are those of the interior tab, with no far-side culling inside the cab.
- **Leaving:** switching to any other tab returns to the orbit camera unchanged.

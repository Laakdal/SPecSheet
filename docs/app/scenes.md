# Tuning scenes, lights and camera

The app opens on the game's own truck configuration scene, `/def/truck_config_scene.sii`. That is the `config_path` of the `ui_truck_scene` widget in `/ui/truck_configuration_bcg.sii`, loaded by `specsheet-data::scene::tuning_scene`. It is the first entry of the scene picker ("Truck configuration"):

- **Model and lights:** `desktop_garage` with its lights, read like the hub scenes below.
- **Orbit camera** (`OrbitSpec`): aim (0, 2.6, −1.465), FOV 50, default azimuth −2.516, elevation −0.068, zoom 1, elevation limits (−0.35, 0), `relative_bounds` 7.5, plus sensitivities.
  - Hypothesis: azimuth turns from +z towards +x, negative elevation raises the camera, and the distance is zoom × the x bound. That gives a front-left three-quarter view about 7.5 m out.
  - `viewer::Orbit::from_spec` uses the elevation limits as the pitch range, so the camera stays between the aim's height and about 20° above it.
  - Non-finite values in the definition fall back to defaults; they never reach `f32::clamp`.
- **Pins:** the file's per-slot pin data (`pin_group`/`pin_look`/`pin_offset`/`pin_dir`/`pin_dir_uk`/`double_sided`, `interior_only`, `hookups_exterior_only`) is parsed into `TuningScene::pins`. Placement still uses our anchors ([hotspots.md](hotspots.md)); how the game applies these fields is F5's open question.

The hub ("desktop") scenes from `/def/desktop/config*.sii` follow in the picker (finding F7). A peer note says they may be hub backgrounds rather than the tuning garage; see [../PARITY.md](../PARITY.md).

`viewer::apply_scene` applies a scene. Each piece falls back on its own: with no camera the current one is kept, and a scene model that fails to load leaves the plain floor.

| Piece | Source | In the app | Status |
|---|---|---|---|
| Scene model | `scene_model_path`, look, variant, position; only the parts in `part_order[]` (the `coll` collision part is skipped) | Loaded like a truck model, under `SceneRoot` | Rendered |
| Camera | Static `camera_position`/`camera_aim`/`camera_fov`, then frame 0 of each `scene_cinematic_camera` animation. The first pose more than 5 m from the truck whose view ray passes within 5 m of it is used, with the target moved to the closest point on that ray. | Orbit camera starts at that pose; FOV is vertical, as in Bevy | Design: the game's choice between the two is unknown |
| Sun | `light_direction` (direction travelled), `light_diffuse` | `DirectionalLight`, colour = diffuse / its peak, illuminance = peak × 1000 lux | Approximation |
| Ambient | `light_ambient` | `GlobalAmbientLight` colour, 1500 cd/m² | Approximation |
| Environment | `light_environment` | The truck's most common reflection cube as a `GeneratedEnvironmentMapLight` (gradient until it loads), intensity = value × 500 (capped at 4) × an exposure balance ([materials.md](materials.md)) | Approximation: the cube is the game's, its exposure is ours |
| Fog | `fog_color`, `fog_density` | `DistanceFog` with `ExponentialSquared` falloff | Rendered |
| Truck pose | `truck_position`, `truck_rot` (degrees) | `TruckRoot` transform; markers follow it | Rendered |
| Not used | `light_specular`, `floor_reflection`, `cube_reflection`, `vehicle_reflection`, scene animations (`.pma`), `vehicle_lights_on`, `truck_steering_rot` | | |

The light units are our own. The camera uses `Exposure::BLENDER` and ACES tone mapping. The light factors in `viewer.rs` (`SUN_LUX`, `AMBIENT_NITS`, `ENVIRONMENT_NITS`) reproduce the balance of the earlier three.js viewer; how the game scales its light values is Unknown. Night scenes (Halloween Fields, Night City) are dark because their definitions are: Halloween Fields has ambient 0.04 and sun 0.15 with vehicle lights on, which we don't render yet.

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

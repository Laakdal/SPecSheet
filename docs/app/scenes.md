# Tuning scenes, lights and camera

The viewer loads the game's own scenes from `/def/desktop/config*.sii` through `specsheet-data::scene` (finding F7). The default garage ("Home at Last") is listed first, and the scene picker lists all 10 scenes found in 1.60. A peer note says these may be hub ("desktop") backgrounds rather than the tuning garage itself; see [../PARITY.md](../PARITY.md).

`viewer::apply_scene` applies a scene. Each piece falls back on its own: with no camera the current one is kept, and a scene model that fails to load leaves the plain floor.

| Piece | Source | In the app | Status |
|---|---|---|---|
| Scene model | `scene_model_path`, look, variant, position; only the parts in `part_order[]` (the `coll` collision part is skipped) | Loaded like a truck model, under `SceneRoot` | Rendered |
| Camera | Static `camera_position`/`camera_aim`/`camera_fov`, then frame 0 of each `scene_cinematic_camera` animation. The first pose more than 5 m from the truck whose view ray passes within 5 m of it is used, with the target moved to the closest point on that ray. | Orbit camera starts at that pose; FOV is vertical, as in Bevy | Design: the game's choice between the two is unknown |
| Sun | `light_direction` (direction travelled), `light_diffuse` | `DirectionalLight`, colour = diffuse / its peak, illuminance = peak × 1000 lux | Approximation |
| Ambient | `light_ambient` | `GlobalAmbientLight` colour, 1500 cd/m² | Approximation |
| Environment | `light_environment` | Gradient `EnvironmentMapLight`, intensity = value × 500 (capped at 4) | Approximation: the game uses its own cube reflections |
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
- A drag only starts when the press lands on the 3D view, not on a panel or marker (`ViewportInput`, set by the UI each frame). Orbit input reads Bevy's raw mouse motion.

# Shader spec: `eut2.flare` (vehicle light flares)

This spec covers the glow sprites ETS2 1.60.1.7 draws at a vehicle's light locators when a light is on. A model locator's hookup names a `flare_vehicle` unit (e.g. `flare.vehicle.headl`), and that unit names a flare model (e.g. `/model/flare/uniform_white_amplified.pmd`) whose material uses `eut2.flare`. It was written clean-room under D13: the maths was read from the game's shaders and is restated in our own notation, with no shader code or names copied. Sources: `research/evidence/shader-spec-flare.txt`.

Labels: **C** = Confirmed (read in the shader, the effect tables, the executable or game data), **H** = Hypothesis, **U** = Unknown. Fog is [dif-spec.md](dif-spec.md) §5.7; the material luminosity rule is [dif-lum.md](dif-lum.md) §6.

## 1. What is drawn (C)

- **The flare model** is a single quad of 1 × 1 m in its own xy plane (z = 0), with one material.
  - `/model/flare/uniform_white_amplified.pmd`: bounds −0.5…0.5 in x and y.
  - Its material: effect `eut2.flare`, `diffuse` (1, 1, 1), `luminance_output` 10000, `luminance_night` 10, `texture_base` `/asset/flare/flare.tobj`.
- **The shaders don't orient the quad.** The vertex stage is a plain model transform with no camera-facing term. So turning the quad to the camera, sizing it and fading it with angle all happen on the CPU (§4).
- **The effect** `eut2.flare` declares itself as `eut2.particle.add`. Its inputs are `diffuse`, `luminance_output`, `luminance_night` and `texture_base` (the effect's interface table).

## 2. Passes (C)

| Pass | Render state | Content |
|---|---|---|
| forward blend | additive: `dst + src·α`; depth test "greater", no depth write; back faces culled | §3, no soft fade |
| deferred blend | the same state | §3 with the soft depth fade of §3 step 4 |

There is no occlusion query in the effect. Whether the engine hides a flare behind geometry by other means (a CPU visibility test) is U; the depth test hides the parts of the quad behind opaque surfaces.

## 3. Maths (C)

With `B` = `texture_base` on UV0, `c` the vertex colour, `Kd` = `diffuse`, and `ML` the material luminosity:
1. `rgb = B.rgb · Kd · 2·c.rgb · ML.rgb · ML.a`.
2. `α = B.a · c.a`.
3. Fog (dif-spec §5.7): `rgb ← Cfog + τ·(rgb − Cfog)`.
4. **Deferred pass only, soft depth fade:** `α ← α · sat(2·(z_flare − z_scene))`, with eye-space depths (the eye looks down −z; `z_scene` is the opaque surface's depth from the G-buffer). The flare fades out over the last 0.5 m before the surface behind it. Where no surface was recorded, the fade is 1.
5. Output `(rgb, α)`, blended additively: the screen gains `rgb·α`.

**Brightness.** `ML` follows the engine's luminosity rule (dif-lum.md §6): `ML.a = s · o.a` and `ML.rgb = o.rgb`, where:
- `s = 10000 · p · 0.001 = 10·p` for this material by day, with `p` the scene's `limited_pre_exposure`: about 0.97 on the tuning screen, 10 in the world. At night it blends in log space towards the `luminance_night` 10 nits.
- `o` is an engine object per draw (U). (H) It is the flare instance's own state: its on/off fade (`state_change_duration`) and its angle fade.
- **Stand-in (our choice):** `o = (1, 1, 1, 1)`, so `rgb = B.rgb · Kd · 2·c.rgb · 10·p`.

## 4. The `flare_vehicle` keys (engine-side)

The engine stores these keys in the `flare_vehicle` class (attribute table, C: `flare_inner_angle` +0xbc, `flare_outer_angle` +0xc0, `scaling_start_distance` +0xc4, `scaling_end_distance` +0xc8, `scale_factor` +0xcc, `default_scale` +0xd0, `model` +0xd8, `visual_offset` +0x100, `state_change_duration` +0x104; `model_hdr` is accepted but not stored). The code that reads them was not found (U), so the meanings below are H, from the key names and the values in the game's units:

| Key | Values seen (47 units) | Meaning (H) |
|---|---|---|
| `light_type` | `parking`, `low_beam`, `high_beam`, `brake`, `left_blinker`, `right_blinker`, `reverse`, `aux`, `beacon`, `electrics`, `police_hooter` | Which light switches the flare on. The engine maps the name to a bit (C, table in the executable): `low_beam` 0x1, `left_blinker` 0x2, `electrics` 0x4, `beacon` 0x20, `high_beam` 0x40, `reverse` 0x80, `police_hooter` 0x100, `aux` 0x200, `brake` 0xc00, `parking` 0x2000, `dim_aux` 0x4000; `right_blinker` is listed without a readable value. (H) The flare shows when its bits meet the vehicle's light state. |
| `dir_type` | `wide`, `narrow` | How directional the light is: (H) `narrow` (headlights, aux, reverse) shows only within the cone of `flare_inner_angle`…`flare_outer_angle` degrees around the locator's forward axis; `wide` (position, brake, indicators) over a wide angle. |
| `flare_inner_angle`, `flare_outer_angle` | e.g. 7 (headlight) | (H) Full strength inside the inner angle, fading to zero at the outer. |
| `default_scale` | 0.405 (parking) … 1.53 (low beam) | (H) The quad's size in metres near the camera. |
| `scale_factor`, `scaling_start_distance`, `scaling_end_distance` | 2–6; 0 and 60 m (low beam) | (H) The size grows from `default_scale` to `default_scale × scale_factor` between the two camera distances, so flares stay visible far away. |
| `visual_offset` | 0.6 (low beam) | (H) Moves the quad this many metres out of the lamp housing. Along the locator's forward axis it lands 0.6 m ahead of the headlight, beside the truck in a three-quarter view (de, on screen), so towards the camera is the likelier reading. The app uses towards the camera (our choice, `docs/app/scenes.md`). |
| `state_change_duration` | 0.083 s (incandescent `.sui`) | (H) Fade time when the light switches; the include file's own comment says bulbs should warm up, but the system uses an almost instant change. |

## 5. Which lights a scene turns on

- A scene with `vehicle_lights_on: true` sets the render view's light mask to 0xd01b (C, `research/evidence/rea-ui-scene-update.txt`).
- Read with the `light_type` bits above, 0xd01b contains `low_beam` (0x1), `left_blinker` (0x2) and `dim_aux` (0x4000), plus 0x8, 0x10, 0x1000 and 0x8000, which have no readable name. One of those four is `right_blinker`. A scene that lights one indicator but not parking lights is implausible, so (H) the scene mask uses another numbering than `light_type`. **U** until a screenshot of a lit garage shows which lamps glow.
- **Stand-in (our choice):** with `vehicle_lights_on`, light `parking` and `low_beam` flares. That's what "lights on" means on a parked truck, and it's easy to change once the mask is decoded.

## Sources

- The game's `eut2.flare` effect (`effect/eut2/particle/eut2.flare.rfx`), its shaders, render state and interface table in `effect.scs`, read under D13. The list is in `research/evidence/shader-spec-flare.txt`.
- The flare model and its material, and the `flare_vehicle` units in `/unit/hookup/vehicle/flare/` (game data).
- `eurotrucks2.exe`, read statically: the `flare_vehicle` attribute table and the `light_type` name table (`research/evidence/shader-spec-flare.txt`).

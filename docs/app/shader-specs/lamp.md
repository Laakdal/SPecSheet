# Shader spec: `eut2.lamp`

This spec covers the ETS2 1.60.1.7 lamp effect: head, tail, indicator and marker lights on trucks and accessories. It is 5.0 % of all model uses, mostly `lamp.add.env` (809 models) and `lamp.tsnmapuv.add.env` (285) (`research/evidence/effect-usage-survey.txt`). It was written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-glass-lamp-lum.txt`.

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown.

**In one line:** a lamp is the `dif.spec.add.env` surface of [dif-spec.md](dif-spec.md) plus an emission term, the lamp glow, read from a mask texture and an engine colour table. With every light off, the glow is zero and the lamp is plain `dif.spec(.add.env)`. Notation and engine inputs are dif-spec's (§3, §5).

## 1. Variants (C)

| Token | Effect |
|---|---|
| `lamp` | base: dif.spec surface + glow (§3, §4) |
| `add.env` | adds the reflection of §5 |
| `tsnmap`, `tsnmapuv` | normal map, exactly dif-spec §5.2.2 (UV0, or UV slot 6) |
| `anim` | animated mask (frame sheets, `animated_lamp_frame_*` inputs); 9 models, not read |
| `shadow` | adds a shadow-map depth pass; colour unchanged |
| `skinned` | vertex skinning; out of scope |

## 2. Passes (C)

| Pass | Render state | Content |
|---|---|---|
| deferred attributes | G-buffer write, as dif-spec §7.1 | the surface |
| deferred post | additive (`dst + src·α`, α = 1), depth "greater or equal", no depth write | glow + reflection over the lit G-buffer (§6) |
| forward opaque | no blend | the whole equation of §3–§5 in one pass |

As in dif-spec, implement the forward maths as canonical.

## 3. Surface (forward, C)

Identical to plain `dif.spec` (dif-spec §5.3, first row, and §5.4), including the specular-mask remap `mremap` and the `sat(10·NL)` gate:
- `A = B.rgb · Kd · c2 · fn`, where `B` is `texture_base` on UV0;
- `S = sat(ms·B.a + mb) · Ks`;
- `surface = (Ld + Cadd·ka) · A + Ls · S`.

## 4. Glow (C)

Inputs:
- `texture_mask` (RGBA), sampled on UV slot 1, which is the material's second UV set (attribute 9; [dif-spec.md](dif-spec.md) §3.2).
- UV1 carries two things at once (C, vertex stage):
  - **tile** `k = floor(u1)`, the integer part of the u coordinate. The colour offset is `min(4k, 20)`, so tiles 0…5 are distinct and higher tiles act as tile 5.
  - **mask coordinate** `fract(u1, v1)`, the fractional parts.
- `Cl[0…23]`: the engine's lamp colour table, 24 rgb entries: 6 tiles × 4 channels. Engine-set (U).
- `γ`: the engine's lamp contrast, a scalar (U).

Steps:
1. `m = texture_mask(fract(uv1))`.
2. **Contrast:** `mr = m.r^γ`, `mg = m.g^γ`, `mb = m.b^γ`, `ma = m.a^(2.2·γ)`. The alpha channel gets 2.2 times the exponent.
3. **Glow colour:** `Lg = Cl[4k]·mr + Cl[4k+1]·mg + Cl[4k+2]·mb + Cl[4k+3]·ma`.
4. **Directionality:** `w = 0.125 + 0.875·nz²`, where `nz` is the eye-space z of the **interpolated vertex normal**, not normalised and not the normal-mapped one, even in `tsnmapuv`. A lamp facing the camera gets the full glow, and one seen edge-on gets 1/8.
5. `glow = Lg · w`. The glow is not multiplied by the base texture, the vertex colour or `diffuse`.

**What the tiles and channels mean** (the SCS wiki's lamp-system page, paraphrased; H for the engine side, since the table `Cl` is filled by the engine). For truck lights, the mask channels are:
- R: indicators;
- G: high beam on front tiles, brake on rear tiles;
- B: low beam (front), reverse (rear), daytime running light (tile 4);
- A: position light.

The tiles are 0 front-left, 1 front-right, 2 rear-left, 3 rear-right, 4 middle/DRL. The engine therefore writes, per tile and channel, the colour of that light when it is on and black when it is off.

## 5. Reflection (`add.env`, C)

As dif-spec §5.5 for plain `add.env` (linear fresnel `F = sat(fb + fs·(1 − N·R̂))`, `Cenv = cube(M·R)·Ke`, strength `g · B.a · F · sat(Aenv)`), with one difference: **there is no far stretch** of the cube coordinate.

## 6. Combination and output (C)

- **Forward:** `colour = surface + glow + Refl`, then the fog of dif-spec §5.7, applied to the sum. The output alpha `B.a·c.a` is written but not blended.
- **Deferred post:** adds `glow + Refl` on top of the composed surface, with `Evis` taken from the diffuse light buffer's alpha (dif-spec §7.4). The glow maths is identical; the directionality uses the same vertex normal z.

## 7. Engine inputs and stand-ins

| Input | Status | Stand-in (our choice, not the game's) |
|---|---|---|
| `Cl` (lamp colour table) | U: the engine writes it from the vehicle's light state | All zero: the tuning screen shows the truck with its lights off (H, to check against a screenshot). The `lamp_preview_color`/`lamp_preview_mask` `.mat` inputs exist for editor preview only. |
| `γ` (lamp contrast) | U | 1.0 |
| `g`, `mremap`, `M` | as dif-spec §3.3 | as dif-spec |

With the stand-ins, `glow = 0`, and lamps render exactly as `dif.spec.add.env` (or `dif.spec` without `add.env`).

**Next step:** add `lamp_color_array|lamp_contrast` to the parked REA regex. It shows how the engine fills the table and whether the tuning screen lights anything.

## Sources

- The game's lamp effects and shaders in `effect.scs` and the uniform table, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Lamp system](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Materials/Lamp_system), paraphrased, for the meaning of the mask channels and UV tiles.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders): a second mask texture marks the parts that light up when a light is on.

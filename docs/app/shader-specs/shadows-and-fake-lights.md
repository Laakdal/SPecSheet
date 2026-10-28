# Shader spec: shadow and fake-light helpers

This spec covers the ETS2 1.60.1.7 effects that add shadow or light to other surfaces, or are never drawn in colour. It was written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-glass-lamp-lum.txt`.

| Effect | Share of model uses | Section |
|---|---|---|
| `shadowonly` (+ `nocull`) | 16.3 % | §1 |
| `decalshadow` | 1.2 % | §2 |
| `fakeshadow` | 0.6 % | §3 |
| `light.tex` | 106 models | §4 |
| `none` | 104 models | §5 |

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown.

## 1. `shadowonly` (C)

There is one pass, into the sun's shadow map:
- depth only, colour writes off;
- depth test "greater", depth write on, depth clamping on;
- **front faces culled**; `nocull` culls nothing;
- the fragment stage does nothing.

The geometry is never visible. It only casts sun shadows (the wiki says the same).

**For our renderer:** never draw it in the main pass. When real-time sun shadows exist, use it as a shadow caster only.

## 2. `decalshadow` (C)

Baked contact shadows, such as the dark patch under a toy on the dashboard.

- **Passes:** a deferred "decal post" and a forward "decal". Both use the `decal_over` state: depth "greater or equal" with offset, no depth write, back faces culled, blend `src·α + dst·(1−α)`.
- **Colour:** black. **Opacity:** the **red** channel of `texture_base` on UV0: `α = B.r`.
- No `.mat` value, vertex colour or base alpha is used. The vertex stream table lists colour and normal for the effect, but the shader does not read them.
- **Forward fog:** the black is fogged, `rgb = (1 − τ)·Cfog`, with `τ` and `Cfog` from dif-spec §5.7. The deferred pass has no fog.

So the effect darkens what is under it: `dst ← dst·(1 − B.r)` without fog.

**The shader and the wiki disagree.** The wiki describes the colour as base × diffuse × vertex colour, and the opacity as vertex alpha × base alpha. Implement the shader. 37 of 68 truck `decalshadow` materials set `diffuse` (`material-survey.txt`), and it has no effect.

## 3. `fakeshadow` (C, engine use U)

- One pass, "fake shadow colour": depth "less" with depth write on, **front faces culled**, no blend.
- It writes a grey value `0.5 + 0.5·z` into all four channels, where `z` is the vertex's clip-space z from the uncorrected projection, interpolated and **not** divided by w.
- The engine uses it as the contour from which its shadow volumes start (the wiki: it outlines the vehicle and wheels). That use is U, not read.
- `a` and `nocull` variants exist.

**For our renderer:** never draw it in colour. It is not a visible surface.

## 4. `light.tex` (C, deferred only)

A textured fake light that adds light to the surfaces under it, e.g. the glow of an under-lit logo on the cab.

- **Pass:** "light source", into the deferred **light buffer**, not into colour. Blend `dst + src` (one, one). Depth "greater or equal" with offset, no depth write, back faces culled.
- The vertex stage pushes eye-space depth by `z' = z + z²·b`, with `b` = `.mat` `depth_bias`, before projecting.
- With the tinted texel `T = (2·c.rgb, c.a) ⊙ B` (`B` = `texture_base` on UV0), it adds:
  - to the diffuse light: `Kd ⊙ T.rgb` (`Kd` = `.mat` `diffuse`);
  - to the specular light: `Ks ⊙ T.rgb` (`Ks` = `.mat` `specular`);
  - to the reflection visibility (the diffuse buffer's alpha, dif-spec §7.4): `sat(e · T.a)`, where `e` comes from `env_factor`. Which component the engine passes is U.
- The deferred composition (dif-spec §7.2) then multiplies the added light by the underlying surface's albedo and specular colour. The effect is always on, day and night (the wiki says so too).

There is no forward pass. A forward renderer has no light buffer.
- **To reproduce it,** treat it as a projected light decal. Add `Kd⊙T.rgb · A_under + Ks⊙T.rgb · S_under` to the surface beneath.
- **Our choice for now:** skip it. It is a subtle glow on a few accessories.

`lsbounce`, `lsdirect` and `lslucent` variants exist (not read).

## 5. `none` (C)

The effect file has no technique, so nothing is drawn. Skip these materials.

## Sources

- The game's `shadowonly`, `shadowmap` (`decalshadow`), `fakeshadow`, `light_tex` and `none` effects and shaders in `effect.scs`, and their render states, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased, for the purpose of each effect and the `nocull` flavour.
- `research/evidence/material-survey.txt` for the `decalshadow` attribute counts.

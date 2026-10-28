# Shader spec: `eut2.unlit.*`

This spec covers the ETS2 1.60.1.7 unlit effects: `unlit.tex` and `unlit.vcol.tex`, with their blend flavours and `flipsheet`. They are rare on trucks: `unlit.vcol.tex.flipsheet.add` (29 models), `unlit.vcol.tex.add` (24), `unlit.vcol.tex.decal.over` (15) and fewer. `unlit.tex` appears in 25 UI-scene models (`research/evidence/effect-usage-survey.txt`). It was written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-glass-lamp-lum.txt`.

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown. Fog is dif-spec §5.7.

## 1. Maths (forward only, C)

With `B` = `texture_base` on UV0, `Kd` = `.mat` `diffuse`, and `ML` the engine's material luminosity (see §3):

| Effect | rgb before fog | Output alpha |
|---|---|---|
| `unlit.tex` | `B.rgb · Kd · ML.rgb · ML.a` | `B.a` |
| `unlit.vcol.tex` | `B.rgb · c2 · Kd · ML.rgb · ML.a` | `B.a · c.a` |

Then fog: `out.rgb = Cfog + τ·(rgb − Cfog)`. No sun, ambient or specular term, and no normal.

**The shader and the wiki disagree.** The wiki describes unlit as texture × diffuse (× vertex colour). The shader also multiplies by `ML.rgb · ML.a`. Implement the shader.

## 2. Flavours (C, from the render states read)

| Flavour | Pass and blend |
|---|---|
| none | one opaque pass, no blend |
| `add` | `dst + src·α` (`forward_add`), no depth write |
| `over`, `decal.over` | `src·α + dst·(1−α)`; `decal` adds depth "greater or equal" with offset (dif-spec §6) |
| `a` | alpha test (H: discard below 0.05 as dif-spec §6; not re-read) |
| `awhite` | not read (decal variants only) |
| `flipsheet` | animated sprite sheet: the vertex stage offsets UV0 by `f.xy · f.zw`, where `f` is an engine-computed frame vector (H: xy = the current frame's column and row, zw = one frame's width and height, from the `animsheet_*` `.mat` inputs and time) |

## 3. Engine inputs and stand-ins

| Input | Status | Stand-in (our choice, not the game's) |
|---|---|---|
| `ML` (material luminosity, from `luminance_output`, default 0) | U: the conversion and its behaviour at the default are engine-side. If the default reached the shader as 0, every unlit material without `luminance_output` would be black, so the engine probably substitutes a value (H). | `ML.rgb · ML.a = 1` |
| `f` (flipsheet frame) | U | frame 0: `f = 0` |

**Next step:** the REA regex for `material_luminosity` in [dif-lum.md](dif-lum.md) §6 settles this too.

## Sources

- The game's `unlit_tex` and `unlit_vcol_tex` effects and shaders in `effect.scs`, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: unlit surfaces are drawn after lighting with no lighting applied, and `vcol` adds the vertex colour and vertex alpha.

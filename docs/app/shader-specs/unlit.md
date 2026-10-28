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
| `ML` (material luminosity, from `luminance_output`) | C, below | `o = (1, 1, 1, 1)` |
| `f` (flipsheet frame) | U | frame 0: `f = 0` |

**Luminosity (C, `research/evidence/exposure-tonemap-data.txt` §9).** The engine computes `ML.a = s · o.a` and `ML.rgb = o.rgb`, where `o` is an engine object it holds per draw (contents not decoded; stand-in `o = (1, 1, 1, 1)`), and `s` comes from the material's luminance pair `(x, y)` = (`luminance_output`, `luminance_night`) (H for the pairing; the engine reads one two-value slot):

1. `x = 0` becomes 50 (nits); `y = 0` becomes `x`.
2. `t` = the scene's night blend, 0 on the tuning screen (C).
3. `v = x` if `x = y`; otherwise `exp(lerp(ln x, ln y, t))` for `x > 0` and `lerp(x, y, t)` for `x ≤ 0`.
4. **`x > 0` (physical nits):** `s = v · p · 0.001`, with `p` the scene's `limited_pre_exposure` (0.0973 on the tuning screen, 1 in the world).
5. **`x ≤ 0` (relative):** `s = |v|`, with no pre-exposure. The usual −1 gives `s = 1`.

So the garage-scene materials with −1 draw at full strength, and the default 0 means 50 nits (`s` ≈ 0.0049 on the tuning screen), not black.

So for the garage backgrounds (−1), `rgb = B.rgb · Kd` (`· c2` with `vcol`). `unlit.tex` draws the background domes of several garage scenes, so this matters on screen.

## Sources

- The game's `unlit_tex` and `unlit_vcol_tex` effects and shaders in `effect.scs`, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: unlit surfaces are drawn after lighting with no lighting applied, and `vcol` adds the vertex colour and vertex alpha.

# Shader spec: `eut2.dif.lum`

This spec covers the ETS2 1.60.1.7 self-lit diffuse effect: dashboards, displays, badges and logos that glow at night. It is 3.8 % of all model uses: `dif.lum` (466 models), `dif.lum.lvcol.decal.over` (228), `dif.lum.decal.over` (139) and `dif.lum.lvcol` (53) (`research/evidence/effect-usage-survey.txt`). It was written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-glass-lamp-lum.txt`.

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown. Notation and engine inputs are [dif-spec.md](dif-spec.md)'s (§3, §5).

## 1. Variants (C)

| Token | Effect |
|---|---|
| `dif.lum` | lit diffuse + specular, plus emission weighted by base alpha (§3, §4) |
| `lvcol` | the emission is also multiplied by the vertex colour (§4 step 4) |
| `a` | alpha test as dif-spec §6 (not re-read, H) |
| `over`, `decal`, `decal.over` | blending and depth as dif-spec §2/§6. The `decal` pass reuses the forward shader with the `decal_over` state: depth "greater or equal" with offset, no depth write, `src·α + dst·(1−α)` (C). |
| `add`, `mult`, `premula` | fog colour and blend variants; not used by trucks |
| `dif.lum.spec[.lvcol][.tsnmapuv]` | a sibling base (22 models), not read |

## 2. Passes (C)

| Pass | Content |
|---|---|
| deferred attributes | the lit surface into the G-buffer |
| deferred post | additive emission (§5) |
| forward opaque (or `decal`/`over`) | §3 + §4 in one pass |

Implement the forward maths as canonical.

## 3. Lit surface (forward, C)

Like plain `dif.spec`, with one difference in the specular colour:
- `A = B.rgb · Kd · c2`, where `B` is `texture_base` on UV0. There is no normal map, so `fn = 1`.
- `S = Ks`. **No texture mask:** the base alpha does not mask the specular here. It is the emission mask instead (§4).
- Sun light as dif-spec §5.4 with the plain gate `sat(10·NL)`.
- `surface = (Ld + Cadd·ka) · A + Ls · Ks`.

There is no reflection term.

## 4. Emission (forward, C)

Inputs:
- `ML` (rgba): the engine's material luminosity, filled from the `.mat` input `luminance_output` (nits, default 0). How the engine converts nits into the four components is U.
- `Cb[0…15]`: the engine's backlight colour table, 16 rgb entries (U).
- The backlight tile: `j = min(floor(u0) + 4·floor(v0), 15)`, from the integer parts of UV0. The texture itself is sampled with the full UV0, so tiles repeat the texture.

Steps:
1. `Lv = Cb[j] · ML.rgb` and `La = ML.a` (vertex stage).
2. **Emission weight** `m = sat(ms·B.a + mb)²`. It uses the same remap `mremap = (ms, mb)` as dif-spec §3.3, and the result is **squared**.
3. `emission = B.rgb · Lv · (m · La)`.
4. With `lvcol`: multiply `emission` further by `c2` (rgb) and by `c.a`.

## 5. Combination and output (C)

- **Forward:** `colour = surface + emission`, then the fog of dif-spec §5.7. The output alpha is `B.a · c.a`, used by `over` and `decal.over`.
- **Deferred post:** adds `B.rgb · Cb[j] · ML.rgb · ML.a · m` with the same weight `m` (with `lvcol`, times `c.rgb · c.a · 2`). The vertex stage halves the colour and the fragment stage doubles it, so this equals the forward emission (C, both read).

## 6. Engine inputs and stand-ins

| Input | Status | Stand-in (our choice, not the game's) |
|---|---|---|
| `ML` from `luminance_output` (and `luminance_night`) | U: the nits-to-uniform scale, and whether `luminance_night` applies by time of day, are engine-side. Truck `dif.lum` materials all set `luminance_output`, from −1 to 2000 (`material-survey.txt`, 63 of 63). | the rule below; never pass the nits through raw |
| `Cb` (backlight table) | U: dashboard and interior backlight colours | white, (1, 1, 1) in every entry |
| `mremap` | H, as dif-spec §3.3 | (1, 0) |

**Luminosity factor `ℓ = ML.rgb · ML.a`** (this rule is our choice, shared with [unlit.md](unlit.md); the engine's conversion is U):

| `luminance_output` | Seen in | `ℓ` |
|---|---|---|
| −1 | 13 of 15 sampled garage-scene materials, e.g. the unlit background domes (`research/evidence/shader-spec-glass-lamp-lum.txt`) | 1. These surfaces are visible in the game, so −1 cannot reach the shader raw; it is a sentinel (H, strong). |
| 0 | the default; 2 scene `dif.lum` materials | 0: no emission |
| > 0 (nits, up to 5000 on trucks) | a few | U; stand-in 1 |

So `emission = B.rgb · ℓ · m` (with `lvcol`, also `· c2 · c.a`). This matters in the garage: the night-city, under-bridge and hangar scenes use `dif.lum` for lit windows and signs.

**Next step:** the parked REA run in TASKS (engine-filled material inputs) settles `ML` and `Cb`.

## Sources

- The game's `dif_lum` effects and shaders in `effect.scs`, and the material-input and uniform tables, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: `dif.lum` adds the base colour where base alpha is high, so the surface looks lit at night, and `lvcol` multiplies that luminance by the vertex colour. Consistent with the shader.
- `research/evidence/material-survey.txt` for `luminance_output` ranges.

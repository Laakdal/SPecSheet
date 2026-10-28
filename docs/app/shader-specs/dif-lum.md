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
- `ML` (rgba): the engine's material luminosity, from the `.mat` inputs `luminance_output` and `luminance_night` (§6).
- `Cb[0…15]`: the engine's backlight colour table, 16 rgb entries (§6). If the drawn object's backlight array is empty, every entry is zero and there is no emission (C); what the garage scenes' arrays hold is U.
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
| `ML` from `luminance_output` and `luminance_night` | C, below. Truck `dif.lum` materials all set `luminance_output`, from −1 to 2000 (`material-survey.txt`, 63 of 63). | `o = (1, 1, 1, 1)` |
| `Cb` (backlight table) | C: copied from a colour array held by the drawn object, each rgb × `limited_pre_exposure`; entries past its length are zero (`research/evidence/exposure-tonemap-data.txt` §9). Its contents are U. | white, (1, 1, 1) in every entry |
| `mremap` | H, as dif-spec §3.3 | (1, 0) |

**Luminosity (C, `research/evidence/exposure-tonemap-data.txt` §9).** The engine computes `ML.a = s · o.a` and `ML.rgb = o.rgb`, where `o` is an engine object it holds per draw (contents not decoded; stand-in `o = (1, 1, 1, 1)`), and `s` comes from the material's luminance pair `(x, y)` = (`luminance_output`, `luminance_night`) (H for the pairing; the engine reads one two-value slot):

1. `x = 0` becomes 50 (nits); `y = 0` becomes `x`.
2. `t` = the scene's night blend, 0 on the tuning screen (C).
3. `v = x` if `x = y`; otherwise `exp(lerp(ln x, ln y, t))` for `x > 0` and `lerp(x, y, t)` for `x ≤ 0`.
4. **`x > 0` (physical nits):** `s = v · p · 0.001`, with `p` the scene's `limited_pre_exposure` (0.0973 on the tuning screen, 1 in the world).
5. **`x ≤ 0` (relative):** `s = |v|`, with no pre-exposure. The usual −1 gives `s = 1`.

So the garage-scene materials with −1 draw at full strength, and the default 0 means 50 nits (`s` ≈ 0.0049 on the tuning screen), not black. The emission is then `emission = B.rgb · Cb[j] · ML.rgb · ML.a · m` (with `lvcol`, also `· c2 · c.a`). This matters in the garage: the night-city, under-bridge and hangar scenes use `dif.lum` for lit windows and signs.

**Next step:** the engine object `o` and the backlight array contents (U).

## Sources

- The game's `dif_lum` effects and shaders in `effect.scs`, and the material-input and uniform tables, read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: `dif.lum` adds the base colour where base alpha is high, so the surface looks lit at night, and `lvcol` multiplies that luminance by the vertex colour. Consistent with the shader.
- `research/evidence/material-survey.txt` for `luminance_output` ranges.

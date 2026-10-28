# Shader spec: `eut2.glass`

This spec covers the ETS2 1.60.1.7 glass effect: windows, light covers, sun visors. It is 3.2 % of all model uses; almost every material is the plain base (`research/evidence/effect-usage-survey.txt`). It was written clean-room under D13: the maths was read from the game's shaders and is restated in our own notation, with no shader code or names copied. Sources: `research/evidence/shader-spec-glass-lamp-lum.txt`.

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown.

Notation, vertex inputs and engine inputs are the ones in [dif-spec.md](dif-spec.md) §3 and §5: `P` (eye-space position), `E`, `N`, `L`, `c` and `c2 = 2·c.rgb`, `Kd`, `Ks`, `p`, `ka`, `Camb`, `Aenv`, `Csun_d`, `Csun_s`, `Cadd`, `M`, `g`, and the fog of §5.7. Eye space looks down −z: a point in front of the eye has `P.z < 0` (C; with the opposite sign the near fade below would hide all glass).

## 1. Variants

| Effect | Uses | Difference |
|---|---|---|
| `glass` | 766 models | the base, §3–§5 |
| `glass.tsnmap` | 5 | normal map on UV0 (§3.2) |
| `glass.tsnmapuv` | not in the truck survey | normal map on the UV Normal stream (§3.2) |
| `glass.tsnmap.distort` | 4 | adds a screen-distortion pass (§6) |
| `.instanced`, `.skinned` | — | vertex setup only; out of scope |

The normal-mapped effects carry a fallback technique without the map, as in dif-spec (condition `nmap`, C).

## 2. Passes (C)

Glass is drawn forward only, after the opaque scene, in two passes in this order. Neither writes depth. Both test depth "greater" (reverse-Z) and cull back faces.

| Order | Pass | Blend | What it does |
|---|---|---|---|
| 1 | transmission | `dst · src` (source factor = destination colour, destination factor 0) | darkens and tints what is behind the glass (§4) |
| 2 | surface | `dst + src·α_out` with `α_out = 1`, so purely additive | adds the glass's own light, specular, reflection and fog (§5) |

There is no G-buffer pass and no shadow pass.

## 3. Inputs

### 3.1 Material (`.mat`)

| `.mat` name | Default | Symbol | Used |
|---|---|---|---|
| `glass_tint_color` | (1, 1, 1) | `Ct` | yes |
| `glass_tint_opacity` | 0.0 | `ot` | yes |
| `diffuse` | (1, 1, 1) | `Kd` | yes |
| `additional_ambient` | 0.0 | `ka` | yes |
| `specular` | (1, 1, 1) | `Ks` | yes |
| `shininess` | 4.0 | `p` | yes |
| `fresnel` | (0.2, 0.9) | — | Glass carries the effect flag that selects the constant **`F0 = 0.07`** (C, `research/evidence/exposure-tonemap-data.txt` §9). Only a `fresnel` whose second value is −1 overrides it, with `F0 = clamp(first value, 0, 1)`. |
| `env_factor` | (1, 1, 1) | — | **Not read (C):** neither the shader nor `g` uses it ([dif-spec.md](dif-spec.md) §3.3). Glass `.mat` files set it up to 24; the engine clamps it to 5 and the glass shader ignores it. |
| `queue_bias` | 0 | — | draw order, engine-side |

Measured ranges on truck glass (`material-survey.txt`, 144 materials): `glass_tint_opacity` −0.01…0.92, `fresnel` first value 0…1.5, `shininess` 0…250, `specular` up to 1.3.

Textures:

| Slot | UV | Used channels |
|---|---|---|
| `texture_base` | UV0 | **alpha only** (`B.a`), the specular mask. Its rgb is not read (C). |
| `texture_reflection` | cube | rgb |
| `texture_nmap` | UV0 (`tsnmap`) or UV slot 6 (`tsnmapuv`) | r, g, decoded as in dif-spec §4 |

### 3.2 Normal

Without a normal map, `N = normalize(Nv)` and `fn = 1`. With `tsnmap`/`tsnmapuv` the decode, the tangent frame and the diffuse factor `fn = sat(0.399902·n.z + 0.600098)` are exactly dif-spec §5.2.2 (C). The base texture stays on UV0 in every variant.

### 3.3 Shared terms

- **Tint** `t = Ct · c2` (rgb). With the default tint and a neutral vertex colour (0.5), `t = 1`.
- **Opacity** `o = ot · c.a`.
- **Transmission** `T = (1 − o) · t²`, per channel. The tint enters **squared** (C). There is no clamp: `o` slightly below 0 gives `T` slightly above `t²`.
- **Near fade** `ν = sat(−6.666667·P.z − 1.333333)`: 0 for glass closer than 0.2 m to the eye plane, 1 beyond 0.35 m, linear between (C). It applies to both passes.

## 4. Pass 1: transmission

`out1.rgb = 1 + ν · (T − 1)`, alpha 1. The blend multiplies the framebuffer by it: `dst ← dst · out1`.

So `glass_tint_opacity 0` with a white tint leaves the background unchanged, and opacity 1 blocks it completely. Close to the eye the glass fades to no effect.

## 5. Pass 2: surface

1. **Sun light,** as dif-spec §5.4 with the plain gate:
   - `H = normalize(E + L)`, `NL = sat(N·L)`, `NH = sat(N·H)`;
   - `lobe = NH^p` if `NH > 0`, else 0;
   - `Ld = Camb + Csun_d·NL`;
   - `Ls = Csun_s · lobe · sat(10·NL)`.
2. **Surface colour,** the light the glass itself scatters: `A = o · Kd · t · fn`. It is proportional to the opacity, so clear glass scatters nothing.
3. **Specular colour:** `S = Ks · B.a` (raw base alpha; no remap, no vertex colour).
4. **Reflection:**
   - `R = P − 2(N·P)·N`, `D = M·R`, with the far stretch of dif-spec §5.5 step 3 (`D.y ← D.y·(1 + 20·sat((|P|² − 1)/90000))`) (C).
   - `Cenv = texture_reflection(D).rgb · g`. No `env_factor`, no vertex colour and **no `B.a` mask** (C).
   - **Fresnel (Schlick, C):** `u = 1 − sat(N·R̂)`, with `R̂ = R/|R|`; `F = sat(F0 + (1 − F0)·u⁵)`. This differs from dif-spec's linear fresnel.
   - `Refl = Cenv · sat(Aenv) · F`.
5. **Combination:** `colour = (Ld + Cadd·ka) · A + Ls · S + Refl`.
6. **Fog,** with `τ` and `Cfog` from dif-spec §5.7: `colour ← colour·τ + (1 − τ)·Cfog·(1 − T)`. The second term adds the fog that pass 1 removed from behind the glass, so fogged glass converges to the fog colour (C).
7. **Near fade:** `out2.rgb = ν · colour`, `out2.a = 1`, blended additively.

**Total effect on a background `Cb`:** `Cb · (1 + ν(T − 1)) + ν·colour`.

## 6. `distort` (4 models, out of scope)

`glass.tsnmap.distort` adds a third pass that writes into the engine's distortion buffer, not into colour (C):
- it takes the eye-space refraction of the view ray through the mapped normal with a relative index of 0.6595 (about 1/1.516, crown glass);
- it outputs the refracted ray's screen-space xy (y flipped), divided by a vertex-stage value that was not decoded (H: depth) and scaled by the first `distortion_strength` value; the second value is passed through;
- its weight is `(1 − F)·ν` times a vertex-stage factor that was not decoded.

The engine applies the buffer later, in a pass we have not read. Skip it: it is four models.

## 7. What is not ours to guess

| Input | Status | Stand-in for our renderer (our choice, not the game's) |
|---|---|---|
| `g` (reflection modulator) | C: 1.0 on the tuning screen ([dif-spec.md](dif-spec.md) §3.3) | 1.0 |
| `M`, `Aenv`, the light colours | engine, as dif-spec §3.3 (z-mirrored cube lookup) and [tonemap.md](tonemap.md) §8 | as for dif.spec |

## Sources

- The game's glass effects and shaders in `effect.scs`, and the material-input and uniform tables (`effect/def/inputs.sui`, `uniforms.sui`), read under D13. The list is in `research/evidence/shader-spec-glass-lamp-lum.txt`.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: glass takes base alpha as its specular map and two tint inputs, the colour and a mix between clear and opaque glass. Consistent with the shader.
- `research/evidence/material-survey.txt` for value ranges.

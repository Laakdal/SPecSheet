# Shader spec: `eut2.baked.spec.add.env` and `eut2.baked.spec.add.env.paint`

This spec covers ETS2 1.60.1.7's "baked" truck shader, as the far LODs of trucks use it. Shading parameters come per texel from textures, not from `.mat` values. Among truck and UI models it is used by 17 models for `.paint` (19 pieces) and 3 models for the plain effect (3 pieces), none of them UI scenes (`research/evidence/effect-usage-survey.txt` and the routing survey in the evidence file). Examples: `/vehicle/truck/daf_2021/ai/daf_2021_xf_4x2_lod3.pmd` (`.paint`) and `/vehicle/truck/daf_2021/cargo/daf_xg_4x2_cargo_lod3.pmd` (plain). (H) It occurs only on far-LOD models (both examples checked are LOD 3), so the tuning screen, which shows the full-detail truck, never draws it.

It was written clean-room under D13: the maths was read from the game's shaders and is restated in our own notation; no shader code or names are copied. Sources: `research/evidence/shader-spec-baked.txt`. Labels: **C** = Confirmed (read in the shader, the effect tables or the game files), **H** = Hypothesis, **U** = Unknown.

**What it shares with `dif.spec`** ([dif-spec.md](dif-spec.md)), unchanged:
- the view vectors (§5.1), the vertex normal (§5.2.1) and the vertex colour doubling `c2 = 2·c.rgb`;
- the Blinn-Phong sun terms `Ld = Camb + Csun_d·NL` and `lobe = NH^p` (§5.4);
- the reflection direction, `M` rotation, far stretch and cube lookup (§5.5 steps 1–4);
- fog (§5.7) and the scene inputs (§3.3, §9).

Differences are stated below. The public wiki ([Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders)) does not describe `baked`.

## 1. Effects and passes (C)

| Effect | Passes (glsl technique) |
|---|---|
| `baked.spec.add.env` | 1. deferred attributes (the **same objects as `baked.spec`**: no reflection in the G-buffer); 2. **deferred post, additive** (the reflection, §5); 3. forward opaque (all of §3–§5 in one pass) |
| `baked.spec.add.env.paint` | as above. The attribute and forward passes are its own objects, with the paint step of §4. The post pass is the same object as without `paint`. |

- Render states: attribute and forward passes as dif-spec §2. The post pass tests depth "greater or equal", does not write depth, culls back faces, and blends `src·α_src + dst` (additive, with `α_src = 1`).
- Out of scope: `day`, `night` (a custom composition pass of their own), `tsnmap`, `shadow`, `instanced`, plain `baked` and `baked.spec`. `baked.spec` is §3 without §5.

## 2. Inputs

### 2.1 Per material

| `.mat` name | Used | Meaning |
|---|---|---|
| `fresnel` | yes | One value is used as Schlick `F0` (§5). (H) It is the first value (bias): this shader reads the third component of a 4-component fresnel constant, and `dif.spec` reads (bias, scale) from the third and fourth components of its constant. Both sample materials have (0.2, 0.9), so `F0 = 0.2`. |
| `diffuse` | **`paint` only** | The paint colour `Kd` (§4). The sample materials do not set it, so its default (1, 1, 1) applies unless the engine supplies a colour. (H) The engine supplies the AI paint colour here, as for the `paint` flavor in dif-spec §1. |
| `specular`, `shininess`, `additional_ambient`, `env_factor`, `reflection` | **no** | Not read by any pass (C). The baked textures replace them, and there is no additional-ambient term. |

Textures (all on **UV slot 0**, the only UV stream in the effect interface table; C):

| Slot | Sample format (C, `.tobj` records) | Channels used | Meaning |
|---|---|---|---|
| `texture_base` | BC1 sRGB | rgb | Albedo, sRGB-decoded; alpha unused |
| `texture_over` | BC3 **linear** | r, g, b | r: specular exponent; g: specular intensity; b: gate flag (§3) |
| `texture_mask` | BC4 (one channel) | r | Reflection strength |
| `texture_mask_1` | BC4 | r | `paint` only: paint mask |
| `texture_reflection` | cube, BC1 sRGB | rgb | Environment; the samples name `fuzzy_reflection` and `vehicle_reflection` |

All the sample `.mat`s clamp u and v on the 2D textures, and u, v and w on the cube, with anisotropy disabled.

### 2.2 Per vertex

Position, normal and colour `c`, plus UV slot 0. The doubled colour `c2` tints the albedo only. Vertex alpha is unused: output alpha is 1.

## 3. Surface values (C)

With `B = texture_base`, `O = texture_over`:

- Albedo `A = B.rgb · c2`. There is no `diffuse` factor except with `paint` (§4).
- Specular exponent `p = 1024 · O.r²`. This is per texel, from 0 to 1024, with no `shininess`.
- Specular colour `S = O.g`, a grey scalar with no `specular` factor and no vertex colour.
- Specular gate:
  - `G = NL` where `O.b > 0.5`;
  - `G = sat(10·NL)` otherwise.

  This is the `weight`/plain switch of dif-spec §5.4, chosen per texel. The G-buffer pass carries the same choice as its gate flag.
- Lighting: `Ld = Camb + Csun_d·NL` and `Ls = Csun_s · lobe · G`, with `lobe = NH^p` when `NH > 0` and 0 otherwise. `p = 0` gives a lobe of 1, as dif-spec notes.
- No normal map (in these two effects), no `fn`, no `Cadd·ka` term. The G-buffer stores `ka = 0`.

## 4. `paint` (C)

Let `m = sat(4 · texture_mask_1.r)`. Then

`A = lerp(B.rgb, B.rgb · Kd, m) · c2`

so the mask paints the base towards `B·Kd`. The mask saturates at 0.25: any mask value of 0.25 or more is full paint. Nothing else changes.

## 5. Reflection (C)

1. `D` and `Cenv = cube(D).rgb` as dif-spec §5.5 steps 1–4. **No** `env_factor`, and no `c2`.
2. Strength `k = 3 · texture_mask.r · g`, where `g` is the engine reflection scalar of dif-spec §3.3. A mask value of 1 gives a reflection of 3× the cube's radiance.
3. Fresnel, **Schlick** (unlike `dif.spec`'s linear one; the same shape as truckpaint's): `F = sat(F0 + (1 − F0) · (1 − sat(N·E))⁵)`. Here `N·E` is evaluated as `N·R̂`, with `R̂` the normalised reflection vector (as in dif-spec), and it is clamped to [0, 1] before use.
4. `Refl = Cenv · k · F · Evis`, with `Evis = sat(Aenv)` in the forward pass. In the deferred post pass, `Evis` is the saturated alpha of the diffuse light buffer (dif-spec §7.4) and the normal comes from the G-buffer.

## 6. Combination and output (C)

`colour = Ld · A + Ls · S + Refl`, then fog (dif-spec §5.7, forward pass only), and `out = (colour, 1)`.

In the deferred path, the composition gives `Ld·A + Ls·S` from the G-buffer (with zero emittance). The post pass adds `Refl` on top, without fog in that pass (fog for the deferred path is a separate pass, U).

## 7. Worked example: `/automat/77/77d28e2dc895589f.mat` (DAF XF 2021 AI LOD 3, `.paint`)

- `F0 = 0.2`.
- Textures `…_baked_dif` (base), `…_baked_bspec` (over), `…_baked_env` (mask), `…_baked_paint` (mask_1); cube `fuzzy_reflection`.

For a cab texel with `O = (0.5, 0.3, 0.0)`, mask 0.6, paint mask 1, paint colour `Kd`:
1. `p = 1024 · 0.25 = 256`, `S = 0.3`, `G = sat(10·NL)`.
2. `A = B.rgb · Kd · c2`.
3. `k = 1.8·g`, and `F` runs from 0.2 (facing) to 1 (grazing).
4. `colour = (Camb + Csun_d·NL)·A + Csun_s·NH²⁵⁶·G·0.3 + cube(D)·1.8·g·F·sat(Aenv)`.

## 8. Open questions

- **H:** `F0` is the first `fresnel` value. Next step: check the packing of the 2-float `fresnel` input into its 4-float uniform, in the executable or by comparing with dif-spec's reading.
- **H:** with `paint`, the engine writes the AI paint colour into `diffuse`.
- **H:** the tuning screen never shows LOD 3 models, so this shader matters only if the app ever draws far LODs.
- **U:** the `day`/`night` variants and their custom composition pass; `tsnmap`; `g`, `M` and `Aenv` (as dif-spec §10).

## Sources

- The game's effect files and shader objects in `effect.scs` (1.60.1.7), read under D13. Their list is in `research/evidence/shader-spec-baked.txt`.
- The material-uniform table, which feeds `diffuse` and `fresnel`, and the effect interface table.
- The two sample `.mat` files and their texture records (same evidence file).

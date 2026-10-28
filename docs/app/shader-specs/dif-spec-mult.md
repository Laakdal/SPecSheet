# Shader spec: `eut2.dif.spec.mult.dif.spec` family and `dif.spec.decal.mult`

This spec covers two things in ETS2 1.60.1.7:
- the base `dif.spec.mult.dif.spec` (two textures multiplied, then lit like `dif.spec`), with the flavors `add.env`, `tsnmap`, `tsnmapuv`, `paint`, and the sibling base `dif.spec.mult.dif.spec.iamod.dif.spec[.tsnmap|.tsnmapuv]`;
- the blend flavor `mult` as in `dif.spec.decal.mult`.

The tuning garage `/model/ui/garage/desktop_garage.pmd` is almost all this base: 22 of its 31 materials (`research/evidence/shader-spec-dif-spec-mult.txt`). It was written clean-room under D13. The maths was read from the game's shaders and is restated in our own notation; no shader code or names are copied. The sources are listed in `research/evidence/shader-spec-dif-spec-mult.txt`.

Labels: **C** = Confirmed (read in the shader, the effect tables or the game files), **H** = Hypothesis, **U** = Unknown.

**This base reuses `dif.spec` almost entirely.** Everything not stated here is [dif-spec.md](dif-spec.md) unchanged: the view vectors (§5.1), the normal and the `fn` factor (§5.2), the sun light with the plain gate `G = sat(10·NL)` (§5.4), the reflection direction, far stretch, fresnel and visibility (§5.5), the combination (§5.6), fog (§5.7), the deferred path (§7) and what Bevy must supply (§9). Notation (`B`, `Kd`, `Ks`, `p`, `c`, `c2 = 2·c.rgb`, `g`, `F`, `Evis`, `sat`) is as in that spec.

## 1. Effect names and what each changes

| Effect (as in `.mat`) | Shaders (C) | Change against [dif-spec.md](dif-spec.md) |
|---|---|---|
| `dif.spec.mult.dif.spec` | own pair | §3: `texture_mult` multiplies the base; the specular mask is not remapped; output alpha is vertex alpha |
| `…mult.dif.spec.tsnmap` | own pair | plus the normal map on UV slot 0, as dif-spec §5.2.2 |
| `…mult.dif.spec.tsnmapuv` | own pair | plus the normal map on UV slot 6, as dif-spec §5.2.2 |
| `…mult.dif.spec.add.env[.tsnmap \| .tsnmapuv]` | own base directory, own pairs | plus the reflection of dif-spec §5.5, with the mult alpha in its strength (§4.3) |
| `…mult.dif.spec[.add.env].paint` | **the same objects** as without `paint` | none in the shader (C). As in dif-spec §1, (H) the engine writes the paint colour into `diffuse`. |
| `dif.spec.mult.dif.spec.iamod.dif.spec[.tsnmap \| .tsnmapuv]` | own pairs | a third texture, faded by vertex alpha (§5) |
| `dif.spec.decal.mult` | **the same objects** as `dif.spec.decal` | only the render state: multiplicative blend (§6) |

- **Technique condition (C):** each normal-mapped effect has two GLSL techniques. One is marked with the normal-map condition. The other falls back to the same effect's non-normal-mapped shaders, e.g. `mult.dif.spec.add.env.tsnmapuv` falls back to `mult.dif.spec.add.env`. As in dif-spec §1, implement the normal-mapped one.
- **Passes (C):** each opaque effect has a deferred-attribute pass and a forward pass with the render states of dif-spec §2. The two passes compute the same surface values; the diffs in the evidence file show this for the plain and the `add.env` pair.
- **Wiki:** SCS's [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders) describes the base as `dif.spec` with one more texture whose colour **and alpha** multiply the base texture. Its example is a company texture multiplied onto a trailer's base texture. In the garage the second texture is baked occlusion instead.
- Not covered here: `over`, `decal`, `decal.over`, `a`, `opasrc01`, `add` (blend), `tg1`, `shadow`, `instanced`, `skinned`, and `dif.spec.fade.mult.dif.spec`.

## 2. Inputs

### 2.1 Per material (`.mat`)

The same attributes and defaults as dif-spec §3.1 (`diffuse`, `specular`, `shininess`, `additional_ambient`; with `add.env` also `env_factor`, `fresnel`, `reflection`). Texture slots:

| Slot | Kind | Flavor | UV slot (C) |
|---|---|---|---|
| `texture_base` | 2D RGBA | all | **0** |
| `texture_mult` | 2D RGBA | all | **1** |
| `texture_iamod` | 2D RGBA | `iamod` | **2** |
| `texture_nmap` | 2D, r and g used | `tsnmap` | **0** (the base's coordinate) |
| `texture_nmap` | the same | `tsnmapuv` | **6** ("UV Normal") |
| `texture_reflection` | cube | `add.env` | direction, dif-spec §5.5 |

- **UV slots (C):** read in the vertex shaders and in the effect interface table. The interface lists exactly UV 0 and UV 1 for the base, adds UV 2 for `iamod` and UV 6 for `tsnmapuv`.
- **Slot to stored set:** the PMG piece field at +8 maps each UV slot to a stored UV set ([models.md](../../formats/models.md); reading it as the slot routing is H1 of `research/evidence/pmg-uv-routing.txt`). The set is per piece. Over `vehicle`, `model` and `model2`, `dif.spec.mult.dif.spec` pieces route slot 1 from set 1 (389 pieces) or from set 0 (839). So always resolve slots through +8 and never assume "mult = set 1".
- **The garage (C, bytes):** every `mult.dif.spec` piece of `desktop_garage.pmd` routes slot 0 → set 0, slot 1 → set 1, and (with `tsnmapuv`) slot 6 → set 0. Set 0 is the tiled set (u up to about ±10), set 1 lies in 0…1 (the occlusion atlas). The `tsnmapuv` normal map therefore tiles with the base.
- **`mult_uvscale`, `mult_uvscale_secondary` (C):** these exist in the material-input table, but no material uniform takes them and none of this family's shaders reads them. Do not scale the mult coordinate. The only per-slot transform is the engine's compressed-UV scale and bias, which is the identity for float UVs (dif-spec §3.2, H).

### 2.2 Sampler state and colour space of the garage textures (C, from the textures' image and sample records)

The shaders do no colour-space conversion. What the sampler returns is multiplied as it is.

| Texture | DXGI format | Decoded as | Address (from the `.tobj`) |
|---|---|---|---|
| `texture_mult`: `/model/ui/garage/ao_textures/metal_plates_v3.tobj`, `escape_light.tobj` | 78, BC3 sRGB | **sRGB to linear** for rgb, alpha linear | u **clamp**, v **repeat** |
| `texture_base`: `/material/tiled/metal_plates_gs_01.tobj` | 78, BC3 sRGB | sRGB to linear | u, v repeat |
| `texture_nmap`: `…/metal_plates_gs_01_nrm.tobj` | 83, BC5 | linear | u, v repeat |
| `texture_reflection`: `/material/environment/showroom_cube/reflection.tobj` | 72, BC1 sRGB, cube | sRGB to linear | clamp |

- The garage `.mat` overrides only `u_address : clamp` on `texture_mult`; v keeps the record's repeat. On the cube it clamps u, v and w and disables anisotropy.
- **The occlusion is sRGB-encoded.** Sampling it as linear data would make it far too bright: an encoded 0.5 is 0.21 linear. Decode it like any colour texture, by its format.

### 2.3 Per vertex

As dif-spec §3.2: position, normal, colour `c`, and, for the normal-mapped flavors, the tangent with its sign. In addition, the UV slots of §2.1.

## 3. Surface values (base, all flavors except `iamod`)

Let `B` = `texture_base`(UV slot 0) and `M` = `texture_mult`(UV slot 1), both RGBA. Define the product `T = B ⊙ M`, all four channels, componentwise (C).

| Value | `dif.spec.mult.dif.spec` (C) | Plain `dif.spec` (dif-spec §5.3), for comparison |
|---|---|---|
| Albedo `A` | `B.rgb · M.rgb · Kd · c2 · fn` | `B.rgb · Kd · c2 · fn` |
| Specular colour `S` | `B.a · M.a · Ks` | `sat(ms·B.a + mb) · Ks` |
| Opacity `α` (output alpha) | `c.a` | `B.a · c.a` |

- **No scale on the product (C).** It is a plain multiply: no ×2, no `sat`, no bias. A white `M` is neutral, and `M` can only darken.
- **The specular mask is the product of both alphas and has no remap (C).** The engine's specular-mask remap `(ms, mb)` from dif-spec §3.3 is absent from this base, in the forward pass and in the G-buffer pass alike. Vertex colour does not tint `S` (as in plain `dif.spec`).
- **What "the second `dif.spec`" means:** the second texture is a full diffuse/specular texture of its own. Its rgb multiplies the albedo and its alpha multiplies the specular mask (and, with `add.env`, the reflection mask, §4.3). Nothing else of a second material exists: one `diffuse`, one `specular`, one `shininess` and one lighting evaluation (C).
- **Alpha:** base and mult alpha are spent on the specular. The output alpha is vertex alpha only. In the opaque passes the output alpha is unused, so these surfaces are opaque.
- `fn` is the normal-map albedo factor of dif-spec §5.2.2 (1 without a normal map). It multiplies `A` only.

## 4. Lighting and reflection

### 4.1 Normal

As dif-spec §5.2.1 (none) or §5.2.2 (`tsnmap` on UV slot 0, `tsnmapuv` on UV slot 6). This includes the decode `x = 2.007874·r − 1.007874`, `z = sqrt(sat(1 − x² − y²))`, the unnormalised tangent frame and `fn = sat(0.399902·z + 0.600098)` (C, the same constants were read in this family).

### 4.2 Sun

dif-spec §5.4 with the **plain** gate `G = sat(10·NL)`: this base has no `weight` behaviour (C). Then `Ld = Camb + Csun_d·NL` and `Ls = Csun_s·lobe·G`, with `lobe = NH^p` when `NH > 0`, else 0.

### 4.3 Reflection (`add.env`)

dif-spec §5.5 steps 1–5 and 7 unchanged: direction, `M` rotation, far stretch, lookup `Cenv = cube(D).rgb · Ke` (no `c2`), fresnel `F = sat(fb + fs·(1 − N·E))`, and `Evis = sat(Aenv)` in the forward pass. Step 6 changes:

`Estr = g · B.a · M.a · F` (C).

So the mult alpha masks the reflection as well as the specular. Here `g` is the engine reflection scalar of dif-spec §3.3 (H: 1 by default).

### 4.4 Combination and output

`colour = (Ld + Cadd·ka) · A + Ls · S + Refl` with `Refl = Cenv · Estr · Evis` (0 without `add.env`). Then fog as dif-spec §5.7, and `out = (colour, c.a)` (C). The deferred path stores the same `A`, `S` and `Cenv·Estr` in the G-buffer (C, diffed), so dif-spec §7.2 reproduces this equation.

## 5. `iamod`: the third, vertex-alpha-faded layer

Base `dif.spec.mult.dif.spec.iamod.dif.spec[.tsnmap|.tsnmapuv]`. Let `I` = `texture_iamod`(UV slot 2), RGBA.

1. Fade it by vertex alpha: `I' = I + c.a · (1 − I)`, componentwise on all four channels (C). This is a lerp from `I` (at `c.a = 0`) to white (at `c.a = 1`).
2. Product: `T = B ⊙ M ⊙ I'` (C).
3. `A = T.rgb · Kd · c2 · fn` and `S = T.a · Ks` (C). The iamod layer therefore darkens the specular as well as the albedo.
4. Lighting, normal and combination as §4 (no `add.env` variant of this base was read; the measured `iamod` uses have none).
5. **Output alpha is the constant 1 (C).** Vertex alpha is used up by the fade.

- **Wiki:** "a third inverse-vertex-alpha texture" for "decal/dirt/wear", used on building walls. Vertex alpha 0 applies it most and **0.5** does not apply it at all.
- **Conflict (U):** in the shader the layer vanishes at `c.a = 1`, not 0.5. The two agree only if the stored vertex alpha is twice the authored one (the exporter doubling alpha, as the shader doubles rgb). This is the same open question as dif-spec §3.2 ("0..1 or pre-doubled"). Implement the shader form on the stored value. Next step: read stored vertex alphas on an `iamod` piece, e.g. `/model/props/lumber/planks_stack_packed_pile_01_poc_pile.pmd` piece 8, and check whether 1.0 (no layer) is common.
- `iamod` is not used by the tuning garage (`desktop_garage.pmd`). Among truck and UI models it occurs only in the UI showroom scene `/model/ui/showroom/showroom_sc.pmd` (7 pieces; UV routing slot 0 → set 0, slot 1 → set 1, slot 2 → set 1 or 2), per `uv_routing_survey vehicle/truck model/ui model2/ui` (evidence file). Whether that scene is shown in the tuning flow is U.

## 6. `dif.spec.decal.mult`: multiplicative decal

### 6.1 Shaders and passes (C)

The effect names exactly the objects of `dif.spec.decal`:
- a "decal post" pass, the deferred-lit variant of dif-spec §7.3;
- a "decal" pass, the plain `dif.spec` forward shader of dif-spec §5.

The surface maths is therefore plain `dif.spec`: base on UV slot 0, `A = B.rgb·Kd·c2`, `S = sat(ms·B.a + mb)·Ks`, `α = B.a·c.a`, no reflection. The effect interface lists only UV 0.

### 6.2 Render state (C, both passes)

- depth test "greater or equal", no depth write, depth offset on;
- back-face culling;
- **blend: source factor = destination colour, destination factor = zero**, so `result = src ⊙ dst` on all four channels.

Consequences (C, they follow from 6.1 and 6.2):
- The decal's **lit** colour (§5.6 of dif-spec, after fog) multiplies what is already in the framebuffer. Output white leaves the target unchanged, and darker values darken it. It is the lit result that multiplies, not the texture.
- **Alpha does not fade the decal.** `α` only multiplies the destination alpha. A texel with `B.a = 0` still multiplies the colour, through its diffuse term.
- With no `a` flavor there is no discard.
- (H) Only one of the two passes runs for a given setup, as for other decals (dif-spec §2).

### 6.3 The `mult` flavor and fog

- **Flavor table (C):** `mult` sets a white-fog define, the black-fog counterpart of `add`.
- **Shader objects (C):** the effect names the same object hashes as `dif.spec.decal`, so the define changes no compiled code here.
- **(H)** The engine supplies white fog colours for `mult` draws, so that fogged decals fade to neutral. Fogging towards the scene colour would tint the target. This is neutral as long as fog transmittance `τ ≈ 1`.
- The desktop scene's fog density is 0.0018 (`research/evidence/ui-hotspot-scene-diagnosis.txt`). At 10 m that gives `τ = 2^(−0.018) ≈ 0.99`, about 1 %, so the choice barely matters in the garage.

### 6.4 The garage's use

`/model/ui/dark_studio.pmd` uses it in 5 materials across its looks (3 in the first look), e.g. `/automat/57/578a47b9c7e49f39.mat`:
- base `/model/ui/texture/floor_1_light_map.tobj`;
- `diffuse` 1, `specular` 0.604, **`shininess` 0**, `additional_ambient` 0.

With `p = 0` the lobe is `NH⁰ = 1` wherever `NH > 0` and 0 elsewhere (C, the shader's branch tests `NH ≤ 0` before the power). The specular is then the constant `Csun_s · G · sat(ms·B.a + mb) · 0.604` over the lit side. It is not a highlight. **Do not clamp or "fix" `pow(x, 0)`.** The floor light map is multiplied onto the floor with that added constant.

## 7. Worked example: the garage plates `/automat/cc/cc9de879db5d57dd.mat`

Effect `dif.spec.mult.dif.spec.add.env.tsnmapuv`:
- `diffuse` 1, `specular` 0.0175, `shininess` 85, `additional_ambient` 0;
- `env_factor` 1, `fresnel` (0.2, 0.9);
- textures as §2.2.

The piece routes slot 0 → set 0, slot 1 → set 1, slot 6 → set 0.

1. `n` from `texture_nmap` at set 0's UV (the tiled UV). `N = normalize(n.x·Tv + n.y·Bv + n.z·Nv)` and `fn = sat(0.399902·n.z + 0.600098)`.
2. `B` = base at set 0 (tiled, repeat), `M` = occlusion at set 1 (u clamp, v repeat), both sRGB-decoded.
3. `A = B.rgb · M.rgb · c2 · fn`, `S = B.a · M.a · 0.0175`.
4. Sun: `Ld = Camb + Csun_d·NL`, `Ls = Csun_s · NH^85 · sat(10·NL)`.
5. Reflection: `Cenv = showroom_cube(D).rgb`, `F = sat(0.2 + 0.9·(1 − N·E))`, `Refl = Cenv · g · B.a · M.a · F · sat(Aenv)`.
6. `colour = Ld·A + Ls·S + Refl`, then fog, alpha `c.a`.

Order of magnitude with the scene values of the desktop config (`light_ambient` 0.3, `light_diffuse` 3, light straight down):
- On an up-facing floor, a texel with base 0.8 and occlusion 0.5 (both sRGB-encoded) has `A ≈ 0.60 · 0.21 = 0.13` (at `c2 = 1`).
- `Ld ≈ 3.3`, so the diffuse is ≈ 0.42.
- At grazing view `F` reaches 1, and the reflection adds up to the cube's radiance times `B.a·M.a`.

The occlusion therefore carries most of the contrast. (Texture alphas were not sampled per texel; whether these BC3 textures carry a non-trivial alpha is U per texture.)

## 8. Open questions and hypotheses

- **U:** the `iamod` vertex-alpha scale, shader against wiki (§5). Next step: read stored vertex alphas on an `iamod` piece.
- **H:** the `mult` flavor gets white fog colours from the engine (§6.3). Next step: REA on `eurotrucks2.exe` for the fog-constant setup per blend mode.
- **H:** only one of the two decal passes runs (§6.2, as dif-spec §2).
- **H:** `paint` writes the paint colour into `diffuse` (as dif-spec §1).
- **H1 (pmg-uv-routing):** piece +8 is the slot routing. Every `mult.dif.spec` piece surveyed routes exactly the slots the interface table names (0 and 1, plus 2 for `iamod` and 6 for `tsnmapuv`), which fits it.
- **U:** the alpha content of the garage's `texture_mult` textures (the specular and reflection masks, §3).
- All of dif-spec §10's open points apply too: `g`, `M`, `(ms, mb)` (not used by this base, but used by `dif.spec.decal.mult`), deferred vs forward, and the light-list pass.

## Sources

- The game's effect files and shader objects in `effect.scs` (1.60.1.7), read under D13. Their list is in `research/evidence/shader-spec-dif-spec-mult.txt`.
- The game's material-input, material-uniform, flavor and vertex-stream tables, and the effect interface table, in `effect.scs`.
- The garage's `.mat` files, texture records and PMG UV routing (same evidence file).
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders) (paraphrased): `dif.spec.mult.dif.spec`, `iamod`, and the `mult` blend mode.

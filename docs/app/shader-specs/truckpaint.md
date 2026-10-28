# Shader spec: `eut2.truckpaint` (paint, masks, flip, flakes)

The surface maths of ETS2 1.60.1.7's truck paint effect, written for an implementer who has not seen the game's shaders (D13). This page covers the inputs, the UV routing, the albedo and the paint features. The lighting it feeds (sun, clear-coat specular, environment reflection, normal maps, vertex colour) is in [truckpaint-lighting.md](truckpaint-lighting.md).

- **Sources:** the GLSL objects of 18 `eut2.truckpaint*` effects, listed with their hashes and the commands in `research/evidence/truckpaint-spec-sources.txt`.
- **Behaviour summary:** SCS's public wiki ([Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased in [../shaders.md](../shaders.md)): "paint from base and paint-job textures; modes default, colormask, airbrush; flipflake".
- **Labels:** **C** = Confirmed, read in the shader. **H** = Hypothesis. **U** = Unknown.

## 1. Variants and how they differ

Each effect has two passes with identical surface maths (C, 6 of 6 pairs compared). The deferred-attribute pass writes a G-buffer, and the forward pass lights the surface itself; see the lighting page.

| Flavour | What it changes (C) |
|---|---|
| (none) | Base texture × material colour. No paint uniforms, no mask, no flakes. |
| `colormask` | Adds the mask texture. Its R/G/B select three mask colours over the base colour, and its A scales the specular and the reflection. |
| `airbrush` | Adds the mask texture as a livery. Its RGB is painted over the paint colour by its A. |
| `flipflake` | Adds the flip colour (view-angle colour shift) and flakes (noise-driven specular). |
| `flipflakeuv` | Like `flipflake`, but the flake noise uses UV set 2. |
| `altuv` | The mask and the flakes use UV set 2 instead of UV set 1. Only the vertex stage changes. On the plain variant it changes nothing: `eut2.truckpaint.altuv` names exactly the objects of `eut2.truckpaint`. |
| `tsnmap` / `tsnmapuv` | Adds a tangent-space normal map; see the lighting page. Both carry a fallback technique, with the same variant minus the normal map, that is used when the condition `nmap` is false (H: a graphics setting). |

Combinations that exist (directory listing, `docs/formats/paint.md`):
- `airbrush.flipflake` and `airbrush.flipflakeuv`;
- `.altuv` with every flavour except `flipflakeuv`: there is no `flipflakeuv.altuv` and no `airbrush.flipflakeuv.altuv`;
- `tsnmap`/`tsnmapuv` with every combination.

No `colormask` + `flipflake` combination exists.

### Variant selection at runtime (H)

Truck `.mat` files usually name only the base, e.g. `eut2.truckpaint` or `eut2.truckpaint.tsnmapuv`. The Scania S cab names plain `truckpaint`, yet the truck has masked and alt-UV paint jobs. The executable holds the flavour names as separate strings. So the engine probably appends flavours from the selected paint job (`PiecePaint`):

| Paint job | Flavour appended |
|---|---|
| `airbrush` true | `.airbrush` |
| a mask and mask colours, not airbrush | `.colormask` |
| `flipflake` true | `.flipflake` (when `.flipflakeuv` is used instead: U) |
| `alternate_uvset` true | `.altuv` |
| no mask and no flipflake (a plain colour) | none: the plain variant, with the base colour in the material colour (§4) |

Name order: base, then `tsnmap`/`tsnmapuv`, then `airbrush`/`colormask`, then `flipflake`/`flipflakeuv`, then `altuv`.

Constraint (C, listing): `flipflakeuv` never appears with `altuv`. It is therefore a different choice from `alternate_uvset`: the mask (if any) stays on UV set 1 and only the flakes move to set 2. Which paint-job or model property selects it is U.

Skinned, `shadow` and `asafew.a` variants exist but were not read (§7).

## 2. Inputs

### Textures

| Texture (`.mat` slot) | Source | UV | Used by |
|---|---|---|---|
| `texture_base` | `.mat` | UV0 | all |
| `texture_paintjob` (the mask) | `PiecePaint.mask` (`.mat` placeholder `empty.tobj`) | UVp (below) | colormask, airbrush |
| `texture_flakenoise` | `PiecePaint.flake.noise` | UVf (below) | flipflake, flipflakeuv |
| `texture_nmap` | `.mat` | UV0 (`tsnmap`) or UVn (`tsnmapuv`) | tsnmap, tsnmapuv |
| `texture_reflection` (cube) | `.mat` (`/material/environment/vehicle_reflection.tobj` in all 4 mats seen) | reflection vector | all |

UV routing (C, from the vertex stages):
- **UVp** (paint UV) is mesh UV set 1. With `altuv` it is set 2.
- **UVf** (flake UV) equals UVp. With `flipflakeuv` it is set 2. In `airbrush.flipflakeuv` the mask stays on set 1 and the flakes use set 2.
- **UVn** comes from a separate vertex input, the 7th texture-coordinate attribute. Which PMG UV set feeds it is U.
- (H) "UV set *n*" means the PMG's *n*-th UV set, counted from 0.
- **Transforms.** In the vertex stage each UV set gets an engine-set scale and bias (C).
  - The PMG stores UVs as plain `f32` pairs (`docs/formats/models.md`, `pmg.rs`). So (H) that scale and bias is identity for our data, and we apply no transform.
  - The fragment stage adds no tile offset or scale to the mask (C). Where a mask tile sits is decided entirely by the mesh's UVs. This fits `paint.md`: "a tile is already cut for its model".
- **Missing UV set (U).** The Scania S cab paint piece has only UV0, and its material is plain truckpaint. Yet the truck has 82 alt-UV paint jobs (`paint.md` [S6]), which need set 2, and masked paint jobs without alt-UV would need set 1. The mesh has neither. Possible engine behaviours: it reuses UV0, or the attribute reads as zero.

### Uniforms (the shader-side names used below)

| Symbol | Meaning in the maths | Packed in | Our input (mapping) |
|---|---|---|---|
| **P** | paint base colour, rgb | `paintjob_base_color` | `PiecePaint.base_color` (H) |
| **Q_r, Q_g, Q_b** | mask colours | `paintjob_r/g/b_color` | `mask_colors[0..2]` (H) |
| **K_flip**, **φ** | flip colour, flip weight | `flip_color` rgb, w | `flip.color` (H), `flip.strength` (U, see §7) |
| **K_flake**, **ρ** | flake colour, flake density | `flake_color` rgb, w | `flake.color` (H), `flake.density` (H) |
| **s**, **p_f**, **k**, **r** | flake UV scale, flake specular power, clear-coat rolloff, v-ratio | `flake_setup` x, y, z, w | `flake.uvscale`, `flake.shininess`, `flake.clearcoat_rolloff`, `flake.vratio` (all H) |
| **X_fl**, **λ** | flake light position (eye space), flake strength | `flake_light` xyz, w | engine-set, U |
| **D_blk** | material-block diffuse, rgb | material block row 1 | plain variant: H, the paint base colour (§4) |
| **D_raw** | second diffuse colour | separate uniform | `.mat` `diffuse` (H) |
| **S_mat**, **p_mat** | specular colour, specular power | material block row 2 | `.mat` `specular`, `shininess` (H) |

The `.mat` keys `flake_*`, `flip_*` and `paintjob_*` in upgrade materials are placeholders (`docs/formats/paint.md`). The paint job's values replace them (H).

## 3. Notation

- Base sample: `B = sample(texture_base, UV0)`, as RGBA.
- Mask sample: `M = sample(texture_paintjob, UVp)`, as RGBA.
- Normal and view: `N` is the unit shading normal in eye space (the mapped normal with `tsnmap`). `V` is the unit vector from the surface point to the eye. `x_eye` is the surface point's eye-space position.
- `sat(x)` clamps to [0, 1]. `lerp(a, b, t) = a + t·(b − a)`. `⊙` multiplies per component.

## 4. Paint colour and albedo

The **paint colour** `C_p` depends on the variant (C):

1. **Plain:** there is no paint term. The albedo is `B.rgb ⊙ D_blk ⊙ D_raw`. The packed material-block diffuse appears in the albedo only in the three plain variants (6 of 26 fragment objects). The paint variants use `P` in its place. So (H) for a plain-colour paint job, the engine writes the paint base colour into `D_blk`.
2. **Colormask:** start from `P`, then apply three blends in this order:
   `C₁ = lerp(P, Q_b, M.b)`, `C₂ = lerp(C₁, Q_g, M.g)`, `C_p = lerp(C₂, Q_r, M.r)`.
   Red therefore wins over green, and green over blue. The mask alpha does not touch the colour. It scales the specular colour and the reflection (lighting page, `m_spec`).
3. **Airbrush:** `C_p = lerp(P, M.rgb, M.a)`. The mask alpha affects only the colour.
4. **Flipflake:** `C_p = F_flip` (§5).
5. **Airbrush + flipflake:** `C_p = lerp(F_flip, M.rgb, M.a)`: the livery is painted over the flip colour.

**Albedo** in the paint variants: `A = B.rgb ⊙ C_p ⊙ D_raw` (C). The lighting page then applies the vertex colour, and the normal-map factor for `tsnmap`.

## 5. Flip colour

From the facing term `f = sat(N·V)` (with the mapped normal when normal-mapped):

- facing weight: `w_f = lerp(f, f², φ)`
- averted weight: `w_a = lerp(1 − f, (1 − f)², φ)`
- `F_flip = w_f · P + w_a · K_flip`

So `φ = 0` blends linearly with the angle, and `φ = 1` blends quadratically, which makes both ends sharper. The two weights are not normalised: at `φ = 1` they sum to less than 1 at mid angles, so the colour darkens there. No other fresnel enters the albedo (C).

## 6. Flakes

The flakes change **only the specular colour and the specular power** of the sun's highlight. They change neither the albedo nor the reflection (C). Steps:

1. Noise UV: `t = (UVf.u · s, UVf.v · s · r)`. Sample `n = sample(texture_flakenoise, t)` as RGBA.
2. Coverage: `c = 1 − ρ · n.a`.
3. Flake light: `L_f = normalize(X_fl − x_eye)`. Strength: `λ' = sat(λ)`.
4. Half vector: `H_f = normalize(V + L_f)`. Then `h = sat(N·H_f)` when `N·L_f ≥ 0`, else `h = 0`.
5. Clear-coat-to-flake blend: `τ = clamp(e^(0.996·k) − e^(k·h), 0, 1)`. The shader computes the constant `1 − e^(0.996·k)` per vertex and forms `1 − sat(e^(k·h) + that constant)`, which equals the line above.
   With `k > 0`, `τ = 0` where `h ≥ 0.996`, the clear-coat mirror direction. It rises towards 1 away from it, and faster the larger `k` is. With `k < 0`, `τ` stays near 0 everywhere. That is why we take `k = +flake_clearcoat_rolloff` (H).
6. Specular colour before weighting: `S = lerp(S_mat, n.rgb ⊙ K_flake, τ · λ')`.
7. Specular power: `p = lerp(p_mat, p_f, c · λ')`.

`S` and `p` replace `S_mat` and `p_mat` in the lighting page. Without flakes, `S = S_mat` and `p = p_mat`.

## 7. Colour spaces, constants, unknowns

**Colour spaces.**
- The shaders contain no gamma steps (C). Every texture value is whatever the sampler returns, and the maths is linear.
- Masks are BC1/BC3 sRGB and the flake noise is B8G8R8A8 sRGB (`docs/formats/paint.md`). Their RGB therefore arrives linearised, and the colormask weights `M.r/g/b` are the **decoded** values. A `shared_RRGGBBAA` constant mask must enter as `srgb_to_linear(rgb)` with linear alpha.
- Uniform colours are used as given: P, Q, K_flip, K_flake, D, S (H: linear, as SpecSheet treats definition colours).

**Constants (C).** 0.996 and e (flakes); the blend order b → g → r (colormask).

**Unknown, with the next step:**
- How the engine packs the paint fields into the uniforms, and whether it rescales them. The ranges conflict with direct use:
  - `flip_strength` goes up to 2.7, which as `φ` makes `w_f` negative over most angles.
  - `flake_density` goes up to 2, which drives `c` below 0.
  - `flake_vratio 0` appears in `.mat` files, and as `r` it would zero the v scale.
  - Next step: one REA regex run on `eurotrucks2.exe` over `flake_setup|flip_color|flake_light|flake_vratio|flip_strength|texture_reflection_modulator|paintjob_base_color`, then read the code that fills these uniforms.
- The source of the flake light `X_fl, λ`: the sun, a camera light or a fixed light. The same REA run settles it.
- When `flipflakeuv` is chosen instead of `flipflake`. Same REA run.
- The PMG UV set behind `tsnmapuv`'s coordinate. Next step: compare the UV sets of a `tsnmapuv` model against its normal map in our PMG reader.
- Whether `D_blk` in the plain variant is `base_color` or `base_color ⊙ .mat diffuse`. Settled by the REA run, or by a parity screenshot of a plain-colour cab.
- What the engine feeds a paint UV the mesh lacks (§2, "Missing UV set"). Next step: a parity screenshot of a Scania S masked cab against our render with the mask sampled at UV0. Or add the binding of UV sets to vertex inputs to the REA regex above.
- The variants not read: `skinned` (135), `shadow` (132) and `asafew.a` (132). `asafew.a` is used on real trucks: the Volvo FH 2024 has a `tsnmapuv.asafew.a` paint material. (H) It is the alpha-tested form of the same maths, and `shadow` and `skinned` change only the pass setup or the vertex skinning. Next step: a second spec pass over one `asafew.a` effect and one `shadow` effect, diffed against the variants above.

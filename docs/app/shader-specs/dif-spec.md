# Shader spec: `eut2.dif.spec` family

This spec covers the ETS2 1.60.1.7 `dif.spec` family as truck, accessory and garage materials use it. It gives the surface maths and the lighting the family shares: inputs, texture lookups, equations per flavor, blending and passes, and what the engine has to supply. Measured share of all model uses: 60 % (`research/evidence/effect-usage-survey.txt`).

It was written clean-room under D13. The maths was read from the game's shaders and is restated here in our own notation; no shader code or names are copied. The sources are listed in `research/evidence/shader-spec-dif-spec.txt`.

Labels: **C** = Confirmed (read in the shader or the effect tables), **H** = Hypothesis, **U** = Unknown.

Related: [truckpaint-lighting.md](truckpaint-lighting.md).
- **Shared:** the same forward lighting and combination (§5.4, §5.6), the normal-map decode and the 0.6/0.4 albedo factor (§5.2), and the reflection lookup with its far stretch (§5.5).
- **Different (C, both read):**
  - `dif.spec` uses a **linear** fresnel built from both `fresnel` values (§5.5); truckpaint uses a Schlick fresnel built from one.
  - `dif.spec` reads `env_factor`; truckpaint does not.
- **New here:** §7.4 answers that spec's open question about the deferred sun pass.

## 1. Effect names, flavors and what they change

An effect name is the base `dif.spec` (or one of its sibling bases), followed by flavors in any order. Each `.rfx` maps one full name to compiled shaders (C). The table lists every token in the measured truck and garage usage.

| Token | Kind | Effect on the maths (§ refs) |
|---|---|---|
| `dif.spec` | base | Diffuse plus Blinn-Phong specular from one texture. Base alpha is the specular mask (§5.3). |
| `weight` | flavor | Vertex colour also tints the specular and the reflection. The specular mask is not remapped. The specular gate is `N·L` instead of `sat(10·N·L)` (§5.3, §5.4). |
| `add.env` | flavor | Adds the cube-map reflection with fresnel (§5.5). |
| `nofresnel` | part of the base name `dif.spec.add.env.nofresnel` (also `…weight.add.env.nofresnel`) | Like `add.env`, without the fresnel factor (§5.5). |
| `oclu` | base `dif.spec.oclu…` | Adds `texture_oclu` on UV1, which multiplies the base texture (§5.3). |
| `tsnmap` | flavor | Tangent-space normal map on UV0 (§5.2). |
| `tsnmapuv` | flavor | The same, on the "UV Normal" stream (§5.2). |
| `tsnmapuv2` | flavor | Two normal maps combined: a detail map on UV0 plus the main map on the UV Normal stream (§5.2.3). Rare: 33 models. |
| `a` | flavor | Alpha test: discard below 0.05 (§6). |
| `over` | flavor | Alpha blended, drawn forward, no depth write (§6). |
| `decal` | flavor | Drawn after opaque geometry with a depth offset and no depth write; blended only together with `over` (§6). |
| `asafew` | flavor | No shader change (C): `dif.spec.tsnmap.asafew.a` falls back to the same shaders as `dif.spec.a`. Any difference comes from the engine's specular-mask remap (§5.3, H). |
| `paint` | flavor | No shader change (C): `dif.spec.add.env.paint` uses the same shaders as `dif.spec.add.env`. The wiki says `paint` tints with a game-defined colour, usually for AI cars. (H) The engine writes that colour into `diffuse`. |
| `indenv` | flavor | No shader change (C), same shaders as `add.env`. The wiki says reflections become independent of specular light, so surfaces that sunlight never reaches still reflect. (H) This is engine-side, through the scene reflection visibility `E_vis` (§5.5). |
| `shadow` | flavor | Adds a depth-only shadow-caster pass. Colour maths unchanged (C). |
| `skinned`, `tg0`, `add`, `mult`, `attr` | flavor | Out of scope. `add` and `mult` change the fog colour to black or white (C, flavor table). |

Flavors combine independently: each one switches its own term in the equations below. For example, `dif.spec.weight.add.env.tsnmapuv` is the base with §5.2 (UV Normal), the `weight` rows of §5.3 and §5.4, and §5.5 with fresnel. §8 works through this case.

**Technique condition (C):** every effect that has a normal map carries two GLSL techniques. One is marked with a normal-map condition and uses the normal-mapped shaders. The other is the same effect without the normal map. (H) The condition is a graphics-quality switch. Always implement the normal-mapped one.

## 2. Passes per flavor (C)

| Flavor set | Passes, in order of listing | Depth | Cull | Blend |
|---|---|---|---|---|
| opaque (no `over`/`decal`) | 1. deferred attributes (G-buffer write); 2. forward opaque (the same surface, lit in-shader) | test "greater" (reverse-Z), write on | back faces | none |
| `over` | one forward blend pass, the same shader as forward opaque | test "greater", write **off** | back | `src·α + dst·(1−α)` |
| `decal` (no `over`) | 1. "decal post" (deferred-lit variant, §7.3); 2. "decal" (the forward shader) | test "greater or equal", write off, depth offset on | back | none |
| `decal.over` | as `decal` | as `decal` | back | `src·α + dst·(1−α)` |
| `+shadow` | adds a shadow-map depth pass | — | — | — |

- Bevy's main pass also uses reverse-Z, so "greater" maps directly.
- The size of the decal depth offset is engine-side (U).
- (H) The engine runs either the deferred pair or the forward pass for an opaque object, not both. Which one the tuning screen uses is U. Both produce the same equation (§7.2), apart from the extra ambient shaping in the deferred sun pass (§7.4).

**Implement the forward maths (§5) as the canonical model.** `over`, `decal` and the forward opaque pass use it verbatim.

## 3. Inputs

### 3.1 Per material (`.mat` attributes)

Each `.mat` names its attributes as below (`research/evidence/material-survey.txt`). The defaults come from the game's material-input table (C). Use a default when the attribute is absent.

| `.mat` name | Type | Default | Used by | Meaning here |
|---|---|---|---|---|
| `diffuse` | rgb | (1, 1, 1) | all | Albedo tint `Kd` (linear) |
| `specular` | rgb | (1, 1, 1) | all | Specular colour `Ks` |
| `shininess` | float | 4.0 | all | Blinn-Phong exponent `p` |
| `additional_ambient` | float | 0.0 | all | Weight `ka` of the scene's additional ambient colour (§5.6). Values seen: −0.25 to 3. |
| `env_factor` | rgb | (1, 1, 1) | `add.env`, `nofresnel` | Reflection tint `Ke` |
| `fresnel` | 2 floats | (0.2, 0.9) | `add.env` (not `nofresnel`) | (bias `fb`, scale `fs`). The table labels the pair "(bias, scale)" (C). That the first is the additive bias is H, strong: the formula has exactly one additive and one multiplicative constant. |
| `reflection` | float | 0.0 | written to the G-buffer only | Not used by the material's own colour. (H) It weights the engine's screen-space or planar reflections. Ignore it for now. |
| `queue_bias` | int | 0 | `decal`, `over` | Draw-order bias, engine-side |

Textures (slot names as in `.mat`):

| Slot | Kind | Flavor | UV |
|---|---|---|---|
| `texture_base` | 2D RGBA | all | UV0 |
| `texture_nmap` | 2D, two channels used (BC5 or RG) | `tsnmap` | UV0 |
| `texture_nmap` | the same | `tsnmapuv`, `tsnmapuv2` | the "UV Normal" stream (§3.2) |
| `texture_nmap_detail` | 2D, two channels used | `tsnmapuv2` | UV0 (the slot name is H, by analogy with the others) |
| `texture_oclu` | 2D, red channel used | `oclu` | UV1 |
| `texture_reflection` | cube | `add.env`, `nofresnel` | direction (§5.5) |

`texture_mult` is not part of this family. It belongs to other bases, e.g. `dif.spec.mult.dif.spec` and the scene shaders.

### 3.2 Per vertex

Every vector below is in eye (view) space. World space works the same, except for the reflection lookup (§5.5) and the fog directions (§5.7).

- **Position** `P`: the eye-space position, so the eye is at the origin.
- **Normal** `Nv`: transformed by the model-view 3×3, not by its inverse transpose (C). This assumes rigid or uniformly scaled models.
- **Tangent** `(Tv, w)`: xyz transformed like the normal, and `w = ±1` as the handedness. **Bitangent** `Bv = (Nv × Tv) · w`, built per vertex from the unnormalised transformed vectors (C). Only normal-mapped flavors have a tangent.
- **Colour** `c` (RGBA): rgb is used doubled, as `c2 = 2·c.rgb`, so 0.5 is neutral. Alpha is used as is (C). Check what our PMG reader delivers, 0..1 or pre-doubled: this is a 2× brightness factor.
- **UV sets (the game's UV slots):**
  - UV slot 0: base texture, `tsnmap`, the `tsnmapuv2` detail map.
  - UV slot 1: `oclu`.
  - **UV slot 6**, which the game's vertex-stream table labels "UV Normal": the `tsnmapuv` normal map. Its exporter default is the first UV layer.

  In the vertex stage, UV slot *n* is vertex attribute 8 + *n*: slot 0 is attribute 8, slot 1 is attribute 9, slot 6 is attribute 14 (C, `research/evidence/shader-spec-glass-lamp-lum.txt`). Which stored PMG UV set feeds each slot comes from the piece's routing field at +8 ([models.md](../../formats/models.md)): nibble *n* names the set for slot *n* (H, strong: the field routes slot 6 exactly for `tsnmapuv` effects and slot 1 exactly for `oclu`, `mult`, `lamp` and `truckpaint`).
- Each UV slot also gets an engine-side scale and bias for compressed UVs. (H) PMG UVs are floats, so this is the identity.

### 3.3 Per scene and engine (not in `.mat`)

| Symbol | Meaning |
|---|---|
| `L` | Unit vector from the surface **towards** the sun. The engine supplies the sun's travel direction; `L` is its negation. |
| `Csun_d`, `Csun_s` | Sun diffuse and sun specular colours (separate colours) |
| `Camb`, `Aenv` | Ambient colour (rgb) and a scalar, the scene reflection visibility. They come together in one 4-vector. |
| `Cadd` | The scene's "additional ambient" colour, scaled per material by `ka` |
| `M` | 3×3 rotation applied to the reflection vector before the cube lookup. (H) It is the eye-to-world (cube) rotation. |
| `g` | An engine-supplied scalar tied to the reflection cube, multiplying the reflection strength. It has no `.mat` input. (H) It is a per-cube brightness, 1.0 by default. It may be the exposure of the game's dark cubes that [materials.md](../materials.md) lists as Unknown; it matters for chrome. |
| `mremap = (ms, mb)` | Specular-mask remap, scale and bias, used by the plain (non-`weight`, non-`oclu`) variants only. It has no `.mat` input. (H) It is (1, 0) normally; with `asafew` it stretches alpha 32/255…1 to 0…1, following the wiki's description of `asafew` (alpha 0 to 32 is opacity, 32 to 255 is specular). |
| fog | Two fog colours, a direction, density, start distance, height plane and gradient (§5.7) |
| material tag | An engine bit, 1 = interior. It only steers interior light masks in the deferred pass. Ignore it. |

`Camb`, `Aenv`, `Csun_d`, `Csun_s` and the aurora colour come from the CPU's global scene light block (`eutpp_scene_light`). The block's other fields have no shader input, and the reflection term carries no light colour: see [tonemap.md](tonemap.md) §8.

## 4. Texture lookups

All lookups use ordinary filtered sampling with hardware mip selection and the sampler state of the `.tobj`. Nothing is selected by roughness.

| Texture | Channels | Meaning |
|---|---|---|
| `texture_base` | rgb | Albedo |
| `texture_base` | a | Three jobs at once: (1) the specular mask, (2) a multiplier on the reflection strength, (3) the opacity, together with vertex alpha (`a`, `over`, `decal`) |
| `texture_nmap`, `texture_nmap_detail` | r, g | Tangent-space x and y. Decode `x = r·255/127 − 128/127`, i.e. `x = 2.007874·r − 1.007874` (same for y with g), so a byte of 128 is exactly 0. b and a are ignored. |
| (derived) | — | `z = sqrt(sat(1 − x² − y²))` |
| `texture_oclu` | r | Occlusion `o`, 0…1 |
| `texture_reflection` | rgb | Reflected radiance. Alpha unused. |

`sat(t)` means clamp to [0, 1].

## 5. The maths (forward pass, canonical)

### 5.1 View vectors

- `E = −P / sqrt(max(|P|², 10⁻⁶))`: the unit vector from the surface to the eye.
- `d² = |P|²`: the squared eye distance, in metres (H: units are metres).

### 5.2 Normal

**5.2.1 No normal map:** `N = normalize(Nv)`.

**5.2.2 `tsnmap`, `tsnmapuv`:** decode `n = (x, y, z)` from `texture_nmap` (§4).
- `N = normalize(x·Tv + y·Bv + z·Nv)`, using the interpolated vectors as they are. They are not normalised one by one; only the sum is.
- Diffuse factor: `fn = sat(0.399902·z + 0.600098)`. It is 1 for a flat texel and falls to about 0.6 at grazing tilt. `fn` multiplies **only the albedo** (§5.3). Without a normal map, `fn = 1`.

`tsnmapuv` differs from `tsnmap` only in the UV the normal map is sampled with. The base texture stays on UV0 (C).

**5.2.3 `tsnmapuv2` (detail map, rare):**

1. **Detail map.** Decode `nd` from `texture_nmap_detail` on UV0.
2. **Main map and its frame.** Decode `nm` from `texture_nmap` on the UV Normal stream. This map is not framed by the vertex tangents. Its frame comes from screen-space derivatives on the UV Normal coordinates, with v flipped (`u' = u`, `v' = 1 − v`):
   - `t` is the normalised surface gradient of `u'`, and `b` the normalised surface gradient of `v'`, both perpendicular to `Nv`. This is the "cotangent frame": with `dp1`, `dp2` the position derivatives along the two screen axes and `duv1`, `duv2` those of `(u', v')`, `t ∝ (dp2 × Nv)·duv1.u + (Nv × dp1)·duv2.u`, and `b` likewise with the v components.
   - (H) The game negates the screen-y derivatives to suit OpenGL's upward y. Choose signs so that `t` points towards increasing `u'` and `b` towards increasing `v'`.
3. **Re-express the main normal.** Move it into the vertex tangent frame: `nm_eye = nm.x·t + nm.y·b + nm.z·Nv`, then `r = (Tv·nm_eye, Bv·nm_eye, Nv·nm_eye)`.
4. **Combine.**
   - Blend weight `k = sat((1 − r.z) / 0.01)`: 0 when the main map is flat, 1 once it tilts by more than about 8°.
   - Combined tangent-space normal: `n = normalize(nd.x + k·r.x, nd.y + k·r.y, nd.z)`.
5. **Final normal.** `N = normalize(n.x·Tv + n.y·Bv + n.z·Nv)`, and `fn = sat(0.399902·|n.z| + 0.600098)`.

### 5.3 Surface values

Here `B` is the `texture_base` sample (rgba) and `o` the `texture_oclu` red value.

| Variant | Albedo `A` | Specular colour `S` | Opacity `α` |
|---|---|---|---|
| plain `dif.spec` (any of `add.env`, `nofresnel`, `tsnmap…`, `a`, `over`, `decal`) | `B.rgb · Kd · c2 · fn` | `sat(ms·B.a + mb) · Ks` (no vertex colour) | `B.a · c.a` |
| `weight` | `B.rgb · Kd · c2 · fn` | `B.a · Ks · c2` (no remap) | `B.a · c.a` |
| `oclu` (no `weight`) | `o·B.rgb · Kd · c2 · fn` | `o·B.a · Ks` (no remap, no vertex colour) | `B.a · c.a` |
| `oclu.weight` | `o·B.rgb · Kd · c2 · fn` | `o·B.a · Ks · c2` | `c.a` (opaque in practice) |

### 5.4 Sun light (Blinn-Phong, no PBR normalisation)

- `H = normalize(E + L)`.
- `NL = sat(N·L)` and `NH = sat(N·H)`.
- Specular lobe: `lobe = NH^p` when `NH > 0`, otherwise 0.
- **Specular gate** `G`:
  - plain and `oclu`: `G = sat(10·NL)`, a soft cut-off at the terminator;
  - `weight`: `G = NL`.
- `Ld = Camb + Csun_d · NL`: diffuse light.
- `Ls = Csun_s · lobe · G`: specular light.

The game applies no 1/π, no `(p+2)/8` normalisation and no energy conservation. Do not "fix" the model towards PBR: the `.mat` values are tuned for this one.

### 5.5 Reflection (`add.env`, `nofresnel`)

1. **Direction.** `R = P − 2(N·P)·N`, the reflection of the eye-to-surface vector. Its length is `|P|`.
2. **Cube coordinates.** `D = M·R`, with `M` the engine rotation from §3.3.
3. **Far stretch.** Multiply the vertical cube coordinate: `D.y ← D.y · (1 + 20·sat((d² − 1)/90000))`. The factor grows from 1 at 1 m to 21 at 300 m and beyond. At garage distances (under about 15 m) it stays below 1.05, so you may skip it there.
4. **Lookup.** `Cenv = texture_reflection(D).rgb · Ke`; with `weight`, multiply further by `c2`.
5. **Fresnel.** `F = sat(fb + fs·(1 − N·E))`. N·E is not clamped before this: it equals `N·R/|R|` and can go negative with normal maps, and the outer `sat` handles that. With `nofresnel`, `F = 1`.
6. **Strength.** `Estr = g · B.a · F`; with `oclu`, multiply further by `o` (C, read in both `oclu.add.env` and `oclu.weight.add.env.tsnmapuv`). `B.a` is the raw alpha, not remapped.
7. **Reflection term.** `Refl = Cenv · Estr · Evis`, with the scene reflection visibility `Evis = sat(Aenv)` in the forward pass (§7.4 gives the deferred value).

Without `add.env`, `Refl = 0`.

### 5.6 Combination

`colour = (Ld + Cadd·ka) · A + Ls · S + Refl`

No other terms. Emission is zero in this family.

### 5.7 Fog (engine parameters, applied in the forward pass)

- `u = sat(Ê·Dfog)²`, where `Ê = P/|P|` (eye to surface) and `Dfog` is the direction of fog colour 2.
- `Cfog = Cfog1 + u·(Cfog2 − Cfog1)`.
- `h = sat(2^(−γ·(P·n_plane + d_plane)))`: the height falloff above the fog gradient plane, with gradient `γ`.
- `τ = sat(2^(−ρ · max(|P| − s0, 0) · h))`: transmittance, with density `ρ` and start distance `s0`.
- `out.rgb = Cfog + τ·(colour − Cfog)`.

The deferred path does fog in a separate pass (U, not read). For the tuning garage, (H) fog is effectively off (`ρ = 0`, so `τ = 1`).

### 5.8 Output

`out = (rgb from §5.7, α from §5.3)`. The result is linear HDR colour. Exposure, tonemapping and bloom come later, engine-side (§9).

## 6. Alpha and blending

- **`a`:** discard the fragment when `B.a · c.a < 0.05` (C), in every pass, including G-buffer and decal passes. Surviving fragments are opaque. The wiki describes `a` as making areas transparent by the base texture's alpha.
- **`over`:**
  - Output alpha `α = B.a · c.a`. Blend with `src·α + dst·(1−α)`, depth test on, depth write off, back-face culling, drawn after opaque geometry.
  - Lighting is the forward model of §5.
  - Base alpha is still the specular mask and the reflection mask. After blending, specular and reflection therefore scale with roughly `α²`: transparent texels are also unshiny (C, follows from §5.3 and §5.5).
- **`decal`:** depth "greater or equal" with a depth offset, no depth write. The wiki says it draws after all other surfaces and recommends it for most blended shaders.
  - `decal` alone: no blending; the colour overwrites, and with `a` the 0.05 discard gives cut-out decals.
  - `decal.over`: blending as for `over`.

## 7. Deferred path (engine-side; how it relates to §5)

### 7.1 G-buffer contents (attributes pass, C)

| Value | Content |
|---|---|
| normal | `N` (eye space) |
| depth | eye depth |
| specular | `S` and `p` |
| albedo | `A` and `ka` |
| emittance | `Cenv·Estr` (the reflection without `Evis`), stored packed as RGB565 with a shared exponent |
| flags | "has reflection" (set by `add.env`), "alpha tested", "interior" (the material tag), and the specular-gate choice: set by `weight`, it selects `G = NL` |
| other | `reflection` × 4096, capped; shadow strength 1 and offset 0 |

### 7.2 Composition (C)

After the lights have accumulated `Ld` and `Ls` per pixel (plus `Evis` in the diffuse buffer's alpha):

`colour = (Ld + Cadd·ka)·A + Ls·S + (has reflection ? sat(Evis) : 1)·emittance`.

This is exactly §5.6.

### 7.3 Decal post pass (C)

`decal` flavors draw a second variant that does not light itself. It reads `Ld`, `Ls` and `Evis` from the light buffers already computed for the surface underneath, then applies §5.6 with its own `A`, `S` and reflection.

In Bevy, the forward decal pass is enough.

### 7.4 Deferred sun light: differences from the forward lighting (C, read from the base sun-light pass without shadows)

The diffuse, specular and gate terms are the same as §5.4. The specular gate follows the G-buffer flag. On top of that:

- **Sun visibility.** Diffuse and specular are multiplied by:
  - a step that is 0 where `N·L ≤ 0`;
  - `sat` of the distance to an engine clip plane;
  - shadow maps, in the shadowed variants (not read);
  - interior light masks, only for materials tagged as interior.
- **Ambient shaping.** `Ld = Camb·a + Csun_d·NL·vis`, where `a` is built as follows. It is enabled by an engine "directional ambient strength" `s`; the value −1 turns all of this off, and then `a = 1`.
  - `a1 = 1 − 0.75·sat(N·(−Up))`: downward-facing surfaces lose up to 75 %. `Up` is the normal of an engine-supplied world plane, in eye space. (H) It points up: every use below reads as if it were world-up.
  - `a2 = 0.865 + s·((1 − 0.27·sat(0.5 − 0.5·(N·L))) − 0.865)`: surfaces facing away from the sun are darker.
  - `q = 0.6 + 0.4·sat(d²/22500)`: near surfaces (under 150 m) have less ambient.
  - `a = a1 · a2 · sat(q + (1 − q)·(1 − sat(N·E))⁵)`.
- **Reflection visibility.**
  - Luminances use Rec.709 weights: `Y(x) = 0.212673·r + 0.715152·g + 0.072175·b`.
  - `c = 0.5·sat(Up·L)`, an assumed ground albedo of 0.5 times the sun's cosine.
  - `R̂ = normalize(R)` and `ga = R̂·Up`.
  - `ap = 0.45 − (ga ≥ 0 ? 0.45 : 0.55)·ga`.
  - `gs = 1 + ap·(clip − 1)`, where `clip` is the sun clip-plane factor above.
  - `occ = sat( (Y(Csun_d)·c·gs + Y(Camb)) / (Y(Csun_d)·c + Y(Camb)) ) · a`.
  - `Evis = sat(Aenv · occ)`.
- An aurora light (night sky) adds another diffuse term, `Caurora·sat(N·Laurora)`. Ignore it.

Other lights (garage spot and point lights) come from the deferred light-list pass, which was not read (U). (H) They use the same Blinn-Phong lobe and gate per light, with distance attenuation.

## 8. Worked combination: `dif.spec.weight.add.env.tsnmapuv`

1. Sample `texture_nmap` on the UV Normal stream and decode `n`. Then `N = normalize(n.x·Tv + n.y·Bv + n.z·Nv)` and `fn = sat(0.399902·n.z + 0.600098)`.
2. Sample `texture_base` on UV0. Then `A = B.rgb·Kd·c2·fn`, `S = B.a·Ks·c2` and `α = B.a·c.a`.
3. Sun light: `NL`, `NH`, `Ld = Camb + Csun_d·NL`, `Ls = Csun_s·NH^p·NL` (weight gate).
4. Reflection:
   - `R = reflect(P, N)`, `D = M·R`, stretch `D.y`;
   - `Cenv = cube(D)·Ke·c2`;
   - `F = sat(fb + fs(1 − N·E))`;
   - `Refl = Cenv·g·B.a·F·sat(Aenv)`.
5. `colour = (Ld + Cadd·ka)·A + Ls·S + Refl`, then fog, out.

For the typical chrome (`material-survey.txt`): `diffuse` 0.214, `env_factor` 0.977, `fresnel` (1, 1), so `F = 1` everywhere. The look comes almost entirely from `Refl`, and needs `B.a` near 1.

## 9. What Bevy must supply (outside this material)

- Sun direction and **separate** diffuse and specular colours. The ambient colour plus the reflection-visibility scalar `Aenv`, the additional ambient colour `Cadd`, the reflection rotation `M`, and the global reflection scalar `g` (start with `g = 1`, `Aenv = 1`, `M` = view-to-world rotation).
- The `.tobj` sampler states, and sRGB decoding of colour textures by format (below).
- The garage's other lights, with the same lobe (§7.4, H).
- Shadows, if wanted: they multiply the sun's diffuse and specular only, not the ambient or the reflection.
- Exposure, eye adaptation, tonemapping, bloom and colour correction: separate engine passes (U, not read). The material writes linear HDR.
- The specular-mask remap `(ms, mb)`: use (1, 0) until the engine's rule is known.

### Colour spaces

The shaders do no colour-space conversion (C).
- Textures flagged sRGB in the `.tobj` are decoded by the hardware sampler (see [materials.md](../materials.md)).
- Normal maps and occlusion are linear data.
- `.mat` colours are linear.
- Whether the reflection cubes are flagged sRGB follows their `.tobj` (U per cube).

### Reflection cubes in the garage

The material samples **its own** `texture_reflection` cube. The cube is not prefiltered by roughness: the plain lookup of §5.5 uses mip level by screen derivatives. The cubes' blur is baked into the textures, e.g. `fuzzy_reflection`, `soft_reflection`, `vehicle_reflection`.

To match the game, sample the cube in the material shader rather than through Bevy's `EnvironmentMapLight`. Its face order is +x, −x, +y, −y, +z, −z with +y up ([textures.md](../../formats/textures.md)).

Whether the tuning scene substitutes `showroom_cube` or `ui_vehicle_reflection` for the material's cube is U. The garage's own materials name `ui_vehicle_reflection` ([materials.md](../materials.md)). If the engine swaps the cube, it would do so through the cube binding or `M`, not in this maths.

## 10. Open questions and hypotheses

- **H:** the PMG UV set behind each UV slot is the piece's routing nibble for that slot (§3.2, [models.md](../../formats/models.md)). Slot *n* = vertex attribute 8 + *n* is C.
- **H:** the engine's specular-mask remap is (1, 0) normally and the `asafew` stretch otherwise. It could be settled from the executable.
- **H:** `M` is the eye-to-world rotation, and `g` is a per-cube brightness, 1 by default. Both are engine-set; the executable could settle them. The x/z mirroring of the cube against wgpu is U.
- **U:** which path, deferred or forward, renders opaque trucks in the tuning screen. This decides whether §7.4's ambient shaping is part of the target look.
- **H:** `fresnel` = (bias, scale) in that order.
- **U:** the meaning of the normal-map technique condition. **H:** it is a quality option.
- **U:** the light-list pass (spot and point lights), the shadow-map variants, the fog pass, tonemapping.
- **U:** what `reflection` drives (only written to the G-buffer).
- **H:** the slot name `texture_nmap_detail` (`tsnmapuv2`); no sampled `.mat` was checked.

## Sources

- The game's effect files and shader objects in `effect.scs` (1.60.1.7), read under D13. The list is in `research/evidence/shader-spec-dif-spec.txt`.
- The game's material-input, flavor and vertex-stream tables in `effect.scs`, for `.mat` names, defaults and UV stream names.
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders) (paraphrased), for:
  - base alpha as the specular intensity in `dif.spec`;
  - `add.env` as a reflection pass with an environment factor;
  - `tsnmap` on the base UV and `tsnmapuv` on its own UV;
  - `weight` as modified specular;
  - `nofresnel` dropping the fresnel calculation;
  - `asafew`'s alpha split;
  - `indenv`, `paint`, `decal` and `oclu` as described in §1 and §6.
- `research/evidence/material-survey.txt` for attribute ranges and typical chrome values.

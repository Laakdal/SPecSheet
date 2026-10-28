# Shader spec: `eut2.truckpaint` lighting

How the truck paint effect lights its surface in ETS2 1.60.1.7: vertex colour, normal maps, the sun's diffuse and specular terms, and the environment reflection with its fresnel. It continues [truckpaint.md](truckpaint.md), which defines the albedo `A`, the specular colour `S` and power `p`, the mask `M` and the notation. The implementer needs no separate `dif.spec` spec for truckpaint: everything it uses is here.

The sources are those of `truckpaint.md`, listed in `research/evidence/truckpaint-spec-sources.txt`. Labels: **C** = Confirmed (read in the shader), **H** = Hypothesis, **U** = Unknown.

## 1. Two passes

- **Deferred attributes** (`defattr`, the normal path, H): the material computes its surface values and writes them to a G-buffer (§6). The scene's light pass, which is not part of these effects, lights them later.
- **Forward** (`fwdopaque`): the same surface values (C, identical maths in 6 of 6 pairs compared), lit in the shader by one ambient term and the sun (§5).

Implement the forward model. It is the only lighting these effects contain. How far the deferred light pass differs from it is U.

## 2. Geometry inputs

- **Spaces.** Position and normal are transformed to eye space by the model-view matrix. The normal uses that matrix too, not an inverse transpose (C), so the model-view is assumed rigid.
  - `x_eye`: the eye-space position.
  - `V = −x_eye / sqrt(max(|x_eye|², 10⁻⁶))`.
  - `N₀`: the normalised interpolated normal.
- **Vertex colour** `v` (RGBA). Its RGB is doubled: `v₂ = 2·v.rgb` (C). So 0.5 is neutral.
  - `v₂` multiplies the albedo, the specular colour and the reflection.
  - The output alpha is `B.a · v.a`. The effect is opaque, so this alpha is not used.
  - What a mesh without vertex colours supplies is U. (H) It is 0.5 grey, i.e. neutral.

## 3. Normal map (`tsnmap`, `tsnmapuv`)

1. Sample `texture_nmap`. `tsnmap` samples it on UV0 and `tsnmapuv` on UVn (truckpaint.md §2).
2. Decode x and y: `x = R·(255/127) − 128/127`, and the same for y with G. A stored byte of 128 maps to 0, 255 to +1 and 1 to −1 (C).
3. `z = sqrt(sat(1 − x² − y²))`.
4. Basis vectors, all in eye space:
   - tangent `T`: the vertex tangent's xyz;
   - bitangent `Bt = (N₀ × T) · t_w`, where `t_w` is the tangent's 4th component, the handedness sign;
   - normal `N₀`.
5. `N = normalize(x·T + y·Bt + z·N₀)`.
6. **Diffuse factor:** the albedo is multiplied by `sat(0.399902·z + 0.600098)` (C). A flat normal gives 1.0, and a tilted one darkens the albedo, down to 0.6. Without a normal map this factor is 1 and `N = N₀`.

`N` from step 5 is used everywhere, including the flip facing term and the flakes.

## 4. Surface values (both passes)

- **Albedo:** `A' = A ⊙ v₂ · d_n`, where `d_n` is the normal-map factor above.
- **Specular weight:** `w_s = sat(B.a · a_w + b_w)`. Here `(a_w, b_w)` is the "alpha to weight" pair of the material block (C). Its values are engine-set, U. (H) They are (1, 0), since truckpaint has no `weight` flavour.
- **Mask factor:** `m_spec = M.a` for `colormask`, else 1 (C).
- **Specular colour:** `S' = w_s · m_spec · S ⊙ v₂`, with `S` from truckpaint.md §6 (`S_mat` without flakes).
- **Specular power:** `p` (truckpaint.md §6).
- **Additional ambient factor:** `a_mat`, the 4th component of the material block's first row. (H) It is the `.mat` `additional_ambient`.

**Environment reflection** `E` (C):
1. Reflection vector in eye space: `R = x_eye − 2·N·(N·x_eye)`. The position is not normalised here.
2. Rotate `R` into cube space with the 3×3 envmap matrix of the second material block, which is engine-set (H: eye space to world space).
3. **Distance widening:** multiply the rotated vector's y component by `1 + 20·sat((|x_eye|² − 1)/90000)`.
   - Below 1 m from the eye nothing changes. The full factor 21 is reached at 300 m.
   - At tuning-screen distances, about 5–10 m, the factor is about 1.0–1.02.
4. `c_env = sample(texture_reflection, rotated R).rgb ⊙ v₂`.
5. **Fresnel (Schlick):** `F = sat(F0 + (1 − F0)·(1 − sat(N·R̂))⁵)`.
   - `R̂ = R / |x_eye|`, so `N·R̂ = N·V`.
   - `F0` is the 3rd component of the material block's fresnel row, the only component read (26 of 26 objects).
6. **Strength:** `k_env = μ · B.a · m_spec · F`.
   - `μ` is the scalar uniform `texture_reflection_modulator`, engine-set (U).
   - The material's "environment" colour row is read by **none** of the 26 fragment objects. So the `.mat` `env_factor` does not reach truckpaint as a colour (C). Whether `μ` is derived from it is U.
7. `E = c_env · k_env`.

## 5. Forward lighting

The scene supplies:
- `L_amb`: the ambient colour, rgb, with a scalar 4th component `e_amb`;
- `l_sun`: the sun direction in eye space (`L = −l_sun`);
- `D_sun` and `S_sun`: the sun's diffuse and specular colours;
- `A_add`: an additional ambient colour.

The CPU derives these from the global scene light block. The block's other fields (`limited_pre_exposure`, the conditional multipliers, `ec_adjustment`) never reach this shader as inputs, and the reflection `E` takes no light colour: see [tonemap.md](tonemap.md) §8.

With `H = normalize(V + L)`, `n_l = sat(N·L)` and `n_h = sat(N·H)`:

- specular lobe: `σ = (n_h > 0 ? n_h^p : 0) · sat(10·n_l)`. This is plain Blinn-Phong with no normalisation factor; the `sat(10·n_l)` gate fades it out at the terminator.
- diffuse light: `I_d = L_amb.rgb + D_sun · n_l + A_add · a_mat`.
- result: `C = I_d ⊙ A' + S_sun · σ ⊙ S' + sat(e_amb) · E`.

Every truckpaint variant sets the material flag that makes the reflection scale by `sat(e_amb)` (C). There is no shadow term and no 1/π, and nothing is clamped (C). The output is linear HDR (H: tone mapping happens later, outside these shaders).

The forward pass finishes with the scene's height fog: an exponential fog over distance and height, and a lerp from the fog colour to `C`. This is generic and not paint-specific. Skip it for the garage (H: no fog there).

## 6. Deferred G-buffer contents (for reference, C)

| Target | Contents |
|---|---|
| 0 | eye-space normal `N` (xyz), eye-space depth `x_eye.z` |
| 1 | `S'` (rgb), specular power `p` |
| 2 | `A'` (rgb), `a_mat` |
| 3 (integers) | `E` packed as 5/6/5-bit RGB with a shared power-of-two exponent; a reflection strength (the first component of the material's "reflections" pair × 4096, capped at 16383); flags; shadow strength 1; shadow offset 0 |

So in the deferred path the environment reflection reaches the screen as emitted light. Whether `sat(e_amb)` or shadows apply to it there is U. The reflection-strength field is U, likely screen-space or planar reflections. Truckpaint `.mat` files carry no `reflection` key.

## 7. Constants (C)

- normal decode: 255/127, 128/127
- normal-map diffuse factor: 0.399902, 0.600098
- vertex colour scale: 2
- terminator gate: 10
- Schlick exponent: 5
- envmap widening: 1/90000, offset 1, gain 20
- eye-vector clamp: 10⁻⁶
- G-buffer: exponent offset 16 (5 bits), reflection scale 4096 and cap 16383

## 8. Unknown, with the next step

- **Which `.mat` `fresnel` component becomes `F0`.** Seen mats hold `{0.2, 0.9}`, and only one value is read. Next step: the REA run named in truckpaint.md §7; add `fresnel` to its regex.
- **`μ`, the envmap matrix, `(a_w, b_w)` and `e_amb`.** All engine-set; same REA run.
- **Vertex colour.** Its value on meshes without one, and its colour space. Next step: check a truckpaint PMG's vertex streams with our PMG reader.
- **The deferred light pass**, including shadows, the reflection-strength use and any normalisation. It lives outside the truckpaint effects. Next step: a separate spec of the deferred lighting effect (`/effect/deferred/`), if parity requires it.

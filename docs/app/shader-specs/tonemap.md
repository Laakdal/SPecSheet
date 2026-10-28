# Shader spec: tonemapping, eye adaptation and bloom (`def.tonemap`, `def.adapt`)

This spec covers the post-processing chain of ETS2 1.60.1.7 that turns the lit HDR scene into display colour. It gives the luminance histogram, the eye adaptation (auto-exposure), the bloom input, the colour correction and the tonemap curve, plus the order they run in and what the engine has to supply.

It was written clean-room under D13. The maths was read from the game's GLSL shaders and is restated here in our own notation; no shader code is copied. The sources are listed in `research/evidence/shader-spec-tonemap.txt`. The data values cited come from `research/evidence/exposure-tonemap-data.txt`.

Labels: **C** = Confirmed (read in the shader), **H** = Hypothesis, **U** = Unknown.

## 1. Notation

- `Y(c) = 0.2126729·c.r + 0.7151522·c.g + 0.0721750·c.b`: the luminance weights every pass in this chain uses (C). These are the Rec.709/sRGB weights, so colours are linear in sRGB primaries.
- `sat(x) = clamp(x, 0, 1)`, `lerp(a, b, t) = a + t·(b − a)`, `max3(c)` / `min3(c)` = largest/smallest channel.
- `ε = 2^−24 ≈ 5.96046e−8`, the shader's small-value floor (C).
- `H = 65504`, the largest half-float value. The histogram range is anchored to it (C).
- "Scene units" means the lit HDR colour as the lighting writes it, before exposure. "Normalised units" means after multiplication by the exposure factor `k` (§4.1).

## 2. Effects and flavours

| Effect | Role | Read |
|---|---|---|
| `def.histogram` | Builds a 32-bin luminance histogram of a screen region (§3.1). | C |
| `def.adapt` | Turns the histogram into an exposure scale and blends it with last frame's (§3.2–3.3). | C |
| `def.downsample.4x4.rgb_blum` | 4×4 downsample of the scene: average colour plus a firefly-safe bloom luminance (§5.1). | C |
| `def.bloom.init` | Bloom bright-pass (§5.2). | C |
| `def.dual_filter.downsample` / `.upsample` | Dual-filter blur pyramid (§5.3). | C (maths), U (whether bloom uses it) |
| `post_scene_fx.convolve_horizontal` / `_vertical` | Separable blur with up to 65 weight/offset pairs (§5.3). | C (maths), U (use) |
| `def.tonemap` | Base tonemap pass (§4). | C |
| `def.tonemap.col` | Base plus colour correction before the curve (§4.3). | C |

The other `def.tonemap` flavours (160 effects in total) add one step each. They are out of scope for the tuning screen, and this spec does not detail them:

| Token | Adds (from its inputs; maths not detailed) |
|---|---|
| `shaft` | A sun-shaft texture tinted by `sunshaft_color` (C inputs). |
| `dist`, `bdist`, `mdist`, `mbdist` | A distortion texture that offsets the scene lookup (C inputs). |
| `dof` | Depth of field from the G-buffer depth (C inputs). |
| `drr` | Beyond 5 m view depth, the scene is replaced by a 5-tap blur, weighted in exposure-scaled Reinhard space, with strength `drr_strength` (C). |
| `grad` | HSV colour grading from a lookup buffer, after colour correction. It is skipped when the table is empty (C). (H) Photo mode or a debug feature. |

The `def.downsample.*.lumlog`, `.exp` and `.expmax` effects compute a log-average and a maximum luminance chain (C). `def.adapt` does **not** read them: it reads the histogram (C). What uses that chain in 1.60 is U.

## 3. Luminance measurement and eye adaptation

### 3.1 Histogram (C)

Inputs:
- an HDR colour texture in scene units. Which one (full scene or a downsample) is U.
- a region `(x0, y0, w, h)` in that texture's texels.
- a centre-weight shape parameter `q`.

Bins:
1. Each sample's luminance is `L = max(Y(rgb), 1e−5)`.
2. Its bin is `b = trunc(max(31.99 + log2(clamp(L/H, 2^−32, 1)), 0))`, so `b ∈ [0, 31]`. Bin `k` holds luminances in `[H·2^(k−32), H·2^(k−31))`: one stop per bin, from about 1.5e−5 up to H. Everything below the range lands in bin 0.

Weight, per sample at uv `p`, with region centre `m` and region size `s` (both in uv):
- `d = 2·|p − m| / s`, per axis;
- `w₀ = sat(1 − d·d)` (dot product, so the weight is 1 at the centre and 0 at the inscribed ellipse);
- `weight = lerp(w₀, w₀³, q)`.

Bins accumulate the weights, not counts.

Coverage:
- The region is cut down to `10·⌊w/10⌋` columns and `32·⌊h/32⌋` rows.
- Every texel column is sampled at integer texel coordinates `(x0 + i, y0 + j)·texel_size`, with no half-texel offset. With bilinear filtering, each sample therefore averages four texels.
- The game accumulates by additive blending into a 4×2 RGBA float target: texel `(x, y)` holds bins `4·(x + 4y) … 4·(x + 4y) + 3` in r, g, b, a.

### 3.2 Exposure from the histogram (C)

Let `n_k` be the bin weights and `N = Σ n_k`.

1. **Trimmed mean.** Only the weight between the 2 % and 98 % points of the cumulative distribution counts. With cumulative sums `C_k = Σ_{j≤k} n_j`, clipped as `c_k = clamp(C_k, 0.02N, 0.98N)`, and `c_{−1} = 0.02N`, bin `k` contributes `Δ_k = c_k − c_{k−1}`.
2. **Mean luminance.** This is the arithmetic mean of the bin-centre luminances (not a geometric or log mean):
   `L̄ = Σ Δ_k · H·2^(k + 0.5 − 32) / max(Σ Δ_k, 1)`.
3. **Re-quantisation.** `L̄` is re-mapped through the same bin formula without truncation: `L_m = H · 2^(max(31.99 + log2(clamp(L̄/H, 2^−32, 1)), 0) − 32)`. In range, that is `L_m = L̄ · 2^−0.01`, a −0.01 stop offset with a floor at the bottom of bin 0. This is not a floor to whole bins.
4. **Raw exposure.** In stops: `s = log2(1 / max(L_m, 1e−4))`. Exposure is therefore capped at 10⁴. No target grey enters here: the scale maps the trimmed mean luminance to 1.0.
5. **Soft limits.** With engine factors `F0 = (s_min, s_max, g_lo, g_hi)`:
   - if `s < s_min`, then `s ← s_min + (s − s_min)·g_lo`;
   - if `s > s_max`, then `s ← s_max + (s − s_max)·g_hi`.

   The slopes compress the excess. A slope of 0 makes a hard clamp.

### 3.3 Adaptation over time (C)

- `P` = last frame's adapted exposure scale (§3.4, red channel).
- `s_p = log2(P)` if `P > 0`, else `s_p = s`. With no history, adaptation is instant.
- Retention factor `r = F1.x` if `s > s_p` (exposure must rise: the scene got darker), else `r = F1.y`.
- Adapted stops: `s_a = lerp(s, s_p, r)`. The blend happens in stops (log domain), and `r` is the fraction of the old value kept.
- (H) `r = exp(−speed·Δt)` or `2^(−speed·Δt)` per frame, from `dark_adaptation_speed` (rising) and `bright_adaptation_speed` (falling) and the frame time. The executable has a string `eye_adaptation_time_delta`. The exact form is U.

### 3.4 Adaptation output (C)

A 1×1 float texture holding:
- r = `E = 2^(s_a)`, the adapted exposure scale;
- g = `H·2^(b_first − 32)`, the lower edge of the first non-empty bin;
- b = `H·2^(b_last + 1 − 32)`, the upper edge of the last non-empty bin;
- a = 0.

When the histogram is empty, both bins are 0. Every pass read for this spec uses only r. (H) The g and b values serve debugging or the HDR overlay, which was not read. This texel must survive to the next frame as `P`.

## 4. Tonemap pass

### 4.1 Inputs

| Input | Role | Source data (label) |
|---|---|---|
| scene texture | Lit HDR colour, scene units, linear | engine (C) |
| bloom texture | Bloom in normalised units, same uv as the scene (bilinear upscale) | §5 (C) |
| adapted exposure `E` | Texel (0,0), red channel, of the §3.4 texture, fetched once per draw | §3 (C) |
| `T0.x` | Exposure multiplier: `k = T0.x · E` | (H) `target_gray`: the `black`/`integrity` climates fix `E` at 5.555, and 0.18 · 5.555 = 1.0 |
| `T0.y` | Not used by the passes read for this spec | (C for those; HDR overlay, SMAA neighbour and TAA compute not read) |
| `T0.z` = `W` | White level: the output's largest channel value | (U), presumably 1.0 |
| `T0.w` | Strength of the per-channel curve blend (§4.5) | (U) |
| `T1…T5` | Curve segment coefficients and the two breakpoints (§4.4) | (U) derived on the CPU from `contrast` and `shoulder_length` (probably also `target_gray`) |
| `low_intensity_factors (a, b)`, `low_intensity_color` | Low-light tint (§4.2) | (H) `a` = `low_intensity_minimum`, `b` = 1/(`low_intensity_maximum` − `low_intensity_minimum`) |
| `color_correction` (`.xyz`, `.w`) | Balance offset, saturation (`col` flavour) | (H) `color_balance`, `color_saturation`. The grayscale climates have saturation 0, which fits. |

The scene file's `pre_exposure` is not a uniform of any deferred shader (C: searched all 716 deferred GLSL fragment shaders). It enters on the CPU, scaling only emissive inputs (§8.3).

### 4.2 Order of operations (C)

1. `c₀ = max(scene.rgb, 0)`, `Y₀ = Y(c₀)`.
2. Low-light tint, in scene units: `f = sat((Y₀ − a)·b)`, `c₁ = lerp(low_intensity_color · Y₀, c₀, f)`. Below `a`, the colour becomes the tint colour scaled by luminance.
3. Exposure and bloom: `c₂ = k·c₁ + bloom.rgb`. Bloom is added after exposure, unscaled.
4. `col` flavour only: colour correction (§4.3), giving `c₃`. Otherwise `c₃ = c₂`.
5. The tone curve with hue handling (§4.5).
6. Output `(rgb, 1)`. Linear, with no gamma, no dither and no clamp beyond what the curve does.

### 4.3 Colour correction (`col`, C)

With `B = color_correction.xyz` and `S = color_correction.w`:
1. `Y₂ = Y(c₂)`; `u = lerp(Y₂, c₂, S)`, per channel. This is a saturation lerp around grey.
2. `v = √u`, per channel.
3. `v′ = max(v + sat(1 − (2v − 1)²)·B, 0)`. The offset peaks at `v = 0.5` and fades to 0 at `v = 0` and `v = 1`. It is a mid-tone balance in square-root space.
4. `c₃ = v′² · Y₂ / max(Y(v′²), 1e−5)`. This restores the luminance from before the correction.

Edge case: with `S > 1`, a channel of `u` can go negative and `√u` is NaN in the game too. Guard it, e.g. by clamping `u ≥ 0`.

### 4.4 The tone curve `F(x)` (C)

`F` maps a positive value to a positive value. It works in log2 space with three segments: toe, middle and shoulder. Write `T_i.xy` and `T_i.zw` for the pairs of row `i` (`i` = 1…5).

1. `t = log2(max(x, ε))`.
2. Pick the segment from the breakpoints `t₁ = T5.z` and `t₂ = T5.w`:

   | Segment | Condition | `(α, β)` | `(γ, δ)` | `(μ, ν)` |
   |---|---|---|---|---|
   | toe | `t < t₁` | `T1.xy` | `T1.zw` | `T2.xy` |
   | middle | `t₁ ≤ t ≤ t₂` | `T2.zw` | `T3.xy` | `T3.zw` |
   | shoulder | `t > t₂` | `T4.xy` | `T4.zw` | `T5.xy` |

3. `u = max(α·t + β, ε)`; `v = 2^(γ + δ·log2 u)`, i.e. `v = 2^γ · u^δ`; `F(x) = 2^(μ·v + ν)`.

Each segment is a scaled and offset power curve, `y = μ·(2^γ·(αt+β)^δ) + ν`, where `t` is input stops and `y` is output stops. (H) This is the shape of a piecewise-power filmic curve applied to log luminance: a linear middle when `δ = 1`, and a mirrored shoulder through negative `α`/`μ`. How the CPU builds the 14 numbers from `contrast`, `shoulder_length` and `target_gray` is U.

### 4.5 Applying the curve with hue preservation (C)

Let `c = max(c₃, ε)` per channel, `Y = Y(c)`, `M = max3(c)`, `m = min3(c)`, and the target luminance `Lt = F(Y)`.

- `n = c·W/M`: the colour scaled so that its largest channel is W. `Yn = Y·W/M`: its luminance.
- **Case A, `Yn ≥ Lt`:** the target fits inside the gamut at this hue. Output `n·Lt/Yn`, which equals `c·Lt/Y`. The hue and saturation are exact.
- **Case B, `Yn < Lt`:** the target is brighter than the hue allows. The output desaturates toward white:
  1. Progress: `x = sat((Lt − Yn)/(W − Yn))`.
  2. Desaturation amount: `g = lerp(x, x¹⁶, sat(Yn/W)) · sat(1 − 0.65·σ²)`, with saturation `σ = sat((M − m)/M)`. Strongly saturated colours resist.
  3. Hue-dependent white point, with `ρ = (c − m)/M`:
     - `r = sat(ρ.r − ρ.g − ρ.b)`;
     - `gr = sat(ρ.g − ρ.r − ρ.b)`.

     If `r > 0`, use tint `(1, 0.4, 0.35)` with weight `r`; otherwise use tint `(0.6, 1, 0.4)` with weight `gr`. The target is `Tw = lerp((W, W, W), W·tint, weight)`. Pure reds burn toward orange-red and pure greens toward yellow-green, instead of toward white.
  4. `D = lerp(n, Tw, g)`.
  5. Per-channel curve on the unnormalised colour: `P = (F(c.r), F(c.g), F(c.b))`.
  6. Output `lerp(D, P, T0.w · x)`.

### 4.6 Output encoding

- The pass writes linear display colour (C).
- sRGB transfer constants (12.92, 1/2.4, 1.055, 0.0031308) occur in the deferred set only in the SMAA edge-detection and debug shaders (C). SMAA edge detection encodes linear to sRGB itself, so its input is linear.
- (H) The final sRGB encode happens in hardware, on an sRGB render target or swap chain.
- (H, strong) TAA runs before the tonemap, in HDR. `def.taa.tonemap` maps `c ↦ c·k/(1 + c·k)` with the same `k = T0.x·E` (C: the forward map and its vertex-shader exposure fetch were read). (H) `tonemap_invert` undoes it after the TAA resolve.
- The exact pass order around TAA and SMAA is U.

## 5. Bloom

### 5.1 Downsample (`rgb_blum`, C)

One output texel per 4×4 input block, from 16 bilinear taps at offsets ±0.5 and ±1.5 texels:
- rgb = the plain mean of the 16 colours;
- a = `Σ w_i·l_i / Σ w_i`, with `l_i = min(Y_i, ℓ)`, `w_i = 1/(1 + l_i)`, and `ℓ = bloom_params.y / E`. The limit is in normalised units, converted to scene units by E alone, without `T0.x`.

### 5.2 Bright pass (`bloom.init`, C)

From the §5.1 texel (`rgb`, `a`), with `k = T0.x·E`:
- `Lb = a·k`, then `Lb = 0` if `Lb < bloom_params.x`. This is a hard gate, not a soft knee.
- `out = rgb · (Lb / max(Y(rgb), 0.05)) · k · bloom_params.z`.

`k` appears twice: once in `Lb`, then again as a factor. This was read, not a typo, so reproduce it. `rgb` stays in scene units, and the 0.05 floor is in scene units. Output alpha is 1.

(H) `bloom_params = (bloom_threshold, bloom_limit, bloom_intensity)`. This matches the key order in `sun_profile`, the threshold value 1.526e−5 being exactly H·2^−32, and intensities being tiny because `k` multiplies them twice.

### 5.3 Blur (C maths, U wiring)

**Dual filter.** Each tap is pre-weighted by `1/(1 + κ·max3(tap))`, and the weighted sum is divided by `(1 − κ·max3(sum))`. This is a reversible Reinhard on the max channel that suppresses fireflies. `κ` and the tap offset `o` are engine values (U).
- Downsample: the centre tap has weight 1/2; the four taps at `(±1, ±1)·o` have weight 1/8 each.
- Upsample: the taps at `(±1, 0)·o` and `(0, ±1)·o` have weight 1/12 each; the taps at `(±½, ±½)·o` have weight 1/6 each.
- Both write with overwrite blending. How levels are combined, if at all, is U.

**Separable convolve.** `Σ weight_i · tex(uv + offset_i·texel)` along one axis, for up to 65 pairs.
- (H) The weights are a Gaussian of `bloom_standard_deviation`.

Which chain feeds the tonemap's bloom texture, and at what resolution, is U.

## 6. Engine-side values not yet known

- The CPU derivation of `T0.x`, `T0.z`, `T0.w` and `T1…T5` from `sun_profile`.
- `F0`/`F1` from `min_scale`, `max_scale`, the adaptation speeds and the frame time.
- `scale_override` and `stability` (U; possibly an override of `E` and a dead band).
- The histogram region and `q`.
- `κ`/`o` for the dual filter.
- Which climate the tuning screen uses.

Next step: trace `deferred_param_tonemap_params` and `deferred_param_eye_adapt_factors` in the executable (F16). Until then, treat all of these as raw per-frame inputs, so the shader does not change when they are found.

## 7. Implementation notes for the WGSL author

- **Two persistent resources.**
  - A 1×1 float exposure value, ping-ponged so §3.3 can read last frame's value.
  - The 32 histogram bins, cleared every frame.
- **Building the histogram.** Additive blending into a 32-bit float target needs a float32-blendable device feature. A compute pass with atomics is the portable route: accumulate fixed-point weights in `atomic<u32>` and convert back. The trimmed mean (§3.2) is a serial loop over 32 bins and fits one invocation.
- **Input units.** Feed the tonemap scene colour in the game's scene units. If the renderer applies its own camera exposure to lit colour, make it exactly 1 or divide it out. The game's whole exposure is `k = T0.x·E`.
- **Output.** Disable the renderer's own tonemapping and write §4.6's linear output into an sRGB-encoding target. Do not apply gamma twice.
- **Built-in auto-exposure.** A built-in auto-exposure that averages log luminance and adapts in EV per second differs from §3: arithmetic bin-centre mean, a 2–98 % trim, centre weighting and retention factors. Check the details before reusing one.
- **Test vectors.**
  - Identity curve: in every segment `(α, β) = (1, 32)`, `(γ, δ) = (0, 1)`, `(μ, ν) = (1, −32)`, with `W = 1`, `T0.w = 0`, no bloom, low-light factors `a = −1, b = 1` (so `f = 1` always) and `k = 1`. Any colour with `max3 ≤ 1` must come out unchanged, up to the `ε` floor per channel (case A).
  - Fixed exposure: `F0 = (log2 5.555, log2 5.555, 0, 0)`. `E` must be 5.555 for every histogram.
- **Numerics.** Keep the `ε`, 1e−5, 1e−4 and 0.05 floors exactly. They decide black and near-black behaviour.

## 8. Scene light block (`eutpp_scene_light`)

The CPU fills this block; its fields and UI values are in `exposure-tonemap-data.txt` §6–7. **No shader declares any of its fields** (C). The scan covered:
- the engine and material uniform table (`/effect/def/uniforms.sui`) and the other interface tables;
- all 716 deferred GLSL fragment objects;
- the forward and G-buffer objects of `dif.spec.add.env` and `truckpaint`.

The block reaches the GPU only through the combined light inputs below. The CPU arithmetic is now read for the forward and material inputs (`exposure-tonemap-data.txt` §9, static disassembly of the uniform callbacks).

### 8.1 What the shaders receive (C)

| Input | Contents | Use |
|---|---|---|
| forward light row 0 `(rgb, w)` | ambient `Camb`, reflection visibility `Aenv` | `Ld = Camb + Csun_d·NL`; reflection × `sat(Aenv)` |
| forward rows 1–3 | sun direction, sun diffuse `Csun_d`, sun specular `Csun_s` | forward sun term |
| deferred sun light: `ambient_environment (rgb, w)`, `diffuse`, `specular` (spec variants) | the same quantities | `Ld = Camb·a + Csun_d·NL·vis`; `Evis = sat(Aenv·occ)` ([dif-spec.md](dif-spec.md) §7.4) |
| deferred `aurora_diffuse`, aurora direction | aurora colour | deferred sun pass only: adds `C_aur·sat(N·L_aur)` when the direction's squared length is over 0.1. Neither forward object read has an aurora term. |
| `texture_reflection_modulator` (`g`) | per-draw reflection scalar | reflection × `g` |
| `additional_ambient_color` | `Cadd` | adds `Cadd·ka` |

Two properties decide where any pre-exposure can go (C):
- The reflection term (`Cenv·g·…·sat(Aenv)` or `sat(Aenv·occ)`) contains **no light colour**. Scaling `Camb`, `Csun_d` and `Csun_s` leaves reflections unchanged. Emissive and lamp outputs contain no light colour either.
- `occ` is a ratio of luminances of `Csun_d` and `Camb`, so a common scale on the lights cancels inside it.

### 8.2 Field mapping

| Field (UI value) | Effect |
|---|---|
| `ambient`, `diffuse`, `specular` | Become `Camb`, `Csun_d`, `Csun_s`. (H, strong) These are the only light-colour inputs, and the triplets correspond one to one. |
| `environment` (`light_environment`) | Becomes `Aenv`, the 4th component of the ambient row. (H, strong) The deferred name is `ambient_environment`, and it is the only environment scalar. |
| `environment_conditional_mult` (1) | No shader input. It **becomes `g`** (`texture_reflection_modulator`) for materials whose cube is not engine-held (C, `exposure-tonemap-data.txt` §9). It does not multiply `Aenv`. |
| `diffspec_conditional_mult` (1) | No shader input. The forward light rows are `Csun_d·m` and `Csun_s·m` with this `m` (C, `exposure-tonemap-data.txt` §9). |
| `light_source_visibility` (1) | No shader input (C). (H) A sun visibility multiplying the sun terms on the CPU. Inside the shader, sun visibility is only the deferred clip-plane, shadow and interior factors. |
| `aurora_diffuse` | (H, strong) Becomes the deferred aurora colour. |
| `limited_pre_exposure` (0.0973 on the tuning screen, 1 in the world) | No shader input. The CPU multiplies **only emissive inputs** by it: the lamp colour table, the backlight colour table, and material luminance given in nits (× 0.001). The forward light rows and `g` do not read it (C, `exposure-tonemap-data.txt` §9; §8.3). |
| `ec_adjustment` (0) | No shader input. The material callbacks use this field as a night factor `n`: lamp contrast `1 + 3n`, and the day-to-night blend of material luminance (C, `exposure-tonemap-data.txt` §9). Whether it also enters the exposure is U. |

### 8.3 Where pre-exposure goes (C for the forward inputs)

The forward and material callbacks settle it (`exposure-tonemap-data.txt` §9):
- **The light colours are not pre-exposed.** The forward rows carry `Camb`, `Csun_d·m` and `Csun_s·m` with `m = diffspec_conditional_mult` (1 on the tuning screen). On the tuning screen the scene's `light_ambient`, `light_diffuse` and `light_specular` therefore reach the shader unscaled.
- **Emissive inputs are pre-exposed.** Lamp and backlight colours are × `p`, and luminance in nits is × `p · 0.001`. Relative luminance (`luminance_output < 0`) is not.
- So a UI scene's `p` brings emissive sources, given in absolute units, down to the level of its lights. The exposure itself still comes from the tonemap's `E` (§3–§4). Whether the UI path meters the histogram or uses a fixed `E` is still U, and so is the deferred light object's filling (the deferred sun pass reads another copy of the lights).

For the implementer: use the scene's light colours as they are, scale emissives as above, and leave exposure to the tonemap.

### 8.4 Exposure modes, compensation, debug overlay

- **One exposure mode (C).** `def.adapt` always meters the histogram. No "sun light value only" switch or exposure-compensation input exists in `def.adapt`, `def.tonemap*` or the bloom passes, and `F1.zw` is unused.
- **Both settings are CPU-side (H).** Two shader-compatible mechanisms:
  - **Sun-only exposure:** set `s_min = s_max` = the wanted stops with `g_lo = g_hi = 0`. §3.2 then returns that exposure whatever the histogram holds, the same way the `black`/`integrity` climates fix 5.555.
  - **Compensation:** multiply `T0.x` by `2^ec`, or shift both limits.
- **HDR debug overlay (`def.hdr.overlay`, C):**
  - It draws `histogram_params[0]` as a pixel rectangle `(x, y, w, h)`: the metering area of §3.1.
  - It tests the stops `log2(1/L)` against `F0.x` and `F0.y`, so the limits live in the same stops domain as `s`.
  - It marks the bin of `1/E` as the effective middle grey.
  - It flags outputs whose `value/W` is below 3.0329e−4 or above 0.99110. These are approximately the linear values of 8-bit sRGB codes 1 and 254. So `W` is display white, and the output is quantised as 8-bit sRGB (supports §4.6).
  - Content drawn without the tonemap is scaled by a `ui_brightness_scale` (value U).

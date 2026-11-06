# Shader spec: ambient occlusion (GTAO, `def.ssao.*`) and how the lighting applies it

ETS2 1.60.1.7 has a screen-space ambient occlusion of the GTAO kind: one horizon slice per pixel, rotated per pixel and per frame, then denoised in space and time. Its result holds a **bent normal** and two visibilities. The deferred sun and light-list passes use it in their `aoc` variants to darken ambient light, sun light and reflections. Written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-gtao.txt`.

Labels: **C** = Confirmed (read in the shaders or the executable), **H** = Hypothesis, **U** = Unknown. The deferred sun pass without AO is [dif-spec.md](dif-spec.md) §7.4; this spec lists only what the AO changes.

## 1. Notation

- `p`: eye-space position of the pixel, rebuilt as `ray(uv) · z`. `ray(uv)` is the view ray scaled to z = 1, from an engine-supplied linear map of the viewport coordinate (C). `z` is the stored eye depth: negative in front of the camera, which looks down −z.
- `V = −p / |p|`: the direction from the surface to the eye. `I = −V`.
- `N`: the G-buffer normal, eye space (dif-spec §7.1).
- `sky`: the depth sentinel −65504. Pixels with it get no AO (§2.1).
- `sat(x)` clamps to [0, 1]. `lerp(a, b, t) = a + t·(b − a)`.
- `W_ao`: the AO buffer's width in pixels. `(x, y)`: the pixel's integer coordinate in the AO buffer.
- `flags`: the G-buffer's 5-bit material mask (C). Its low 3 bits sit at bits 13–15 of the last word of the flag target, its high 2 bits at bits 0–1 of the word before. Values read from the G-buffer passes and their readers: 1 = interior (the material tag), 4 = has reflection (set by `add.env`), 8 = alpha tested (set by `a`). Neither the `dif.spec` family nor its `add.env` and `a` variants sets 2 or 16; what sets them is U.
- `c0`, `c1`: the engine's two GTAO parameter vectors (`gtao_config0/1`); `A = (Ax, Ay, Az, Aw)`: the AO application parameters (`ao_apply_params`). Their values are U (§7).

## 2. Passes, in order

The executable names seven render targets: full-res z, half-res z, noisy AO, spatial denoise 1/2 and 2/2, temporal denoise, full-res AO, plus the depth and AO history (C). The effects map onto them as below. The order is H, from those names and from each pass's inputs. It is supported by the storage scale: `compute` and `a` store the visibility divided by π/2, `b` stores it unscaled, and the sun pass reads it unscaled. Every pass is a full-screen quad that overwrites its target, ignoring depth (C).

| # | Effect | Reads | Writes |
|---|---|---|---|
| 1 | `def.ssao.zfetch` | G-buffer depth and flags | full-res z (§2.1) |
| 2 | `def.ssao.zreduce` | z | half-res z: the minimum of a 2×2 block (§2.2) |
| 3 | `def.ssao.compute` | half-res z (with mips), G-buffer normal and flags | noisy AO (§3) |
| 4 | `def.ssao.spatial_denoise_a` | noisy AO, z | AO, filtered along x (§4) |
| 5 | `def.ssao.spatial_denoise_b` | that, z, G-buffer normal | AO, filtered along y, plus reflection visibility (§4, §5) |
| 6 | `def.ssao.temporal_denoise` | that, z, last frame's AO and z | temporal AO (§6) |
| 7 | `def.ssao.upscale` | AO, half-res z, full-res z | full-res AO (§6.2) |

The compute pass runs at `gtao_viewport_pixel_scale` times the viewport size (C). (H) That scale is 0.5, given the "half-res z" target.

### 2.1 Depth fetch (C)

`z_out = z` from the G-buffer, except `sky` where the stored depth is 0 or flag 16 is set (§1). (H) Flag 16 excludes a material class from AO.

### 2.2 Reduce (C)

Four taps at ±0.5 source texels; the output is their minimum. (H) With a point sampler, these hit the four texels of the 2×2 block; the sampler state was not read. With negative eye depths, that keeps the farthest of the four. (H) The pass is also run per mip, to give the compute pass its depth mips (up to level 3).

## 3. Horizon search and visibility (`compute`, C)

Pixels with `z = sky` output `(0.5, 0.5, 1, 1)`: a bent normal facing the eye and full visibility.

**Normal.** `N` from the G-buffer, or `(0, 0, 1)` (facing the eye) where flag 2 is set (§1; its name is U). (H) Flag 2 marks surfaces whose normal is not meaningful for AO, such as foliage.

**Radius and steps.**
- `r = min(c0.w / |p|, c1.y)`: the search radius as a fraction of the AO buffer's width. In texture units it is `(r, r·H/W)` times the viewport's uv scale, with y flipped from eye to texture space.
- `R_eye = r·|p|`: the falloff radius in eye space.
- `L = r · W_ao`: the radius in pixels. Steps per side: `n = int(min(L/2, c0.z))`. With `n = 0` there are no steps.

**Per-pixel noise.**
- Slice angle `φ = c0.x + a·π/16`, with `a = ((x + y) & 3)·4 + (x & 3)` (0…15).
- Step jitter `s = (1/n) · 0.25 · (((x − y) & 3) + c0.y)`.
- Slice direction `d = (cos φ, sin φ)` on screen, `D = (cos φ, sin φ, 0)` in eye space.

**March.** Keep two horizon cosines `h₁` (along −d) and `h₂` (along +d), both starting at −1. For `i = i₀ … n`:
1. `t = sat(i/n − s)`. Here `i₀ = 2` when the first step (at `i = 1`) would move less than about 0.7 pixel (squared length ≤ 0.5 pixel²), else 1.
2. Mip level `m = clamp(floor(log2(L·t) · c1.z), 0, 3)`.
3. Sample the depth at `uv ± t·d·(radius in uv)`, at level `m`, with the uv clamped to the viewport. Rebuild each sample position `q`.
4. Per side: `δ = q − p`, `cosH = (δ·V) / |δ|` (with `|δ|²` at least 1e-6). A side whose uv was clamped keeps its current `h`.
5. Distance falloff: `u = sat(|δ|² / R_eye²)`, `f = sat(u·(2.28571·u − 1.28571))`. It is 0 up to `u = 0.5625` and reaches 1 at `u = 1`.
6. Update: if `cosH ≥ h`, then `h ← lerp(cosH, h, f)`; otherwise `h ← lerp(cosH, h, c1.x)`. A higher horizon is taken at once when near, and ignored when at the radius. A lower one pulls `h` down at the rate `1 − c1.x`. (H) This is the thickness heuristic, so thin objects do not occlude forever.

**Angles.** `acos` is approximated as `acos(x) ≈ (π/2 − 0.156583·|x|)·√(1 − |x|)` for x ≥ 0, and `π − that` for x < 0. Then `θ₁ = −acos(h₁)`, `θ₂ = +acos(h₂)`.

**Slice plane and projected normal.**
- `axis = normalize(D × V)`, `T = V × axis` (the slice's tangent).
- `Np = N − axis·(N·axis)`, `ℓ = |Np|` (with `ℓ²` at least 1e-6).
- `cos ν = clamp(Np·V/ℓ, −1, 1)`, `ν = acos(cos ν)` (approximation above). The sign is negative when `Np·T ≥ 0`, positive otherwise. `sin ν = −clamp(Np·T/ℓ, −1, 1)`.
- Clamp the horizons to the normal's hemisphere: `θ₁ ← max(θ₁, ν − π/2)`, `θ₂ ← min(θ₂, ν + π/2)`.

**Visibility.**
`v = ℓ · ¼·Σ_{k=1,2} ( −cos(2θₖ − ν) + cos ν + 2θₖ·sin ν )`.
This is the cosine-weighted visible arc of the slice, scaled by the projected normal's length.

**Bent normal.** `β = (θ₁ + θ₂)/2`, `B = T·sin(−β) + V·cos(−β)`.

**Output.** `(enc(B).xy, N.z, v/(π/2))`. `enc` is the sphere-map packing in §8. The `N.z` channel is the edge-stopping key of the denoise. Dividing by π/2 is only a storage scale: the denoise multiplies it back.

## 4. Spatial denoise (C)

Two 1-D bilateral passes, first along x (`a`), then along y (`b`), in AO texels. Each pass:
1. `sky` pixels output `(0.5, 0.5, 1, 1)`.
2. Decode the centre's `B₀`, `v₀` and key `k₀ = N.z`.
3. Take four taps at `uv + (2j − 0.5)` texels, for `j = −1, 0, 1, 2`, with uv clamped to the viewport minus half a texel. (H) The sampler is bilinear, so each tap averages two texels and a pass spans 8 texels: the half-texel offsets only make sense that way, but the sampler state was not read. With nearest sampling the filter would cover only 4 texels.
4. Weight: `w = 2^( −0.08·j² − (|z_j − z₀|·σ_z)² − (|k_j − k₀|·σ_n)² )`, where `σ_z = gtao_denoise_sharpness.x · sat(400 / z₀²)` and `σ_n = gtao_denoise_sharpness.y`. Depth edges soften beyond 20 m.
5. `v = Σ w·v_j / Σw`, `B = normalize(Σ w·v_j·B_j)`. With `Σw = 0`, keep the centre.
6. Pass `a` outputs `(enc(B).xy, k₀, sat(v/(π/2)))`. Pass `b` outputs `(enc(B).xy, v_R, sat(v))`, where `v_R` is the reflection visibility (§5).

## 5. Applying the AO

### 5.1 The cone test (C)

For a direction `X` (the reflection ray or the light direction), treat the visible part of the hemisphere as a cone around `B` whose cosine is `cosC = √(1 − v)`:
- cone mode: `vis(X, k) = 1 − sat(k · sat((cosC − B·X)/2))`;
- uniform mode, selected by a negative strength: `vis(k) = 1 − sat(|k|·(1 − v))`, independent of `X`.

### 5.2 The adjustment (C)

`adj(x) = sat((1 − Aw)·x^Ax + Aw)`: `Ax` is a contrast exponent and `Aw` a floor (minimum visibility).

### 5.3 Reflection visibility (in `spatial_denoise_b`, C)

`R = I − 2N(N·I)`; `v_R = 1 − sat(Ax·(1 − vis(R, Az)))`. This uses `Ax` linearly here, not as an exponent, and has no `Aw` floor.

### 5.4 Deferred sun pass, `aoc` variants (C)

The changes against dif-spec §7.4:
- **Sun.** `o = adj(vis(L, Ay))`, with `L` the direction to the sun. Contact factor `κ = 1 + σ·(o − 1)`, with `σ = def_light_smap_strength` (the sun's shadow-map strength) `×` the pixel's shadow strength (the G-buffer field, 6 bits / 63). The object-shadow factor is `min(1, κ)`, then the minimum with the interior gobo factor, as without AO. The diffuse (and, in `spec` variants, the specular) sun term is multiplied by it.
- **Ambient shaping.** `a1` and `a2` use `B` instead of `N`. The near-distance and grazing term `q + (1 − q)(1 − sat(N·E))⁵` still uses `N`.
- **Ambient AO.** `a ← a · adj(v)`.
- **Reflection.** `occ = (the luminance ratio of §7.4) · v_R`, in place of `· a`. Then `Evis = sat(Aenv · occ)`.

(H) The pass samples the full-res AO of §6.2 with the pixel's own uv.

### 5.5 Light-list pass, `aoc` variant (C)

For each lit light, both the diffuse and specular factors are multiplied by `adj(vis(Lᵢ, Ay))`, with `Lᵢ` the direction to that light. It is skipped where both factors are already 0.

## 6. Temporal denoise and upscale (C)

### 6.1 Temporal

For each of two reprojections `k = 0, 1`, with the engine matrices `PV_k` (eye → last frame's clip) and `V_k` (eye → last frame's eye):
1. `c = PV_k·(p, 1)`. Reject the reprojection when `|c.x| > c.w` or `|c.y| > c.w`.
2. `uv' = (c.xy/c.w · (0.5, −0.5) + 0.5)` times the viewport's uv scale. Rebuild `p'` from the history depth at `uv'`.
3. `e = 200 · max|V_k·(p, 1) − p'| / |(V_k·p).z|` (largest component). Reject when `e ≥ 1`.
4. `ω_k = max(((1 − e)·(1 − |hist.w − cur.w|))², 0.05)`, where `.w` is the diffuse visibility. A rejected reprojection has `ω_k = 0`.

Both reprojections read the same history textures (C). Then `Ω = max(ω₀, ω₁)`. History `= ` the one with the larger weight when `Ω > 0.6`, else `(ω₀h₀ + ω₁h₁)/max(ω₀ + ω₁, 1e-4)`. Output `= lerp(cur, history, 0.85·Ω)`, all four channels.

### 6.2 Upscale

1. `sky` (from the full-res z) outputs `(0.5, 0.5, 1, 1)`.
2. Take the four low-res depths at ±0.5 AO texels: `e_j = |z_j / z_full − 1|`.
3. When all `e_j < 0.05`, output the AO sampled at the pixel's uv (H: bilinearly, from the first AO binding).
4. Otherwise output the AO at the tap with the smallest `e_j`: uv `+ (±0.5, ±0.5)` AO texels, from the second AO binding. (H) That binding uses a point sampler.

## 7. Engine-side values not yet known (U)

| Input | Use here | Reading |
|---|---|---|
| `c0.x` | slice angle offset | (H) a per-frame rotation, for the temporal pass to gather other slices |
| `c0.y` | step jitter offset | (H) per frame |
| `c0.z` | maximum steps per side | |
| `c0.w` | radius numerator, `r = c0.w/|p|` | (H) a world radius times a projection factor |
| `c1.x` | horizon decay | |
| `c1.y` | largest screen radius | |
| `c1.z` | mip selection scale | |
| `gtao_viewport_pixel_scale` | AO resolution | (H) 0.5 |
| `gtao_denoise_sharpness` (x, y) | depth and normal edge strength | |
| `ao_apply_params` `(Ax, Ay, Az, Aw)` | exponent, light-direction strength, reflection strength (the signs select uniform mode), floor | |
| `def_light_smap_strength` | sun contact strength | also used without AO |
| the two reprojection pairs | §6.1 | (H) last frame and one more history set; with a static camera both equal the identity |

The executable reads `c0`, `c1`, the pixel scale and the sharpness from one GTAO parameter object (at +0x18, +0x28, +0x38 and +0x50; C). Which code fills it was not found. The game has a setting `r_ssao` and an "ssao_mode" option (C, strings). Whether the tuning screen runs GTAO, and at which quality, is U.

## 8. Packing (C)

- `enc(B) = B.xy · 1/√(8·sat(B.z) + 8) + 0.5`.
- Decode: `f = 4·e − 2`, `d = f·f`, `B = (f·√(1 − d/4), 1 − d/2)`.
- Only eye-facing bent normals (`B.z ≥ 0`) survive the packing.

## 9. Implementation notes for the WGSL author

- Bevy's own SSAO is also GTAO-based and outputs a visibility, but not this bent normal or the two directional visibilities. A first step could use Bevy's visibility for `v` and apply §5.2 to ambient only. That is ours, not the game's: say so in the code.
- The game's look depends on §5.4: the bent normal drives the ambient shaping, and AO dims reflections and the sun's contact shadows. Without a reference screenshot, keep `Ax = 1`, `Aw = 0` and the strengths at 1 as stand-ins.
- On the tuning screen the truck is static and the camera only orbits, so the temporal pass mostly smooths the per-pixel slice rotation. A spatial-only version (§4) will look noisier than the game.

# Shader spec: the vehicle fake shadow

The dark area under a truck in ETS2 1.60.1.7 doesn't come from the sun shadow. It comes from a **fake shadow**: the vehicle's `fakeshadow` contour geometry is rendered from a projector into a small depth map. A deferred pass then projects that map onto the scene and darkens the light falling on it. Written clean-room under D13, with no shader code or names copied. Sources: `research/evidence/shader-spec-fake-shadow.txt`.

Labels: **C** = Confirmed (read in the shaders, render states or game data), **H** = Hypothesis, **U** = Unknown. It complements [shadows-and-fake-lights.md](shadows-and-fake-lights.md) §3, which describes the contour pass on its own.

## 1. The two passes

1. **Contour into a depth map (C, shadows-and-fake-lights §3).**
   - The `fakeshadow` geometry (a vehicle's and its wheels' outline) is drawn from the projector into a one-channel target.
   - The value written is `0.5 + 0.5·z`, from the projector's uncorrected clip-space z.
   - Render state: depth "less" with depth write, front faces culled.
   - `nocull` and `a` variants exist.
2. **Apply on the light buffer (C, `def.apply.fake.shadow.dist`, below).**
   - A projector volume is drawn over the scene with front faces culled, so it covers the screen area inside the volume even from inside it.
   - Blend `dst · src`: the pass multiplies the deferred light buffer (diffuse and specular light), not the final colour.

A second apply variant without `.dist` projects the alpha of a colour texture instead of a depth map (§4). Which one the trucks use is H: the `.dist` variant, since trucks carry `fakeshadow` contour geometry and the contour pass writes depth.

## 2. The apply maths (`.dist`, C)

For each covered pixel:
1. **Receiver position.** Rebuild the eye-space position from the G-buffer's linear depth and the pixel's view ray.
2. **Projector space.** `q = Pfs · pos`, where `Pfs` is the projector's matrix (engine-set). `q.xy` is the map coordinate and `q.z` the receiver's depth in the map's units. Sample the projector's colour texture there: `m = M(q.xy)` (rgba).
3. **Four jittered taps.**
   - A 2D jitter vector `j` comes from a small noise texture, tiled every 64 screen pixels, and is scaled to texels.
   - `zr = q.z + b·j.x` (`b` is a depth bias).
   - Sample the map at `q.xy ± j.xy` and `q.xy ± j.zw`, giving occluder depths `d1…d4`.
4. **Per tap:**
   - `Δ = zr − di` is how far the receiver is behind the occluder.
   - Strength `si = sat(1 − (Δ·m.r − f0)·f1)`, where `m.r` scales the distance per texel and `(f0, f1)` is the "scene fade-out": offset and rate.
   - `si` counts only when `Δ > 0`, i.e. the receiver is behind the contour; otherwise 0.
   - So the shadow is full where the ground is close under the contour and fades out further below.
5. **Average** the four: `s = (s1 + s2 + s3 + s4) / 4`.
6. **Projector range fade:** `s ← s · sat(1 − (zr − g0)·g1)`, with `(g0, g1)` the "light fade-out".
7. **Strength:** `I = sat(m.a · k · s)`: the colour texture's alpha masks the projector's footprint, and `k` is the projector's intensity (the red channel of its "diffuse" colour).
8. **Apply to the light buffer:**
   - diffuse light ×= `1 − I`;
   - specular light ×= `(1 − I)²`;
   - the diffuse buffer's alpha (the reflection visibility, dif-spec §7.4) ×= `sat((1 − I)²)`.

Only lighting is darkened: albedo, emission and anything drawn after composition are untouched. A forward-only renderer can apply the same factor to its sun, ambient and reflection terms for pixels under the projector.

## 3. Engine inputs (U), with stand-ins (our choice)

| Input | Status | Stand-in (our choice, not the game's) |
|---|---|---|
| Projector matrix `Pfs` | U: engine-set per vehicle | An orthographic projection straight down (−y) over the truck's footprint plus a margin, from just above its roof to about 1 m below its lowest point |
| Intensity `k` | U | 0.8 |
| Scene fade-out `(f0, f1)` | U | Full shadow up to 0.1 m below the contour, gone by 1 m: `f0 = 0.1`, `f1 = 1/0.9` in metres |
| Light fade-out `(g0, g1)` | U | Fade over the last 20 % of the projector's depth range |
| Colour texture `m` | U | Red 1 (distance unscaled); alpha a soft radial falloff over the footprint (1 in the middle, 0 at the edge) |
| Jitter and bias `b` | U | A 4-tap rotated kernel of about 1 texel; `b` about 1 texel |
| Map resolution | U | 512 × 512 |

**Whether the tuning screen draws it** is U from the executable. The user reports a shadow under the truck in the game's garage, which this pass explains, since the garage itself casts nothing (shadows-and-fake-lights §1). Treat it as on.

## 4. The non-`.dist` variant (C, for completeness)

Same set-up, without the depth map: `I = sat(m.a · k · t)`, with `t = q.z` while `q.z ≤ 1` and 0 beyond. The shadow is then the mask texture's alpha, strengthening with depth through the volume.

## Sources

- The game's `effect/deferred/def_apply_fake_shadow/def.apply.fake.shadow{,.dist}.rfx`, their shaders and the render state `apply_fake_shadow`, read under D13. The list is in `research/evidence/shader-spec-fake-shadow.txt`.
- The `fakeshadow` effect (shadows-and-fake-lights.md §3) and its use: 155 truck models use `eut2.fakeshadow` (`research/evidence/effect-usage-survey.txt`).
- SCS Software, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), paraphrased: `fakeshadow` outlines the vehicle's contour where shadow casting starts, and must outline the whole wheel because wheels move vertically.

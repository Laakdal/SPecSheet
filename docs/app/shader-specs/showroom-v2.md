# Shader spec: `eut2.showroom_v2` (animated background dome)

This spec covers ETS2 1.60.1.7's unlit, animated backdrop shader. It is used by `/model/ui/background_dome.pmd` and `/model/ui/background_dome_trailers.pmd`, which is 2 models, 3 materials and 8 UI-scene uses (`research/evidence/effect-usage-survey.txt`). The "waves" desktop scene `/def/desktop/config.waves.sii` loads the dome as its scene model with look `color1` and variant `c_cabin`.

It was written clean-room under D13: the maths was read from the game's shaders and is restated in our own notation; no shader code or names are copied. Sources: `research/evidence/shader-spec-showroom-v2.txt`. Labels: **C** = Confirmed (read in the shader, the effect tables or the game files), **H** = Hypothesis, **U** = Unknown. The public wiki does not describe this shader.

## 1. Passes (C)

- One forward pass: depth test "greater", depth write on, back-face culling, no blending (the `forward_none` state of dif-spec §2).
- No G-buffer pass and **no lighting**: the colour does not depend on the sun, ambient or reflection.
- Fog is applied as in [dif-spec.md](dif-spec.md) §5.7.

## 2. Inputs

### 2.1 Per material (`.mat`)

| `.mat` name | Default | Used as (C) |
|---|---|---|
| `showroom_speed` | (1, 2, 3) | `(s1, s2, s3)`: horizontal scroll speeds of the three wave layers, in UV units per time unit |
| `showroom_r_color` | (1, 0, 0) | **Only its first component** is used, as `sc`: the vertical scroll speed through the colour ramp. The input table's UI label ("Top (R) color") does not match this use. The sample material has `(0.000077, 0, 0)`. |
| `showroom_color`, `showroom_g_color`, `showroom_b_color` | — | Not read by this shader |

Textures (slot names as in `.mat`). The formats are those of the sample material's textures (C, `.tobj` records):

| Slot | Format | Channel | UV | Meaning |
|---|---|---|---|---|
| `texture` | BC4, linear | r | UV slot 1, scrolled by `s1` | Wave layer 1 (`/model/ui/waves_0.tobj`) |
| `texture_base` | BC4, linear | r | UV slot 1, scrolled by `s2` | Wave layer 2 (`waves_1`) |
| `texture_base_1` | BC4, linear | r | UV slot 1, scrolled by `s3` | Wave layer 3 (`waves_2`) |
| `texture_mult` | BC4, linear | r | **UV slot 0**, not scrolled | Multiplier on the background colour (`waves_mult`) |
| `texture_mask` | 4 × 512, 8-bit BGRX **sRGB**, one mip | rgba | computed (§3.1) | Colour ramp (`/model/ui/bcg_colors.tobj`). The `.mat` sets `mip_filter : none`. |

- All the wave textures repeat in u and v.
- The ramp's format has no alpha channel, so its sampled alpha is 1 (C, by format).

### 2.2 Per vertex

Position, colour `c` (doubled: `c2 = 2·c.rgb`; alpha unused), UV slot 0 `(u0, v0)` and UV slot 1 `(u1, v1)`. The normal is in the interface table but unused by the maths. The sample dome piece routes slot 0 to set 0 and slot 1 to set 1, both spanning 0…1 (C, PMG +8).

### 2.3 Engine values (not in `.mat`)

| Symbol | Meaning |
|---|---|
| `t` | The fourth component of the showroom parameter vector, whose first three are `showroom_speed`. It multiplies every speed. (H) Elapsed time in seconds. |
| `b_ui` | A UI brightness scale multiplying the output. (H) 1.0 by default. |
| fog | As dif-spec §5.7 |

## 3. The maths (C)

### 3.1 Per draw: four ramp colours

The ramp row is `w = t · sc`, with v wrapping because the ramp repeats. Read four colours from the ramp at the centres of its four texel columns, at mip 0:

`K1 = ramp(0.125, w)`, `K2 = ramp(0.375, w)`, `K3 = ramp(0.625, w)`, `K4 = ramp(0.875, w)`.

All four are RGBA. The rgb is sRGB-decoded, and alpha is 1 for this format. The game samples them per vertex, but every vertex gets the same values, so they are constant per draw.

With `sc = 0.000077`, one full cycle through the 512 rows takes `1/0.000077 ≈ 13,000` time units. If `t` is in seconds (H), that is about 3.6 hours: a very slow colour drift.

### 3.2 Per fragment

1. **Wave layers.** Each layer is the squared red value of its texture, at UV slot 1 shifted horizontally by its own scroll:
   - `r = texture(u1 + s1·t, v1).r²`
   - `g = texture_base(u1 + s2·t, v1).r²`
   - `b = texture_base_1(u1 + s3·t, v1).r²`

   Only u scrolls; v stays fixed.
2. **Background.** `X = K4.rgb · texture_mult(u0, v0).r`.
3. **Layer the waves over it, back to front** (each step is a lerp towards the layer colour):
   - `X = X + b·K3.a · (K3.rgb − X)`;
   - `X = X + g·K2.a · (K2.rgb − X)`;
   - `X = X + r·K1.a · (K1.rgb − X)`.
4. **Output.** `colour = c2 · X · b_ui`, then fog, and `out = (colour, 1)`.

Squaring the wave values works as a gamma of 2 on linear BC4 data, which softens the wave edges (C for the squaring; the purpose is H).

## 4. Worked example: `/automat/1a/1ab632d5ded5f7ef.mat` (`background_dome.pmd`, first look)

- `showroom_speed = (−0.05, 0.06, −0.075)` and `sc = 0.000077`.
- At `t = 10`, the three wave layers are offset in u by −0.5, +0.6 and −0.75. Layers 1 and 3 drift one way and layer 2 the other.
- The ramp row is `w = 0.00077`, almost the first row.
- A fragment with wave values 0.8, 0.5 and 0.2 (before squaring) gives `r = 0.64`, `g = 0.25` and `b = 0.04`.
- With mult 1 and the vertex colour neutral (`c2 = 1`):
  - `X = K4`;
  - then 4 % of the way to `K3`;
  - then 25 % of the way to `K2`;
  - then 64 % of the way to `K1`.

## 5. Open questions

- **H:** `t` is the time in seconds. Next step: REA on `eurotrucks2.exe` for the code that fills the showroom parameter constant, or a timed capture of the game's waves scene.
- **H:** the UI brightness scale `b_ui` is 1 by default. Next step: the same REA run, searching for its setter.
- **U:** the `showroom_*` values of the other looks (`color1`, `color2`, `color3`). The waves scene uses `color1`, while §4 used the first look's material. Next step: list the materials of every look of `background_dome.pmd` (our `model_info` example prints only the first look).
- **U:** whether the tuning screen ever shows the waves scene. It is one of the desktop background scenes (`config.waves.sii`); the tuning garage itself is `desktop_garage.pmd`.

## Sources

- The game's effect file `eut2.showroom_v2.rfx` and its shader objects in `effect.scs` (1.60.1.7), read under D13. Their list is in `research/evidence/shader-spec-showroom-v2.txt`.
- The material-input and material-uniform tables (`showroom_*` names, defaults and which uniform they feed), and the effect interface table.
- The sample `.mat`, its texture records, and `/def/desktop/config.waves.sii`.

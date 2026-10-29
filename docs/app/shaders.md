# Shaders and lighting: status and plan

Why the app doesn't look like the game yet, which game shaders matter, and how we get to 1:1 within D13 (clean-room shaders).

## Why it doesn't look right yet

The work so far went into *what* is drawn: the right trucks, parts, positions and the game's own tuning UI. *How it is lit and shaded* is still mostly an approximation ([materials.md](materials.md)). Status on 2026-10-28:

| What you see | Cause |
|---|---|
| The garage looks flat and washed out | Only `texture_base` is drawn. The baked shading (`texture_mult`), the normal maps and the scene's `showroom_cube` reflection are ignored. |
| The paint still looks too smooth | **The surface is done:** colour masks, airbrush liveries, flip colour and vertex colour follow the clean-room spec (`render/truckpaint.{rs,wgsl}`). It is still lit by Bevy's PBR, so the game's highlight, clear-coat reflection and flake sparkle are missing. |
| Chrome and metal look dull or dark | The truck's most common reflection cube is used as Bevy's environment light (`viewer::ReflectionCube`), with our own exposure. The game samples each material's own cube with its own fresnel; that comes with the shared lighting module. |
| No shadows on or under the truck | The `fakeshadow`/`shadowonly` geometry is hidden, and real-time shadows are off. |
| The overall light feels off | The lights are in the game's units and the game's tonemap pass runs, but with a fixed exposure and an identity curve: the tuning screen's sun profile, eye adaptation and bloom are not reproduced ([scenes.md](scenes.md)). |

## Which shaders the trucks use

Measured with `effect_usage` (`research/evidence/effect-usage-survey.txt`): 7,756 truck and UI-scene models use 5,002 materials and **196 distinct effect names**. An effect name is a **base shader plus flavors**. For example `eut2.dif.spec.weight.add.env.tsnmapuv` is the base `dif.spec` with the flavors `weight`, `add.env` and `tsnmapuv`. So the job is one material system with switchable features, not 196 shaders.

Share of all 24,441 model uses:

| Base shader | Share | What it is (SCS's description, paraphrased) |
|---|---|---|
| `dif.spec` | 60.0 % | Diffuse plus specular from one texture; the texture's alpha is the specular strength |
| `shadowonly` | 16.3 % | Invisible sun-shadow casters |
| `truckpaint` | 6.4 % | Paint from base and paint-job textures; modes default, colormask, airbrush; flipflake |
| `lamp` | 5.0 % | Lights; a mask texture marks the parts that light up when on |
| `dif.lum` | 3.8 % | Diffuse with self-illumination weighted by texture alpha (night lighting) |
| `glass` | 3.2 % | Alpha is the specular map; tint colour and tint opacity |
| `decalshadow` | 1.2 % | Transparent baked shadows |
| `fakeshadow` | 0.6 % | Shadow-volume contours (wheels) |
| everything else | under 4 % together | `dif`, `dif.spec.mult.dif.spec` (baked occlusion over a tiled base: the tuning garage), `light.tex`, `unlit.*`, scene shaders such as `showroom_v2`, `baked`, `water` |

| Flavor | Share | Meaning |
|---|---|---|
| `add.env` | 44 % | Adds a reflection pass and texture, scaled by an environment factor |
| `tsnmap` / `tsnmapuv` | 30 % | Tangent-space normal map (the `uv` variant uses another UV set) |
| `weight` | 28 % | Modified specular |
| `a` / `over` / `decal` | 7 % / 7 % / 5 % | Alpha test, blended overlay, decal |
| `nofresnel` | 5 % | Reflection without the fresnel term |
| `paint` | 3 % | Takes the truck paint colour |
| `altuv`, `airbrush`, `flipflake`, `colormask` | about 2 % together | `truckpaint` modes |

The paraphrases come from SCS's public modding wiki, [Supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders), which documents 44 base shaders, their inputs and their flavors. It is SCS's own description of behaviour, not code, so it is a primary input for the specs. Cite it; don't copy its text.

## Plan

### Step 1: quick wins (no shader code involved)

Status: 3 is partly done (one cube for the whole truck, not the scene's `showroom_cube`); the rest is open. Items 1, 2 and 5 are covered by the `dif.spec` material in step 2, so they are only worth doing on their own if that material is far off.

These use only `.mat` parameters, textures and Bevy's renderer:
1. Draw `texture_mult` (baked shading) as an occlusion layer, on the garage and on the truck.
2. Normal maps (`tsnmap` / `tsnmapuv`, BC5), on the UV set the flavor names: done for `dif.spec` in step 2.
3. The scene's `showroom_cube` (and other cube textures) as Bevy's environment map, loaded at runtime from the install. This replaces the gradient stand-in.
4. Real-time shadows from the scene's main light.
5. The `.mat` values (`specular`, `shininess`, `env_factor`, `diffuse`) on every material.

### Step 2: the game's look (clean-room shaders, D13)

| Base | Spec | WGSL |
|---|---|---|
| `truckpaint` | Done: [shader-specs/truckpaint.md](shader-specs/truckpaint.md), [truckpaint-lighting.md](shader-specs/truckpaint-lighting.md) | Surface done, under Bevy's lighting ([materials.md](materials.md#truck-paint-eut2truckpaint)) |
| `dif.spec` | Done: [shader-specs/dif-spec.md](shader-specs/dif-spec.md), with the lighting model and the deferred path | Done on the shared lighting module, with per-material cube sampling (§5.5) and normal maps (§5.2) |
| `dif.spec.mult.dif.spec` (with `add.env`, `tsnmap`, `tsnmapuv`, `paint`, `iamod`) and `dif.spec.decal.mult` | Done: [shader-specs/dif-spec-mult.md](shader-specs/dif-spec-mult.md), as deltas on `dif-spec.md`: `texture_mult` on UV slot 1 multiplies the base, and the tuning garage is almost all this base | Written as flavours of the `dif.spec` material ([materials.md](materials.md#dif-spec-eut2difspec)), `iamod` without its third layer; not yet seen in the app |
| `baked.spec.add.env`, `.paint` | Done: [shader-specs/baked.md](shader-specs/baked.md): per-texel shading textures; seen only on far truck LODs (Hypothesis), so the tuning screen likely never draws it | Not started |
| `showroom_v2` | Done: [shader-specs/showroom-v2.md](shader-specs/showroom-v2.md): unlit, animated background dome | Not started |
| `glass` (+ `tsnmap`, `tsnmapuv`) | Done: [shader-specs/glass.md](shader-specs/glass.md): a multiply pass (tint² × (1 − opacity)), then an additive surface pass with a Schlick reflection | Not started |
| `lamp` (+ `add.env`, `tsnmap(uv)`) | Done: [shader-specs/lamp.md](shader-specs/lamp.md): `dif.spec.add.env` plus a masked glow from the engine's lamp colours (zero with the lights off) | Not started |
| `dif.lum` (+ `lvcol`, `decal.over`) | Done: [shader-specs/dif-lum.md](shader-specs/dif-lum.md): lit surface plus emission weighted by base alpha | Not started |
| `unlit.tex`, `unlit.vcol.tex` | Done: [shader-specs/unlit.md](shader-specs/unlit.md) | Not started |
| `decalshadow`, `light.tex`, `fakeshadow`, `shadowonly`, `none` | Done: [shader-specs/shadows-and-fake-lights.md](shader-specs/shadows-and-fake-lights.md): `decalshadow` is black with base red as opacity; `light.tex` writes the deferred light buffer; the rest are never drawn in colour | Not started |

1. **Specs.** A spec agent reads the game's shader for one base and its flavors, and writes the maths in prose and equations to `shader-specs/<base>.md`, citing the wiki for behaviour. No code is copied or quoted.
2. **WGSL.** A different agent, which has never seen the game's shaders, writes one Bevy material per base, with the flavors as switches (shader defs), from the spec only.
3. **Order by coverage:**
   1. `dif.spec` with `add.env`, `tsnmap(uv)`, `weight`, `nofresnel`, `a`, `over` and `decal`: about 60 % of all uses, most of the look.
   2. `truckpaint` with `altuv`, `airbrush`, `colormask` and `flipflake`, together with the paint-job data ([TUNING_SYSTEM.md](../TUNING_SYSTEM.md), paint research).
   3. `glass` and `lamp` (including `add.env`).
   4. `dif.lum`, `decalshadow`, `light.tex` and `unlit.*`.
   5. The tuning garage's base `dif.spec.mult.dif.spec` (`texture_mult` baked occlusion), then the scene shaders `showroom_v2` and `baked`.
   6. Lighting model, exposure and tonemapping.
4. **Measure.** After each base, run `specsheet --parity` against the reference screenshots and record the scores in [../PARITY.md](../PARITY.md). Reference screenshots are deferred for now; until they exist, engine conventions (exposure, tonemapping, camera) come from the executable via REA, and each material is checked by eye in a `SPECSHEET_SCRIPT` screenshot.

Not planned: shaders the trucks and garage don't use (roads, terrain, buildings, sky, water, grass).

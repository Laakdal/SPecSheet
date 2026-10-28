# Materials and textures

How the app draws game materials. The effect files themselves are described in [../formats/effects.md](../formats/effects.md), and `.mat` and `.tobj` in [../formats/models.md](../formats/models.md) and [../formats/textures.md](../formats/textures.md).

## Effect tokens → Bevy `StandardMaterial`

`specsheet-data::render::MaterialInfo` classifies each `.mat` by its effect tokens. `crates/specsheet/src/assets.rs::material` maps the result:

| Effect tokens | `MaterialInfo` | Bevy |
|---|---|---|
| `shadowonly`, `fakeshadow`, `decalshadow`, `mlaaweight` | `Hidden` | not drawn (filtered in `build_mesh`) |
| `glass`, `decal`, `over`, `water` | `Blend` | `AlphaMode::Blend`; glass opacity 0.35, water 0.6 |
| `add` not followed by `env` (e.g. `dif.lum.add`) | `Additive` | `AlphaMode::Add` (light shafts, glows) |
| `a` | `AlphaTest` | `AlphaMode::Mask(0.5)` |
| `truckpaint` | `paintable` | the truck paint material (below) |
| `lamp`, `lum`, `light` | `emissive` | emissive = base texture × 40 |
| `decal`, `over` | `decal` | `depth_bias` 8 |
| an `env_factor` on an opaque or cut-out, non-paint, non-lamp material | `env_factor`, `shininess` | reflective metal (below) |

**Reflective materials (chrome, polished metal).** The game draws chrome as a dark `diffuse` (about 0.21) plus an environment reflection scaled by `env_factor` (about 0.98), with `shininess` 70 and a `texture_reflection` cube (`research/evidence/material-survey.txt`). `assets::reflective_surface` turns that into PBR:
- metallic = the luminance of `env_factor`, clamped to 0–1;
- base colour = `diffuse` blended towards the `env_factor` colour by the same amount, so chrome becomes a bright metal and a barely reflective part stays close to its diffuse;
- perceptual roughness = `(2 / (shininess + 2))^¼`, the GGX roughness matching a Blinn-Phong exponent (70 → 0.41), clamped to at least 0.089.

Glass, decals and lamps keep their own rules; paint has its own material.

- Colours in `.mat` (`diffuse`, `env_factor`) and paint jobs (`base_color`) are linear RGB, and are used as such. Paint values such as Coffee Brown (0.034, 0.023, 0.014) only make sense as linear. Textures flagged `_SRGB` are uploaded as `*UnormSrgb` formats.
- `MaterialInfo` also carries `specular` and `fresnel`; only the paint material uses them. Per `.mat`, only `texture_base` is uploaded. Normal maps and `texture_mult` (baked AO) are ignored; paint masks and flake noise come from the paint job.
- Other materials use metallic 0.1 and roughness 0.6.

**Environment map (the game's reflection cube).**
- `viewer::ReflectionCube` waits until every truck model has loaded, then counts the `texture_reflection` of the truck's reflective materials and takes the most common cube. For the Scania S this is `/material/environment/fuzzy_reflection/fuzzy_reflection.tobj`.
- `render::load_cube_texture` loads all six faces on the async pool; `assets::cube_image` validates them: six square, power-of-two faces with identical mip chains, BC dimensions a multiple of 4, BC support on the GPU.
- The cube goes on the camera as Bevy's `GeneratedEnvironmentMapLight`, which filters it on the GPU. Until it is ready, or when it fails validation, the hemispherical gradient stays.
- Face order is taken as stored: +x, −x, +y, −y, +z, −z, the order wgpu expects. Sky up and ground down are Confirmed ([../formats/textures.md](../formats/textures.md)); x/z mirroring is Unknown.
- **Exposure (ours):** the cube's mean luminance, from its smallest mip, is scaled to the gradient's (about 0.5), clamped to ×0.25–×4. The game's cubes are dark, and the base cubes hit the ×4 cap. How the game itself exposes them is **Unknown** (REA queued).
- The garage scene's own materials use `/material/environment/ui_vehicle_reflection.tobj`, which no truck material names. It is a flat grey cube: all six faces are (0.258, 0.246, 0.210) (`research/evidence/reflection-cubes.txt`). Whether the game substitutes it for vehicle reflections in UI scenes, which would give flat reflections on the tuning screen, is **Unknown** (REA queued).

See [scenes.md](scenes.md) for the environment strength.

This is an approximation, not the game's shaders.

## Truck paint (`eut2.truckpaint*`)

`crates/specsheet/src/render/truckpaint.{rs,wgsl}`: a Bevy `ExtendedMaterial<StandardMaterial, TruckPaint>` written from the clean-room spec [shader-specs/truckpaint.md](shader-specs/truckpaint.md).

**Provenance (D13).** A spec agent read the game's truckpaint shaders and wrote `truckpaint.md` and `truckpaint-lighting.md` (sources: `research/evidence/truckpaint-spec-sources.txt`). The session that wrote the WGSL and its Rust side never saw the shaders, their extracted text or that evidence file; it read only the two spec pages.

**Per piece.**
- Each placed accessory's paint comes from `specsheet_data::paint::piece_paint` (F15). Placements without a paint job take the configuration's colour.
- `render::PaintMaterials` shares one material per (`.mat`, paint). It loads the mask and the flake noise on the async pool.
- A texture that is still loading or failed leaves its flag clear. The piece then draws in the plain paint colour.

**What follows the spec.**
- **Paint colour:**
  - plain: P;
  - colour mask: blue, then green, then red mask colour over P;
  - airbrush: the livery over the paint by its alpha;
  - flip: the facing/averted blend of P and the flip colour.
- **Albedo** = base texture × `.mat` diffuse × paint colour × 2 × vertex colour.
- **Constant masks** (`shared_RRGGBBAA`) enter as linear RGB with linear alpha.
- **Flakes:** the noise at `(u·s, v·s·r)` changes only the highlight's colour and its power (`τ`, coverage `c`). Test case: Under Wraps on the MAN TGX Euro 6 has no mask; its gift-wrap pattern comes from its own flake noise (`tile_christ3.tobj`). The pattern shows in the highlights since missing paint values take the game's defaults: before, a job without `flake_density` got density 0, so no flakes were drawn.
- **Mask factor:** the colour mask's alpha and the base alpha scale the specular and the reflection.

**Settled by our data** (`research/evidence/paint-vertex-streams.txt`, example `paint_uv_survey`, 7,740 truck models):
- **Paint UV (Confirmed on screen).** Sets count from 0, as the spec says: the mask and flakes use PMG UV set 1, and `alternate_uvset` jobs use set 2 (`specsheet_data::render::paint_uv_set`).
  - Test: Bulgarian Flag Decal (`alternate_uvset`) on the Actros 2014 Gigaspace cab, whose paint piece has 3 sets. From set 2 the flag lands on the door; from set 1 it does not show.
  - Our first reading (mask on set 0, a `.mat`'s `altuv` moving it to set 1) also lined up Altamira and Driving Academy on the MAN and Actros cabs. Those `.mat`s name `altuv` and the jobs don't, so both readings land on set 1 there.
  - A mesh without the set falls back to its last one (Hypothesis). 393 of 1,725 paint pieces lack set 1, among them the Scania S cab (only set 0), whose masks line up.
  - The mesh carries the chosen set as Bevy `UV_1`. Paint pieces whose `alternate_uvset` set differs get a second mesh, which the viewer picks for such jobs.
- **Vertex colours (Confirmed).** Every paint piece has a colour stream, so the spec's "mesh without one" case does not arise for paint. The mean is grey 0.326, so 2 × colour ≈ 0.65 darkens paint about 35% compared with before. The colours are uploaded doubled, and only on paint meshes. Byte order: lowest byte red, a **Hypothesis**; only 464 of 2.8M vertices are not grey.

**Variant choice.** The game rebuilds a painted part's material as `eut2.truckpaint` plus flags taken from the paint job; the `.mat`'s own `airbrush`, `flipflake` and `altuv` are replaced (REA: `research/evidence/rea-paint-variant.txt`, F17).
- **Livery or colour mask (Confirmed, REA and on screen):** only the paint job's `airbrush` decides. With a mask, the paint is a livery if the job says `airbrush`; else a colour mask, unless the job is `flipflake`: the game never combines colour mask and flipflake. No base-game flipflake job has a mask without `airbrush` (9,526 flipflake jobs), so that rule shows nowhere yet. The MAN TGX Euro 6 cab's `.mat` names `airbrush`, yet its Driving Academy job draws correctly only as a colour mask; drawn as a livery, the mask's channel weights showed as red and green stripes.
- **`altuv` (Confirmed, REA and on screen):** the paint job's `alternate_uvset`, except on a part listed in the job's per-part file, which never takes it (`PiecePaint::alternate_uvset`).
- **`flipflake` (Confirmed, REA):** the paint job's flag alone.
- **`flipflakeuv`:** not implemented. What selects it is Unknown, and the flakes use the mask's UV.

**Missing paint values** take the game's defaults (F17, `specsheet_truck::DEFAULT_*`), applied in `specsheet_data::paint::piece_paint`:
- base white;
- mask colours red, green and blue;
- flip red, strength 0.27;
- flake green, density 1, shininess 50, clear-coat roll-off 2.2, UV scale 32, v-ratio 1.

The game copies these into the material unscaled.

**Ours, in place of the spec's Unknowns** (constants at the top of `truckpaint.rs`):

| Input | Value | Spec item |
|---|---|---|
| φ (`flip_strength`) | clamped to 0–1 | truckpaint.md §7: values reach 2.7 |
| ρ (`flake_density`) | clamped to 0–1 | §7: values reach 2 |
| r (`flake_vratio`) | 1 when 0 | §7 |
| λ, flake light | 1; the sun's direction | §7 |
| F0 | the `.mat`'s first `fresnel` value, else 0.2 | lighting §8 |
| μ, `texture_reflection_modulator` | 1 | lighting §4 |
| (a_w, b_w), alpha to specular weight | (1, 0) | lighting §4 |
| `specular`, `shininess` absent | 1, 130 (the old roughness 0.35) | — |
| no paint job and no colour | 0.6 grey | — |

**Lighting: the game's forward model** ([truckpaint-lighting.md](shader-specs/truckpaint-lighting.md) §4–5), from the shared module `render/game_lighting.wgsl`, which `dif.spec` will use too:
- ambient plus the sun's diffuse, unnormalised Blinn-Phong specular with the `sat(10·N·L)` gate, and the `.mat`'s `additional_ambient` (the scene gives no additional ambient colour, so that term is 0);
- the reflection from the `.mat`'s own `texture_reflection` cube, with Schlick's fresnel, scaled by `sat(light_environment)`;
- no normal maps yet (`tsnmap`/`tsnmapuv` need tangents); the normal-map factor is 1.

The scene's light reaches every game material through one shared storage buffer (`render::GameLighting`), filled from the scene's `light_direction`, `light_diffuse`, `light_specular` (else the diffuse colour), `light_ambient` and `light_environment` (Aenv, Hypothesis), or the viewer's defaults.

Ours, each a Hypothesis:
- **Output scale 1.** The game's linear HDR goes straight into Bevy's ACES tonemapper. The game's eye adaptation and tone mapping are Unknown (F16). At 1, the MAN TGX Euro 6's white Driving Academy cab matches the unpainted parts beside it, which still use Bevy's lighting; at 1/π it drew grey.
- **Cube direction.** The reflected vector in world space, with z flipped as Bevy samples its environment cubes, which keeps the orientation the viewer's reflections had. The game's x/z mirroring is Unknown; if reflections look left/right flipped, suspect that.

Seen working: Under Wraps now reads dark crimson like its thumbnail (brown under Bevy's lighting), with the forest cube of `vehicle_reflection.tobj` upright on the cab. Until `dif.spec` is ported, paint and its unpainted neighbours use different lighting models.

## Texture upload

`specsheet-data::render::load_texture` unpacks a `.tobj` surface chain. `assets::texture_image` then builds a Bevy `Image` with the full mip chain and the sampler from the `.tobj` (repeat or clamp, linear filtering, 4× anisotropy). Before upload it rejects, with a warning rather than a wgpu panic:

- BC surfaces whose base size is not a multiple of 4;
- a mip whose size is not `max(1, base >> level)`;
- a mip whose byte count does not match its block-padded size;
- more mips than the chain allows, sides above 16384, or no surfaces;
- any BC texture when the adapter lacks `TEXTURE_COMPRESSION_BC`.

A rejected texture leaves its material untextured. Meshes are checked the same way: attribute counts must match and every index must be in range (`assets::piece_mesh`).

| `TextureFormat` | wgpu format |
|---|---|
| BC1 / BC2 / BC3 | `Bc1RgbaUnorm[Srgb]` / `Bc2…` / `Bc3…` |
| BC4 / BC5 | `Bc4RUnorm` / `Bc5RgUnorm` |
| RGBA8 / R8 | `Rgba8Unorm[Srgb]` / `R8Unorm` |

R16G16 (two car-paint normal maps) is unsupported.

## Next: the game's own shaders

The game's shaders are readable GLSL inside `effect.scs` ([../formats/effects.md](../formats/effects.md); survey in `research/evidence/effect-shaders-survey.txt`). We write our own WGSL Bevy materials clean-room (D13): a spec agent writes the maths to `docs/app/shader-specs/<effect>.md`, and a different agent writes the WGSL from it without seeing the shaders.

Order:
1. `truckpaint`: the surface and the game's lighting are done (above).
2. `dif.spec` on the shared lighting module, with per-material cube sampling (dif-spec §5.5), which replaces `ReflectionCube` and `reflective_surface` for those materials.
3. Glass.
4. Lamp.

Garage materials also need `texture_mult` and the `showroom_cube` reflection. Never commit extracted shader text; it is game code.

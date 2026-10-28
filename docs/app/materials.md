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
- **Flakes:** the noise at `(u·s, v·s·r)` changes only the highlight's colour and its power (`τ`, coverage `c`).
- **Mask factor:** the colour mask's alpha and the base alpha scale the specular and the reflection.

**Settled by our data** (`research/evidence/paint-vertex-streams.txt`, example `paint_uv_survey`, 7,740 truck models):
- **Mask UV (Confirmed by fit).** The mask uses PMG UV set 0, and alternate-UV paint uses set 1 (`MeshPiece::uvs2`, Bevy `UV_1`). The spec's guess that sets count from 0 (mask on set 1, alternate UVs on set 2) would leave 393 of 1,725 paint pieces without their set; this reading leaves 1. A piece without set 1 falls back to set 0.
- **Vertex colours (Confirmed).** Every paint piece has a colour stream, so the spec's "mesh without one" case does not arise for paint. The mean is grey 0.326, so 2 × colour ≈ 0.65 darkens paint about 35% compared with before. The colours are uploaded doubled, and only on paint meshes. Byte order: lowest byte red, a **Hypothesis**; only 464 of 2.8M vertices are not grey.

**Variant choice (Hypothesis).** The flags are the union of the `.mat`'s effect flavours (`airbrush`, `flipflake`, `altuv`) and the paint job's (`airbrush`, `flipflake`, `alternate_uvset`). A mask makes the paint a colour mask unless it is airbrush. Many `.mat`s already name flavours (e.g. 72 `eut2.truckpaint.altuv` pieces), so the engine does not only append them from the paint job, as the spec guesses. `flipflakeuv` is not implemented: what selects it is Unknown, and the flakes use the mask's UV.

**Ours, in place of the spec's Unknowns** (constants at the top of `truckpaint.rs`):

| Input | Value | Spec item |
|---|---|---|
| φ (`flip_strength`) | clamped to 0–1 | truckpaint.md §7: values reach 2.7 |
| ρ (`flake_density`) | clamped to 0–1 | §7: values reach 2 |
| r (`flake_vratio`) | 1 when 0 or missing | §7 |
| λ, flake light | 1; the sun's direction | §7 |
| F0 | the `.mat`'s first `fresnel` value, else 0.2 | lighting §8 |
| `specular`, `shininess` absent | 1, 130 (the old roughness 0.35) | — |
| no paint colour | 0.6 grey | — |

**Not the game's yet: the lighting.** The paint feeds Bevy's PBR lighting, like every other material, so the truck stays lit by one model. The mapping:
- metallic 0;
- F0 = the fresnel value × the specular colour's ratio to the `.mat`'s × base alpha × mask factor;
- perceptual roughness from the Blinn-Phong power, as for chrome.

The spec's own lighting (ambient, unnormalised Blinn-Phong with the terminator gate, Schlick cube reflection, normal maps) replaces it together with dif.spec's, in one shared WGSL module ([shader-specs/dif-spec.md](shader-specs/dif-spec.md)).

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
1. `truckpaint`: the surface is done (above).
2. `dif.spec`, with the shared game lighting model for both. This includes per-material cube sampling (dif-spec §5.5), which replaces `ReflectionCube` for those materials.
3. Glass.
4. Lamp.

Garage materials also need `texture_mult` and the `showroom_cube` reflection. Never commit extracted shader text; it is game code.

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
| `truckpaint` | `paintable` | base colour replaced by the paint colour; metallic 0.35, roughness 0.35 |
| `lamp`, `lum`, `light` | `emissive` | emissive = base texture × 40 |
| `decal`, `over` | `decal` | `depth_bias` 8 |
| an `env_factor` on an opaque or cut-out, non-paint, non-lamp material | `env_factor`, `shininess` | reflective metal (below) |

**Reflective materials (chrome, polished metal).** The game draws chrome as a dark `diffuse` (about 0.21) plus an environment reflection scaled by `env_factor` (about 0.98), with `shininess` 70 and a `texture_reflection` cube (`research/evidence/material-survey.txt`). `assets::reflective_surface` turns that into PBR:
- metallic = the luminance of `env_factor`, clamped to 0–1;
- base colour = `diffuse` blended towards the `env_factor` colour by the same amount, so chrome becomes a bright metal and a barely reflective part stays close to its diffuse;
- perceptual roughness = `(2 / (shininess + 2))^¼`, the GGX roughness matching a Blinn-Phong exponent (70 → 0.41), clamped to at least 0.089.

Glass, decals, lamps and paint keep their own rules.

- Colours in `.mat` (`diffuse`, `env_factor`) and paint jobs (`base_color`) are linear RGB, and are used as such. Paint values such as Coffee Brown (0.034, 0.023, 0.014) only make sense as linear. Textures flagged `_SRGB` are uploaded as `*UnormSrgb` formats.
- `MaterialInfo` also carries `specular` and `fresnel`, which are not used yet. Per material, only `texture_base` is uploaded. Normal, mask and `texture_mult` (baked AO) textures are ignored.
- Other materials use metallic 0.1 and roughness 0.6.

**Environment map (the game's reflection cube).**
- `viewer::ReflectionCube` waits until every truck model has loaded, then counts the `texture_reflection` of the truck's reflective materials and takes the most common cube. For the Scania S this is `/material/environment/fuzzy_reflection/fuzzy_reflection.tobj`.
- `render::load_cube_texture` loads all six faces on the async pool; `assets::cube_image` validates them: six square, power-of-two faces with identical mip chains, BC dimensions a multiple of 4, BC support on the GPU.
- The cube goes on the camera as Bevy's `GeneratedEnvironmentMapLight`, which filters it on the GPU. Until it is ready, or when it fails validation, the hemispherical gradient stays.
- Face order is taken as stored (+x, −x, +y, −y, +z, −z; Hypothesis).
- **Exposure (ours):** the cube's mean luminance, from its smallest mip, is scaled to the gradient's (about 0.5), clamped to ×0.25–×4. The game's cubes are dark, and the base cubes hit the ×4 cap. How the game itself exposes them is **Unknown** (REA queued).
- The garage scene's own materials use `/material/environment/ui_vehicle_reflection.tobj`, which no truck material names. Whether the game substitutes it for vehicle reflections in UI scenes is **Unknown** (REA queued).

See [scenes.md](scenes.md) for the environment strength.

This is an approximation, not the game's shaders.

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

The game's shaders are readable GLSL inside `effect.scs` ([../formats/effects.md](../formats/effects.md); survey in `research/evidence/effect-shaders-survey.txt`). The plan is to write our own WGSL Bevy materials clean-room (D13): a spec agent writes the maths to `docs/app/shader-specs/<effect>.md`, and a different agent writes the WGSL from it without seeing the shaders. Order: `truckpaint` (flip colour, paint-job texture, flakes), `dif.spec.add.env`, glass, lamp. Garage materials also need `texture_mult` and the `showroom_cube` reflection. Never commit extracted shader text; it is game code.

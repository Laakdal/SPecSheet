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

- Colours in `.mat` (`diffuse`) and paint jobs (`base_color`) are linear RGB, and are used as such. Paint values such as Coffee Brown (0.034, 0.023, 0.014) only make sense as linear. Textures flagged `_SRGB` are uploaded as `*UnormSrgb` formats.
- Only `texture_base` is used. Normal, reflection, mask and `texture_mult` (baked AO) textures are ignored.
- Other materials use metallic 0.1 and roughness 0.6.
- Reflections come from a hemispherical gradient environment map. It is a stand-in for the game's cube reflections, which is why chrome still looks flat. See [scenes.md](scenes.md) for its strength.

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

The game's shaders are readable GLSL inside `effect.scs` ([../formats/effects.md](../formats/effects.md); survey in `research/evidence/effect-shaders-survey.txt`). The plan is to write our own WGSL Bevy materials that follow the same maths, in this order: `truckpaint` (flip colour, paint-job texture, flakes), `dif.spec.add.env`, glass, lamp. Garage materials also need `texture_mult` and the `showroom_cube` reflection. Never commit extracted shader text; it is game code.

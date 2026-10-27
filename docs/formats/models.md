# Models: PMD, PMG, tokens and materials

What model files contain. Implemented in `crates/specsheet-formats/src/{pmd,pmg,token,mat}.rs`. How the app draws them is in [../RENDERING.md](../RENDERING.md); how accessories attach at locators is in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md) (F4).

## PMD (model descriptor, version 4)

| Offset | Field                                                                                    |
|--------|------------------------------------------------------------------------------------------|
| 0x00   | version                                                                                  |
| 0x04   | material count                                                                           |
| 0x08   | look count                                                                               |
| 0x0C   | piece count                                                                              |
| 0x10   | variant count                                                                            |
| 0x14   | part count                                                                               |
| 0x18   | attribute count                                                                          |
| 0x1C   | per-variant value block size                                                             |
| 0x20   | material block size                                                                      |
| 0x24…  | offsets: looks, variants, part→attribute ranges, values, attribute header, material offsets, material strings |

- Looks and variants are named by tokens.
- Material paths are NUL-terminated `/automat/xx/<hash>.mat` strings. They are reached through a `look × material` table of absolute `u32` offsets.

Attribute evidence from `/vehicle/truck/scania_2016/cabin/cab_s_low.pmd`:
- Each part owns attribute `visible`.
- Variant `default` shows parts `defaultpart` and `eu_part`; variant `uk` shows `defaultpart` and `uk_part`.

Status: Confirmed. Every truck `.pmd` parses, its part count equals the `.pmg`'s, and every piece's material index is below the material count.

## PMG (geometry, version 0x15, signature `\x15gmP`)

**Header** (0x70 bytes):
- counts: pieces, parts, bones, weight width, locators;
- skeleton hash;
- bounding sphere (centre + diagonal) and AABB;
- offsets: skeleton, parts, locators, pieces, string pool (+ size), vertex pool (+ size), index pool (+ size).

**Part** (24 bytes): token name, piece count + start, locator count + start.

**Locator** (44 bytes):
- token name;
- position `vec3`;
- scale `f32`;
- rotation quaternion stored **(w, x, y, z)**. Identity records read `(1, 0, 0, 0)`, and mirrored EU/UK locators differ only in the sign of x/z;
- hookup offset into the string pool. Wheel particles use `wheel.particles`; license plates use `license.plate` and `license.plate_rear`.

**Piece** (100 bytes):
- index count, vertex count, UV mask, UV set count, material index;
- bounding sphere and AABB;
- skeleton offset;
- nine stream offsets: position, normal, uv, colour, colour2, tangent, bone index, bone weight, index. `-1` marks an absent stream.

**Vertex streams** are interleaved:
- position `3f`
- normal `3f`
- [tangent `4f`]
- colour `u32`
- [colour2 `u32`]
- [uv `2f × n`]
- [bone idx `4u8`]
- [bone weight `4u8`]

**Indices** are a `u16` triangle list with **clockwise** front faces. In 99.3% of 4.85M sampled triangles, the face normal is opposite the stored vertex normal (`research/evidence/geometry-survey.txt`).

Status: Confirmed. Every truck model parses with all indices inside their piece.

## Tokens

A `u64` holding base-38 digits, least significant digit first. The alphabet is `\0`, `0-9`, `a-z`, `_`.

Confirmed: `0x1573c3e700` decodes to `default` and `0x0aed71b9ce` to `visible`.

## MAT (materials)

Text files in the form `effect : "<effect name>" { key : value | { tuple } … texture : "<slot>" { source : "<tobj>" … } }`. Example: `/automat/fe/fe81458fc4e29314.mat`.

- Texture slots seen: `texture_base`, `texture_nmap`, `texture_reflection`, `texture_mult`.
- `source` paths are absolute or relative to the `.mat`.
- Colours (`diffuse`, `specular`, `env_factor`) are linear RGB.
- Every material referenced by a sampled truck model parses (`geometry_survey`: 0 errors).
- The effect name selects shaders in `effect.scs`; see [effects.md](effects.md).

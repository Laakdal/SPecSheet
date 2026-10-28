# Models: PMD, PMG, tokens and materials

What model files contain. Implemented in `crates/specsheet-formats/src/{pmd,pmg,token,mat}.rs`. How the app draws them is in [../RENDERING.md](../RENDERING.md); how accessories attach at locators is in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md) (F4).

## PMD (model descriptor, version 4)

Evidence: `research/evidence/pmg-layout-derivation.txt` (`tools/research/model_layout.py` on six dumped models). Each block's record size is the gap to the next offset divided by its count, and the blocks tile the file exactly.

| Offset | Field |
|--------|-------|
| 0x00 | version (4) |
| 0x04 | material count |
| 0x08 | look count |
| 0x0C | **Unknown**: 0 in every sample |
| 0x10 | variant count |
| 0x14 | part count |
| 0x18 | attribute count |
| 0x1C | per-variant value block size (4 × attribute count) |
| 0x20 | material string block size (the strings end the file) |
| 0x24… | offsets: looks, variants, part→attribute ranges, values, attribute headers, material offsets, material strings |

In the file the blocks follow the header in this order:
1. looks: one token each;
2. variants: one token each;
3. part ranges: `u32` from, `u32` to, a `[from, to)` range of attributes. The ranges tile the attribute list;
4. attribute headers (16 bytes): token name, `u32` (**Unknown**, 0 for `visible`), `u32` byte offset in the value block;
5. values: one block per variant, a `u32` per attribute;
6. material offsets: a `look × material` table of absolute `u32` file offsets;
7. material strings: NUL-terminated `/automat/xx/<hash>.mat` paths.

Attribute evidence from `/vehicle/truck/scania_2016/cabin/cab_s_low.pmd`:
- Each part owns attribute `visible`.
- Variant `default` shows parts `defaultpart` and `eu_part`; variant `uk` shows `defaultpart` and `uk_part`.

Status: Confirmed. Every truck `.pmd` parses, its part count equals the `.pmg`'s, and every piece's material index is below the material count.

## PMG (geometry, version 0x15, signature `\x15gmP`)

Evidence: `research/evidence/pmg-layout-derivation.txt`. Record sizes come from offset gaps, and the pools chain to the end of the file. Stream names come from their values (positions inside the piece AABB, unit normals, tangents with w = ±1).

**Header** (0x70 bytes):
- `u8` version 0x15, then `gmP`;
- `u32` counts at 0x04: pieces, parts, 0x0C (**Unknown**, 0 in every sample; bones?), 0x10 (**Unknown**, 0), locators;
- `u64` at 0x18: **Unknown**, 0 in every sample;
- bounding sphere at 0x20: centre `vec3` + radius. Every vertex lies inside it. It is not derived from the AABB;
- AABB at 0x30: min, max;
- `u32` at 0x48: an offset to an empty block (0x70; skeleton?), then offsets of parts, locators and pieces, then (offset, size) pairs for the string pool, vertex pool and index pool.

**Part** (24 bytes): token name, piece count + start, locator count + start.

**Locator** (44 bytes):
- token name;
- position `vec3`;
- scale `f32`;
- rotation quaternion stored **(w, x, y, z)**. All 115 identity locators in the samples store 1.0 first. The 180° turns (`r_light`, `exhaust_m`) are `(0, 0, 1, 0)`, turns about y;
- `i32` hookup offset into the string pool. Locators without a hookup point at an empty string, usually at offset 0. Hookups seen: `flare.vehicle.beacon`, `wheel.particles`, `license.plate`.

**Piece** (100 bytes):
- `u32` index count, vertex count;
- `u32` at +8: UV slot routing, eight nibbles with slot 0 the least significant. Each nibble is a stored UV set index or `0xf` (unused) (`research/evidence/pmg-uv-routing.txt`, 611,638 pieces under `vehicle`, `model`, `model2`, `prefab`, `prefab2`):
  - **Confirmed:** only slots 0, 1, 2 and 6 are ever used, and no nibble names a missing set. The UV set count is 1 + the highest routed set, and every stored set is routed. Slot 0 is set 0 whenever UVs exist; pieces without UVs store `0xffffffff`. Slot 6 is used exactly when the material's effect has `tsnmapuv` or `tsnmapuv2`. In truck models, slot 1 is used exactly for `oclu`, `mult`, `lamp` and `truckpaint`, and slot 2 exactly for `truckpaint`. Other effects (`amod`, `window`, `leaves`, …) also use slots 1 and 2. No `.mat` has a per-texture UV-set attribute (59,516 materials checked).
  - **Hypothesis:** nibble *n* is the stored set that feeds the shader's UV slot *n*. Slot 6 is the `tsnmapuv` normal map, slot 1 the `oclu`/paint mask, slot 2 the `altuv` mask. The set is per piece: truck pieces route slot 6 from sets 0/1/2/3 (1,316/5,766/1,003/94);
- `u32` UV set count (0–4 seen across all base archives, `research/evidence/cleanroom-phase1.txt`), material index;
- bounding sphere (centre + radius) at +20, AABB at +36;
- `u32` vertex stride at +60 (28, 36, 52, 60, 68 seen);
- nine `i32` stream offsets: position, normal, uv, colour, colour2, tangent, bone index, bone weight, index. `-1` marks an absent stream.

**Vertex streams** are interleaved, one stride per vertex. Measured order and sizes:
- position `3f`;
- normal `3f`;
- [tangent `4f`];
- colour `u32`;
- [uv `2f × n`].

colour2 and the bone streams never occur in the six dumps. The parser sizes them as `u32`, `4u8` and `4u8`, and rejects any piece whose stored stride disagrees with the present streams. Every `.pmg` in the base archives passes: 53,917 models, of them 4,388 skinned. This confirms the bone stream sizes; colour2 remains Reference.

**Indices** are a `u16` triangle list per piece. Each piece's list starts 4-byte aligned. Front faces are **clockwise**: in 99.3% of 4.85M sampled triangles, the face normal is opposite the stored vertex normal (`research/evidence/geometry-survey.txt`).

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

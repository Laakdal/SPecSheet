# Packed textures (`.tobj` + `.dds`)

What texture entries contain. Implemented in `crates/specsheet-archive/src/{texture,gdeflate}.rs`. The archive container itself is covered in [hashfs.md](hashfs.md).

The `.tobj` path is the entry key. The archive does not contain a separate `.dds`. An entry has an image record (`0x01`), a sample record (`0x02`) and a mip-tail blob (`0x84`).

## Image flags

The image record is two `u32`s: a dims word (width − 1 in bits 0–15, height − 1 in bits 16–31) and the flags below. Example: BC5 sample `0x09800537` is 128², 8 mips, BC5, with a 256-byte pitch and a 512-byte image alignment.

| Bits  | Field                       |
|-------|-----------------------------|
| 0–3   | mip count − 1               |
| 4–11  | DXGI format                 |
| 12–13 | cube flag                   |
| 14–19 | face count − 1              |
| 20–23 | log₂ pitch alignment        |
| 24–27 | log₂ image alignment        |

**Status: Confirmed** (`research/evidence/texture-unpack-derivation.txt`, `texture_layout` example). The fields predict the exact decompressed size of every sampled texture, about 25 per format and face count, cube maps included. The prediction fails under any other padding model. Across every texture in the base archives, the cube flag is set exactly on the 60 six-face textures.

## Sample record

| Bit(s) | Field | Status |
|---|---|---|
| 0 | mag linear | Confirmed by content |
| 1 | min linear | Confirmed by content |
| 2–3 | mip filter | Reference |
| 4–6 | address U | Confirmed by content |
| 7–9 | address V | Confirmed by content |
| 10–12 | address W | Reference |

Address modes: 0 repeat and 2 clamp-to-edge (Confirmed), 3 clamp-to-border (Confirmed); 1 clamp, 4 mirror, … (Reference; never seen in the base archives).

Evidence (same file) comes from which textures carry the rare states:
- point sampling (bits 0/1 clear) appears only on lookup data such as LUTs, jitter and MLAA maps;
- clamp-to-border appears only on projected shadow and light masks;
- repeat-U with clamp-V appears on skybox panoramas.

## Surface data

Surface data is a DirectStorage GDeflate stream:
- an 8-byte header: `id = 4`, `!id`, `u16` tile count, and `u32` bits (tile-size index in bits 0–1, last tile size in bits 2–19);
- `u32` tile offsets, where entry 0 is the compressed size of the last tile;
- the 64 KiB tiles.

After decompression, each mip level starts on the image alignment, and each row (each block row for BC formats) on the pitch alignment. Padding goes only *before* a mip or a row, so the data ends with the last row, unpadded. Mip levels are tightly ordered; faces are face-major (all mips of face 0 first: Hypothesis, the DDS order, which a size check cannot distinguish).

**Verification:** the `texture_layout` example matches the decompressed size exactly for every sampled texture whose format `pixel_layout` knows. `PackedTexture::unpack` follows the same model.

## Observed DXGI formats

| DXGI | Format          |
|------|-----------------|
| 35   | R16G16_UNORM    |
| 61   | R8_UNORM        |
| 67   | R9G9B9E5_SHAREDEXP (skyboxes, HDR UI images; not unpacked yet) |
| 71   | BC1_UNORM       |
| 72   | BC1_UNORM_SRGB  |
| 77   | BC3_UNORM       |
| 75   | BC2_UNORM_SRGB  |
| 78   | BC3_UNORM_SRGB  |
| 80   | BC4_UNORM       |
| 83   | BC5_UNORM       |
| 87   | B8G8R8A8_UNORM  |
| 88   | B8G8R8X8        |
| 91   | B8G8R8A8_SRGB   |
| 93   | B8G8R8X8_SRGB   |

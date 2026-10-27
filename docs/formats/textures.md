# Packed textures (`.tobj` + `.dds`)

What texture entries contain. Implemented in `crates/specsheet-archive/src/{texture,gdeflate}.rs`. The archive container itself is covered in [hashfs.md](hashfs.md).

The `.tobj` path is the entry key. The archive does not contain a separate `.dds`. An entry has an image record (`0x01`), a sample record (`0x02`) and a mip-tail blob (`0x84`).

## Image flags

Confirmed on BC5 sample `0x09800537`: 128², 8 mips, BC5, 256-byte pitch, 512-byte image alignment.

| Bits  | Field                       |
|-------|-----------------------------|
| 0–3   | mip count − 1               |
| 4–11  | DXGI format                 |
| 12–13 | cube flag                   |
| 14–19 | face count − 1              |
| 20–23 | log₂ pitch alignment        |
| 24–27 | log₂ image alignment        |

## Sample record (Reference)

| Bit(s) | Field |
|---|---|
| 0 | mag linear |
| 1 | min linear |
| 2–3 | mip filter |
| 4–6 | address U |
| 7–9 | address V |
| 10–12 | address W |

Address modes: 0 repeat, 1 clamp, 2 clamp-to-edge, 3 clamp-to-border, 4 mirror, …

## Surface data

Surface data is a DirectStorage GDeflate stream:
- an 8-byte header: `id = 4`, `!id`, `u16` tile count, and `u32` bits (tile-size index in bits 0–1, last tile size in bits 2–19);
- `u32` tile offsets, where entry 0 is the compressed size of the last tile;
- the 64 KiB tiles.

After decompression, each mip level starts on the image alignment, and each row (each block row for BC formats) on the pitch alignment.

**Verification:** `PackedTexture::unpack` succeeded for samples of every observed format, using the `survey` example.

## Observed DXGI formats

| DXGI | Format          |
|------|-----------------|
| 35   | R16G16_UNORM    |
| 61   | R8_UNORM        |
| 71   | BC1_UNORM       |
| 72   | BC1_UNORM_SRGB  |
| 77   | BC3_UNORM       |
| 78   | BC3_UNORM_SRGB  |
| 80   | BC4_UNORM       |
| 83   | BC5_UNORM       |
| 87   | B8G8R8A8_UNORM  |
| 88   | B8G8R8X8        |
| 91   | B8G8R8A8_SRGB   |
| 93   | B8G8R8X8_SRGB   |

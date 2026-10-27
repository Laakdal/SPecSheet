# HashFS v2 (`.scs` archives)

What the game's archives contain. Implemented in `crates/scs-archive` (`hashfs.rs`, `city.rs`). Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). Texture entries are covered in [textures.md](textures.md).

Every shipped archive starts with `SCS#`, version `2`, hash method `CITY`.

## Header (little-endian, 0x2C bytes)

| Offset | Type   | Field                                       | Status      |
|--------|--------|---------------------------------------------|-------------|
| 0x00   | `[4]u8`| magic `SCS#`                                | Confirmed   |
| 0x04   | u16    | version = 2                                 | Confirmed   |
| 0x06   | u16    | salt (always 0 in 1.60; non-zero rejected)  | Confirmed   |
| 0x08   | `[4]u8`| hash method `CITY`                          | Confirmed   |
| 0x0C   | u32    | entry count                                 | Confirmed   |
| 0x10   | u32    | entry table stored (zlib) size              | Confirmed   |
| 0x14   | u32    | metadata table size in `u32` words          | Confirmed   |
| 0x18   | u32    | metadata table stored (zlib) size           | Confirmed   |
| 0x1C   | u64    | entry table offset                          | Confirmed   |
| 0x24   | u64    | metadata table offset                       | Confirmed   |
| 0x2C   | u64    | security descriptor offset (unused by us)   | Reference   |

## Path hashing

The key is CityHash64 **v1.0.3** of the path without a leading slash; the root directory is the empty string.

- The v1.1 algorithm (crate `cityhasher`) gives `0x2813cf36833255d7` for `version.sii`, but the archive stores `0x02d338506339918d`, which v1.0.3 produces.
- Every name in every directory listing of `def.scs` (66,928 entries), `base_vehicle.scs` (53,455) and `dlc_actros_tuning.scs` (2,321) resolves to an entry. Our hash therefore matches for every path length class (≤16, 17–32, 33–64, >64 bytes).
- Evidence: `research/evidence/survey-hashfs.txt`, test `city::tests`.

## Entry table

16-byte records: `u64 hash`, `u32 metadata word index`, `u16 metadata record count`, `u16 flags`.

- Flags observed: `0x0` and `0x4` on files, `0x5` on directories, and `0x1` on the root of `version.scs`.
- Bit 0 marks a directory. The other bits are unknown.

## Metadata records

Each record header is one `u32`: `type << 24 | data_word_index`.

| Type   | Meaning                         | Data words                              | Status    |
|--------|---------------------------------|-----------------------------------------|-----------|
| `0x80` | plain file                      | 4: blob                                 | Confirmed |
| `0x81` | directory listing               | 4: blob                                 | Confirmed |
| `0x01` | image (packed `.tobj`+`.dds`)   | 2: `w-1:u16, h-1:u16`, flags            | Confirmed |
| `0x02` | sample (sampler state)          | 1                                       | Reference |
| `0x84` | mip tail (texture surface blob) | 4: blob                                 | Confirmed |
| `0x05` | secondary record on some models | unknown                                 | Unknown   |
| `0x06` | secondary record on `.pmg`/`.pmd` | 2 (usually zero)                      | Unknown   |
| `0x07` | secondary record on `effect.scs` shader entries (8,970 `.rfx`) | 2 (e.g. `0x408, 0`) | Unknown purpose; content reads as plain (Confirmed: all 24,481 `effect.scs` entries index) |

Entries with an unrecognised record type are reported by `Archive::skipped_entries()` instead of failing the archive. The real-install tests assert that none exist in 1.60.

The 4-word **blob** is:
- `stored_size` (bits 0–27), with bit 28 = compressed and bit 29 = set on GDeflate textures;
- `size` (bits 0–27);
- an unknown word, always 0 so far;
- `offset / 16`.

Plain blobs use zlib and packed textures use GDeflate.

## Directory listing

`u32 count`, then `count × u8` name lengths, then the concatenated names. Sub-directories are prefixed with `/`.

## Mount order

How the game layers the archives is a runtime finding: see F2 in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md).

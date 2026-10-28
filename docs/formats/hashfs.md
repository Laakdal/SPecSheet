# HashFS v2 (`.scs` archives)

What the game's archives contain. Implemented in `crates/specsheet-archive` (`hashfs.rs`, `city.rs`). Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). Texture entries are covered in [textures.md](textures.md).

Every fact below was derived from the bytes of the 115 ETS2 1.60.1.7 archives. The evidence is in [`research/evidence/hashfs-layout-derivation.txt`](../../research/evidence/hashfs-layout-derivation.txt); "D:n" refers to its sections.

## Layout at a glance

| Region | Where | Notes |
|---|---|---|
| Header fields | 0x00..0x34 | then zero up to 0x80 |
| Opaque block | 0x80, about 596 bytes | high-entropy, probably a signature; not needed |
| Data (blobs) | from 0x1000 | 16-byte aligned, gaps under 16 bytes |
| Entry table | header offset | zlib, 16-byte aligned |
| Metadata table | header offset | zlib, 16-byte aligned, ends at EOF |

Placement, alignment and the EOF end hold in all 115 archives (Confirmed, D:1, D:2).

## Header (little-endian, fields span 0x34 bytes)

| Offset | Type   | Field                                       | Status      |
|--------|--------|---------------------------------------------|-------------|
| 0x00   | `[4]u8`| magic `SCS#`                                | Confirmed   |
| 0x04   | u16    | version = 2                                 | Confirmed   |
| 0x06   | u16    | salt (0 in all 115 archives; non-zero rejected) | Confirmed |
| 0x08   | `[4]u8`| hash method `CITY`                          | Confirmed   |
| 0x0C   | u32    | entry count                                 | Confirmed   |
| 0x10   | u32    | entry table stored (zlib) size in bytes     | Confirmed   |
| 0x14   | u32    | metadata table size in `u32` words          | Confirmed   |
| 0x18   | u32    | metadata table stored (zlib) size in bytes  | Confirmed   |
| 0x1C   | u64    | entry table offset                          | Confirmed   |
| 0x24   | u64    | metadata table offset                       | Confirmed   |
| 0x2C   | u64    | `0x80` in every archive; points at the opaque block | Hypothesis (offset); content Unknown |

- The last field is the u64 at 0x2C, so the header fields end at **0x34**, not 0x2C as this page used to say. Bytes 0x34..0x7F are zero in all 115 archives (Confirmed, D:1).
- In version.scs the offsets and stored sizes land exactly: 0x3180 + 36 → padding → 0x31B0 + 37 = EOF. The tables inflate to exactly `count × 16` and `words × 4` bytes in every archive (Confirmed, D:1).
- The offsets are 64-bit: base.scs's entry table is at 0x2_68A5_52B0 (Confirmed, D:1).
- No field gives the opaque block's length. It is 596 non-zero bytes from 0x80 in 113 archives and 595 in the other two (D:1).

## Path hashing

The key is CityHash64 **v1.0.3** of the path without a leading slash. The root directory is the empty string.

- The root's key is `0x9ae16a3b2f90404f` = `city_hash_64("")`, which is CityHash's `k2` constant. `version.sii`'s key is `0x02d338506339918d` (Confirmed, D:6).
- All 655,931 names listed in the 115 archives resolve as `parent/name` keys. That covers every length class: ≤16 (8,349), 17–32 (110,175), 33–64 (354,042) and >64 (183,365) bytes (Confirmed, D:5, D:6).

## Entry table

zlib-compressed (`78 DA` in all 115 archives, D:2). 16-byte records, sorted ascending by hash, with unique hashes (Confirmed, D:2):

| Offset | Type | Field |
|---|---|---|
| 0 | u64 | path hash |
| 8 | u32 | index of the entry's first metadata word |
| 12 | u16 | metadata record count |
| 14 | u16 | flags |

- Flag bit 0 is set exactly on entries with a directory record: 43,940 entries, `0x5` plus one `0x1` (the version.scs root). Confirmed, D:2.
- Bit 2 (`0x4`) is set on all files of some archives (e.g. def.scs), on none in others, and on some in most DLCs, where it tracks text definitions. Its meaning is Unknown. It doesn't affect reading, and the reader takes an entry's kind from its records (D:2).
- No other flag bits occur.

## Metadata records

zlib-compressed. An entry has `count` consecutive header words starting at its metadata index. Each header word is `type << 24 | payload_word_index`, and the payloads follow directly after the headers. Payload lengths were derived from how the starts tile the table (every header block and payload in all 115 archives), not assumed (Confirmed, D:3).

| Type   | Meaning                         | Payload words | Seen on | Status    |
|--------|---------------------------------|---------------|---------|-----------|
| `0x80` | plain file                      | 4: blob       | 474,070 files | Confirmed |
| `0x81` | directory listing               | 4: blob       | 43,940 directories | Confirmed |
| `0x84` | texture surface data ("mip tail") | 4: blob (GDeflate) | 138,581 `.tobj` | Confirmed |
| `0x01` | image (packed `.tobj`+`.dds`)   | 2: `w-1:u16, h-1:u16`, flags | 138,581 `.tobj` | Confirmed ([texture-unpack-derivation.txt](../../research/evidence/texture-unpack-derivation.txt)) |
| `0x02` | sample (sampler state)          | 1             | 138,581 `.tobj` | Confirmed ([texture-unpack-derivation.txt](../../research/evidence/texture-unpack-derivation.txt), "sample record, 0x02") |
| `0x05` | extra record on animations      | 8             | 4,116 `.pma` | Size Confirmed; meaning Unknown |
| `0x06` | extra record on geometry        | 2 (zero on most; 2,940 non-zero in base + base_vehicle) | 70,109 `.pmg` | Size Confirmed; meaning Unknown |
| `0x07` | extra record on effects         | 2 (e.g. `0x408, 0`) | 8,970 `effect.scs` `.rfx` | Size Confirmed; meaning Unknown |

Only these combinations occur (Confirmed, D:3):
- `0x80` alone, or with exactly one of `0x05`/`0x06`/`0x07`;
- `0x81`;
- `0x01 + 0x02 + 0x84`.

The extra records never change how a file reads. Every `.pma`, `.pmg` and `.rfx` reads as a plain file of its recorded size (Confirmed, D:3).

The reader skips any entry with an unknown record type or an unexpected combination, and reports it in `Archive::skipped_entries()` instead of failing the archive. The real-install tests assert that no entries are skipped in 1.60; the survey of all 115 archives finds 0 skipped.

## Blob location (4 payload words)

| Word | Bits | Field | Status |
|---|---|---|---|
| 0 | 0–27 | stored size | Confirmed |
| 0 | 28 | compressed | Confirmed |
| 0 | 29 | GDeflate: set only on compressed `0x84` blobs, together with bit 28 | Confirmed |
| 1 | 0–27 | size after decompression | Confirmed |
| 1 | 28–29 | set on **every** `0x84` blob, raw or compressed; never on `0x80`/`0x81` | Unknown meaning; masked off |
| 2 | all | always 0 | Unknown; ignored |
| 3 | all | offset / 16 | Confirmed |

- Every blob lies between 0x1000 and the entry table, and the first one is at exactly 0x1000. Sorted by offset, the 656,591 blobs never overlap, and every gap (including the one before the entry table) is under 16 bytes (Confirmed in all 115 archives, D:1, D:4).
- Every raw blob has stored size = size (Confirmed, D:4).
- Compressed plain blobs and listings are zlib. Every one inflates to exactly its recorded size: 496,297 checked across all archives, plus base.scs and base_vehicle.scs inflated completely (Confirmed, D:4).
- Compressed texture blobs are GDeflate (DirectStorage tile streams, `crate::gdeflate`). All 138,477 start with stream id 4/¬4, and the size implied by their tile header equals word 1 bits 0–27 (Confirmed, D:4).
- An older version of this page said size was bits 0–27 and stopped there. On texture blobs, word 1 also carries `0x3` in bits 28–31, e.g. raw `00000200 30000200 00000000 055fb27a`.
- The reader rejects any flag combination other than raw or zlib on `0x80`/`0x81`, and raw or GDeflate on `0x84`. A decompressed size that differs from the recorded one is `Corrupt`, and so is a size that deflate cannot reach from the stored bytes (more than 1032:1).

## Directory listing

`u32 count`, then `count × u8` name lengths, then the names back to back. A name starting with `/` is a sub-directory. Example, def.scs root: `02 00 00 00 | 07 04 | /custom /def` (D:5).

- All 43,895 listings in the 115 archives decode to exactly their size (Confirmed, D:5).
- All 655,931 listed names resolve to an entry whose kind matches the `/` marker (Confirmed, D:5).
- Listings are **not exhaustive**: 547 entries are named by no listing (Confirmed, D:5). core.scs's root lists nothing, while 20 files and a directory sit under it. locale.scs and base_cfg.scs have no root entry at all. base.scs and base_share.scs each have a few unlisted files.

## Mount order

How the game layers the archives is a runtime finding: see F2 in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md).

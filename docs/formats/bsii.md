# Binary SII (`BSII`)

The binary form of SII that many save files use once their ScsC container is decrypted ([profiles.md](profiles.md)). Implemented in `crates/specsheet-formats/src/bsii.rs`; `scsc::document` reads a save or profile in any form. Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). Evidence: `research/evidence/bsii-derivation.txt` (32 binary saves, compared with 54 text saves). The layout and type table are cross-checked against sii-decode-rs (MIT); every fact here is confirmed on our own bytes.

## Layout

Little-endian. A string is a u32 length and its bytes.

| Part | Content | Status |
|---|---|---|
| Header | `BSII`, u32 version. All 32 saves are version 3; other versions are rejected. | Confirmed |
| Structure block | u32 `0`, u8 `1`, u32 structure id, string name, then fields until a u32 `0`. A field is a u32 value type and a string name; an enum (type `0x37`) adds a u32 count and that many (u32 key, string) pairs. | Confirmed |
| Unit block | u32 structure id (non-zero), the unit's id, then one value per field in declared order. | Confirmed |
| End marker | u32 `0`, u8 `0`. Nothing follows it. | Confirmed (32/32 files end exactly there) |

**Ids.** A u8 part count:
- `0` is `null`;
- `0xff` is followed by a nameless u64;
- otherwise that many name tokens follow, joined with `.`.

All 555,560 nameless references in the saves name a unit of the same file (Confirmed). Our text form `_nameless.177.f825.8278` (16-bit groups in hex, high to low, leading zero groups dropped) is a Hypothesis about the game's form.

**Name tokens.** A u64 in base 38 over `0-9`, `a-z`, `_`: digit `d` is character `d − 1`, and the first character is the lowest digit (Confirmed). Bit 63 is ignored (Unknown meaning). Digit 0 has no character and is an error.

## Value types

All 32 types declared in the saves, and their element sizes, are confirmed by whole-file parses. Any other type id is an error.

| Type | Value | | Type | Value |
|---|---|---|---|---|
| `0x01` / `0x02` | string / array | | `0x25` / `0x26` | i32 / array |
| `0x03` / `0x04` | name token / array | | `0x27`, `0x2f` / `0x28` | u32 / array |
| `0x05` / `0x06` | f32 / array | | `0x2b` / `0x2c` | u16 / array |
| `0x07` | 2 × f32 | | `0x31` / `0x32` | i64 / array |
| `0x09` | 3 × f32 | | `0x33` / `0x34` | u64 / array |
| `0x11` / `0x12` | 3 × i32 / array | | `0x35` / `0x36` | bool (u8) / array |
| `0x18` | array of 4 × f32 | | `0x37` | enum: u32 key into the field's table |
| `0x19` / `0x1a` | placement, 8 × f32 / array | | `0x39`, `0x3b`, `0x3d` / `0x3a`, `0x3c` | id / array |

Arrays are a u32 count, then the elements.

**Meaning.** Compared with the text saves, value kinds agree for 470 of the 482 shared fields (Confirmed). The exceptions:
- **Placements.** Floats 4–7 are the rotation quaternion in all 316 values (Confirmed), and floats 0–2 the position. Float 3 is always a whole number just above 2²³, probably packed position data (Unknown). Text writes 7 values, `(x, y, z) (w; x, y, z)`, without it. The parser keeps all 8 floats.
- **`nil`.** Text writes some u64 fields as `nil`. The binary value there is u64::MAX (Hypothesis, from `player_job.online_job_id`). The parser keeps the number.

## Output

`bsii::parse` returns the same `SiiDocument` as the text parser: one unit per unit block, class = structure name, name = id.
- Floats become exact `&xxxxxxxx` tokens; integers and bools become decimal and `true`/`false` tokens.
- Name tokens and ids become bare tokens; strings stay strings.
- An enum key missing from its table is kept as its number.
- An array becomes `key: n` plus `key[i]` attributes, as in text.

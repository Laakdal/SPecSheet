# SII (text definitions) and localisation

What the game's definition files contain. Implemented in `crates/specsheet-formats/src/sii.rs`, with localisation in `crates/specsheet-data/src/localization.rs`. How the tuning rules use these definitions is covered in [../TUNING_SYSTEM.md](../TUNING_SYSTEM.md).

## Syntax

Files have the form `SiiNunit { class : unit.name { key: value … } }`.

- Comments use `#`, `//` and `/* */`.
- Arrays are written `key[]` (append) or `key[n]` (indexed). A scalar `key: n` gives the element count, as in `sounds: 3`.
- Tuples are written `(a, b, c)`. `&xxxxxxxx` is a raw IEEE-754 float.
- Saves write placements as two tuples on one line, `(x, y, z) (w; x, y, z)`: a position and a rotation quaternion, `;` after its `w`. The parser joins them into one 7-item tuple (Confirmed on 54 text saves; test `sii::tests::parses_placements_as_one_seven_item_tuple`).
- Class names may contain `::`, as in `ui::group`.
- Strings escape `\n`, `\r`, `\t` and `\xNN`. A `\xNN` is one byte of UTF-8, so non-ASCII text is written as a run of them (Confirmed: profiles, 31 `/ui` layouts such as the euro sign `\xe2\x82\xac` and glyph-font icons `\xee\xa3\x83` in `truck_configuration_*.sii`, and 6,184 `/def/sign` files; see [profiles.md](profiles.md)).
- `@include "file"` is textual and may appear inside a unit body (for example `cabin/normal.sii` includes `sound.sui`). Paths are relative to the including file.
- Unit names starting with `.` (such as `.data`) are file-local references.
- Every `/def/vehicle/**/*.sii` in 1.60 parses (test `real_install::every_vehicle_definition_parses`).

Encrypted (`ScsC`), binary (`BSII`) and `3nK` SII are not used in shipped definitions. The parser rejects them. Profiles are `ScsC`: `scsc::sii_text` decrypts them to text first ([profiles.md](profiles.md)).

## Localisation

- `/locale/<language>/local*.sii` hold `localization_db` units with parallel `key[]` / `val[]` arrays. They are plain SII with includes such as `/locale/common.sui`.
- Definitions reference strings as `@@key@@`.
- Accessory slot names double as keys: `r_grill` → "Light Bar", `sideskirt` → "Sideskirts" (Confirmed).

# SII (text definitions) and localisation

What the game's definition files contain. Implemented in `crates/specsheet-formats/src/sii.rs`, with localisation in `crates/specsheet-data/src/localization.rs`. How the tuning rules use these definitions is covered in [../TUNING_SYSTEM.md](../TUNING_SYSTEM.md).

## Syntax

Files have the form `SiiNunit { class : unit.name { key: value … } }`.

- Comments use `#`, `//` and `/* */`.
- Arrays are written `key[]` (append) or `key[n]` (indexed). A scalar `key: n` gives the element count, as in `sounds: 3`.
- Tuples are written `(a, b, c)`. `&xxxxxxxx` is a raw IEEE-754 float.
- Class names may contain `::`, as in `ui::group`.
- `@include "file"` is textual and may appear inside a unit body (for example `cabin/normal.sii` includes `sound.sui`). Paths are relative to the including file.
- Unit names starting with `.` (such as `.data`) are file-local references.
- Every `/def/vehicle/**/*.sii` in 1.60 parses (test `real_install::every_vehicle_definition_parses`).

Encrypted (`ScsC`), binary (`BSII`) and `3nK` SII are not used in shipped definitions. The parser rejects them.

## Localisation

- `/locale/<language>/local*.sii` hold `localization_db` units with parallel `key[]` / `val[]` arrays. They are plain SII with includes such as `/locale/common.sui`.
- Definitions reference strings as `@@key@@`.
- Accessory slot names double as keys: `r_grill` → "Light Bar", `sideskirt` → "Sideskirts" (Confirmed).

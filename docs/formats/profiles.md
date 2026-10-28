# Profiles and the ScsC container

What a player profile contains, as far as the app reads it. Implemented in `crates/specsheet-formats/src/scsc.rs`. Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). Evidence: `research/evidence/profile-scsc-survey.txt` (4 profiles, ETS2 1.60.1.7).

Profiles are the player's data. The app only reads them; nothing writes to them.

## Where profiles are

- `<home>/profiles/<hex name>/profile.sii`. The home directory is `Documents/Euro Truck Simulator 2` unless the game runs with `-homedir` (Confirmed, game log).
- The folder name is the profile name as hex of its UTF-8 bytes, in lowercase or uppercase hex digits; a profile named "Profile" would be `50726f66696c65` (Confirmed, 4 folders).
- Each profile also holds `save/`, `config.cfg`, controls and gearbox layouts (Confirmed, listing). Not read.

## ScsC container

| Offset | Size | Field | Status |
|---|---|---|---|
| 0x00 | 4 | magic `ScsC` | Confirmed |
| 0x04 | 32 | unknown (an HMAC according to the reference); not checked | Unknown |
| 0x24 | 16 | AES-CBC IV | Confirmed |
| 0x34 | 4 | u32 size of the payload after inflating | Confirmed |
| 0x38 | rest | body: AES-256-CBC of a zlib stream, PKCS#7-padded to 16 bytes | Confirmed |

- **Key.** A fixed 32-byte AES-256 key, kept in `scsc.rs` as a format constant (Reference: sii-decode-rs, MIT, taken as a fact). It is not stored as contiguous bytes in `eurotrucks2.exe`, so how the game holds it is Unknown.
- **Check (Confirmed).** With that key, all 4 bodies decrypt to zlib streams that inflate to exactly the declared size. The size check fails if the key, IV or offsets are wrong.
- **Payload.** SII text (`SiiN`) in all 4 profiles (Confirmed). Saves (`save/*/game.sii`, 86 here) use the same container: 47 decrypt to `SiiN`, 32 to binary SII (`BSII`), and 7 are already plain text (Confirmed, same size check). `BSII` is parsed by `bsii::parse` ([bsii.md](bsii.md)); `scsc::document` reads all three forms, and `scsc::sii_text` stays text-only.
- **Plain text.** Players decrypt and edit profiles with community tools. `scsc::sii_text` therefore passes `SiiN` through unchanged (Confirmed for a decrypted copy). That the game loads a plain-text profile is a Hypothesis; nothing here writes one to test it.

## SII text quirks

- Non-ASCII strings are written as `\xNN` escapes of their UTF-8 bytes (Confirmed: a Workshop display name in Japanese). `sii::parse` decodes them.
- Line ends are CRLF (Confirmed).

## `user_profile`

One unit of class `user_profile`. The app reads only:

| Attribute | Content | Status |
|---|---|---|
| `active_mods[]` | `"<package name>\|<display name>"`, index 0 first | Confirmed |

- **Package name.** For a local mod, the file stem in `<home>/mod/`. For a Workshop item, `mod_workshop_package.<published file id as 16 hex digits>` (Confirmed). See [mods.md](mods.md).
- **Order (Confirmed).** For the profile the game log covers, the list in index order equals the game's "Active N mods" list and its mount order (57/57).
- Which end of the list wins when two mods hold the same file is Unknown (F14).

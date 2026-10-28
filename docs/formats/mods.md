# Mod packages (`/home/mod`, Steam Workshop)

What a mod package contains and how the game finds, gates and mounts it. Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). The runtime behaviour is finding F14 in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md). Evidence: `research/evidence/mod-loader-survey.txt` (55 local packages, 77 Workshop items, the game log, `eurotrucks2.exe` strings).

The app reads all three containers: ZIP (`ZipArchive`, `zip.rs`), HashFS v1 (`HashFsV1Archive`, `hashfs_v1.rs`) and HashFS v2 (`Archive`), all in `crates/specsheet-archive/src/`. Nothing is mounted yet (D15 in [../DECISIONS.md](../DECISIONS.md)).

## Where packages come from

| Source | Location | Package |
|---|---|---|
| Local | `<home>/mod/`; the home directory is `Documents/Euro Truck Simulator 2` unless the game runs with `-homedir` | each file is one package; its file stem is the package name |
| Workshop | `<steam>/steamapps/workshop/content/227300/<published id>/` | `versions.sii` picks a sub-folder or a `.zip`/`.scs` in the item folder |

- The game opens every local file at start-up, reads its manifest and closes it again (Confirmed, log).
- Workshop items come from a Steam query for the subscribed items, not from a folder scan (Confirmed, log: "Received list of the steam workshop subscribed mods").
- In the log, Workshop packages are named `mod_workshop_package.<id as 16 hex digits>` (Confirmed: `00000000CE19853F` = item `3457779007`).

## Container: decided by content

The extension means nothing. `SCS#` goes to the `hashfs` driver and `PK\3\4` to the `zipfs` driver. 28 of the 33 ZIP packages here are named `.scs` (Confirmed, every file against the driver the log names for it).

### HashFS v2
Identical to the game's own archives ([hashfs.md](hashfs.md)). Both v2 mods here open with `Archive::open`, root listing and manifest included (Confirmed).

### HashFS v1

Every field below is derived from our own hex dumps of 20 packages: `research/evidence/hashfs-v1-derivation.txt` (sections 1–5, with annotated bytes). The reader `HashFsV1Archive` was written from that file.

Header (little-endian, 0x14 bytes):

| Offset | Type | Field | Status |
|---|---|---|---|
| 0x00 | `[4]u8` | magic `SCS#` | Confirmed |
| 0x04 | u16 | version = 1 | Confirmed |
| 0x06 | u16 | salt (0 in 19 of 20 files) | Confirmed |
| 0x08 | `[4]u8` | hash method `CITY` | Confirmed |
| 0x0C | u32 | entry count (equals the count the game logs) | Confirmed |
| 0x10 | u32 | entry table offset (usually `0x1000`; at the end of the file in one) | Confirmed |

Entry table: `count` × 32-byte records, sorted by hash (Confirmed, 20 files):

| Offset | Type | Field | Status |
|---|---|---|---|
| +0 | u64 | path hash | Confirmed |
| +8 | u64 | data offset | Confirmed |
| +16 | u32 | flags | see below |
| +20 | u32 | CRC | Unknown whether the game checks it |
| +24 | u32 | size | Confirmed |
| +28 | u32 | stored size | Confirmed |

- **Flags.** Bit 0 marks a directory (Confirmed). Bit 1 marks zlib-compressed content (Confirmed: every flagged entry inflates to exactly `size`, every unflagged one has `stored == size`). Bit 2 is set on every entry of one producer's packages, with no visible effect (Unknown).
- **Path key.** CityHash64 v1.0.3 of the path without a leading slash, as in v2. With a non-zero salt, the decimal text of the salt is prepended (`18708manifest.sii` for salt `0x4914`) (Confirmed: `manifest.sii` resolves in 20 of 20 files; no other form does).
- **Directory listing.** Text with one name per line; sub-directories are prefixed `*`. Lines end in `\n` or `\r\n` (Confirmed).
- **No root listing.** The root directory (key of ``) is missing from all 20 files, so a v1 mod cannot be walked from the root. Its files are reachable only by known path (Confirmed). Deeper listings exist.
- **Listings are optional altogether.** Two packages (`NG Sound.scs`, 91 entries; `skybox.scs`, 329) have no directory entry at all (Confirmed). The game does not need names, since it looks files up by full path, but a tool that lists a package has only hashes.
- **Name recovery (`mod_names`).** Names are recovered by hashing candidate paths and keeping those in the index, so a guess can only miss, never mis-name. Candidates: every path of the mounted game, parents and listings, paths mentioned inside any entry, the manifest's icon and description, and file conventions (same-stem `.pmg`, `.tobj`, `.mat`, tried even when the first file is missing, since a package may keep a texture's `.tobj` without the texture; addon units at `accessory/<slot>/<name>.sii`). Across the 22 v1 packages, 169,777 of 171,094 entries get a name when the other mod packages' files are candidates too: a v1 package's names often sit in another package, such as a model package used by a separate definitions package (Confirmed, `research/evidence/mod-name-recovery.txt`).
- **More name rules (Confirmed, evidence sections 8–9).**
  - A standalone `.tobj` names its texture exactly: from offset `0x28`, records of a `u64` length and the path bytes fill the file (one record; six for a cube map; 24,979 of 24,981). Paths may use `?*<>:#`, which a character scan misses.
  - Generated materials live at `automat/<xx>/<hash>.mat`, the CityHash64 of their own content (65,473 of 65,473 in the game), so an unnamed material is named by hashing it.
  - Files packed next to a named file often share its folder and stem, so its stem is tried with the package's extensions (packing order).
  - Compared with the community tool Extractor on the same 22 packages, both name the same files in 17. Before these rules it named 2,110 files ours did not (695 automat, 1,409 `.dds` from `.tobj`, 6 models not in the 1.60 install) and ours 1,973 it did not; the rules added 2,104 names in that package, matching the first two groups in count (a file-by-file recheck is open).
- **Content types.** An unnamed entry's likely extension comes from its first bytes (`specsheet_formats::sniff`); a label never names a file.
- **Search limits.** Every candidate must be a plausible path (at most 512 bytes; no empty, `.` or `..` part). Per package, at most 4 million candidates, 256 MiB of candidate text and 4 GiB of scanned bytes, each well above the largest real need (evidence section 6).
- **Listing encoding.** Some listings are not UTF-8 (22 of 106 in one package; names with bytes like `0xb9` beside Polish words, probably Windows-1250: Hypothesis). The reader decodes them lossily, so such a name cannot be hashed back to its entry.
- **Reader check (Confirmed).** `HashFsV1Archive` opens all 20 packages. The entry count equals the game log's in every one, and all 170,674 entries read to their declared size (`mod_probe`, derivation section 6).

### ZIP
- Only stored (0) and deflate (8) occur in 33 packages; none needs ZIP64 (Confirmed). The game's reader also handles ZIP64 records (Confirmed, its error strings). Whether it accepts other methods is Unknown.
- **Deliberately malformed archives exist.** One package sets the encryption bit on every central-directory entry and gives both headers wrong CRCs. Its local headers also carry a wrong method. Its data is still plain deflate at `local header + 30 + name + extra` and inflates to the central-directory size (Confirmed). The game opens it without a zipfs error (Confirmed, log).
  - Hypothesis: the game takes names, method and sizes from the central directory and the data offset from the local header, and checks neither the encryption bit nor the CRC.
  - An off-the-shelf ZIP library refuses such a package.
- **Our reader (`ZipArchive`)** does what the game tolerates:
  - names, method and sizes come from the central directory, the data offset from the local header (`+ 30 + name + extra`);
  - the encryption bit and both CRCs are ignored;
  - methods 0 and 8 are read, and any other method fails only that entry;
  - ZIP64 end records and extra fields are honoured;
  - directories are implied by file paths, since ZIPs need no directory records.
  - Confirmed on all 33 ZIP packages: the record count equals the game's "Created, N entries" in every one, and every file reads to its declared size (0 failures), the malformed package included.

## Package layout

- **Ordinary:** `manifest.sii`, an icon (`.jpg`) and a description (`.txt`) at the root, then the game's own tree (`def/`, `vehicle/`, `material/`, …). A mod overrides a game file by storing the same path.
- **Multi-folder:** `manifest.sii` and the icon at the root, and the content in `base/` and `dlc_<name>/` folders. Each folder becomes its own device (Confirmed, one "mounted to mod pool" line per folder). That `base/def/x.sii` is then seen as `/def/x.sii` is a Hypothesis.
  - `base/` is always mounted.
  - A `dlc_<name>/` folder is mounted only when that DLC is installed (Confirmed: of 17 `dlc_*` folders, the two that match no installed DLC were skipped at profile load).
- **No manifest:** the game creates default information (Confirmed, log).

## `manifest.sii`

Plain SII, one `mod_package : <unit name>` unit. The unit name is dot-prefixed (`.name`) in 28 of 31 ZIP manifests; CRLF and LF line ends both occur. Attributes seen across 54 manifests:

| Attribute | Use | Status |
|---|---|---|
| `package_version` | free string shown in the mod manager and log | Confirmed |
| `display_name` | shown name; falls back to the package name | Confirmed (log warning) |
| `author` | free string | Confirmed |
| `category[]` | mod-manager categories; invalid or missing values fall back to defaults. Values seen: `tuning_parts`, `truck`, `map`, `graphics`, `sound`, `other` | Confirmed (log warnings) |
| `icon`, `description_file` | root-relative files | Confirmed (present in all 54); `.jpg`-only icons and `.txt`-only descriptions are Hypothesis (exe strings) |
| `compatible_versions[]` | game-version globs (`"1.60.*"`); no array means compatible; an array with no matching glob makes the mod "Incompatible" | Confirmed in both directions: of 54 manifests, exactly the 2 without a glob matching 1.60.1.7 are the 2 the log calls incompatible |
| `dlc_dependencies[]` | DLC package names; a missing one marks the mod incompatible | Hypothesis (exe string; every dependency here is installed) |
| `mp_mod_optional` | multiplayer: clients may join without the mod | Hypothesis (name only) |

## Workshop `versions.sii`

- One `package_version_info : .<name>` unit per variant, with `package_name` and optional `compatible_versions[]` (Confirmed, 77 items).
- `package_name` names a sub-folder or a `.zip`/`.scs` file in the item folder. Folders are mounted as directory devices (Confirmed, log).
- Variants without `compatible_versions` (`.latest`, `.universal`) act as the fallback (Hypothesis: the game warns about "Duplicated version info with no compatible_versions array").

## Activation and order

- Packages are mounted only when a profile is selected, into a separate **mod pool** (Confirmed, log).
- The active list is stored per profile (`active_mods`). Its order is the mount order: the log prints "Active N mods" and then mounts each mod in that same order (Confirmed for all 57 active mods).
- The list is `active_mods[]` in `profile.sii`, entries `"<package name>|<display name>"`, index 0 mounted first (Confirmed; [profiles.md](profiles.md)).
- Which mod wins when two store the same path, and how the mod pool ranks against the scs pool: Unknown (REA question in F14).
- `/home/mods_info.sii` is game-written plain SII: `info[]` holds `"<package name>|<number>"` for every known package (Confirmed). What the number means is Unknown.

## Writing a mod package

`ZipWriter` and `write_new_package` (`specsheet-archive`) and `mod_manifest` (`specsheet-formats`) write new packages (D18); the example `mod_write` builds a test package. From the observations above:
- The game loads a ZIP (stored or deflate) or a HashFS archive placed in `<home>/mod/`, with the overriding files at their game paths.
- A ZIP needs no directory records to mount: a package with none mounts and its top-level folders resolve (Confirmed, `research/evidence/mod-writer-checks.txt`). That nested folders list without them is a Hypothesis. Our writer emits none.
- The package name is the file stem, so a new package must not share a stem with an existing one (the game warns about duplicated package names).
- Our output is a standard ZIP: Python's strict `zipfile` reader checks every CRC of the test package without error (Confirmed). Whether the game lists and mounts it is not yet checked.
- Extraction (D19, `specsheet-data::extract`, example `mod_extract`) writes a package's files to a folder the user names, never inside or around the game install, the ETS2 home folder, the package's own folder or the repository, never over an existing file, and never more than a size limit (64 GiB by default). Names Windows cannot hold are changed and listed in `_renamed.txt`; unnamed entries go to `_unnamed/<hash>.<extension>`. Game (HashFS v2) archives are not extracted yet: their textures are packed and would need a new `.tobj` and `.dds` written (Unknown fields).
- ZIP stores every name in its central directory, so a package we write can never lose its names the way a v1 package without listings does. Any future v1 output must write the root listing and every listing below it.
- `manifest.sii` at the root is optional: without it the game uses default information (`No_Driver`). It is needed for a display name, an icon and version gating.
- To take effect, the package must be enabled for a profile. The player does this in the in-game mod manager, so a tool that writes a package never has to touch the encrypted profile.

## `.soundref` (sound mods)

- A text file linking a sound slot to an FMOD bank event. One `key=value` per line: `source="/sound/truck/<bank>.bank#<event path>"` in all 37 of one package, and `position_offset=x;y;z` in 8 (Confirmed on that package, `mod-name-recovery.txt` section 5).
- Truck sound includes (`sound*.sui`) list slots as `sounds[]: "<slot>|/def/.../x.soundref"`.
- The game ships `.soundref` files too (for example `engine/sound_590s_engine.soundref`); whether they use the same keys is Unknown.

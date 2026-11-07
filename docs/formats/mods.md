# Mod packages (`/home/mod`, Steam Workshop)

What a mod package contains and how the game finds, gates and mounts it. Status labels are explained in [../ASSET_FORMATS.md](../ASSET_FORMATS.md). The runtime behaviour is finding F14 in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md). Evidence: `research/evidence/mod-loader-survey.txt` (55 local packages, 77 Workshop items, the game log, `eurotrucks2.exe` strings).

The app reads all three containers: ZIP (`ZipArchive`, `zip.rs`), HashFS v1 (`HashFsV1Archive`, `hashfs_v1.rs`) and HashFS v2 (`Archive`), all in `crates/specsheet-archive/src/`. Since D20 ([../DECISIONS.md](../DECISIONS.md)) the app mounts a chosen profile's local mods the game's way (`specsheet-data::mods`, `GameInstall::mount_with_mods`): in `active_mods[]` order into a mod pool above every base, def and DLC archive and below the core pool. Each package is opened by content and gated by `compatible_versions[]`. A package with a `base/` or `dlc_*/` folder at its root mounts `base/` and each installed DLC's `dlc_<name>/` folder as its own device, with the folder stripped from paths (Hypothesis, see Multi-folder below). Steam Workshop items are mounted from the Workshop folder of the install's Steam library, each with the entry its `versions.sii` picks (see Workshop below); an item not in that folder is listed as not loaded. Mod textures are loose `.tobj` + `.dds` ([textures.md](textures.md#loose-textures-in-mods)). On the test install's 1.60 profile, all 57 active mods (34 local, 23 Workshop) mount in about 1.4 s; they add four trucks, and the Volvo FH2022 loads with all its models and base textures (example `mod_mount`, `research/evidence/mod-workshop.txt`).

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
- **Name recovery (`mod_names`).** Names are recovered by hashing candidate paths and keeping those in the index, so a guess can only miss, never mis-name. Candidates: every path of the mounted game, parents and listings, paths mentioned inside any entry, the manifest's icon and description, and file conventions (same-stem `.pmg`, `.tobj`, `.mat`, tried even when the first file is missing, since a package may keep a texture's `.tobj` without the texture; addon units at `accessory/<slot>/<name>.sii`). Across the 22 v1 packages, 169,816 of 171,094 entries get a name when the other mod packages' files are candidates too: a v1 package's names often sit in another package, such as a model package used by a separate definitions package (Confirmed, `research/evidence/mod-name-recovery.txt`).
- **More name rules (Confirmed, evidence sections 8–9).**
  - A standalone `.tobj` names its texture exactly: from offset `0x28`, records of a `u64` length and the path bytes fill the file (one record; six for a cube map; 24,979 of 24,981). Paths may use `?*<>:#`, which a character scan misses.
  - Generated materials live at `automat/<xx>/<hash>.mat`, the CityHash64 of their own content (65,473 of 65,473 in the game), so an unnamed material is named by hashing it.
  - Files packed next to a named file often share its folder and stem, so its stem is tried with the package's extensions (packing order).
  - Word swaps learned from the game's own folders (two names differing in one `_` word, as `blinker_on`/`blinker_off`, seen in at least 20 pairs) give variants of found names (evidence section 10).
  - A file found under one vehicle folder (`def/vehicle/<kind>/<name>/…`) is tried under every other vehicle of that kind the candidates mention: a sound mod lays out each truck it covers alike (evidence section 11).
  - An unnamed file packed between two named files of one folder gets the names that sort between theirs, from words of paths with the same extension (`sound.sui` < `sound_2024.sui` < `sound_effects.sui`; evidence section 12).
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
  - `base/` is always mounted, first; then the `dlc_<name>/` folders in name order (Confirmed, log, three packages: `research/evidence/mod-multi-folder.txt`). Which of two folders wins for a path both hold is Unknown from the log (Hypothesis: the later); it matters for 40 files of the Sanax Volvo FH2022 definitions, the cheapest in-game check.
  - A `dlc_<name>/` folder is mounted only when that DLC is installed (Confirmed: of 17 `dlc_*` folders, the two that match no installed DLC were skipped at profile load).
  - A package may hold `dlc_*/` folders and no `base/` (`promods-eu-dlcsupport-v283.zip`): only its installed DLCs' folders mount (Confirmed, log). A folder holding only directory records is still mounted. The app treats a package as multi-folder when its root has `base/` or any `dlc_*/` folder.
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
- `package_name` names a sub-folder or a `.zip`/`.scs` file in the item folder, by its stem. Folders are mounted as directory devices (Confirmed, log: 38 of 77 items mount a folder, 39 a file).
- **Which unit (Confirmed, data, 77 of 77 items, `research/evidence/mod-workshop.txt`):** the first unit whose `compatible_versions[]` match the game version; when none matches, the first unit without `compatible_versions` (`.latest`, `.universal`). Taking the first unit in file order that is versionless or matches misses 8 items, each listing a versionless unit before a matching one.
- A folder entry that holds `base.*` and `dlc_<name>.*` packages mounts the folder, then `base`, then the DLC packages by name (Confirmed, log, item 695738881; that only installed DLCs are taken is a Hypothesis, as for multi-folder packages).
- A `versions.sii` may start with a UTF-8 byte-order mark; the game reads it (Confirmed, item 3294653064).
- **Implementation:** `specsheet-data::mods` resolves `mod_workshop_package.<hex id>` to `<workshop>/<decimal id>`, picks the unit, and mounts a folder as `specsheet_archive::FolderPackage` (read only; paths with `..`, empty parts or drive separators are refused, and links leading outside the folder are not followed) or a `.zip`/`.scs` file as a local package, both gated by their `manifest.sii`. `package_name` must be one plain name, and the folder must be a real folder whose resolved path stays inside the item; links named like packages are not opened. An item that fails part way mounts nothing.

## Activation and order

- Packages are mounted only when a profile is selected, into a separate **mod pool** (Confirmed, log).
- The active list is stored per profile (`active_mods`). Its order is the mount order: the log prints "Active N mods" and then mounts each mod in that same order (Confirmed for all 57 active mods).
- The list is `active_mods[]` in `profile.sii`, entries `"<package name>|<display name>"`, index 0 mounted first (Confirmed; [profiles.md](profiles.md)).
- Which mod wins when two store the same path: the later one in `active_mods[]`. A mod beats every base, def and DLC path, but never a core-pool path (`effect`, `core`, `locale`, `base_cfg`) (Confirmed, static, F14, `research/evidence/mod-pool-priority-static.txt`).
- `/home/mods_info.sii` is game-written plain SII: `info[]` holds `"<package name>|<number>"` for every known package (Confirmed). What the number means is Unknown.

## Writing a mod package

`ZipWriter` and `write_new_package` (`specsheet-archive`) and `mod_manifest` (`specsheet-formats`) write new packages (D18); the example `mod_write` builds a test package. From the observations above:
- The game loads a ZIP (stored or deflate) or a HashFS archive placed in `<home>/mod/`, with the overriding files at their game paths.
- A ZIP needs no directory records to mount: a package with none mounts and its top-level folders resolve (Confirmed, `research/evidence/mod-writer-checks.txt`). That nested folders list without them is a Hypothesis. Our writer emits none.
- The package name is the file stem, so a new package must not share a stem with an existing one (the game warns about duplicated package names).
- Our output is a standard ZIP: Python's strict `zipfile` reader checks every CRC of the test package without error (Confirmed). Whether the game lists and mounts it is not yet checked.
- Extraction (D19, `specsheet-data::extract`, example `mod_extract`) writes a package's files to a folder the user names, never inside or around the game install, the ETS2 home folder, the package's own folder or the repository, never over an existing file, and never more than a size limit (64 GiB by default). Names Windows cannot hold are changed and listed in `_renamed.txt`; unnamed entries go to `_unnamed/<hash>.<extension>`. A name that reaches another entry through its 8.3 short alias (`______~1.dds` opens `_________.dds`; 24 files in `Add-On MAN TG3 2020 by Gloover.scs`) takes the next `~N`, as does one taken on disk by that alias. Named entries are streamed to disk (`ZipArchive::open_file`, `HashFsV1Archive::open_entry`, 64 KiB at a time, size checked exactly), so no entry is held in memory whole and none is limited to 2 GiB; unnamed entries are still read whole (at most 2 GiB), as their extension is guessed from all their bytes. Streamed output equals the in-memory output byte for byte on NG Sound (91 files), MAN TG3 (7,528) and `promods-eu-map-v283.scs` (42,108, 3.1 GB, peak memory 20 MB, 33 MB before) (`research/evidence/extract-streaming.txt`). A `PathFilter` takes only the entries whose archive path matches a pattern (`*` matches any run of characters, `/` included; case ignored; unnamed entries match as `_unnamed/<hash>`), and the size limit applies to the files taken; `list` gives the files as extraction would name them, without reading content (example: `--list`, `--only`). Game (HashFS v2) archives are extracted too, read only, the container told by its header. Their names come from walking their own listings, then from every path of the mounted game for entries no listing names (`v2_names`; `core.scs`, whose root lists nothing, gets 14 of 20 so). Each entry is read whole, at most 256 MiB by its 28-bit size field. Packed textures (`.tobj` with the image in the archive's texture records) are listed in `_packed_textures.txt`, and their size does not count against the limit (537 in `dlc_daf_xd.scs`). With `Archive::V2 { textures: true }` (example: `--textures`) each named one is written as a mod package holds it (`extract::loose_files`): a standalone `.tobj` naming `/<stem>.dds`, with the type, mipmaps, address modes and colour space of the packed record (the header from `tobj-loose.txt`; Unknown bytes take their most common value), and a DX10 `.dds` naming the packed DXGI format exactly, every face and mip tightly packed (a cube is one six-face file). All 537 in `dlc_daf_xd.scs` and the 94 under `material/environment/` of `base.scs` and `base_share.scs` (37 cubes) read back equal to the packed record (`extract_check --textures`, `research/evidence/texture-convert.txt`), and the files load through the mod texture loader as the same texture (unit test). Whether the game itself loads them is Unknown. On `def.scs` (64,636 files), `dlc_daf_xd.scs` (1,643) and `core.scs` (20), every file equals the game's bytes (`extract_check`, `research/evidence/extract-v2.txt`).
- ZIP stores every name in its central directory, so a package we write can never lose its names the way a v1 package without listings does. Any future v1 output must write the root listing and every listing below it.
- `manifest.sii` at the root is optional: without it the game uses default information (`No_Driver`). It is needed for a display name, an icon and version gating.
- To take effect, the package must be enabled for a profile. The player does this in the in-game mod manager, so a tool that writes a package never has to touch the encrypted profile.

## `.soundref` (sound mods)

- A text file linking a sound slot to an FMOD bank event. One `key=value` per line: `source="/sound/truck/<bank>.bank#<event path>"` in all 37 of one package, and `position_offset=x;y;z` in 8 (Confirmed on that package, `mod-name-recovery.txt` section 5).
- Truck sound includes (`sound*.sui`) list slots as `sounds[]: "<slot>|/def/.../x.soundref"`.
- The game ships `.soundref` files too (for example `engine/sound_590s_engine.soundref`); whether they use the same keys is Unknown.

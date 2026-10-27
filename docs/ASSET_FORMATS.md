# Asset formats

What ETS2 1.60.1.7 game files contain, as implemented in `crates/scs-archive` and `crates/scs-formats`. How the app uses them is described in [RENDERING.md](RENDERING.md).

## Status labels

- **Confirmed:** verified against real game files by our parser, plus a check that would fail if the layout were wrong.
- **Reference:** taken from community code (sk-zk TruckLib.HashFs / TruckLib.Models / GisDeflate). It agrees with our observations but has not been verified independently.
- **Hypothesis / Unknown:** not yet established.

Evidence files live in `research/evidence/`.

## Formats

| Doc | Covers |
|---|---|
| [formats/hashfs.md](formats/hashfs.md) | `.scs` archives: header, CityHash64 v1.0.3 path keys, entry and metadata tables, directory listings |
| [formats/textures.md](formats/textures.md) | Packed `.tobj`/`.dds` entries: image and sample records, GDeflate surfaces, DXGI formats |
| [formats/sii.md](formats/sii.md) | Text SII definitions (`@include`, arrays, tuples) and localisation |
| [formats/models.md](formats/models.md) | PMD descriptors, PMG geometry and locators, tokens, `.mat` materials |
| [formats/effects.md](formats/effects.md) | `effect.scs`: `.rfx` effect → shader map, `.vso`/`.fso` shaders |

## Not yet implemented

| Format | Status |
|---|---|
| `.rfx` / shader parsing | Readable through the VFS; no parser yet |
| `.pmc` collision | Not needed |
| `.pma` animation | Not needed for the static preview |
| HashFS v1, ZIP | Community mods only; mods are not mounted |

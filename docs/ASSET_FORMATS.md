# Asset formats

What ETS2 1.60.1.7 game files contain, as implemented in `crates/specsheet-archive` and `crates/specsheet-formats`. How the app uses them is described in [RENDERING.md](RENDERING.md).

## Status labels

- **Confirmed:** verified against real game files by our parser, plus a check that would fail if the layout were wrong.
- **Reference:** a layout fact taken from community code (sk-zk TruckLib.HashFs / TruckLib.Models, GPL-2.0; GisDeflate, MIT). It agrees with our observations but has not been verified independently. Only facts are taken from these projects, never code (`THIRD_PARTY_NOTICES.md`).
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
| [formats/ui.md](formats/ui.md) | `/ui/` layout SII (widget units, 1440×900 coordinates, templates, markup), SDF `.font`, UI `.mat`, accessory icon paths |
| [formats/ui-truck-config.md](formats/ui-truck-config.md) | The truck tuning screen (`truck_configuration_*.sii`, filter dialog): widget trees, runtime-filled containers, captions |
| [formats/paint.md](formats/paint.md) | Paint jobs: `accessory_paint_job_data`, per-part `simple_paint_job_data` files, masks and `shared_` constants, preset colours |

## Not yet implemented

| Format | Status |
|---|---|
| `.rfx` / shader parsing | Readable through the VFS; no parser yet |
| `/ui/` layouts, `.font` | Documented in [formats/ui.md](formats/ui.md); parsed as plain SII, no layout or font model yet |
| `.pmc` collision | Not needed |
| `.pma` animation | Not needed for the static preview |
| HashFS v1, ZIP | Community mods only; mods are not mounted |

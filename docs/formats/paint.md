# Paint jobs

How ETS2 1.60.1.7 describes a truck paint job and how it reaches each painted part. The finding is F15 in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md); evidence is `research/evidence/paint-job-survey.txt` (sections S1–S10, cited as [S*n*]). Labels follow [../ASSET_FORMATS.md](../ASSET_FORMATS.md). No shader code was read for this page (D13): effect variants are known by file name only.

Code: `specsheet_core::{PaintJob, PartPaint, Flake, Flip, PaintLocks, PaintOverride, shared_mask_color}`, loaded by `specsheet-data/src/catalog.rs` and resolved per placed part by `specsheet_data::paint::piece_paint`.

## Files

| File | Holds | Status |
|---|---|---|
| `/def/vehicle/truck/<truck>/paint_job/<file>.sii` | One `accessory_paint_job_data` unit (0 of 19,998 files hold more), usually `@include`-ing a shared `<design>_settings.sui` | Confirmed [S1] |
| `/def/vehicle/truck/<truck>/paint_job/accessory/<token>.sii` | The paint job's per-part units: nameless `simple_paint_job_data` (`.ovr0`, `.ovr1`, …) only, 1 to 29 per file | Confirmed [S1, S3] |
| `/def/vehicle/truck/<truck>/accessory/<slot>/paint_job/<pj token>.<accessory token>.sii` | A rarer per-accessory unit, one nameless `simple_paint_job_data` each, no `acc_list` (131 files in 5 directories) | Confirmed data; 127 of 131 name no installed paint job [S3] |
| `/vehicle/truck/upgrade/paintjob/<truck>/<design>/*.tobj` | Mask tiles and `shared_` constant masks | Confirmed [S5] |
| Presets (`/def/vehicle/truck_desktop/*.sii`, `truck_company`, `truck_dealer`, `truck_used`) and saves | One `vehicle_paint_job_accessory` per vehicle | Confirmed [S8, S10] |

`<token>` is the **first token of the paint job's unit name**, not its file stem. The executable holds the format string `%s/paint_job/accessory/%s.sii` [S3]. Over 19,998 paint jobs, 17,410 link through the token and none only through the stem; for example `color0m.sii` holds `col0m_s_s_a.scania.s_2016.paint_job`, and its parts are in `accessory/col0m_s_s_a.sii` (Confirmed [S1, S3]). Old trucks (Iveco Hi-Way and Stralis, MAN TGX, Renault Magnum and Premium) and plain colours elsewhere have no per-part file (2,588 paint jobs). 24 per-part files match no paint job.

## `accessory_paint_job_data`

Attribute counts are over all 19,998 units [S2]. Colours are RGB tuples in [0, 1]; SpecSheet treats them as linear, like every definition colour.

| Key | Units | Meaning | `PaintJob` field |
|---|---|---|---|
| `name`, `price`, `unlock`, `icon`, `suitable_for[]` | | as for every accessory; `suitable_for` names the cabin(s) the paint job is cut for | `Accessory` fields |
| `base_color` | 19,550 | body colour (448 units have none) | `base_color` (also `Accessory::base_color`) |
| `paint_job_mask` | 18,137 | mask or livery texture of the main body | `mask` |
| `mask_r_color`, `mask_g_color`, `mask_b_color` | 11,675 / 11,844 / 11,920 | colours for the mask's channels; a unit may write only some | `mask_colors` |
| `airbrush` | 14,942 (true 14,937) | the mask is a painted livery (Hypothesis) | `airbrush` |
| `alternate_uvset` | 5,487 (always true) | the mask uses a second UV set (Hypothesis, from the `altuv` effect variants) | `alternate_uvset` |
| `flipflake` | 9,526 (always true) | flakes and flip colour are drawn (Hypothesis) | `flipflake` |
| `flake_color`, `flake_density` (0–2), `flake_shininess` (0–60), `flake_clearcoat_rolloff` (2.24–5.5), `flake_uvscale` (1–128) | 15,901 / 6,935 / 8,721 / 12,574 / 4,707 | metallic flakes | `flake` |
| `flake_noise` | 2,488 | flake pattern texture; `/material/custom/flake_noise.tobj` on 1,063, DLC patterns on the rest | `flake.noise` |
| `flip_color`, `flip_strength` (0.3–2.7) | 15,901 / 12,938 | colour seen at grazing angles | `flip` |
| `base_color_locked`, `mask_{r,g,b}_locked`, `flake_color_locked`, `flip_color_locked` | 9,736 / 8,611, 8,472, 9,035 / 7,370 / 7,371 | the colour is fixed (Hypothesis); absent means false | `locks` |
| `stock` | 720 | Unknown (true 400, false 320) | `stock` |
| `color_variant[]` | 2,451 | tuple3 array, always a multiple of 4 long; no 4-group equals (base, mask r, g, b) in 2,138 units, so the meaning of the four is Unknown | `color_variants` (groups of 4) |
| `part_type`, `steam_inventory_id`, `academy_reward` | | not used | |

Default flake noise: when `flake_noise` is absent SpecSheet uses `/material/custom/flake_noise.tobj` (Hypothesis: the executable holds the path [S7], and it is the most common explicit value). `Flake::noise_texture()` applies it.

Feature combinations [S2, S7]: non-airbrush units with mask colours never set `flipflake` (2,931 units, 0 with it), and the effect directory has no `airbrush`+`colormask` and no `colormask`+`flipflake` variant. This fits three kinds of paint job (Hypothesis): plain/metallic colour (no mask), colour mask (`mask_*_color` pick colours per channel), airbrush (the mask is the livery itself).

## Per-part units: `simple_paint_job_data`

| Key | Meaning | `PartPaint` field |
|---|---|---|
| `acc_list[]` | `<slot>.<accessory>` entries; each names the accessory unit `<accessory>.<truck>.<slot>` | `accessories` (resolved), `unresolved` |
| `paint_job_mask` | this part group's mask, in the same directory as the paint job's mask (119,118 of 119,118) | `mask` |
| `flake_uvscale` | flake scale for these parts | `flake_uvscale` |
| `flake_vratio` | when written, equals the mask tile's height over width (22,634 of 22,634); absent on many non-square tiles, so it is not derived | `flake_vratio` |

- `acc_list` resolution (Confirmed [S4]): 560,896 of 561,084 entries resolve; 188 do not (mostly MAN TGX 2020, e.g. `sideskirt.6x24_p`). Resolved units are `accessory_addon_data` (552,947), `accessory_addon_int_ui_data` (5,218) and `accessory_chassis_data` (2,731). No entry repeats within one paint job's file.
- A unit without `paint_job_mask` occurs only when the paint job has no mask either (8,409); every unit of a masked paint job has its own mask.
- Coverage (Confirmed [S4], five paint jobs checked): every listed accessory has a `truckpaint` material, but most painted parts are not listed. On the Scania S, `770s_met_a` lists 64 of 158 painted accessories; the 94 unlisted are 42 addons, 3 cabins, 1 chassis and 48 interior parts. The Volvo FH 2024 `academy_a` and `v_m_inb_a` list 51 of 175.
- **Unlisted painted parts use the paint job's main mask (Hypothesis).** The unlisted Scania S bumper and door handles share the cab's material and lie inside the cab's UV square [S6].
- SpecSheet skips the per-accessory directory (`accessory/<slot>/paint_job/`): only 4 of its 131 files match an installed paint job (Design).

## Masks

- **Tiles (Confirmed [S5]).** `pjm_at_<x>x<y>_size_<w>x<h>.tobj`: `<w>x<h>` equals the texture size in 33,578 of 33,578 readable masks. Tiles of one directory never overlap and imply a canvas (e.g. 4096×8192). Different cabins use tiles at different positions with identical UV ranges (Volvo `globe_xl_aero_2024` uses `0x6144`, `globe_aero_2024` uses `0x4096`), so a tile is already cut for its model. How `<x>`,`<y>` relate to the UVs is Unknown.
- **Formats (Confirmed [S5]).** Tiles are BC1 sRGB (DXGI 72) or BC3 sRGB (78) with full mip chains, clamp-to-edge. Legacy-named masks (`paintjob_001_daf.tobj`, …) may also be 8-bit BGRA/BGRX sRGB (91/93). `/material/custom/flake_noise.tobj` is 128×128 B8G8R8A8 sRGB, repeat-wrapped.
- **Constant masks (Confirmed [S5]).** `shared_<RRGGBBAA>.tobj` (4,816 paths, 680 names, per-part units only) is a uniform 4×4 texture whose decoded texels equal the hex name within 1.9/255 in 4,816 of 4,816. `shared_mask_color` decodes the name without reading the texture and returns **sRGB-encoded RGB with linear alpha**, i.e. the texel values; sampling the sRGB texture on a GPU gives `srgb_to_linear(rgb)` and the same alpha.
- 27 referenced masks are missing from the install [S5]. The catalog skips the per-accessory files and the 24 orphan per-part files; of the 49,418 masks it references, 4 are missing (Volvo FH16 `ee_lio2_v` 512x128 tiles; real-install test `every_paint_mask_exists_or_is_a_constant`). It resolves 560,456 `acc_list` entries and leaves 158 unresolved.

## Painted materials

Paint reaches a model piece through its material's effect: `eut2.truckpaint` with optional `.tsnmap`/`.tsnmapuv` base and the feature variants `airbrush`, `colormask`, `flipflake`, `flipflakeuv`, `altuv` (names from the `/effect/eut2/truckpaint` listing only [S7]). Cabin materials carry no paint parameters; a few upgrade materials name a pre-composed variant and carry placeholder paint parameters (`paintjob_base_color (1,0,0)`, `texture_paintjob …/empty.tobj`) [S6]. Which pieces of a model are painted is therefore decided by the material, not by the paint job.

## Presets and saves

`vehicle_paint_job_accessory` stores `data_path` (the paint job's **file** path, 548 of 548 presets) and up to six colours: `base_color`, `mask_r_color`, `mask_g_color`, `mask_b_color`, `flake_color`, `flip_color` (Confirmed [S8]). Saves write all six plus `refund` (Confirmed [S10]).

- Attribute sets over 548 presets: `data_path` only 258; plus `base_color` 46; plus the mask colours 158; all six 86.
- 242 of the 244 presets that write mask colours write exactly (1,0,0)/(0,1,0)/(0,0,1), and 242 of those point at a paint job without mask colours.
- `TruckCatalog::by_def_path` resolves `data_path`: one unit per file makes the file path a key.
- The colours load into `Configuration`'s `PaintOverride`; absent keys stay `None` and use the paint job's own values. A key that is present but not a colour is a `LoadWarning`.

## Resolution in SpecSheet

`piece_paint(catalog, config, slot, accessory)` returns the paint of one placed accessory (`PiecePaint`), or `None` without a paint job:

| Value | Source |
|---|---|
| base, mask r/g/b, flake and flip colours | the configuration's override, else the paint job's value |
| mask | the `PartPaint` that lists the accessory, else the paint job's `paint_job_mask` (Hypothesis); `Mask::Constant` for `shared_` names |
| flake scale | the part's `flake_uvscale`, else the paint job's |
| flake v-ratio | the part's `flake_vratio` only |
| flake noise | `flake_noise`, else `/material/custom/flake_noise.tobj` (Hypothesis) |
| `airbrush`, `alternate_uvset`, `flipflake`, density, shininess, rolloff, flip strength | the paint job |

Locks are exposed, not enforced: an override wins over a locked colour (Design).

## Unknown

- The meaning of `color_variant[]` and of `stock`.
- How tile coordinates in mask names relate to model UVs, and which UV set an `alternate_uvset` mask uses on a model with only one (the Scania S has 82 alt-UV paint jobs, but its cab paint piece has one UV set [S6]).
- How the shader combines base, mask, flakes and flip (needs a clean-room spec, D13).

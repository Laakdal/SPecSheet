# Reverse-engineering log

Findings about ETS2 that the implementation relies on, with their evidence.

**Labels**
- **Confirmed**: verified on game files or game output.
- **Hypothesis**: consistent with the evidence but not proven.
- **Unknown**: not yet understood.

**Target**
- Game: ETS2 1.60.1.7 (Steam).
- Executable: `bin/win_x64/eurotrucks2.exe`, SHA-256 `b7dffe6b27402c7db6dfd52cf982cd5bf292584138b35e3eb8efb311814ab3f8`.
- The archive formats are documented in [ASSET_FORMATS.md](ASSET_FORMATS.md) and the tuning rules in [TUNING_SYSTEM.md](TUNING_SYSTEM.md).

## Tools and evidence types

| Tool | Used for | Evidence |
|---|---|---|
| REA 6.4.0 (MCP + CLI, `npx rea-agents@6.4.0`) with Ghidra 12.1.4 | Executable analysis | "REA Evidence" |
| Our own parsers and the research examples (`specsheet-archive/examples/{survey,scs}.rs`, `specsheet-data/examples/{model_info,def_stats,def_grep}.rs`) | Archive and definition content | "Parser observation", reproducible with the commands in [DEVELOPMENT.md](DEVELOPMENT.md) |
| The game's own `game.log.txt` from a real session | Runtime behaviour | "Runtime log" (passive; we did not run the game) |
| sk-zk TruckLib.HashFs / TruckLib.Models / GisDeflate (MIT) | Cross-checking our layouts | "Reference", never proof on its own |

REA limitations met so far:
- `inspect_artifact` rejects `.scs` ("unsupported binary format"), so archive evidence comes from our own parsers.
- Ghidra auto-analysis of the 51 MB executable exceeds REA's default 330 s deadline. A long CLI run with `REA_GHIDRA_STARTUP_TIMEOUT_MS` and `--snapshot` is used instead; see "Open questions".

## Findings

### F1: Archive container is HashFS v2 with CityHash64 v1.0.3 (Confirmed)
- Every shipped `.scs` begins `SCS#`, version 2, `CITY`.
- Parser observation: full directory walks of `def.scs`, `base_vehicle.scs` and `dlc_actros_tuning.scs` resolve every listed name and reach 100% of entries. CityHash v1.1 does not match (see ASSET_FORMATS).
- Evidence: `research/evidence/survey-hashfs.txt`.

### F2: Mount order (Confirmed for base archives, Hypothesis for DLC order)
- Runtime log `game.log.txt` shows the base mounts in this order: `effect`, `base`, `base_map`, `base_vehicle`, `base_share`, `def`, `core`, `locale`, `base_cfg`. These form the core and scs pools.
- DLC archives then join the scs pool ("scs pool: 110 packs"). The log does not show their mount priority.
- Implementation: base archives in log order, then `dlc_*.scs` alphabetically. Later mounts win and directory listings are merged.

### F3: Truck definitions are a tree of SII units (Confirmed)
For each truck `/def/vehicle/truck/<brand.model>/`:

| Path | Unit | Contents |
|---|---|---|
| `data.sii` | `accessory_truck_data` | `info[]`, `fallback[]` (`"<slot>\|<file>"`), `require[]` (slots that must be filled) |
| `chassis/*.sii` | `accessory_chassis_data` | `model`/`detail_model`, `variant`, `look`, `defaults[]` (including the default cabin) |
| `cabin/*.sii` | `accessory_cabin_data` | `model`/`detail_model`, `variant`/`variant_uk`, `look`, `suitable_for[]` (chassis), `defaults[]` |
| `accessory/<slot>/*.sii` | `accessory_addon_data` and specialised `accessory_addon_*_data` classes | `exterior_model[_uk]`, `interior_model[_uk]`, `look`, `variant`, `suitable_for[]`, `conflict_with[]`, `defaults[]`, `overrides[]`, `hide_in` |
| `engine/`, `transmission/`, `head_light/`, `interior/`, `paint_job/` | own classes | |

- Unit names follow `<item>.<brand.model>.<slot>`, e.g. `stock.scania.s_2016.s_mirror`.
- Counts across all trucks: `research/evidence/truck-def-attribute-stats.txt`. For example, 8,939 `accessory_addon_data` units, 3,728 with `suitable_for` and 795 with `conflict_with`.

### F4: Accessories attach at model locators named after their slot (Hypothesis, strong)
- Parser observation from `cab_s_low.pmd`/`.pmg` (Scania S cabin) and `chs_4x2` (chassis). Run `model_info` to reproduce.
  - The cabin has locators `s_mirror`, `f_badge`, `s_panel`, `f_light_top`, `sunshld`, `f_intake_cab`, …
  - The chassis has `f_bumper`, `r_bumper`, `exhaust_l`, `sideskirt`, `tank`, `wheel_f_0`, …
  - These names match the slot directory names under `accessory/`.
- Cabin and chassis models share one coordinate space: the cabin AABB starts at z ≈ −3.05, the same as the chassis front. They are composed without a transform.
- Addon models are authored around their own origin. `side_mirror_01.pmd` has an AABB of about ±0.14 m.
- Some locators carry suffixes, e.g. `mirror_s`, `doorhndl_s`, `beacon_sl`, `toybig_1`, `set_lglass3`, `doorsteps`.
  - **Rule measured:** a locator belongs to the longest slot name it starts with. Example `locator_check` applies it to the visible locators of every truck preset's chassis and cabin:
    - 626 of 626 preset-selected exterior addons find a locator (`research/evidence/locator-check-prefix.txt`).
    - A stricter variant (`exact | slot_<x> | slot<digits>`) missed four (`doorstep` vs `doorsteps`; `research/evidence/locator-check-v1.txt`).
    - Three slots have two visible locators each: `flag_r` (Iveco Hi-Way), `toybig` (Magnum), `carpet` (Renault T). The model is placed at each.
  - The engine's actual rule is still **Unknown** (REA pending). The rule above reproduces every preset.
- Variant-dependent parts carry mirrored locators:
  - `eu_part` has `s_mirror` at x = +1.25; `uk_part` has it at x = −1.25.
  - PMD variants `default`/`uk` toggle these parts (`visible` attribute).
  - The cabin definition picks the variant (`variant` / `variant_uk`).

### F5: The tuning screen places "accessory pins" at runtime (Confirmed layout, Unknown placement)
- `ui/truck_configuration_center.sii` contains `ui::group : _nameless._.accpins` (id 9000) with **no children**. Pins are created by code, not by the UI definition.
- **Hypothesis:** pins sit at the projected positions of slot locators (F4). REA executable analysis is in progress to confirm this.

### F6: Preset configurations use the save-game vehicle structure (Confirmed)
- `/def/vehicle/truck_desktop/<truck>.sii` holds one `vehicle` unit whose `accessories[]` point to:
  - `vehicle_accessory` units (`data_path` to chassis/cabin/engine/…);
  - `vehicle_addon_accessory` units (addons);
  - paint-job and wheel units.
- We use these presets as the initial configuration of each truck.

### F7: Tuning scene configuration (Confirmed; rendered by the app)
- `/def/desktop/config*.sii` (`ui_truck_scene_base_config`) defines:
  - the garage scene model `/model/ui/garage/desktop_garage.pmd`;
  - camera position/aim (`(0,3,0)` → `(0,3,1)`) and FOV 45;
  - light direction/colours;
  - truck position/rotation.
- Ten scene files exist in 1.60 (`config.sii`, `config.hangar.sii`, `config.under_bridge.sii`, …). Each also references:
  - a `scene_cinematic_camera` (`/def/camera/ui/*.sii`). Its `cinematic_camera` lists `orbit_camera` units (with `camera_fov`) and `camera_animation_info` units (`camera_index`, `positions[]`, `aims[]`, `frame_duration`);
  - `part_order[]`, the scene-model parts drawn (the `coll` part is not listed).

### F8: Executable strings around truck configuration (REA Evidence, static)
- **REA binary overview** `ev_a558c2ae067820ea6ebd2a3491bad402adfb94658763d5d104b1558d5f34aa56` (Ghidra 12.1.4, about 28 min): 69,598 procedures, 51,977 defined strings. File: `research/evidence/rea-binary-overview.json`.
- **REA string search** `ev_435ac3b9f96db5a759052912dc5f978bcca832aee147ba7ef25af4706da37110`. Regex over `accpins|exterior_model|hookup|suitable_for|conflict_with|wheel_f|variant_uk|locator|…`: 518 hits, 372 after dropping "allocator" noise. Files: `research/evidence/rea-search-tuning-strings.{json,txt}`. Addresses are virtual (image base `0x140000000`).

Observations (strings only; no code has been decompiled yet, so how they are used is inferred from the text):

| Address | String | Bearing |
|---|---|---|
| `0x14223cf30` | `...\eurotrucks\game\ui\truck_config_hdl.cpp` | The tuning screen is `truck_config_hdl_u` (`0x14223c8b8`), matching `truck_config_hdl` in `ui/truck_configuration_center.sii` (F5) |
| `0x14223d860` | `[truck_config_hdl_u] Failed to find required button. Base id: %u Pin id: %Iu` | Supports F5: pins are buttons addressed by a base id plus a pin index, created by code |
| `0x14223cd30`, `0x14223cd90`, `0x14223cb08`, `0x14223ccb0` | `A hookup already exists on the given position (%Iu) for parent '%s'`, `Parent accessory not found for hookup...`, `...hookup occupying slot '%s'`, `Failed to create hookup slot name` | Addon hookups attach to a parent accessory at indexed positions and get their own slot names |
| `0x14215ae00` | `There is no visible cabin locator!` | The engine looks for a visible `cabin` locator. Both cabin and chassis models carry one; its role (cabin pivot?) is **Unknown** |
| `0x14215ec08`, `0x1421601e0`, `0x1421600b0` | `Missing wheel model locator '%s' (chassis '%s')`, `No wheel locators found for '%s'`, `...fewer pendle axle locators than pendle axle models` | Wheels are placed at chassis wheel locators, and axle models exist (lift/pendle axles: Phase 4) |
| `0x1421c2f90`, `0x1421aae50` | `Auxiliary model placement failed, locator '%s' wasn't found in any provided model instances`, `aux_models_locators` | Locators are searched across several model instances (base models): consistent with F4 |
| `0x14212e210`, `0x14212e330` | `External locator for toy addon group '%s' was not found`, `...pach addon group...` | Addon groups can attach at locators of other model instances |
| `0x14225ab38` | `hookups_exterior_only` | Hookups can be exterior-only (relevant for `hide_in`/interior) |
| `0x1421a4e28`, `0x1421a4e38`, `0x1421b3f98`-`0x1421b3fd0`, `0x14215fa30` | `suitable_for`, `conflict_with`, `exterior_model[_uk]`, `interior_model[_uk]`, `variant_uk` | The attribute names F3 relies on are read by the executable |
| `0x142177778`, `0x142177948` | `/def/vehicle/addon_hookup_storage.sii`, `Loaded addon hookups: %Iu` | Hookup catalogue entry point |

- **REA trace** `ev_5455a45c5b46a548623e6cb05dfd550bef755fd710b92f42ef0472fb0905a4f1` (`research/evidence/rea-trace-pin-button.json`): the pin-button string `0x14223d860` is referenced only from `0x140fe1465`, inside `FUN_140fe0fc0` (`0x140fe0fc0`-`0x140fe1569`, 1,450 bytes). This is the `truck_config_hdl_u` routine that looks up pin buttons by base id + pin index. Static reference only; nothing was executed.

Next REA step: decompile `FUN_140fe0fc0` (pin creation; a background `rea function` run is under way, output `C:\Users\Nightingale\rea-ets2\function-pin-setup.json`), then the caller of `0x1421c2f90` (locator lookup). The aim is to turn F4/F5 into Confirmed.

### F9: Rims and discs carry the tyre's lateral offset in `tire_shift` (Confirmed attribute, Hypothesis rule)
Evidence: `research/evidence/wheel-tire-shift.txt`. Rerun with `wheel_fit`, `def_stats` and the commands at the top of that file.

- **Attribute (Confirmed, parser observation).** `tire_shift` is a float on `accessory_rim_data` units:
  - f_disc: 30 of 32 units, values −0.054 … +0.025;
  - r_disc: 16 of 16 units, all −0.01;
  - f_rim: 10 of 10 units, all +0.025;
  - r_rim: 5 of 5 units, all −0.01.
  - Hubs, nuts, covers, tyres and complete wheels (`f_wheel`/`r_wheel`) never carry it.
  - The executable contains the string `tire_shift` (plain byte scan, offset 35332488). It is static only: no code reference is traced yet.
- **Units and the moving part.** The disc, hub and nuts all share the locator frame.
  - The disc mounting face at r ≈ 13 cm sits at x ≈ −0.170 (left variant) on discs 01, 05 and 06, whose `tire_shift` values are +0.022, −0.038 and −0.054.
  - The inner face of the nuts is also at −0.1706.
  - The disc's outer lip moves from disc to disc, so the tyre is the part that is offset.
- **Rule (Hypothesis, strong).** The tyre's model-space x offset is `+tire_shift` on the `left` variant and `−tire_shift` on the `right` variant. Positive values move the tyre towards the truck's centre line, negative values outboard. Units are metres.
  - Test: the disc's outer lip (vertices within 2 cm of its maximum radius) meets the tyre's outboard bead (vertices within 2 cm of its minimum radius). The stock tyres are `tire/front_1` and `tire/rear_1`.
  - f_disc: 62 of 64 disc×side cases fit within 5 mm (maximum 3.6 mm). The two outliers are `fdisc_claws_alu_paintable`, at 12 mm. Units without the attribute count as 0 (`front_disc_08`, residual 3 mm).
  - r_disc: 32 of 32 cases fit within 5 mm (maximum 4.8 mm).
  - Without the shift, only 4 of 64 front cases and 0 of 32 rear cases fit.
  - The residual ranges from 0 to −3.6 mm and always points outboard (left variant). The exception is `claws`, at −12 mm.
  - The sign and scale are established on the front axle, where the discs carry 8 distinct values.
  - The rear axle is only consistent with the rule: all 16 r_disc units carry −0.01, so the rear cannot test it.
  - Rear tyres are one twin model (`rear_1`, x ±0.339), and the rear disc spans both tyres. Only the outboard bead was measured.
  - Scania S 2016 preset: `front_disc_05` has `tire_shift` = −0.038. The tyre moves 3.8 cm outboard. Before this offset was applied, the disc lip stuck out 4.2 cm past the bead, which matched the reported floating rim.
- **Rims (`f_rim`/`r_rim`, pairing Unknown).** The complete wheels embed rim geometry; for example, `f_wheel/front_1.pmd` contains the `rim/front_1` geometry. Which tyre or wheel an `f_rim` is meant to pair with is not known.
  - Front rims 1, 2, 4, 6, 7, 8 and `steel` fit the stock tyre within 9 mm.
  - Rims 3, 5, `raven` and every `r_rim` have lip radii of 0.34 to 0.48 m, so the lip/bead test does not apply.
  - 0 of 26 presets use this family.
- **`wheel_shift` (Unknown).** It appears only on 3 DLC Michelin 385 tyres, always 0.0325. The non-DLC definitions of the same model lack it, and the tyre's outboard bead (−0.1444) matches the stock tyre's.
  - It is not the rim→tyre offset.
  - 0 of 26 presets use it, and it is not applied.
- **Wheel locator rotation (Hypothesis).** 11 of 562 chassis wheel locators (91 chassis models) carry a ±90° or 180° yaw: MAN TGX 4x2/6x4/6x2_4, Renault Magnum, Iveco Stralis.
  - Applying that yaw turns a wheel sideways or puts it on the inboard side of its locator.
  - Wheels therefore take only the locator's position. The engine's own handling is not traced.

### F10: Interiors have an exterior-view model placed at the cabin's `ext_interior` locator (Confirmed data, Hypothesis rule)
Evidence: `research/evidence/interior-survey.txt` (peer research, reproduced by our parser).

- **Data (Confirmed).** All 438 `accessory_interior_data` units carry two models:
  - `model` (with `animated_model` and `collision`): the full cockpit, used by the interior camera;
  - `ext_model`, `ext_variant` and `ext_look`: a light model shown through the windows. It is 8–15× smaller.
- **Placement (Hypothesis, strong).** The light model is placed at the cabin's only `ext_interior` locator.
  - That placement fits inside the cabin box for all 26 presets.
  - Placing it at the origin would fail for 6 of them: MAN TGX 2020 and Euro 6, Scania R 2016, S 2016 and S 2024e, and Volvo FH16.
- **Implementation.** `specsheet-data::assembly` places it there. The real-install test `every_preset_validates_and_assembles_without_warnings` requires an interior placement for every preset in both drives.
- **Right-hand drive (Hypothesis, strong).** 219 of the 438 units are UK twins: they carry `left_hand_traffic: true`, a UK `model` and their own `ext_variant` (e.g. `s_h_std_eu` → `s_h_std_uk`). Nothing in the data links a pair.
  - **Rule:** the twin is in the same directory, and its file name is the left-hand-drive one with one extra `uk` word (`uk_a`, `a_uk`, `standard_low` → `standard_uk_low`). It pairs all 219 one-to-one with no ambiguity.
  - **Implementation:** `specsheet-data` stores the twin's exterior model as `exterior_uk` / `variant_uk` and places it in right-hand drive.
  - **Tests:** `every_interior_has_a_right_hand_drive_twin` (219 of 438), and the preset test's RHD branch.

### F11: Addon hookups mount on numbered positions of their host accessory (Confirmed data, Hypothesis on position rules)
Evidence:
- `research/evidence/hookup-slot-survey.txt` (example `hookup_survey`);
- the definitions below, read with the `scs`, `attr_scan` and `model_info` examples;
- F8 strings `A hookup already exists on the given position (%Iu) for parent '%s'` and `Parent accessory not found for hookup`.

- **Catalogue (Confirmed).**
  - `/def/vehicle/addon_hookup_storage.sii`, plus one `addon_hookup_storage.<dlc>.sii` per DLC, include `addon_hookups/*.sui`.
  - Classes: `accessory_hookup_data` (aux lights, beacons, small lights), `accessory_horn_hookup_data`, and `accessory_hookup_int_data` / `accessory_hookup_int_ui_data` (dashboard, glass and cup-holder items).
  - Each unit (`<id>.addon_hookup`) has `model`, `name`, `icon`, `price`, `unlock` and `suitable_for[]`. Unlike accessories, its `suitable_for[]` lists **host slot names**: `light` → `r_grill`, `f_grill`; `smalllight` → 16 host slots including `f_bumper`, `sideskirt`, `exhaust_l`; `horn` → `r_grill`; dashboard items → `set_dashbrd`.
- **Positions (Confirmed).**
  - Host accessory models carry locators `slot_<n>`. Example: `dutch_lightbox_modern.pmd` has `slot_74` … `slot_78`.
  - The survey found them on 20 host slots, from 1 model (`f_logo`) to 303 models (`r_grill`).
- **Configuration (Confirmed).**
  - A host's `vehicle_addon_accessory` stores parallel arrays `slot_name[i]` (position locator) and `slot_hookup[i]` (hookup unit).
  - Example `/def/vehicle/truck_company/oversize/mercedes_6x4.sii`: `.r_grill` has `slot_name[] = slot_4, slot_0` and `slot_hookup[] = cyl01.addon_hookup` (×2).
  - 4,131 `vehicle_addon_accessory` units carry the arrays, mostly empty.
- **Placement (Hypothesis, strong).**
  - A hookup sits at the host model's position locator, composed with the host's own placement.
  - Real-install test `configured_hookups_sit_on_their_host_model`: both beacons of the example land within the light bar's bounds.
- **Unknown.**
  - Whether a position restricts which hookup types fit it (big vs small lights). Today any hookup that fits the host slot may go on any of its positions.
  - The per-type size limits the game may apply.

### F12: Two display details that are game data, not bugs (Confirmed)
- **"Volvo FH6".** The game's own series labels: `/locale/common.sui` has `series_volvo_fh16 = FH3`, `series_volvo_fh16_2012 = FH4`, `series_volvo_fh_2021 = FH5`, `series_volvo_fh_2024 = FH6`. Truck names are `info[]` keys (`@@brand_volvo@@`, `@@series_volvo_fh_2024@@`) resolved through it.
- **No side mirrors on the Volvo FH 2024 preset.** The preset (`/def/vehicle/truck_desktop/volvo_fh_2024.sii`) selects `mirror/digital_aero.sii`. Its name is `@@mirror_camera@@` and its model is `/vehicle/truck/volvo_fh_2024/accessory/mirror/digital.pmd`: the camera-mirror option.

## Open questions

| Question | Next step |
|---|---|
| Exact slot-to-locator matching rule (suffixes `_s`, `_sl`, digits) | REA: decompile the caller of `0x1421c2f90` (F8) |
| How pins are positioned and which slots get one | REA: decompile `FUN_140fe0fc0` (references the pin-button string, F8) |
| DLC mount priority | Runtime log does not show it; low impact (DLCs rarely override base files) |
| Metadata record types `0x05` and `0x06` | No impact on reading content |
| Meaning of `hide_in` bitmask on addons | Probably camera-dependent hiding; not needed for exterior preview |
| `tire_shift` semantics in the engine (the F9 rule is measured, not traced) | REA: trace references to the `tire_shift` string and decompile the wheel setup |
| `wheel_shift` on the DLC Michelin 385 tyres | REA in the same run, then compare with the wheel track width |
| Legacy `f_rim`/`r_rim` + `f_wheel`/`r_wheel` fit, and which `tire_shift` applies when several rim parts on one axle carry one | Not used by any preset; measure against the `f_wheel` models when that family is exposed |
| How the engine uses (or ignores) yawed wheel locators | REA: the caller of `Missing wheel model locator '%s'` (`0x14215ec08`, F8) |
| `addon_hookups` (accessories on accessories, e.g. horns on roof grills, `suitable_for[]: "r_grill"`) | After base addons work |

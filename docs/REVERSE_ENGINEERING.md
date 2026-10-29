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
| Our own parsers and the research examples (`specsheet-research/examples/{survey,scs}.rs`, `specsheet-research/examples/{model_info,def_stats,def_grep}.rs`) | Archive and definition content | "Parser observation", reproducible with the commands in [DEVELOPMENT.md](DEVELOPMENT.md) |
| The game's own `game.log.txt` from a real session | Runtime behaviour | "Runtime log" (passive; we did not run the game) |
| sk-zk TruckLib.HashFs / TruckLib.Models (GPL-2.0), GisDeflate (MIT) | Cross-checking our layouts | "Reference", never proof on its own. Facts only, never code (see `THIRD_PARTY_NOTICES.md`) |

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

### F5: The tuning screen places "accessory pins" at runtime (Confirmed ids, tabs and placement)
- `ui/truck_configuration_center.sii` contains `ui::group : _nameless._.accpins` (id 9000) with **no children**. Pins are created by code, not by the UI definition.
- The same file has the tab row: `ui::button_row` id 499, values `cabin~~chassis~~engine~~transmission~~color~~e_accessories~~interior~~i_accessories`, ids `510~~500~~520~~530~~550~~560~~540~~570`.
- **Pin and tab ids (Confirmed, REA static).** `FUN_140fe0fc0` (`ev_47817a8…`, `research/evidence/rea-pin-select.txt`) shows an accessory on the tuning screen. It switches the tab row to the accessory's tab, then activates its pin:
  - accessory pin = button `10000 + n`, where `n` indexes the screen's `acc_info_t` records (one per vehicle and slot token);
  - hookup pin = button `11000 + m`;
  - the tab ids come from an `.rdata` table sorted by id: `500 510 520 530 540 550 560 570 571 572`. Names come from the `id_list` positions above: 500 chassis, 510 cabin, 520 engine, 530 transmission, 540 interior, 550 color, 560 e_accessories, 570 i_accessories. 571/572 are `trailer_chain`/`trailer_body` in `ui/trailer_configuration_center.sii` (`research/evidence/ui-layout-survey.txt` §K).
- **Which slots get a pin (Hypothesis, strong).**
  - Slots in a token table filled at run time use a fixed tab button and get no pin. These are presumably cabin, chassis, engine, transmission and paint. The loop scans 12 tokens, but the id table holds 10 ids; how the last two map is **Unknown**.
  - Every other accessory gets a pin under `e_accessories` (560). It goes under `i_accessories` (570) instead when its slot is in an interior-slot token list or is `steering_w` (a token stored in the executable).
  - **File-backed candidate for the interior list (Hypothesis).** `/def/truck_config_scene.sii` has `interior_only[]` with 17 slot names, all of them truck slot directories. It also has `hookups_exterior_only[]: sunshld` (ui-layout-survey §M).
- **Pin data (Confirmed contents, Hypothesis use).** `/def/truck_config_scene.sii` is the `config_path` of the scene widget in `ui/truck_configuration_bcg.sii` (`ui_truck_scene`, id 400). It holds 65 parallel entries: `pin_group`, `pin_look`, `pin_offset`, `pin_dir`, `pin_dir_uk`, `double_sided` (§J).
  - `pin_look` is `custom_acc` 59 times, and `acc_cover`, `acc_hub` and `acc_nuts` twice each.
  - 60 of 65 `pin_group` values match truck slot directory names. Unmatched: `cabdefl`, `f_fendr_cab`, `hoodlatch`, `m_mudflap`, `markers`.
  - Most likely this is the per-slot pin offset and direction (with a UK direction) applied to the slot's locator.
- **How pins are built and shown (Confirmed, REA static).** Evidence: `research/evidence/pin-attributes.txt` §5–8. The builder is `FUN_140fcf210` (`ev_ddace5e…`); the per-frame update is `FUN_140fcba50` (`ev_bf0e8de…`). The pin data loads into `ui_truck_scene_config` and, per truck, `accessory_truck_data`. The app follows these rules in [app/hotspots.md](app/hotspots.md).
  - **Base point:** the first visible locator whose name starts with the slot name, so one per slot, never a mean. The model search order is a Hypothesis. A slot without a locator gets no pin.
  - **Position and facing:** position = base + `pin_offset`, in truck space. The facing comes from `pin_dir` as heading and pitch, with no roll. A zero direction gives the identity rotation.
  - **Without a table entry:** the pin sits on the locator with the locator's rotation.
  - **`pin_dir_uk`:** used when a flag of the interior object is set (Hypothesis: right-hand drive).
  - **`double_sided`:** adds a mirrored pin, with offset x and direction x negated.
  - **Wheels:** the exterior tab only. Pins for tire, disc, hub, cover and nuts on every wheel: left wheels at wheel + offset, right wheels mirrored when `double_sided`. **Rims, `f_wheel` and `r_wheel` never get a pin.** Hub, cover and nut pins link to their wheel's disc pin and show only while that disc pin is open: selecting a pin stores its index at the field the update compares.
  - **Per-truck table:** a truck's own pin table replaces matching scene entries. Its `use_non_wheel_pin_data_from_model` puts non-wheel pins on their raw locators; its default is Unknown, and no 1.60 truck sets it.
  - **Every frame:** opacity = clamp((cos θ − 0.1) / 0.9, 0, 1), where θ is between the pin's facing and the camera's view axis. A pin is hidden outside the view depth or at zero opacity. The button is 32 px square, 36 px in one state. **No merging of nearby pins and no depth test.**
  - **Correction (2026-10-28):** the user's earlier in-game note "the rim pin sits on the wheel's locator" cannot be a rim pin, since no rim record exists. It is most likely the **hub** pin (offset −0.1 / −0.12, at the wheel centre). To re-check in game.
  - Data side: `research/evidence/pin-table.txt` (the 65 entries), `pin-survey.txt` (pins on every preset land on their parts: median 0 m, 90 % within 0.1 m).
- **Input (Confirmed, REA static).** The single caller `FUN_140fbf860` is the screen's input handler, not pin creation (`ev_f6f0551…`, `research/evidence/rea-pin-handler.txt`). It handles `joy_sec_prv`/`joy_sec_nxt` (tabs, by name), `joy_nav_prv`/`joy_nav_nxt` (pin to pin), `j_trzoom_in`/`j_trzoom_out`, select and back, and treats ids 10000–19999 as pins.
- **Unknown:** the default of `use_non_wheel_pin_data_from_model`, the game's model search order, where interior pins end up, and what the pulsing ring (record flag +0x3c) marks.

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
- **Hypothesis:** the tuning screen itself renders `/def/truck_config_scene.sii` (FOV 50, the `config_path` of `ui/truck_configuration_bcg.sii`; see F5), and the `/def/desktop` scenes are hub backgrounds. The executable string `/ui/truck_configuration_bcg.sii` (`0x14223bbb0`) sits beside the `truck_config_hdl_u` strings (`research/evidence/ui-layout-survey.txt`).
- **Tuning camera orbit (Hypothesis; read as `specsheet-data::scene::OrbitSpec::position`, nothing traced).** From `/def/truck_config_scene.sii` (see `docs/app/scenes.md`):
  - `camera_default_azimuth` turns around +y, from +z towards +x;
  - a negative `camera_default_elevation` raises the camera above `camera_aim`;
  - distance = `camera_default_zoom` × `camera_relative_bounds.x` (7.5);
  - `camera_elevation_limit` (−0.35, 0) bounds the elevation, keeping the camera between the aim's height and about 20° above it.

  With the 1.60 values (aim (0, 2.6, −1.465), azimuth −2.516, elevation −0.068) this gives a front-left three-quarter view about 7.5 m out, which looks plausible. A reference screenshot (`research/parity/`) or REA on the `ui_truck_scene` camera code would confirm or refute it. **Unknown:** `camera_aim_y_bounds`, and the units of `mouse_rot_sens` and `camera_zoom_sens`.

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

- **REA function** `ev_47817a8206a278f996aa8bdd5002e18731d8e65fbf27ad42fa0857b42bc3d520` (`research/evidence/rea-pin-select.txt`): `FUN_140fe0fc0` selects a pin rather than creating one. Results are in F5. Its only caller is `FUN_140fbf860`.
- **REA function** `ev_f6f0551988f753985a7b81707b39a259d33f67755e2876037bbb577fb61d657a` (`research/evidence/rea-pin-handler.txt`): `FUN_140fbf860` (4,913 bytes) is the tuning screen's input handler. Results are in F5.

Next REA step: one combined string search (the `pin_*` keys of `truck_config_scene.sii`, camera, exposure, lift-axle and paint-effect seeds), then decompile the functions it points to, plus the caller of `0x1421c2f90` (locator lookup), to turn F4/F5 into Confirmed.

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
- **Implementation.** `specsheet-assembly` places it there. The real-install test `every_preset_validates_and_assembles_without_warnings` requires an interior placement for every preset in both drives.
- **Right-hand drive (Hypothesis, strong).** 219 of the 438 units are UK twins: they carry `left_hand_traffic: true`, a UK `model` and their own `ext_variant` (e.g. `s_h_std_eu` → `s_h_std_uk`). Nothing in the data links a pair.
  - **Rule:** the twin is in the same directory, and its file name is the left-hand-drive one with one extra `uk` word (`uk_a`, `a_uk`, `standard_low` → `standard_uk_low`). It pairs all 219 one-to-one with no ambiguity.
  - **Implementation:** `specsheet-data` stores the twin's exterior model as `exterior_uk` / `variant_uk` and places it in right-hand drive.
  - **Tests:** `every_interior_has_a_right_hand_drive_twin` (219 of 438), and the preset test's RHD branch.
- **The full cockpit sits at the same locator, which is the driver's eye (Hypothesis, strong).**
  - The interior's `model`/`look`/`variant` (the cockpit) is placed at the cabin's `ext_interior` locator, like `ext_model`.
  - The cockpit's locators are head-relative. In `/vehicle/truck/scania_2016/interior/model.pmd`, `steering_w` is at (−0.0007, −0.372, −0.555) and the cab-centre locators sit at x +0.6827, while `ext_interior` is at x −0.683 in truck space. So the cockpit origin is the driver's seat.
  - The interior accessories' locators (`steering_w`, `curtain_f`, `toyhang`/`toybed`/`toyseat`, `set_glass`, cup-holder sets, `intlight_*`, `flag_l`/`flag_r`, `frntglss_mid`, …) are on the cockpit model, not the cabin. This explains why those slots get no pin on the exterior.
  - Implementation: `Assembly::cockpit` (peer commit d19ebca); test `real_install::every_preset_has_a_cockpit_at_the_eye_point`.

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

### F13: Each chassis has one wheel-locator pair per declared axle, and the locator decides the tyre type (Confirmed)
Evidence: `research/evidence/axle-survey.txt` (example `axle_survey`), the chassis definitions, and `/def/vehicle/rim_data.sii`.

- **Axles in the definition (Confirmed).**
  - `accessory_chassis_data` has one entry per axle, front to back, in `powered_axle[]`, `kerb_weight[]` and `residual_travel[]` (all 164 chassis).
  - `steerable_axle[]` is present on 61 chassis and `liftable_axle[]` on 71.
  - Example: Scania S `6x2_long_taglift` has `powered = false, true, false` and `liftable = false, false, true`.
  - No chassis attribute names tyres or axle models per axle.
- **Wheel locators (Confirmed).**
  - Group the chassis model's visible `wheel_*` locators by z, within 5 cm. Left and right locators of one axle differ by up to 0.5 mm.
  - All 164 chassis then have exactly one left/right pair per declared axle: 132 with 3 or 4 axles, 0 mismatches.
  - Names are `wheel_f…` or `wheel_r…` only. The trailing numbers vary per model and aren't axle indices.
- **Tyre type (Confirmed).**
  - The locator letter decides it: `wheel_f*` gets the front (single) wheel parts and `wheel_r*` the rear (twin) parts. This is what `wheel_placements` already does.
  - No definition flag predicts the letter:
    - "steerable" matches 160 of 164 chassis. MAN TGX `6x22` and Volvo FH `6x2_m` have a non-steerable, liftable second axle with `f` locators.
    - "liftable" fails as well: Scania S `6x2_long_taglift`'s liftable tag axle uses `r` (twin).
  - `rim_data.sii` defines truck wheel types `f`, `r`, `a` and `b`, but no truck model uses `a` or `b`.
- **Test.** The real-install test `every_chassis_has_one_wheel_pair_per_declared_axle` guards all of the above.
- **Unknown.**
  - Whether the tuning screen shows liftable axles raised or lowered. The app shows them lowered. To settle: a reference screenshot (docs/PARITY.md) or REA on `liftable_axle`.

### F14: How the game finds, gates and mounts mods (Confirmed sequence and priority mechanism; the winning lookup is a Hypothesis)
Evidence: `research/evidence/mod-loader-survey.txt` (game log, `eurotrucks2.exe` strings, the example `mod_probe` over 55 local packages, 77 Workshop items). Formats are in [formats/mods.md](formats/mods.md).

- **Discovery (Confirmed).**
  - At start-up, after base and DLC archives, the game opens every file in `/home/mod` and reads its `manifest.sii`.
  - It picks the driver by content (`SCS#` → hashfs, `PK` → zipfs), never by extension.
  - DLC archives are logged "Created and validated" with a SHA-256; mods never are.
  - Workshop items come from a Steam query; `versions.sii` picks the variant.
- **Gating (Confirmed).**
  - A mod is "Incompatible" when it has a `compatible_versions[]` array and no glob in it matches the game version. Checked in both directions: of 54 manifests, exactly the two without a 1.60 glob are the two the log flags.
  - Hypothesis (exe string only): a missing `dlc_dependencies[]` DLC also marks a mod incompatible.
  - In multi-folder packages, `dlc_<name>/` folders for DLC that isn't installed are skipped.
- **Mounting (Confirmed).**
  - Nothing is mounted until a profile is selected.
  - Then the game logs "Active N mods (local, workshop)" and mounts each active mod, in that order, into a separate mod pool. Base and DLC stay in the scs and core pools.
- **Unknown.**
  - How the mod pool ranks against the scs pool.
- **Priority (Confirmed mechanism, `research/evidence/mod-priority-static.txt`, static disassembly).**
  - Mods mount in active-list order with rising priorities, starting at 1001.
  - The file system keeps its mounted devices sorted by priority, highest first.
  - **Hypothesis (strong):** a lookup takes the first device that has the path, so for a path two mods share, the mod **later** in `active_mods[]` wins. The lookup walk itself was not read.
- **Profile list (Confirmed, since resolved).** The active list is `active_mods[]` in the `ScsC`-encrypted `profile.sii`, read with `scsc::sii_text`. Index 0 is mounted first ([formats/profiles.md](formats/profiles.md)).

### F15: Paint jobs: one unit, a token-named per-part file, masks cut per cabin (Confirmed data, Hypothesis on unlisted parts)
Evidence: `research/evidence/paint-job-survey.txt` (S1–S10). Format details: [formats/paint.md](formats/paint.md). No shader code was read (D13).

- **Paint-job unit (Confirmed).** Each `paint_job/<file>.sii` holds one `accessory_paint_job_data` unit (19,998 units, 0 files with more). It carries `base_color`, `paint_job_mask`, `mask_{r,g,b}_color`, the flags `airbrush`, `alternate_uvset` and `flipflake`, flake (`flake_color`, `_density`, `_shininess`, `_clearcoat_rolloff`, `_uvscale`, `_noise`) and flip (`flip_color`, `flip_strength`) parameters, `*_locked` flags, `stock` and `color_variant[]` (S2).
- **Per-part file (Confirmed).** `paint_job/accessory/<token>.sii`, where `<token>` is the first token of the paint job's unit name, not its file stem. The executable has the string `%s/paint_job/accessory/%s.sii`; 17,410 paint jobs link by token and 0 by stem only (S3). The file holds nameless `simple_paint_job_data` units: a mask, `flake_uvscale`, `flake_vratio` and `acc_list[]` entries `<slot>.<accessory>`, which name units `<accessory>.<truck>.<slot>` (560,896 of 561,084 resolve, S4).
- **Masks (Confirmed).** `pjm_at_<x>x<y>_size_<w>x<h>.tobj` tiles, BC1/BC3 sRGB, sized as named; each cabin variant of a design (`_a`, `_b`, `_c`, one `suitable_for` cabin each) uses its own tile. `shared_<RRGGBBAA>.tobj` masks are uniform 4×4 textures equal to the hex name (4,816 of 4,816, S5).
- **Presets and saves (Confirmed).** `vehicle_paint_job_accessory` references the paint job by file path (`data_path`) and stores up to six colours: base, mask r/g/b, flake, flip; saves write all six (S8, S10).
- **Hypothesis.**
  - Painted parts that no `acc_list` names use the paint job's main mask: the Scania S bumper and door handles share the cab's material and lie in the cab's UV square (S6). The executable's per-part rule agrees (F17).
  - Three kinds of paint job: plain/metallic, colour mask, airbrush livery (feature combinations and effect variant names, S2, S7). The effect flags in F17 fit this.
- **Confirmed by the executable (F17).**
  - `flake_noise` defaults to `/material/custom/flake_noise.tobj`.
  - A `*_locked` colour wins over the truck's saved colour.
- **Unknown.** `color_variant[]` and `stock`; how mask tile coordinates map to UVs; the per-accessory files `accessory/<slot>/paint_job/` (127 of 131 name no installed paint job, S3); the shading maths (needs a clean-room spec).
- **Test.** The real-install tests `every_paint_job_parses`, `paint_job_parts_resolve_by_unit_token`, `every_paint_mask_exists_or_is_a_constant`, `every_desktop_preset_loads_its_paint_colours` and `a_cabin_change_keeps_the_paint_job_design`.

### F16: Executable strings for the tuning scene, post-processing, lift axles and paint effects (REA Evidence, static; uses Unknown)
Evidence: `research/evidence/rea-search-batch2.{txt,json}` (`ev_f88b185e…`). These are strings only: which code reads them is not traced yet.
- **Pins.** `pin_group`, `pin_look`, `pin_offset`, `pin_dir`, `pin_dir_uk` and `double_sided` sit together (`0x1421b7360`–`0x1421b73c0`). This is the reader of the `truck_config_scene.sii` pin arrays (F5) and the next target for pin placement.
- **Tuning camera.** `ui_truck_scene_config` carries `camera_elevation_limit`, `camera_default_elevation`, `camera_default_azimuth`, `camera_default_zoom`, `camera_aim_y_bounds` and `camera_relative_bounds`. `ui_truck_scene_base_config` carries `camera_aim`, `camera_aim_lv`, `camera_fov_lv`, `light_environment`, `floor_reflection` and `cube_reflection`. The `_lv` keys are **Unknown** (a low-detail variant?).
- **Floor reflection (Confirmed message).** The game forces `floor_reflection: false` on a scene that has "no parts to be customized for reflection". So the floor reflection needs a scene part to customise, presumably the `reflection` entry of `part_order[]` (F7).
- **Reflection cubes.** The executable names `/material/environment/vehicle_reflection.tobj` itself, and never `ui_vehicle_reflection`. The flat UI cube is named only by the garage's own materials (`research/evidence/reflection-cubes.txt`).
- **Post-processing.** A `pp_deferred_parameters` unit with `deferred_param_eye_adapt_factors`, `deferred_param_tonemap_params`, `deferred_param_bloom_params` and `deferred_param_color_correction`. Also `eye_adaptation_speed`, eye adaptation through `def.adapt.rfx`, a setting `r_color_correction`, and `env_factor_*` weather factors (sunlight, rain, wetness, fog). The tuning screen therefore likely runs the deferred pipeline with eye adaptation (Hypothesis).
- **Lift axles.** Input actions `truck.lift_axle` / `truck.trailer.lift_axle`, the stick animations `lift_axle_stick_anim` / `lift_axle_trailer_stick_anim`, indicator colours, and the keys `liftable_axle` / `liftable_axles`. Nothing yet says whether the tuning screen shows them raised (F13 stays Unknown).
- **Paint effects.** The effect suffixes `flipflake`, `altuv`, `colormask`, `airbrush` and `flipflakeuv` sit next to `%s/paint_job/accessory/%s.sii`, along with `alternate_flipflake_uvset` and `/paint_job/`. So the paint effect name is built from suffixes (Hypothesis, F15). Two hard-coded paths name single trucks, `kenworth.w900` and `volvo.fh16_2012` `/paint_job/%s`; their purpose is **Unknown**.
- **Next.** `rea trace --query=<string>` resolves one string per run to its function (about 30–45 min each), then `rea function` decompiles it. In order: `pin_offset`, `camera_default_azimuth`, `deferred_param_tonemap_params`, `%s/paint_job/accessory/%s.sii`, `liftable_axles`.

### F17: How the game applies a paint job to a part's material (Confirmed, REA static; two caller paths Unknown)
Evidence:
- `research/evidence/rea-paint-variant.txt`: the material builder `FUN_1408d1100`, decompiled (`ev_6e3f6685…`); found by tracing `colormask` (`ev_e5217996…`).
- `research/evidence/rea-paint-apply.txt`: its callers, from static disassembly.
- `research/evidence/paint-class-layout.txt`: field offsets from the executable's attribute tables.

Facts only (D13); the format side is in [formats/paint.md](formats/paint.md).
- **Material.** For each material of a painted part, the game clones the material onto the effect `eut2.truckpaint` with these flags:
  - `airbrush` = the paint job's `airbrush`;
  - `flipflake` = its `flipflake`, named `flipflakeuv` when `alternate_flipflake_uvset` is set;
  - `colormask` = it has a mask, and neither `airbrush` nor `flipflake` is set;
  - `altuv` = `alternate_uvset`, forced off for a part listed in the per-part file (below).

  It also copies `shadow`, `skinned` and two more flags from the original material (which two is **Unknown**). If the RFX conversion fails, the part keeps its material ("Unable to create material when applying paint job.").
- **Opt-out.** A material that passes an engine check (`FUN_14042e920`, Unknown), or carries the sentinel (−1, −2, −3, −4) at material `+0x70`, gets plain `eut2.truckpaint` without paint flags.
- **Textures.**
  - The mask goes into the slot `texture_paintjob` when `colormask` or `airbrush` is set.
  - `flake_noise` goes into `texture_flakenoise` when `flipflake` is set.
  - `base_texture_override` replaces `texture_base`.
- **Colours: the saved colour unless locked.** Each of base, mask r/g/b, flake and flip comes from the truck's saved paint (`vehicle_paint_job_accessory`). The paint job's own colour is used only when that colour's `*_locked` is set. Without a paint job, all six are the saved ones.
- **Per-part rule.** When the part is named `<slot>.<accessory>` in `paint_job/accessory/<token>.sii`, its unit supplies the mask, `flake_uvscale` and `flake_vratio`, and `altuv` is turned off. Otherwise the paint job's own mask is used, with `alternate_uvset`.
- **Shader constants** (four vec4 at material `+0xf0`, `+0x100`, `+0x110`, `+0x120`):
  - without `flipflake`: (mask_r, 0), (mask_g, 0), (mask_b, 0), (base, 0);
  - with `flipflake`: (flake_uvscale, flake_shininess, flake_clearcoat_rolloff, flake_vratio), (flake_color, flake_density), (flip_color, flip_strength), (base, 0).
- **Defaults.** Both paint classes embed one parameter block. Its default construction gives:
  - mask r/g/b = (1,0,0), (0,1,0), (0,0,1); base (1,1,1);
  - flip (1,0,0), `flip_strength` 0.27; flake (0,1,0);
  - `flake_uvscale` 32, `flake_vratio` 1, `flake_density` 1, `flake_shininess` 50, `flake_clearcoat_rolloff` 2.2;
  - `flake_noise` `/material/custom/flake_noise.tobj`.

  Confirmed for the per-part instance; for absent paint-job attributes, Hypothesis (strong, same block type).
- **Attributes the base game never sets** (0 units each): `alternate_flipflake_uvset`, `base_texture_override`, and `base_texture` (stored in no plain field; handled by code).
- **Thumbnails.** `FUN_1408d05b0` builds the paint list's thumbnail from the job's own colours:
  - with an `icon`: `<icon>.mat`;
  - with `flipflake`: layered `clr_0` base, `clr_1` flip, `clr_2` flake, `clr_3` uncoloured;
  - otherwise: `color_icon.mat` in the base colour.

  Each colour is sRGB-encoded, ×255 truncated, and written as `FF`+BB+GG+RR ([formats/paint.md](formats/paint.md#thumbnails-f17)).
- **No builder call.** One caller (`0x1408cea60`) calls the builder only when the job has a mask, `flipflake`, `airbrush` or a base override; otherwise it calls `0x1408d0ea0` (Hypothesis: restores plain truckpaint). How a plain-colour job's colour reaches the shader is **Unknown**.
- **UV sets (peer screen tests, consistent with the above).**
  - The paint mask uses PMG UV set 1, and `altuv` uses set 2, counted from 0. Decided on the Actros 2014 Gigaspace with `bg_sml_m4_a` (airbrush, `alternate_uvset`; the cab paint piece has 3 UV sets): only set 2 puts the livery on the door.
  - An earlier MAN TGX Euro 6 result that seemed to need the `.mat`'s own `altuv` was a coincidence: on those cabs both readings give the same set. The MAN cab is not in Altamira's `acc_list`, so it takes the job's main mask.
- **No rescale.** The builder copies the record's colours and `flip_strength` into the constants unchanged. `flip_strength` therefore reaches the shader raw: up to 2.7 in the data.
- **Unknown.**
  - Two of the five callers (`0x14068e680`, `0x1408d2740`) compute the `altuv`/`flipflakeuv` flags from other values.
  - Which UV set is sampled when the model lacks the one named above (1-set and 2-set cabs).
  - How the shader combines the constants, and whether it clamps `flip_strength` (D13: needs a clean-room spec).

### F18: How the engine fills the material and light uniforms (Confirmed, static disassembly)
Evidence: `research/evidence/exposure-tonemap-data.txt` §9 (no Ghidra run: string xrefs, `.pdata` extents and capstone listings, kept out of the repository). Facts only (D13); the shader-side meaning is in the specs under [app/shader-specs/](app/shader-specs/).
- **Registration.** `FUN_1403dcf80` binds each material uniform name to a CPU callback that fills it per draw. The light object every callback reads has the layout of `eutpp_scene_light` (F16, `exposure-tonemap-data.txt` §7).
- **Forward lights.** The rows are ambient with `environment`, the sun direction, then diffuse and specular × `diffspec_conditional_mult`. **`limited_pre_exposure` is not applied to them.** On the tuning screen the scene's light colours reach the shader unscaled.
- **Pre-exposure scales emissive inputs only:**
  - lamp and backlight colour tables (copied from the drawn object, × `limited_pre_exposure`, zero past the array's end);
  - material luminance given in nits: × `limited_pre_exposure` × 0.001.

  Negative `luminance_output` is relative (|value|, so −1 gives 1), and 0 means 50 nits. Day and night values blend in log space by a night factor that the tuning scene sets to 0.
- **Reflection modulator** (`g`): the scene's `environment_conditional_mult`, 1.0 on the tuning screen. It is 1.0 for cubes in an engine-held set, and 9999 without a reflection texture.
- **Envmap matrix.** The camera's rotation rows with the **third row negated**: cube lookups mirror world z (Hypothesis that the rows are eye→world).
- **Fresnel by effect flags** (the `.rfx` `flags:` field):
  - flags 0 (`dif.spec` family): the `.mat` pair raw;
  - `0x10000` (truckpaint): F0 = 0.035;
  - `0x18000` (glass): F0 = 0.07;
  - `0x8000`: F0 = 0.14.

  A `.mat` `fresnel` whose second value is −1 overrides the constant with its clamped first value.
- **Reflection tint.** The material block's colour row is the product of two material colour slots, the second being `specular`, so `Ke = env_factor ⊙ specular` (Hypothesis, strong, for the first slot). The engine clamps `diffuse`, `specular` and `env_factor` to [0, 5] when it applies a `.mat`.
- **Lamp contrast** = 1 + 3·(night factor), so 1 on the tuning screen.
- **Unknown.**
  - The per-draw object behind `material_luminosity`'s colour.
  - The contents of the lamp and backlight arrays in the garage.
  - The deferred light object's filling.
  - The adaptation limits and speeds on the tuning screen. The UI scene's per-frame update (`FUN_140f8acc0`, Ghidra, `research/evidence/rea-ui-scene-update.txt`) sets no exposure, adaptation or tonemap parameter (Confirmed), so the histogram adaptation runs with values the UI never sets.
  - Which lights the scenes' `vehicle_lights_on: true` switches on (8 of 10 desktop scenes set it).

### F19: Vehicle lights are locator hookups that name `flare.*` units (Confirmed data; drawing Unknown)

- A light locator's hookup string names a unit in `/unit/hookup/vehicle/flare/*.sii`, e.g. `flare.vehicle.headl` on a headlight locator. The classes are `flare_vehicle` (steady), `flare_blink` (blinking) and `beacon_vehicle` (rotating). Each unit names its `light_type` (`low_beam`, `high_beam`, `parking`, `brake`, `reverse`, the blinkers, `aux`, `beacon`, `electrics`) and usually a glow model under `/model/flare/`, whose material uses `eut2.flare`. Many also carry a real light: colour, range, cone angles (`research/evidence/flare-survey.txt`).
- Of the 49 distinct `flare.*` hookups on truck models, 47 resolve. `flare.vehicle.lblinkerl_ledl`/`rblinkerl_ledl` (one model) name no unit; the folder defines `lblinkr_ledl`/`rblinkr_ledl` (Hypothesis: a naming slip, so those lights get no flare). Every glow model named exists; `_noflr`/`_nofl` units have none.
- Beacons also use `flare_only.*` and `light_only.*` hookups from outside this folder (not resolved yet).
- Code: `specsheet-data::flare::vehicle_lights`; `Scene::vehicle_lights_on`.
- The shader is specified in [app/shader-specs/flare.md](app/shader-specs/flare.md) (71): a 1×1 m quad, additive, no occlusion query; billboarding, size and angle fade happen on the CPU.
- **Unknown:** how the engine uses `dir_type`, the angles and the scaling keys, and which light types a scene's lights-on mask switches on. The tuning screen's own scene does not set `vehicle_lights_on`, so it shows no flares there (Hypothesis, from the data).

## Open questions

| Question | Next step |
|---|---|
| Mod pool vs scs pool; the lookup walk that confirms the later mod wins | Static: the readers of the sorted device list (`0x140155610` registers it, F14) |
| Exact slot-to-locator matching rule (suffixes `_s`, `_sl`, digits) | REA: decompile the caller of `0x1421c2f90` (F8) |
| How pins are positioned, and which slot tokens use fixed tabs instead | REA: string search for `pin_offset`/`pin_dir`/`pin_group`, decompile the reader; and the writer of the token table `0x14350cc00` |
| DLC mount priority | Runtime log does not show it; low impact (DLCs rarely override base files) |
| Metadata record types `0x05` and `0x06` | No impact on reading content |
| Meaning of `hide_in` bitmask on addons | Probably camera-dependent hiding; not needed for exterior preview |
| `tire_shift` semantics in the engine (the F9 rule is measured, not traced) | REA: trace references to the `tire_shift` string and decompile the wheel setup |
| `wheel_shift` on the DLC Michelin 385 tyres | REA in the same run, then compare with the wheel track width |
| Legacy `f_rim`/`r_rim` + `f_wheel`/`r_wheel` fit, and which `tire_shift` applies when several rim parts on one axle carry one | Not used by any preset; measure against the `f_wheel` models when that family is exposed |
| How the engine uses (or ignores) yawed wheel locators | REA: the caller of `Missing wheel model locator '%s'` (`0x14215ec08`, F8) |
| `addon_hookups` (accessories on accessories, e.g. horns on roof grills, `suitable_for[]: "r_grill"`) | After base addons work |

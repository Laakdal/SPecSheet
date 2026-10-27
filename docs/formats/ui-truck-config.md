# The truck tuning screen layout

The four layout files of screen `truck.config` plus the filter dialog, as read from 1.60.1.7. Format: [ui.md](ui.md). Full trees with every id and rectangle: section D of `research/evidence/ui-layout-survey.txt` (`ui_survey.py . tree <file>`).

Rects are `x l..r, y b..t` in the 1440 × 900 reference with y up. The structure, ids, classes and rects below are Confirmed (read from the files). **Roles are Hypothesis** unless a localisation key or icon name states them.

## truck_configuration_bcg.sii: 3D view

- The `ui::window` root (`keep_aspect: none`, `clip_children`) holds `[400] ui_truck_scene` at full screen, `layer −2`, with `config_path: "/def/truck_config_scene.sii"`.
- That file is a `ui_truck_scene_config`:
  - camera: `camera_fov 50`, `camera_aim (0, 2.6, −1.465)`, default azimuth/elevation/zoom, bounds and sensitivities;
  - light vectors;
  - the scene model `/model/ui/garage/desktop_garage.pmd` + `.pma`;
  - `part_order[]`;
  - **pin data**: 65 entries of parallel arrays `pin_group[]`, `pin_look[]`, `pin_offset[]`, `pin_dir[]`, `pin_dir_uk[]` and `double_sided[]` (Confirmed). `pin_look` is `custom_acc` ×59, and `acc_cover`, `acc_hub` and `acc_nuts` ×2 each.
  - **Hypothesis:** `pin_group` values are slot names. 60 of 65 match a truck accessory slot directory or a wheel slot under `/def/vehicle/` (`beacon`, `mirror`, `f_tire`, …). `cabdefl`, `f_fendr_cab`, `hoodlatch`, `m_mudflap` and `markers` match none.
  - `interior_only[]` lists 17 slots, all of them existing slot directories;
  - `hookups_exterior_only[]: sunshld`.
  - What the pin fields do (offset from the locator? facing for culling?) is a Hypothesis to test against [F5](../REVERSE_ENGINEERING.md).
- **Which scene config the screen uses** (Hypothesis):
  - `truck.config` in `/ui/desc/truck.sui` has no `background:` line, and no file under `/ui/` references `truck_configuration_bcg.sii`.
  - The executable holds the string `/ui/truck_configuration_bcg.sii` at `0x14223bbb0`, next to the `truck_config_hdl_u` strings (`research/evidence/rea-search-tuning-strings.txt:297`). The handler probably loads this background in code, which would make `/def/truck_config_scene.sii` the tuning scene.
  - The app currently renders `/def/desktop/config*.sii` (F7, `camera_fov 45`).
  - Next step: an REA xref of `0x14223bbb0`.

## truck_configuration_center.sii: canvas (`keep_aspect: center`, handler `truck_config_hdl`)

```
[0] ui_blend_window 0..1440 × 0..900
└ [1001] group _.all.components (layer 6)
  ├ [1000] group −2..1440 × 0..900
  │ ├ [9000] group _.accpins          full screen, NO children: pin buttons created by code (F5)
  │ ├ [8005] button  btn.remove_acc   46×57 at 773..819 × 396..453   remove-accessory button beside a pin
  │ ├ [600] group 375..1065 × 85..137 message bar: [0] text bg box + cross.mat, [610] text txt.normal.left
  │ ├ [620] text  txt.acc_conflict    64×64 at 620..684 × 265..329   conflict marker (acc_conflict.mat)
  │ ├ [410] button_row _.channel.buttons  btn.truck_chnl_s, radio, 557..884 × 745..807  paint channels
  │ └ [0] linear_layout dimension x, 464..976 × 824..888, background_id 911   category bar
  │   ├ group: [576] prev-tab glyph (glyphs_xb U+E8C3) + [575] key-hint bg
  │   ├ [499] button_row btn.truck_grp radio, value_list/id_list:
  │   │      cabin 510 · chassis 500 · engine 520 · transmission 530 · color 550 ·
  │   │      e_accessories 560 · interior 540 · i_accessories 570   (icons /material/ui/truck_config/<v>.mat, 64×64)
  │   ├ group: [578] next-tab glyph (U+E8C5) + [577] key-hint bg
  │   └ [911] text txt.window.bcg_rect4 "@@clr_bg_main@@"   bar background
  ├ [333] text @@notification_walking_camera@@ ("First person camera enabled"), 564..876 × 139..203
  └ [587] text crosshair.mat 48×48, layer 99 (walk-mode crosshair)
```

**Tab ids** (Confirmed from the layouts):
- The tab row `[499]` pairs `value_list` with `id_list` by position. Each value names the icon `/material/ui/truck_config/<value>.mat`.
- The ids are not in value order: 500 is chassis and 510 is cabin.
- `trailer_configuration_center.sii` reuses 500, 550 and 560, and adds 571 and 572.
- Together these cover all ten ids of the executable's tab table (F5).

| id | Truck tab (`truck_configuration_center.sii:329-330`) | Trailer tab (`trailer_configuration_center.sii:329-330`) |
|---|---|---|
| 500 | chassis | trailer_chassis |
| 510 | cabin | |
| 520 | engine | |
| 530 | transmission | |
| 540 | interior | |
| 550 | color | trailer_color |
| 560 | e_accessories | trailer_e_accessories |
| 570 | i_accessories | |
| 571 | | trailer_chain |
| 572 | | trailer_body |

**Pins:** pin buttons are created in code under `[9000]`, with ids `10000 + n` for accessories and `11000 + m` for hookups ([F5](../REVERSE_ENGINEERING.md)). No unit in the five files has an id of 10000 or more (Confirmed).

## truck_configuration_left.sii: list panel (`keep_aspect: left`, handler `truck_config_dispatch_hdl`)

```
[0] ui_blend_window 0..1440 × 0..900
├ [111] overlay_point bottomleft 356..625 × 10..50       control-hint anchor
└ [1000] group _.whole_ui 7..345 × 10..893 (fitting)
  ├ group title: [10] text txt.title.center @@truck_config_t@@ ("VEHICLE CONFIGURATOR") 7..345 × 840..880
  └ group _.left.panel 7..345 × 10..843
    ├ [385] group _.shared 7..345 × 25..177              price box
    │ ├ [364] _.cartprice / [362] _.oneytorefund / [360] _.neytopaycart   values, right-aligned 197..335
    │ ├ [386] buy.mode.texts: @@price@@: / @@refund@@: / @@total@@         labels at y 140/113/86
    │ ├ [387] customize.mode.texts: @@tc_orig_price@@ / @@tc_extra_price@@ / @@tc_new_total@@
    │ └ [210] buttons: [201] _.back cart.mat 38×30 (btn.normal) · [200] _.checkout @@checkout@@ 266×30
    ├ [380] group _.firstmessage: [381] "@@clr_sel@@|@@select_cat@@" (shown before a category is chosen)
    ├ [305] group _.components 7..345 × 174..887 (fitting)   accessory browser
    │ ├ [300] list_common _.acclist btn.truck_acc 17..309 × 174..775, item 292×96, gap 5, slider_management
    │ │   └ slider_common sld.slider0d 309..343
    │ ├ [310] text _.title (empty; caption set by code) 7..345 × 823..849
    │ ├ [401] inputline _.search.input 242×30, default_text @@search@@
    │ ├ [403] button btn.input.clear 16×16 · [404] tooltip @@clear_input@@
    │ └ [402] button _.filter.button.id btn.truck_config.package.filter 40×30   opens the filter dialog
    ├ [355] group _.cart 7..334 × 34..862                order view (alternative to _.components)
    │ ├ text "@@clr_sel@@|@@basket_t@@" ("ORDER") · [350] list_common _.cartlist btn.truck_cart 292×96 items
    │ └ [582] button @@clear_cart@@ 318×30
    ├ [391] _.part.type.text + [390] _.part.type.bcg.text   tooltip at 355..598 (accessory origin badge)
    └ [0] text txt.window.bcg_rect4 @@clr_bg_main@@ 7..345 × 10..893   panel background
```

## truck_configuration_right.sii: info panel (`keep_aspect: right`, handler `truck_config_dispatch_hdl`)

```
[0] ui_blend_window 0..1440 × 0..900 (clip_children)
├ [1001] group 1050..1440 × 226..900
│ └ [1000] group
│   ├ [11] text _.money txt.normal.center 1113..1433 × 853..893 (+ bg rect)   player's money
│   ├ [6001] group _.truck.analysis.group.0000           analysis, folded: title @@truck_analysis_t@@ + [6300] unfold btn
│   ├ [6000] group _.truck.analysis.group 1113..1433 × 239..843   analysis, unfolded
│   │ ├ title @@truck_analysis_t@@ ("VEHICLE ANALYSIS") · [7500] help icon · [6301] fold button
│   │ ├ [6100..6104] txt.conf_bar.big 280×33   suitability bars (args: lost|current|gain|colour|label)
│   │ ├ heading @@current_truck_params@@ · [6200..6209] txt.conf_bar.small 280×28 (args: colour|name|value)
│   │ ├ [7000..7004] tooltips 845..1109 · [7510] @@truck_analysis_info_tooltip@@
│   │ └ text_table_common row_height 59, interleaved light/dark rows · bg rect
│   └ [420] group _.color.picker.group 1113..1433 × 316..843   paint editor
│     ├ [410] button_row btn.truck_chnl_s (channels) · [415] droplist btn.drop_ao → list btn.truck_cv4 228×54
│     ├ [430] ui_color_picker 280×230 · [473] copy wheel colour 48×48
│     ├ [465] droplist btn.drop_clr → list btn.list_clr · [460..462] inputs R/G/B (tab 1..3, limit 3) + labels
│     ├ [440] use preset · [445] save colour · [446] save preset (87×42 btn.generic)
│     └ [450] portal child_path /ui/user_colors_view.sii + [451] slider · bg rect
└ [20] button btn.truck_sw value "leave" 1384..1434 × 10..60   exit (truck_config/leave.mat)
```

## truck_config_filters.sii: content filter dialog (modal overlay, handler `accessory_filters_hdl`)

```
[0] ui_blend_window (keep_aspect center)
├ text window bg 499..942 × 133..771 · [10] title @@accessory_filters_t@@ ("CONTENT FILTERS") · [20] close btn.closewin
├ [150] @@add_all@@ ("Enable all") · [151] @@remove_all@@ ("Disable all")  164×30
├ [110] list_common _.list btn.truck_config.package.filter.entry item 381×30, gap 3 (+ slider, striped table)
├ [105] group _.checkbox.group (empty: checkboxes created by code)
└ text full-screen dim background (no_scale/msg_bcg.mat)
```

## Filled at runtime

- **Containers with no children, filled by code** (Confirmed empty): `[9000] _.accpins`, `[300] _.acclist`, `[350] _.cartlist`, `[105] _.checkbox.group`, `[110] _.list`.
- **Widgets whose text code sets:**
  - `[310] _.title`;
  - the price values (`[364]` holds the placeholder `1500,-`);
  - `[11] _.money` (placeholder `€ 22 563,-`);
  - `[610]` message;
  - the analysis bars (placeholder values).
- **Not found in any `/ui/` layout file:** the toolbar icons in `/material/ui/truck_config/` (`camera`, `default_view`, `walk_mode`, `fullscreen`, `show_vehicles`, `lights`). How they appear is **Unknown**: code-created, or another file.

## Captions

| Use | Key → en_us | Label |
|---|---|---|
| Category tabs | none: the tabs are icon-only (`btn.truck_grp` has no text parameter) | Confirmed |
| Category names for `[310] _.title` | `cabin_t` CABIN, `chassis_t` CHASSIS, `engine_t` ENGINE, `transmission_t` TRANSMISSION, `interior_t` INTERIOR, `accessory_t` ACCESSORIES, `paint_job_t` STOCK COLOR; `in_accessory` "Interior Accessories" has no `_t` form | Hypothesis: which key the code uses per tab |
| `color`, `e_accessories`, `i_accessories` as keys | missing from `en_us/localization.sui`, `accessories.sui`, `local.sii`, `common.sui` and `ui_colors.sui` | Confirmed missing |
| `imv_unf_joy_nav_prv/nxt` (tab-key hints) | in none of the 14 `en_us` and shared locale files, though 5 `/ui/` files use them | Hypothesis: the engine supplies them (bound key names); Unknown how |
| Title, order and price labels | `truck_config_t`, `select_cat`, `basket_t`, `checkout`, `clear_cart`, `price`, `refund`, `total`, `tc_orig_price`, `tc_extra_price`, `tc_new_total`, `search`, `clear_input`, `truck_analysis_t`, `current_truck_params`, `accessory_filters_t`, `add_all`, `remove_all` | Confirmed (all resolve) |

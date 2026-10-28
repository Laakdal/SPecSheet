# UI layouts, templates, fonts and icons

What the game's `/ui/` layout files and the files they reference contain. Nothing in the app parses them yet. The tuning screen built from these files is described in [ui-truck-config.md](ui-truck-config.md).

Evidence: `research/evidence/ui-layout-survey.txt`, built by `tools/research/ui_survey.py`, `tools/research/ui_evidence.sh` and the `ui_icon_survey` example. Labels are those of [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md).

## Files and screens

- `/ui/` in 1.60 holds 457 `.sii`, 31 `.sui` and `credits.txt`. All are text SII ([sii.md](sii.md)).
  - Our parser (`specsheet-formats::sii`, with includes) reads every `/ui/**/*.sii`: `def_stats -- /ui` exits 0 and reports 288 unit classes.
  - The survey script finds the same 288 classes (Confirmed).
- **Screens** are `screen_desc` units in `/ui/desc/*.sui`, included by `/ui/!desc.sii` (Confirmed). A screen lists layout files as `ui_script[]: "<handler name>|<path>"`. It may also have `background:`, `overlay:`, `modal:` and `render_order`.
  - `truck.config` loads three layouts: `truck_config_left|…left.sii`, `truck_config_right|…right.sii` and `truck_config|…center.sii`.
  - `truck.config.accessory.filters` loads `accessory_filters|/ui/truck_config_filters.sii` with `overlay: true, modal: true`.
- **Handlers:** each layout file has one window root whose `window_handler` names a non-widget unit, for example `truck_config_hdl { window_link: <root> }`. That class is the engine's C++ handler: `truck_config_hdl_u` in F8 (Confirmed for the name). All behaviour lives in that code.

## Units

Every widget unit has these attributes (Confirmed, 9,024 of 9,024 units carrying `coords_*`):

| Attribute | Type | Observed |
|---|---|---|
| `coords_l/r/t/b` | int, sometimes `&hex` float | the widget rectangle (below) |
| `area_l/r/t/b` | int or float | see "coords vs area" |
| `id` | int | 0 = unaddressed. Other values are looked up by code. Ids of code-created widgets (tab buttons, pins) are in [F5](../REVERSE_ENGINEERING.md); tab ids per layout are in [ui-truck-config.md](ui-truck-config.md) |
| `layer` | int | −2…99. Draw order within the window (Hypothesis: higher draws on top; the full-screen 3D scene is −2, the crosshair 99) |
| `tab` | int | −1 almost everywhere. 1/2/3 on the R/G/B inputs of the colour picker (Hypothesis: keyboard focus order) |
| `pointer` | int | −1 in 9,024 of 9,024 units. Meaning **Unknown** |
| `my_parent` | unit ref or `null` | `null` only on window roots |
| `my_children` + `my_children[]` | count + refs | on containers |

Containers also carry `fitting: bool` (Hypothesis: the container sizes itself to its children). Window roots (`ui::window`, `ui_blend_window`, `ui_blend_only_window`) add:
- `keep_aspect: none|left|center|right`;
- `clip_children`;
- `window_handler`;
- `user_string_data`;
- `first_direction_focus_id`;
- for blend windows, `blend_x_shift`/`blend_y_shift`.

**Widget types are unit classes.** The whole `/ui/` tree has 10,552 units in 288 classes. The tuning screen uses these (counts in the five files):

| Class | n | Role and own attributes |
|---|---|---|
| `ui::text_common` | 59 | Text through a template: `look_template`, `value` (template arguments), `text` (always `""` here) |
| `ui::text` | 5 | Raw markup in `text` (backgrounds, crosshair) |
| `ui::button_common` | 17 | Button: `look_template`, `value`/`value2`, `button_type: normal\|radio\|check`, per-state `n/s/s2/d/p_pml` overrides (empty here) |
| `ui::button_row` | 3 | A row of buttons generated from `value_list`/`id_list`, split by `list_delimiter` (`~~`). Also `row_align`, `item_distance`, `button_size_type: dynamic`, `button_type: radio` |
| `ui::list_common` | 5 | Scrolling list filled by code: `item_width`, `item_height`, `item_gap`, `max_list_height`, `multi_column`, `slider_management`; usually a `ui::slider_common` child |
| `ui::slider_common` | 5 | Scrollbar: `tick`, `margin`, `minimal_handle_size`, `action_on_drag`, `move_to_point` |
| `ui::inputline_common` | 4 | Text input: `limit`, `default_text`, `allowed_characters`, `make_uppercase`, `selection_color`/`ime_color` (decimal ARGB-like ints) |
| `ui::droplist_common` | 2 | Drop-down whose child `ui::list_common` is the popup |
| `ui::text_table_common` | 2 | Striped table background: `row_height`, `col_widths`, `v_cellspacing`, `indexing_cycle` |
| `ui::linear_layout` | 1 | Flow layout: `dimension: x`, `inner_padding`, `alignment_x/y`, `background_id` (id of the sibling drawn behind), `background_padding_*` |
| `ui::group` | 22 | Plain container |
| `ui::portal` | 1 | Embeds another layout file: `child_path`, `hslider_id`/`vslider_id` |
| `ui::overlay_point` | 1 | `overlay_anchor_type: bottomleft`, `value: "left\|bottom"` (Hypothesis: anchor for the control-hint overlay) |
| `ui_color_picker` | 1 | `hue_width`, `hue_bar_pos`, `hue_cursor`/`sat_cursor` materials, `padding` |
| `ui_truck_scene` | 1 | The 3D truck view: `config_path` names a `ui_truck_scene_config` SII |

`ui_survey.py . stats` lists every class and attribute with value kinds.

## Coordinates

- **Reference resolution 1440 × 900** (Confirmed). `game.log.txt` prints `Setting UI reference mode to: 1440x900...` at start-up. The 152 blend windows and 68 plain windows that are root screens use `coords 0..1440 × 0..900`. Smaller roots (popups, widgets, dashboards) use their own sizes.
- **y points up, with the origin at the bottom left** (Hypothesis, but strong). `coords_t ≥ coords_b` holds in 9,024 of 9,024 units. The page title sits at y 840–880, and the leave button at y 10–60 in the bottom-right corner.
- **Coordinates are absolute in the window's reference space, not relative to the parent** (Hypothesis). 7,978 of 8,599 children lie inside their parent's rectangle, with values in the same 0–1440 range. Counterexamples: `_.all.components` in the center file is 128 × 128 but holds a full-screen child, and the children of `ui::linear_layout` lie outside its rectangle. Hypothesis: layouts and `fitting` groups reposition children at runtime, and parent rectangles do not clip.
- Negative values occur in 79 units. Example: the center group at x = −2, which is a 2-unit bleed.
- **coords vs area** (Hypothesis):
  - In 6,611 leaves, `area_*` is the sentinel `(l 1, r 0, t 0, b 1)`, an empty rectangle with l > r.
  - In 2,054 containers `area == coords`.
  - In 45 containers it differs, mostly windows and portals with a larger area. Example: `adviser_chat_messages` coords 0..280, area 0..328. This suggests the scrollable content extent.
  - Every unit in the tuning files follows the first two patterns.
- **Scaling to other resolutions** (Hypothesis): the UI is scaled by `screen_height / 900`, and `keep_aspect` positions the 1440-wide window inside the wider virtual width (2560 × 1440 → 1600 × 900):
  - `left` pins it to the left edge;
  - `right` pins it to the right edge;
  - `center` centres it;
  - `none` stretches it to fill.

  The tuning screen splits into left/center/right files with exactly those three values, and the 3D scene window uses `none`. No UI-scale cvar exists in `config.cfg`.

## Templates and text style

- `look_template` names a unit in `/ui/template/*.sii` (Confirmed for all 42 used by the tuning files):
  - `ui::text_template` (`name`, `text`, optional `width`/`height`): 306 units, `txt.*`;
  - `ui::button_template` (`n_pml` normal, `s_pml` selected/hover, `s2_pml`, `p_pml` pressed, `d_pml` disabled, `width`, `height`, `action_sound`): 309 units, `btn.*`;
  - `slider_template` (`sld.*`), `inputline_template` (`inp.*`), `text_table_template`, `tooltip_template`, `frame_template`.
- **Templates are markup with parameters.** A widget's `value` is split on `|` into `%0`, `%1`, …, which are substituted into the template (Hypothesis, consistent with every use checked). Example: `txt.normal.color.center` is `<color value=%0>%1`, used with `value: "@@clr_sel@@|@@select_cat@@"`. Button rows substitute each `value_list` item as `%0`. List items (`btn.truck_acc`) receive up to `%9` from code.
- **Markup tags** used by the screen's files and templates:
  - `<img src=… width= height= xscale=stretch|repeat yscale= color= left= right= top= bottom=>` (pN = pixel offsets);
  - `<font face=/font/x.font xscale= yscale=>`;
  - `<color value=AABBGGRR>`;
  - `<align hstyle=left|center|right vstyle=top|center|bottom>`;
  - `<margin left= right=>`, `<offset hshift= vshift=>`;
  - `<ret>`, `<br>`, `<nowrap>`.

  The full tag grammar is **Unknown**.
- **Markup layout** (all Hypothesis). These rules make the geometry of `txt.window.bcg_rect4`, `btn.truck_grp` and `btn.truck_acc` add up exactly. They are implemented in `specsheet-gameui::flow`, and the unit tests reproduce those three templates:
  - The cursor starts at the widget's top-left, with y pointing down inside the box. Units are reference units, and one texel is one unit.
  - `<ret>` returns the cursor to the origin; it does not start a new line. In `btn.truck_grp`'s selected state, `…</align><ret><img icon>` draws the icon over the background, not below it. `<br>` starts a new line.
  - `<offset hshift= vshift=>` moves the cursor by that much from where it is (Confirmed). In `btn.truck_acc`, `<img height=22><ret><offset vshift=22><img height=74>` fills the 96-unit item exactly. About 2,000 of the 4,180 offsets in `ui/*.sii` sit mid-line as spacing, e.g. `<img width=60><offset hshift=10><img width=60>` in `nonlinear_rank_desc.sii`, which would overlap if offsets were absolute. The game's layered paint thumbnail steps back over its 256-wide icon with `hshift=-256` (F17).
  - `<img left= right= top= bottom=>` with `pN` values select texels N. Reversed ranges mirror the texture: `left=p16 right=p0` is the mirrored corner. The texel span is the natural size.
  - `width`/`height` override the natural size. A negative value is the space remaining on the line (or below the cursor) minus N, and `xscale`/`yscale` `stretch|repeat` fill that space. With these rules, `bcg_rect4` comes out as a 4-unit top strip, a middle part and a 16-unit bottom strip that tile the box.
  - Edges of `<margin>` and `<align>`: a positive value is an absolute coordinate, while zero or negative is an inset from the far edge. Symmetric `<margin left=15 right=-15>` is the most common margin.
  - `<align hstyle= vstyle=>` lays out its content from the top-left of its own box, which runs from the current line down to the bottom of the widget, then shifts that content as one block.
  - Unverified until the reference screenshots are compared: the text metrics, `<nowrap>`, `<hbr>`, how `repeat` differs from `stretch`, and how glyph baselines line up.
- **`@@key@@`** expands from the localisation database ([sii.md](sii.md)). That includes colours.
- **Colours** are localisation keys in `/locale/ui_colors.sui` (153 `clr_*` keys, included from `/locale/common.sui`). Values are 8 hex digits, read as **AABBGGRR** (Hypothesis): `clr_red` = `FF4C4CFC`, `clr_green` = `FF00DD75` and `clr_blue` = `FFFFB204` decode to the named colour only in that order. Example: `clr_sel` = `FF03B0FA`, an orange accent. Literal `0xC5000000` also appears.

## Fonts

- `/font/<name>.font` is a text descriptor, `# SCS Font (SDF format)` (Confirmed):
  - `vert_span`, `line_spacing`, `default_scale`;
  - `image:<.mat>, width, height, scale x, scale y, SDF padding` lines;
  - one row per glyph: `xHHHH, atlas x, atlas y, width, height, plane x, plane y, advance, image index`.

  The header says the metrics assume a **1024 × 768** reference. That is separate from the 1440 × 900 UI reference, and how the two combine is **Unknown**.
- Atlas pages are `.mat` files (`effect "ui.font.msdf.rfx"`, `aux[0] = {w, h, padding, 0}`) over packed `.tobj` textures. `normal_0` is 1024 × 512, DXGI 88 (B8G8R8X8). The fonts are multi-channel SDF (Hypothesis, from the effect name).
- The tuning screen uses `normal`, `small`, `big_bold`, `title_bold` and `glyphs_xb`. `glyphs_xb` is an icon font; the prev/next tab arrows are its glyphs U+E8C3/U+E8C5.

## UI materials and icons

- UI `.mat` files come in two syntaxes (Confirmed, accessory icons):
  - `effect : "ui.rfx" { texture : "texture" { source : "<x>.tobj" … } }`, in 5,647 of 5,848 files;
  - legacy `material : "ui" { texture : "<x>.tobj" texture_name : "texture" }`, in 201.

  `source` is relative to the `.mat`.
- Category and toolbar icons in `/material/ui/truck_config/` use `effect "ui.sdf.rfx"` with `aux[]` parameters, over 64 × 64 DXGI 88 textures. They are single-channel SDF glyphs tinted by `color=` (Hypothesis).
- `white.mat` (1 × 1) and `corner_4px.mat` (16 × 16) build every panel background through `txt.window.bcg_rect4` and its relatives.
- **Accessory icons** (Confirmed on files): `icon: "<p>"` in an accessory definition resolves to `/material/ui/accessory/<p>.mat`.
  - Across all trucks, 5,848 of 5,851 distinct icon values resolve, and 5,844 of those `.mat` files name `<basename of p>.tobj` beside them. The other 4 share a sibling's texture, such as an `_r` exhaust using the `_l` image.
  - Icons are 256 × 64 (5,815), 64 × 64 (31, engine icons) or 512 × 128 (2). All are DXGI 91 (B8G8R8A8_SRGB) with one mip.
  - Across all of `/def/vehicle`, 8,960 of 9,006 resolve; 43 of the 46 misses are owned trailers.
  - The tile template uses `<img src=/material/ui/accessory/%1>` with no extension, and the engine splices markup into `%1` (Confirmed for paint jobs, F17, `research/evidence/rea-paint-apply.txt`). For a paint job:
    - with an icon: `<icon>.mat`;
    - without one (stock RAL colours): `color_icon.mat color=<base>`;
    - without one and with `flipflake`: `clr_0.mat` (base), `/material/ui/clr_1.mat` (flip) and `clr_2.mat` (flake), each a tinted layer stepped back with `<offset hshift=-256>`, under an untinted `clr_3.mat`.
    - The colours are the paint job's own, linear → sRGB, ×255 truncated, written `FF` + blue + green + red. `specsheet_session::view` builds this as the option's `icon`.

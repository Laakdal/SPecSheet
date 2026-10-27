# App UI

The app draws the game's own truck configuration screen, `truck.config`. It builds the screen from the layout files, templates, colours, icons and textures in the user's install. What those files contain is in [../formats/ui.md](../formats/ui.md) and [../formats/ui-truck-config.md](../formats/ui-truck-config.md). This page covers how the app uses them.

## Pipeline

| Step | Code |
|---|---|
| Load the screen (`/ui/!desc.sii` → `ui_script[]` layouts) and every template | `specsheet-data::ui::{load_screen, load_templates}` |
| A widget's markup: its template with its `value` split on `\|` as `%0…%9`, then `@@key@@` resolved | `ui::substitute`, `Localization::resolve` |
| Markup → positioned images and text (rules: [../formats/ui.md](../formats/ui.md), "Markup layout") | `specsheet-data::ui::flow` |
| Reference space → window | `crates/specsheet/src/ui/screen.rs` `Mapping` |
| UI materials → egui textures | `crates/specsheet/src/ui/textures.rs` |

- **Mapping (Hypothesis).** The 1440 × 900 reference space scales by window height / 900. Each layout's `keep_aspect` pins its 1440-wide window to the left, centre or right of the wider virtual width; `none` stretches it.
- **Textures.** UI images are B8G8R8A8/X8 and upload as they are.
  - Distance-field materials are converted to white images whose alpha is the shape's coverage (smoothstep at 0.5 ± 0.08), so the template's `color=` tints them. These are the category and toolbar icons (`ui.sdf.rfx`, single channel: the strongest channel) and the font pages (`ui.font.msdf.rfx`: median of RGB).
  - Compressed UI textures are not supported yet; they are skipped with a warning.
- **Text** uses egui's font, sized per game face: normal 15, small 12.5, big 20 and title 24 reference units, times the markup's `yscale`. Rendering the game's MSDF fonts is a known parity gap.
- **Wrapping.** Text wraps at spaces inside its margins unless `<nowrap>` is open. A wrapped block is aligned as a whole; its lines are not yet centred one by one.

## What is shown (Design)

The game's handler shows, hides and fills widgets in code that has not been traced. Until it is, `screen.rs` follows this table, keyed by the layout files' own widget ids:

| Widgets | Shown |
|---|---|
| Panel backgrounds, title, folded vehicle analysis `[6001]`, leave button `[20]`, category bar | always |
| Tab row `[499]` | one `btn.truck_grp` button per `value_list` token (the 64-unit icons fill the row). The open tab uses the selected state, and hover does too |
| "Select a category" `[380]` | while no list is open |
| Accessory browser `[305]` with list title `[310]` (`<color>@@clr_sel@@ …` + the tab's caption key) and list `[300]` | while a slot's options are open |
| Remove button, messages, conflict marker, paint channels, walk-camera notice, crosshair, cart, origin badge, price box, money, unfolded analysis, colour picker, control and tab-key hints, search, filter | hidden: their behaviour is not wired up yet. The money widget's separate background rect still shows |

## Tabs and pins

The eight tabs follow finding F5 (`specsheet-data::view::Tab`, `tab_of`):
- **Fixed tabs:** cabin, chassis, engine, transmission, colour (paint job) and interior. They have no pins; opening one lists its slot's options at once.
- **Exterior and interior accessories:** every other accessory goes under one of these two, by whether its anchor is inside the cab. Opening one shows the pins for that tab only ([hotspots.md](hotspots.md)); clicking a pin lists that slot's options.

## Accessory list

The list `[300]` is filled with `btn.truck_acc` tiles: 292 × 96 items with a 5-unit gap, scrolled with the mouse wheel and clipped to the list. The template arguments:

| Argument | Value |
|---|---|
| `%0` | accessory name |
| `%1` | its `icon` (resolves under `/material/ui/accessory/`) |
| `%3` | price, printed like the game's placeholders (`€ 22 563,-`) |
| `%4` | "new" badge alpha `00` |
| `%5` | "owned" badge alpha: `FF` for the fitted part, which shows the ✓ |
| `%6` | the reason, for incompatible options, which use the template's disabled state (lock icon) |

`%2`, `%8` and `%9` are not filled yet. `%9` names `truck_config/accessory_%9.mat`, which then doesn't resolve.

Hovering a tile uses the selected state. Clicking a compatible tile selects the part, and the truck re-assembles in the background.

## SpecSheet window

The truck picker, drive toggle, scene picker, game directory and messages have no place on the game's screen. They live in a small collapsible "SpecSheet" window at the bottom centre. When no game is open and discovery has failed, a centred box asks for the game directory.

## Busy state and settings

Work that parses files (opening the game, loading a truck, selecting, switching drive) runs one job at a time, and later clicks queue as actions. `%APPDATA%\dev.specsheet.app\settings.json` keeps the last game directory and the last truck.

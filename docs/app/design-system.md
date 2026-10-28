# SPecSheet's design system

One design language for every screen the game itself doesn't have: the home screen, truck picker, settings, saved configurations, mods and profiles, parity results and about. The method follows Horizon's design system (horizon-rust/horizon, MIT): a token crate as the single source of truth, a few strict rules, and one list component. The look is SPecSheet's own.

**Out of scope: the tuning screen.** It is ETS2's own UI, drawn 1:1 from the game's `ui/truck_configuration_*.sii` ([ui.md](ui.md)), because matching the game is the point. Don't restyle it with these tokens.

## Tokens: `crates/specsheet-theme`

A small crate with no dependencies holds every design value as Rust constants. The app maps them onto egui once (`crates/specsheet/src/ui/theme.rs`), so no screen hard-codes a colour, size or spacing. Change a value in the crate and every screen follows.

- **Colour roles**, in a light and a dark appearance:
  - `page` (background), `plate_soft` (hover, fields, segmented track), `plate` (pressed, switch off), `surface` (dialogs and menus above the page), `rule` (1 px row dividers);
  - `ink` (text, selection), `ink_soft` (secondary text, values), `muted` (labels, descriptions, mono indices);
  - `accent` (the main action), `on_accent`, `danger`, `success`.
  - Neutrals stay slightly warm. The accent is a truck-signal amber, not Horizon's violet.
- **Contrast tests.** A unit test checks that every text role meets WCAG AA (4.5:1) on every background it is drawn on, in both appearances. A palette change that fails the test doesn't ship.
- **Type scale:** `micro 11`, `caption 12`, `label 13`, `ui 14`, `body 15`, `heading 20`, `title 28`, `display 44`, `wordmark` (very large, faint). Each step has a line height, letter spacing and weight.
- **Spacing:** `4, 8, 12, 16, 24, 32, 48, 64, 96` px; page margin 64 px (16 px when narrow); corner radius 8 px.
- **Fonts:** one sans family and its mono, bundled under the SIL Open Font License with the license file in the crate, and credited in `THIRD_PARTY_NOTICES.md`. Never a font from the game.

## Rules

1. **Type is the interface.** Large, tight headings carry the hierarchy. No decorative boxes, and icons only where a word won't do.
2. **Lists, not cards.** Content is one numbered-row component: a small mono index on the left (`01`, `02`), the name large, a mono detail or control on the right, and a 1 px `rule` between rows. Trucks, saved configurations, settings, mods and parity results all use it.
3. **One accent, used once per screen**, for the single main action ("Open in tuning", "Choose game folder"), plus focus, links and progress. Selection is shown in `ink`, not colour.
4. **Space instead of borders.** No shadows, gradients or outlines around groups. A `plate_soft` tint appears only under the pointer, or behind something that must stand apart (a dialog, a log).
5. **Quiet motion.** Items arrive with a short fade and an 8 px rise, a few tens of milliseconds apart, eased out and never bouncy. Hover fades a plate in behind the row.
6. **A signature.** A giant, faint "SPecSheet" wordmark behind the home screen.

## Screens

| Screen | Content |
|---|---|
| Home | Game status (found, version vs `TESTED_GAME_VERSION`), the last truck, the main action "Open in tuning" |
| Trucks | Numbered list of trucks (brand and series from the game's locale) |
| Configurations | Saved configurations: name, truck, date, price |
| Mods and profiles | The profile's mod list and the readable containers (D15); read-only |
| Parity | `specsheet --parity` results: case, MAE/SSIM, pass or fail |
| Settings | Numbered rows: game folder, language, default drive side, appearance (light or dark) |
| About | Version, license (MIT), third-party notices, the game's trademark notice |

## Building it

- **Phase 1 (done 2026-10-28):** the `specsheet-theme` crate with its contrast tests, and `ui/theme.rs` mapping it onto egui. The app's current controls take their colours from the dark palette; fonts are not bundled yet.
- **Phase 2:** the shared widgets (numbered row, segmented control, switch, field), then the screens.
- Check each screen with a `SPECSHEET_SCRIPT` screenshot in both appearances.

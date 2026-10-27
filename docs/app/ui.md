# App UI (egui)

`crates/specsheet/src/ui.rs` draws the interface with bevy_egui 0.42 (egui 0.36). It renders what `specsheet-data::view` reports and turns clicks into `studio::Action`s; it holds no tuning rules. The layout is our own design (see [../PARITY.md](../PARITY.md)). Matching the game's `ui/truck_configuration_center.sii` layout, icons and categories is ROADMAP Phase 5.

## Layout

```
┌ top bar: ● SpecSheet │ truck ▾ │ LHD/RHD │ scene ▾ │ ☐ Interior markers │ spinner · game dir ┐
│                                                                          │ sidebar (320 px)  │
│                      3D view + hotspot markers                           │ parts or options  │
│                                                                          │                   │
└ messages: error, then up to three warnings (only when there are any) ──────────────────────────┘
```

- egui draws through its own full-window 2D camera (`ui::setup`, order 1, alpha-blended over the 3D view). The 3D camera's viewport is shrunk each frame to the rectangle the panels leave free. That way markers project into the right area, and the truck is centred in the visible space.
- When no game is open and discovery has failed, a centred box asks for the game directory.

## Sidebar

- **Parts:** groups "Chassis & cabin", "Exterior", "Wheels", "Interior" (slots with markers), then "General" (slots without). Each row shows the slot name and the selected part. A click opens the slot.
- **Options:** "‹ Parts" goes back. Options are listed selected first, then compatible ones, then the rest; ties go by price (`TruckSession::options`). Each row shows the name, the price in € and the unlock level.
  - The selected option is highlighted.
  - Incompatible options are greyed out, with their reason in red, e.g. "Requires one of: 8x4.scania.s_2016.chassis".
  - A click selects the option. The truck re-assembles in the background, and the list refreshes.

## Busy state

Work that parses files (opening the game, loading a truck, selecting, switching drive) runs one job at a time. Further clicks queue as actions. Option buttons and the drive toggle are disabled while a job runs, and the spinner shows while a job or a model load is in progress.

## Remembered settings

`%APPDATA%\dev.specsheet.app\settings.json` keeps the last game directory that opened and the last truck loaded. The app starts with that truck, or the installation's first truck if it is gone.

## Fonts

egui's default fonts lack some symbols (`←`, `●`). The brand mark is painted, and the back button uses `‹`.

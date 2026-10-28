# Hotspots (Design D7)

The clickable pins on the truck. The exterior pins follow the game's own rules, read from the executable ([F5](../REVERSE_ENGINEERING.md); evidence in `research/evidence/pin-attributes.txt`). Which parts of this match the game is tracked in [../PARITY.md](../PARITY.md).

## Exterior pins (`specsheet-assembly::pins`)

The game builds one record per pin from the truck's locators and the pin table of `/def/truck_config_scene.sii` (`pin_group`, `pin_offset`, `pin_dir`, `pin_dir_uk`, `double_sided`; tabulated in `research/evidence/pin-table.txt`).

- **Base point:** the first visible base-model locator whose name starts with the slot name (12-character names must match exactly). One base per slot, never a mean (`pins::slot_locator`). This differs from part placement, which uses the longest matching slot name (F4).
  - Models are searched cabin first, then chassis. The game's model order is a Hypothesis.
  - A slot without a locator gets no pin.
- **Position:** base + `pin_offset`, in truck space, neither rotated nor scaled. Many paired parts hang on one centre locator (`mirror_s` at x = 0), so their offset x is about half the truck's width.
- **Facing:** the unit vector of `pin_dir`. A zero direction gives the identity rotation, which faces forward (−z, Hypothesis). On a right-hand-drive truck `pin_dir_uk` replaces it (Hypothesis).
- **Without a table entry:** the pin sits on the locator and faces the locator's own forward.
- **`double_sided`:** a second, mirrored pin (offset x and direction x negated), built first.
- **Wheels:** pins for the tyre, disc, hub, cover and nuts of every wheel, axle by axle from the front, left before right.
  - The base is the wheel locator.
  - Left wheels get base + offset; right wheels get the mirrored pin when the entry is `double_sided`.
  - Rims and complete wheels (`f_wheel`, `r_wheel`) never get a pin.
- **Not used:** the per-truck pin table in `accessory_truck_data`, which in 1.60 only the Renault T and E-Tech T set (`f_light_top`). Also not used is its `use_non_wheel_pin_data_from_model` switch, taken as false (Hypothesis): otherwise the scene table's non-wheel offsets would never apply.

Measured on every preset (`research/evidence/pin-survey.txt`): the pins land on their parts, with a median of 0 m and 90 % within 0.1 m.

## Which slots get a pin (`specsheet-session::view::overview`)

Only accessory slots get a pin; base parts are picked through their fixed tab (F5, see [ui.md](ui.md)).
- An exterior slot is pinned when it is an addon or wheel slot with at least one option, or another slot with at least two. The game drops only slots with no options.
- The exterior pass skips the scene's `interior_only` slots (17 in 1.60), as the game does.

Each `Hotspot` carries:
- its tab;
- the selected part's name;
- an `empty` flag (nothing placed in the slot);
- its `facing`;
- for a wheel's hub, cover and nuts, a `link` to that wheel's disc pin.

Slots without a pin are listed in `TruckOverview::general` with their tab.

## Interior pins (`specsheet-assembly`)

The game's interior pass, with the same rules as the exterior slots (`pins::interior_pins`):
- **Base point:** the first locator of the selected interior's full cockpit whose name starts with the slot name. Only that model carries the interior slots' locators, and they are head-relative (F10), so the cockpit's placement at the eye point (`ext_interior`, or the UK eye for right-hand drive) puts them in the cab. The exe adds a per-vehicle translation whose content the decompile doesn't settle, so this is a **Hypothesis** consistent with the data (`research/evidence/pin-attributes.txt`).
- **Position, facing, mirroring:** base + `pin_offset`, `pin_dir`, and `double_sided` twins, as for the exterior.
- **Which slots:** every accessory slot whose name matches a cockpit locator, so `flag_l`/`flag_r` get interior pins too. The interior pass has no wheel pins.
- **Tab:** interior accessories.

## On screen (`crates/specsheet`)

Every frame, `ui::ui` projects each pin of the open accessory tab through the truck's transform and the 3D camera:

- **Fade** (`hotspots::opacity`, Confirmed F5): opacity = clamp((cos θ − 0.1) / 0.9, 0, 1).
  - θ is the angle between the pin's facing and the camera's view axis, the same axis for every pin.
  - A pin is fully opaque head-on and gone at about 84°.
  - From the default orbit (front-left), left-facing pins show at about 0.54, and their mirrored twins on the right are hidden.
  - Inside the cab, every pin shows fully.
- **Disc pins** (Confirmed F5): opening a wheel's disc pin shows that wheel's hub, cover and nut pins. Opening any other unlinked pin hides them again (`hotspots::open_after_click`).
- **Panels hide pins:** a pin that projects under a drawn part of the game screen is dropped.
- **No merging, no depth test:** like the game, nearby pins don't merge and pins aren't hidden behind the truck's geometry.

### Marker styles (ours)

| Marker | Look |
|---|---|
| Pin | Dot in its category colour: exterior orange, wheels blue, interior lilac. Hover shows the slot name. |
| Empty slot | Hollow ring: nothing is fitted in the slot |
| Active slot (open in the sidebar) | Extra orange ring |

The game draws its pins with its own button templates (`btn.custom_accn`, `btn.custom_accs` when selected, and the `acc_hub`/`acc_cover`/`acc_nuts` looks), 32 px square. We don't draw those yet.

## Known gaps

- Pin graphics are ours, not the game's button templates. The game also pulses a 64 px ring on some pins (Hypothesis: newly available parts).
- Interior pins follow the game's interior pass, but the transform it adds is a Hypothesis (above); mirroring without a table entry relative to the interior object is not reproduced.
- The per-truck pin table and `use_non_wheel_pin_data_from_model` are not read.
- Badge slots and the Renault T's `s_panel` can take another slot's locator by the first-prefix rule (`pin-survey.txt`); to check in game.

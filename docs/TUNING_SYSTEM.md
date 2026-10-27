# Tuning system

How the game describes truck customisation, and how `specsheet-core` + `specsheet-data` reproduce it.

Labels follow [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md): **Confirmed**, **Hypothesis**, **Design** (our own choice).

## Catalog

| Concept | Source | Our type |
|---|---|---|
| Truck model | `/def/vehicle/truck/<brand.model>/data.sii` (`accessory_truck_data`) | `TruckCatalog` |
| Slot | Directory name: `accessory/<slot>/`, or the group directory (`chassis`, `cabin`, `engine`, …); shared wheel groups `/def/vehicle/{f,r}_{tire,disc,rim,hub,nuts,cover,wheel}` | `SlotId` |
| Accessory | One unit per definition, named `<item>.<brand.model>.<slot>` | `Accessory` |
| Display names | `@@key@@` resolved via `/locale/en_us/local*.sii` (`localization_db` `key[]`/`val[]`). Slot labels use the slot name as key, e.g. `r_grill` → "Light Bar" (Confirmed) | `Localization` |

Unit classes map to `AccessoryKind` as follows (Confirmed by `research/evidence/truck-def-attribute-stats.txt`):

| Class | Kind |
|---|---|
| `accessory_chassis_data` | Chassis |
| `accessory_cabin_data` | Cabin |
| `accessory_addon*_data`, `accessory_horn_addon_data` | Addon |
| `accessory_wheel_data`, `accessory_rim_data` | Wheel |
| `accessory_paint_job_data` | PaintJob |
| `accessory_engine_data`, `accessory_transmission_data`, `accessory_head_lights_data`, `accessory_interior_data` | Engine, Transmission, HeadLights (not visual), Interior (its `ext_model` is placed, F10) |

Other classes are not accessories (e.g. `simple_paint_job_data`, `physics_patch_data`) and are ignored.

Model references:
- Chassis and cabin use `detail_model`, else `model`, together with `look`, `variant` and `variant_uk`.
- Addons use `exterior_model[_uk]`, `look` and `variant`.
- Wheels use `model` and `look`.

## Rules (`specsheet_core::Configuration`)

| Rule | Data | Status |
|---|---|---|
| An accessory is allowed only while one of `suitable_for[]` is selected. Entries may use `*` globs, e.g. `*.renault.etech_t.chassis` | 3,728 addons, all cabins | Confirmed semantics. All 26 game presets satisfy it |
| An accessory is not allowed while any `conflict_with[]` entry is selected, checked in both directions | 795 addons | Confirmed by presets |
| Selecting applies `defaults[]` unless the slot already holds a compatible choice | chassis → cabin, cabin → interior/paint/addons, addons → related addons | Hypothesis on the "keep compatible" part |
| `require[]` slots (truck data, cabin) must stay filled; base slots are always required | `data.sii`, some cabins | Confirmed data; Design on enforcement |
| Addons with an exterior model need a matching locator on the current chassis/cabin | see F4 | Hypothesis, 626/626 presets |
| Every selection, base parts included, must satisfy its own rules against the rest of the configuration (e.g. an 8x4-only cabin cannot go on a 4x2 chassis) | | Design; regression test `base_parts_must_satisfy_their_own_suitable_for` |
| Repair after a base-part change: drop selections that became incompatible, then refill required slots from `fallback[]`, then selected `defaults[]`, then the first compatible option | | Design |
| Selecting a paint job clears the preset's colour override, so the paint job's `base_color` applies | | Design |

Validation: example `catalog_info` validates every truck's game preset (`/def/vehicle/truck_desktop/*.sii`) and finds 0 issues for all 26 trucks, wheels included.

## Configuration state

`Configuration` holds:
- the selected accessory per slot;
- an optional paint-colour override (taken from the preset's `vehicle_paint_job_accessory.base_color`; otherwise the paint job's `base_color`).

The app starts every truck from its game preset.

## Slot ↔ locator rule (finding F4)

A base-model locator belongs to the longest slot name it starts with (`mirror_s` → `mirror`, `doorsteps` → `doorstep`). Only locators in parts visible in the current variant count. That variant is `variant`, or `variant_uk` in right-hand drive. The UK cabin parts carry mirrored locators.

## Addon hookups (F11)

- `Hookup` units (aux lights, beacons, horns, small lights, dashboard items) are loaded into each truck's catalog when they fit one of its slots. The catalog adds them with `TruckCatalog::with_hookups`.
- `Configuration` maps `host slot → position (slot_NN) → hookup`:
  - `set_hookup` requires that the host slot is filled, that the hookup lists the host slot in `suitable_for[]`, and that the position exists on the host model (positions come from `Assembly::hookup_positions`);
  - changing or removing a host accessory removes its hookups (Design);
  - `validate` reports `InvalidHookup`.
- Game configs (`slot_name[]` / `slot_hookup[]`) load through `load_preset` and `load_vehicle_config`.

## Not implemented yet

- `overrides[]`.
- `hide_in`.
- Paint-job textures and masks (only the base colour is applied).
- Interior camera (only the exterior-view interior model is shown; right-hand drive shows the interior's UK twin, F10).
- Engine, transmission, head lights, interior and paint job are selectable from the sidebar's "General" group (slots without a 3D marker), but only the paint job's base colour has a visual effect.

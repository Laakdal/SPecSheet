# Parity with the game

For each feature, how close we are to in-game tuning. The goal is a 1:1 replica ([ROADMAP.md](ROADMAP.md)).

- **Matches** means the behaviour is backed by evidence that the game does the same.
- **Ours** is a design choice we made where the game's behaviour is unknown or not yet reproduced.
- **Unknown** means not established.
- Finding IDs (F*n*) refer to [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md) and decision IDs (D*n*) to [DECISIONS.md](DECISIONS.md).

| Feature | Matches | Ours | Unknown | Notes |
|---|---|---|---|---|
| Archive reading, base mount order | ✔ F1, F2 | | | Every entry of every archive indexes |
| DLC mount priority | | alphabetical | ✔ | The game log does not show it (F2) |
| Truck list, catalogs, display names | ✔ F3 | | | Localisation keys resolved from `/locale/en_us` |
| Starting configuration | ✔ F6 | | | `truck_desktop` presets; all 26 validate |
| `suitable_for` / `conflict_with` | ✔ | | | All presets satisfy our reading; `*` globs supported |
| `defaults[]` applied on selection | partly | "keep a compatible current choice" | ✔ | [TUNING_SYSTEM.md](TUNING_SYSTEM.md) |
| Repair after a chassis/cabin change | | ✔ D9 | ✔ | |
| Accessory attachment at locators | | longest-prefix rule | engine rule | 626/626 preset parts resolve (F4); engine looks across model instances (F8) |
| LHD / RHD | ✔ `variant_uk`, `exterior_model_uk` | | | |
| Wheels | ✔ left/right model variants; `tire_shift` (F9, rule fits 94/96 disc cases); one wheel pair per declared axle on all 164 chassis, tyre type from the locator letter (F13) | side from locator x | lift axles raised or lowered in the tuning screen; `wheel_shift` | F8 mentions axle locators |
| Tuning pins: existence, ids | ✔ F5, F8 | | | Buttons `10000 + n` per accessory slot, `11000 + m` per hookup position |
| Accessory grouping (tabs) | ✔ F5 | | | Game tabs: cabin, chassis, engine, transmission, color, e_accessories, interior, i_accessories |
| Tuning pins: placement, which slots | which slots: Hypothesis (F5) | ✔ D7 | placement ✔ | Base parts (cabin, chassis, engine, transmission, paint) use tabs, not pins. Wheel parts share one anchor per axle group and empty slots are flagged (`Anchor::group`, `Anchor::empty`); the app decides how to show them |
| Tuning screen layout | ✔ drawn from the game's `ui/truck_configuration_*.sii`, templates, colours and icons ([app/ui.md](app/ui.md)) | which widgets show when; egui font instead of the game's MSDF fonts; slot list on accessory tabs; markup layout rules (Hypothesis, [formats/ui.md](formats/ui.md)) | how the handler fills and toggles widgets | Price box: `Configuration::price`, sum of part prices (Hypothesis) |
| Truck configuration scene (`/def/truck_config_scene.sii`) | model, lights, pin data parsed | orbit reading (Hypothesis, F7) | camera conventions until a reference screenshot confirms them | Default scene of the app |
| Tuning scenes (`/def/desktop`) | data rendered (F7) | camera choice, light mapping | ✔ | Peer note: these may be hub backgrounds rather than the tuning garage |
| Materials and shading | | ✔ approximation | | Game shaders are readable in `effect.scs` ([formats/effects.md](formats/effects.md)) |
| Paint jobs | base colour | | masks, flakes, liveries | |
| Interior seen through the windows | ✔ `ext_model` at `ext_interior`; UK twin in RHD (F10) | twin pairing by file name | | |
| Interior camera and interior slots | | | ✔ | |
| Addon hookups (accessories on accessories) | ✔ catalogue, `slot_NN` positions, `slot_name`/`slot_hookup` (F11) | dropped when the host changes | per-position type limits | Placement verified on the game's oversize Mercedes |
| `overrides[]`, `hide_in` | | | ✔ | |

## Visual checks

`specsheet --parity` renders the cases in `parity/cases.json` and compares each with the user's capture of the real game in `research/parity/<case>.png`. It scores mean absolute luma error and SSIM outside the masked UI regions. How to run it and how to capture references: [DEVELOPMENT.md](DEVELOPMENT.md), "Visual parity checks".

| Case | Truck, steps | Threshold (MAE) | Last score |
|---|---|---|---|
| `scania-s-preset` | `scania.s_2016` preset | 0.08 | no reference yet |
| `volvo-fh-2024-preset` | `volvo.fh_2024` preset | 0.08 | no reference yet |
| `scania-s-lightbar` | Scania S, `slot r_grill`, `pick 0` | 0.08 | no reference yet |

The thresholds are placeholders until the first references are scored. Renders are deterministic: two runs of a case are pixel-identical (MAE 0.0000, SSIM 1.0000 against itself).

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
| Wheels | ✔ left/right model variants; `tire_shift` (F9, rule fits 94/96 disc cases) | side from locator x | axle and lift-axle variants | F8 mentions axle locators |
| Tuning pins: existence | ✔ F5, F8 | | | Created by `truck_config_hdl_u` (`FUN_140fe0fc0`) |
| Tuning pins: placement, which slots | | ✔ D7 | ✔ | Wheel parts share one anchor per axle group and empty slots are flagged (`Anchor::group`, `Anchor::empty`); the app decides how to show them |
| Tuning screen layout | | ✔ own UI | | Game layout: `ui/truck_configuration_*.sii` |
| Tuning scenes (`/def/desktop`) | data rendered (F7) | camera choice, light mapping | ✔ | Peer note: these may be hub backgrounds rather than the tuning garage |
| Materials and shading | | ✔ approximation | | Game shaders are readable in `effect.scs` ([formats/effects.md](formats/effects.md)) |
| Paint jobs | base colour | | masks, flakes, liveries | |
| Interior seen through the windows | ✔ `ext_model` at `ext_interior`; UK twin in RHD (F10) | twin pairing by file name | | |
| Interior camera and interior slots | | | ✔ | |
| Addon hookups (accessories on accessories) | ✔ catalogue, `slot_NN` positions, `slot_name`/`slot_hookup` (F11) | dropped when the host changes | per-position type limits | Placement verified on the game's oversize Mercedes |
| `overrides[]`, `hide_in` | | | ✔ | |

# SpecSheet

[![CI](https://github.com/Laakdal/SPecSheet/actions/workflows/ci.yml/badge.svg)](https://github.com/Laakdal/SPecSheet/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**SpecSheet: ETS2 Save Edit**

A standalone truck-tuning studio for **Euro Truck Simulator 2**. It reads the original trucks, accessories, models and textures from your own game installation, assembles a truck the way the game's definitions describe it, and lets you browse and swap parts in a 3D view. You don't need to run the game.

> **Status:** working prototype. All 26 base-game and DLC truck models load from game data. See [Supported features](#supported-features) and [Limitations](#limitations).

## Features

- **Reads the game directly.** Opens `.scs` (HashFS v2) archives read-only and follows the game's mount order and DLC layering. Nothing is extracted or modified.
- **Assembles from the original data.** Chassis, cabin, accessories and wheels are placed using the game's own definitions, model variants and attachment locators.
- **Interactive tuning.** Clickable markers sit on the truck's parts and follow the camera. A sidebar lists each slot's options with prices, unlock levels and compatibility reasons. Selections re-assemble the truck live.
- **Game rules.** Compatibility (`suitable_for`, `conflict_with`), defaults, required slots and part availability are derived from the definitions and validated against the game's own preset trucks.
- **Left- and right-hand drive** variants.

## Requirements

- Windows 10/11 with a DirectX 12 or Vulkan GPU.
- Euro Truck Simulator 2 1.60+ installed (DLCs are picked up automatically).
- For development: [rustup](https://rustup.rs) and a C/C++ toolchain (MSVC Build Tools). The Rust version is pinned in `rust-toolchain.toml`, and rustup installs it on the first build.

## Getting started

```sh
git clone https://github.com/Laakdal/SPecSheet.git && cd SPecSheet
# Optional: point at the game if it is not in a default Steam library
set ETS2_DIR=D:\Steam Library\steamapps\common\Euro Truck Simulator 2
cargo run --release -p specsheet
```

If the game is not found automatically, the app asks for its directory.

## Supported features

| Area | Supported |
|---|---|
| Archives | HashFS v2, zlib, GDeflate textures |
| Formats | SII text, PMD v4, PMG v0x15, MAT, packed TOBJ/DDS (BC1–5, RGBA8, R8) |
| Trucks | All trucks under `/def/vehicle/truck` in the installed archives |
| Parts | Chassis, cabins, exterior addons, wheels (tyre, rim, hub, nuts, cover), paint colour |
| UI | Bevy 3D view in the game's own scenes, hotspots, slot list, options with compatibility reasons, LHD/RHD |

## Limitations

- Paint jobs apply the base colour only, with no masks or liveries. There are no normal maps, and shading only approximates the game's shaders.
- Interior accessories are listed, but there is no interior camera. Addon hookups (accessories mounted on accessories) are not implemented.
- Mods are not loaded.
- The rule matching accessories to model locators reproduces all game presets but is not yet confirmed from the executable. See [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md).

## Documentation

| Document | Contents |
|---|---|
| [ARCHITECTURE](docs/ARCHITECTURE.md) | Crates, layering, data flow |
| [REVERSE_ENGINEERING](docs/REVERSE_ENGINEERING.md) | Findings about ETS2 with evidence |
| [ASSET_FORMATS](docs/ASSET_FORMATS.md) | HashFS, SII, PMD/PMG, MAT, textures |
| [TUNING_SYSTEM](docs/TUNING_SYSTEM.md) | Slots, compatibility rules, presets |
| [RENDERING](docs/RENDERING.md) | Asset pipeline, materials, hotspots |
| [DEVELOPMENT](docs/DEVELOPMENT.md) | Building, testing, research tools |
| [DECISIONS](docs/DECISIONS.md) | Architecture decisions |
| [ROADMAP](docs/ROADMAP.md) | Phases and milestones |
| [CHANGELOG](CHANGELOG.md) | What changed |
| [TASKS](TASKS.md) | Current progress |

## License

[MIT](LICENSE), for the project's original work only. Dependencies keep their own licenses; see [Third-party notices](THIRD_PARTY_NOTICES.md).

Euro Truck Simulator 2 is a trademark of SCS Software, and its game files (models, textures, definitions and shaders) belong to SCS Software. This project is not affiliated with or endorsed by SCS Software. It contains no game assets: everything shown is loaded at runtime from the user's own licensed installation and is never redistributed.

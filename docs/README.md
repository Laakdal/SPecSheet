# Documentation index

Docs are organised by topic. Each fact has one home: `formats/` describes what game files contain, and the app docs describe how the app uses them.

## Project
- [ARCHITECTURE.md](ARCHITECTURE.md): crates, layering, data flow
- [DECISIONS.md](DECISIONS.md): design decisions (D*n*)
- [ROADMAP.md](ROADMAP.md): phases and milestones
- [PARITY.md](PARITY.md): which features match the game, which are our design, and which are unknown
- [DEVELOPMENT.md](DEVELOPMENT.md): building, testing, research tools

## The game (reverse engineering)
- [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md): findings log (F*n*) with evidence
- [TUNING_SYSTEM.md](TUNING_SYSTEM.md): slots, compatibility rules, presets

## Game file formats
- [ASSET_FORMATS.md](ASSET_FORMATS.md): hub and status labels
  - [formats/hashfs.md](formats/hashfs.md): `.scs` archives
  - [formats/textures.md](formats/textures.md): packed textures
  - [formats/sii.md](formats/sii.md): definitions and localisation
  - [formats/models.md](formats/models.md): PMD, PMG, tokens, materials
  - [formats/effects.md](formats/effects.md): `effect.scs` shaders
  - [formats/ui.md](formats/ui.md): `/ui/` layouts, templates, fonts, UI icons
  - [formats/ui-truck-config.md](formats/ui-truck-config.md): the truck tuning screen's widget tree

## The app
- [RENDERING.md](RENDERING.md): pipeline, coordinates, assembly, caching
  - [app/materials.md](app/materials.md): effects to Bevy materials, texture upload, shader port
  - [app/shaders.md](app/shaders.md): why the look isn't 1:1 yet, which game shaders the trucks use, and the plan
  - [app/scenes.md](app/scenes.md): tuning scenes, lights, fog, orbit camera
  - [app/hotspots.md](app/hotspots.md): anchors, culling, clustering, marker styles
  - [app/ui.md](app/ui.md): egui layout, sidebar, busy state, settings
  - [app/design-system.md](app/design-system.md): tokens, rules and screens for the app's own UI (not the 1:1 tuning screen)

Root files: [../README.md](../README.md), [../TASKS.md](../TASKS.md), [../CHANGELOG.md](../CHANGELOG.md), [../AGENTS.md](../AGENTS.md).

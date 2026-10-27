# Decisions

Each entry records the context, the decision and the alternatives considered. Newest last.

### D1: Cargo workspace with four library crates plus the app
- **Context:** the user asked for tuning logic that is testable apart from UI, renderer and archive code.
- **Decision:** `specsheet-archive`, `specsheet-formats`, `specsheet-core` and `specsheet-data` as separate crates, plus `app/src-tauri` (replaced by D11, moved by D12).
- **Alternatives:**
  - A single crate with modules. Rejected: nothing would stop `specsheet-core` from importing archive code.
  - Merging `specsheet-archive` into `specsheet-formats`. Rejected: the archive needs compression dependencies and file I/O, while the parsers are pure byte functions.

### D2: Our own HashFS reader instead of a library
- No maintained Rust crate reads HashFS v2.
- sk-zk's C# TruckLib.HashFs served as a cross-check only.
- The reader is about 400 lines, validated by walking whole archives.

### D3: CityHash64 v1.0.3 implemented in-tree
- The available crates implement v1.1, which gives different hashes (see ASSET_FORMATS).
- The function is about 150 lines and covered by tests against archive-observed hashes.

### D4: `gdeflate` crate (libdeflate C code) for texture decompression
- **Alternative:** port GDeflate to Rust (~800 lines). Rejected for now.
- The crate's safe wrapper does not validate tile sizes against the buffer. We therefore parse and validate the tile stream ourselves before calling it (`specsheet-archive/src/gdeflate.rs`).
- **Revisit** if the C build becomes a portability problem.

### D5: Tauri 2 + React/TypeScript + Three.js (superseded by D11)
- **Context:** the user wanted a Rust backend; the UI is list-heavy (sidebar, icons, filters) with a 3D viewport and DOM hotspots.
- **Decision:** Tauri gives a native Rust backend with a web frontend. Three.js offers mature WebGL2, compressed textures (S3TC/RGTC) and easy world→screen projection for HTML hotspots.
- **Alternatives:**
  - Bevy + egui (all-Rust). Rejected: much more UI work, and egui list UIs are weaker.
  - wgpu with a custom UI. Rejected: too much renderer work for the scope.
  - Electron. Rejected: the backend would not be Rust.

### D6: Models delivered as typed buffers plus a JSON manifest, not glTF (obsolete since D11)
- **Context:** pieces must stay addressable (per-slot visibility), and SCS textures are BC-compressed DDS surfaces that glTF cannot carry without extensions.
- **Decision:** the backend sends per-model JSON (pieces, material refs, transforms) and binary vertex/index buffers. Textures are sent as compressed surfaces with mip chains.
- **Alternative:** GLB with PNG textures. Rejected: costly re-encoding, larger transfers, and the BC compression would be lost.

### D7: Hotspot anchors from placed models, clustered in screen space
- **Context:** the game creates its pins at runtime (F5), and its exact placement code is still unconfirmed. Scania alone has about 80 slots, many of them centimetres apart.
- **Decision:**
  - Anchor each slot at the bounding-box centre of its placed models. Empty slots use their locators.
  - A centre hidden inside the cabin moves to the cabin's −x face.
  - Slots whose models lie inside the cabin are "interior" and hidden by default.
  - Markers within 22 px merge into a numbered cluster.
- **Alternatives:**
  - Raw locator positions. Rejected: centred locators such as `s_panel` and `mirror_s` sit inside the cabin.
  - Show every slot. Rejected: unusable.
- This is a design choice consistent with F4/F5, not a reproduction of the game's pin placement.

### D8: Only base game and DLC archives are mounted
- Mods (`Documents/Euro Truck Simulator 2/mod`) use HashFS v1 and ZIP, and change data arbitrarily.
- Mods are out of scope for the prototype.

### D9: Rule repair is our design
- **Context:** the definitions say what is compatible, not how the game repairs a configuration after a base-part change.
- **Decision:**
  - Drop selections that became incompatible.
  - Refill required slots from `fallback[]`, then from the `defaults[]` of selected parts, then with the first compatible option.
  - Pin the slot the user just changed.
- **Revisit** when REA shows the game's behaviour.

### D10: Tauri icon workaround for apostrophes in paths (obsolete since D11)
- `tauri-winres` escapes `'` as `\'`, which RC.EXE rejects. The project lives under `C:\Mudrock's Headquarters\`.
- When the manifest path contains `'`, `build.rs` copies the icon to the temp directory.

### D11: All-Rust front end with Bevy 0.19 + bevy_egui 0.42
- **Context:** the goal became a 1:1 replica of the game's tuning (see ROADMAP Phases 4 and 5). That means recreating the game's shaders, lights and reflections. The user wanted the whole project in Rust: a TypeScript front end "kinda defeats the purpose of all rust".
- **Decision:** replace the Tauri shell and the React/three.js UI with one Bevy app (`crates/specsheet`). The pieces:
  - The 3D view uses Bevy's renderer on wgpu (DirectX 12/Vulkan). `specsheet-data::render` meshes and BC textures are uploaded directly, with no wire format. That ends D6.
  - The UI (top bar, sidebar, hotspot markers) uses egui.
  - Labels, marker selection and incompatibility reasons moved from the Tauri adapter into `specsheet-data::view`, so any front end shows the same thing.
- **Versions:** Bevy 0.20 needs Rust 1.97.1 and this machine has 1.95. 0.19.1 is the newest release that builds here; upgrading is a separate step.
- **Alternatives:**
  - Keep Tauri + three.js (D5). Rejected: custom shaders would live in a second language and a WebView, which is the hardest part of matching the game.
  - Leptos/Dioxus inside Tauri: Rust UI, web styling. Rejected: the 3D view would still be WebGL in a WebView.
  - A hand-written wgpu renderer. Rejected: Bevy gives cameras, lights, fog, PBR and custom WGSL materials for free.
- **Kept open:** a later C++ front end. The core crates stay free of UI and renderer code, so a C interface (cbindgen) over `specsheet-data` remains possible.
- **Costs accepted:**
  - Styling the UI to match the game takes custom egui drawing.
  - UI changes need a Rust rebuild, where Vite reloaded instantly.
  - App checks use a dev script mode (`SPECSHEET_SCRIPT`) that replays actions and saves screenshots, replacing CDP.

### D12: Project-prefixed crate names, all crates under `crates/`
- **Context:** the crates were named after the game's file formats (`scs-archive`, `scs-formats`) or generically (`tuning-core`, `game-data`), and the app lived alone in `app/specsheet`.
- **Decision:** every crate carries the project prefix and lives in `crates/`, with the folder named like the crate, as large Rust workspaces do (ruff, uv, Bevy):
  - `specsheet`: the Bevy app (binary)
  - `specsheet-archive` (was `scs-archive`), `specsheet-formats` (was `scs-formats`)
  - `specsheet-core` (was `tuning-core`), `specsheet-data` (was `game-data`)
  - The workspace is `members = ["crates/*"]`.
- **Alternatives:**
  - `game-archive`/`game-formats`, matching `game-data`. Rejected in favour of one prefix for every crate.
  - Bare names (`archive`, `formats`). Rejected: `use formats::pmd` doesn't say whose formats.
- **Costs accepted:** one large rename commit; commands in older evidence files were updated to the new `-p` names.

### D13: Clean-room code for everything learned from non-MIT sources
- **Context:** SPecSheet is MIT. Some format code was written while reading sk-zk's TruckLib (GPL-2.0), and the materials work would follow the game's own shaders (SCS's code, no license). Facts (offsets, layouts, the maths) are free to use; copied or translated code is not.
- **Decision (the user's choice):** rewrite or write such code clean-room. Interface stubs and our own evidence go to an agent that has never seen the source code, and the result must match the old behaviour on the whole install.
  - Phase 1: `pmd.rs`, `pmg.rs`, `texture.rs`. Done: output identical to the old code for all 278,633 models and textures in the install (`research/evidence/cleanroom-phase1.txt`).
  - Phase 2: `hashfs.rs`, after byte-level evidence for its layout (`hashfs.md` facts were only functionally confirmed).
  - Shaders: a spec agent writes `docs/app/shader-specs/<effect>.md` (maths in prose and equations); a different agent writes the WGSL from it. Visual parity against reference screenshots.
- **Provenance of the rest:** `city.rs` ports Google's CityHash v1.0.3 (MIT, notice in `THIRD_PARTY_NOTICES.md`); `gdeflate.rs` follows GisDeflate's stream header (MIT, notice included) and decodes with libdeflate; `read_at.rs` is unreviewed; everything else is our own work.
- **Alternatives:**
  - Keep the verified code with an evidence trail. Rejected by the user in favour of zero doubt.
  - Load the user's own compiled shaders at runtime. Rejected: legal, but brittle against SCS's engine bindings.
  - Generic PBR without reading the shaders. Rejected: not 1:1.

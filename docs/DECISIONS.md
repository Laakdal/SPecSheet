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

### D7: Hotspot anchors from placed models, clustered in screen space (exterior superseded)
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
- **Superseded for exterior pins (2026-10-28).** F5 now decodes the game's pin builder and per-frame update, so exterior pins follow the game ([app/hotspots.md](app/hotspots.md)):
  - base locator + `pin_offset`, with mirrored twins for `double_sided` entries;
  - pins per wheel part and per wheel, with hub, cover and nuts shown through their disc pin;
  - a fade by facing instead of the far-side cut;
  - no clustering, since the game has none.
  - **Interior pins** follow the game's interior pass too (cockpit locator + `pin_offset`; the added transform is a Hypothesis), and the bounding-box anchors are gone.
  - **Still ours:** the marker look (dots, the hollow ring for empty slots).

### D8: Only base game and DLC archives are mounted (partly superseded by D15)
- Mods (`Documents/Euro Truck Simulator 2/mod`) use HashFS v1 and ZIP, and change data arbitrarily.
- Mods are out of scope for the prototype. Since D15, mod containers and profiles are read, but still nothing is mounted.

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
  - Phase 2: `hashfs.rs`. Done: its layout was first derived from the game's bytes (`hashfs-layout-derivation.txt`), and the reader's index, listings and sampled contents are identical to the old reader on all 116 archives (`research/evidence/cleanroom-phase2.txt`).
  - Shaders: a spec agent writes `docs/app/shader-specs/<effect>.md` (maths in prose and equations); a different agent writes the WGSL from it. Visual parity against reference screenshots.
- **Provenance of the rest:** `city.rs` ports Google's CityHash v1.0.3 (MIT, notice in `THIRD_PARTY_NOTICES.md`); `gdeflate.rs` follows GisDeflate's stream header (MIT, notice included) and decodes with libdeflate; `read_at.rs` is unreviewed; everything else is our own work.
- **Alternatives:**
  - Keep the verified code with an evidence trail. Rejected by the user in favour of zero doubt.
  - Load the user's own compiled shaders at runtime. Rejected: legal, but brittle against SCS's engine bindings.
  - Generic PBR without reading the shaders. Rejected: not 1:1.

### D14: `image` for the parity runner's PNG I/O
- **Context:** `specsheet --parity` must read the user's reference PNGs and the app's screenshots, and write diff heatmaps.
- **Decision:** depend on `image` 0.25 (PNG only) in the `specsheet` crate. Bevy already builds it, so it adds no compile time.
- **Alternatives:**
  - Bevy's own image types. Rejected: they don't expose PNG decoding outside the asset system.
  - A separate tool crate. Rejected: the runner re-runs the app binary itself, and keeping it in the binary avoids a second build target.

### D15: Read mod containers and profiles; mount nothing yet
- **Context:** the user asked for mod packages and their profile's mod list to be readable (F14, `docs/formats/mods.md`, `docs/formats/profiles.md`).
- **Decision:**
  - `specsheet-archive` gets its own tolerant ZIP reader (`zip.rs`). Like D2, we own the reader: a stock reader (Python's `zipfile`) refuses a package the game loads (encryption bit set, wrong CRCs), and we need the game's tolerance, measured on 33 packages. The `zip` crate was not tried.
  - `specsheet-formats` decrypts ScsC profiles (`scsc.rs`) with the RustCrypto `aes` 0.8 and `cbc` 0.1 crates (MIT or Apache-2.0, pure Rust, no `unsafe` in our code). The AES key is a format constant from sii-decode-rs (MIT), taken as a fact and confirmed on 4 profiles.
  - `specsheet-formats` reads binary SII saves (`bsii.rs`) into the same `SiiDocument` as text. The layout is cross-checked against sii-decode-rs (MIT) and confirmed on our own bytes (`research/evidence/bsii-derivation.txt`).
  - Profiles, saves and the mod folder are only read. Nothing in the project writes to them. D18 adds writing new mod packages; profiles and saves stay read-only.
  - `specsheet-archive` also reads HashFS v1 (`hashfs_v1.rs`), a module separate from the v2 reader. It was written from our byte observations only (`research/evidence/hashfs-v1-derivation.txt`), with no community code and no part of the v2 reader (D13).
  - Mounting mods into the `Vfs` and their priority come later, after the priority question in F14 is answered.
  - Names in v1 packages without listings are recovered by hashing candidate paths (`specsheet-data::mod_names`, with the byte scanner `specsheet-formats::paths`). The scanner only reads bytes; the search that joins archive and formats lives in `specsheet-data`. A candidate counts only when its hash is in the package.
- **Alternatives:**
  - The `zip` crate. Not chosen: tolerance for scrambled headers would have to be patched in, and the reader is about 300 lines.
  - Depending on sii-decode-rs. Rejected: it is not on crates.io, and it pulls in a logger and a parser framework.
  - Implementing AES ourselves. Rejected: a reviewed crate is safer than our own cipher code.

### D16: One design system for the screens the game doesn't have
- **Context:** besides the 1:1 tuning screen, the app needs its own screens: home, truck picker, settings, configurations, mods and profiles, parity, about. The user chose Horizon's method (horizon-rust/horizon, MIT) for them.
- **Decision:** a dependency-free `specsheet-theme` crate holds the tokens: colour roles in light and dark, a type scale, spacing, radius, and bundled OFL fonts. Contrast tests enforce WCAG AA. The app maps the tokens onto egui once. The six rules and the screens are in [app/design-system.md](app/design-system.md). The tuning screen stays the game's own UI.
- **Alternatives:**
  - A separate GPUI launcher like Horizon's. Rejected: a second app and UI framework for screens egui can draw.
  - Copying Horizon's look. Rejected: only the method is taken; SPecSheet has its own accent, wordmark and font choice.

### D17: Crates by responsibility, with checked layers and shared foundations
- **Context:** `specsheet-data` grew to about 8,500 lines with four jobs (loading, assembly, the game-UI layout engine, app-facing state), and every session edits it. Shader code is about to grow inside the app crate. Basic types are duplicated (`Aabb` in formats, `Transform` in data, colour conversion in core and the app). Research tools are spread over three crates' examples.
- **Decision:** the target structure in [ARCHITECTURE.md](ARCHITECTURE.md#target-structure-d17):
  - a foundation `specsheet-core` (ids, maths, colour, units);
  - the tuning domain renamed `specsheet-truck`;
  - `specsheet-data` split into `-data`, `-assembly`, `-session` and `-gameui`;
  - `specsheet-render` out of the app, and `specsheet-research` for the tools;
  - an `xtask` crate with a layer check.
  It is done in staged passes, each a pure move that builds and passes the checks on its own:
  - **2026-10-28 (done):** `xtask` (`check`, `layers`, also in CI); `specsheet-core` → `specsheet-truck`; `specsheet-data` → `-data`, `-assembly`, `-gameui`, `-session`; the examples → `specsheet-research`. No compatibility re-exports: every user was moved.
  - **2026-10-28, part 2 (done):** the foundation `specsheet-core` (ids, `Aabb`, `Transform`, one sRGB conversion instead of four); `specsheet-theme`; `specsheet-render` with the materials and GPU upload, after `dif.spec` landed; `xtask` `guard`, `real`, `parity`, `version-port`.
  - **Still open:** module splits of `catalog.rs` and `config.rs`.
- **Engineering practices adopted with it:**
  - `cargo xtask check` (the "Done means" list) and `cargo xtask layers`, which CI also runs;
  - `cargo-deny` for licences and advisories;
  - `#![warn(missing_docs)]` on library crates;
  - `#[non_exhaustive]` on enums that grow with game versions;
  - fuzzing the parsers that read untrusted mod and save files;
  - versioned settings and configuration files;
  - benchmarks for parsing and assembly.
- **Alternatives:**
  - Generic tuning vs truck-specific crates. Rejected: almost all of the tuning logic is generic, and trailers are not planned.
  - Splitting by file count. Rejected (D12).

### D18: Write new mod packages as standard ZIP; never replace anything
- **Context:** the user asked for the app to write mod packages. D15 kept the mod folder read-only; the game install stays read-only under the absolute rule.
- **Decision:**
  - `specsheet-archive` builds packages in memory (`zip_write.rs`, `ZipWriter`) as a standard ZIP: correct CRCs, no encryption bit, sizes in both headers, a fixed timestamp. Files are deflated, or stored when that is not smaller. No ZIP64 and no directory records, which stays within what the game is seen to load (`research/evidence/mod-writer-checks.txt`).
  - `write_new_package` creates the package file once at its final name and refuses when any entry of the folder has the same file stem, ignoring case, since the stem is the package name. It never renames over or replaces a file.
  - `specsheet-formats` writes `manifest.sii` (`mod_manifest.rs`) with only attributes seen in real manifests, escaping with `\xNN` as the game does.
  - Profiles and saves stay read-only. A package takes effect only after the player enables it in the in-game mod manager.
  - A package never contains game files from this repo. What goes into one beyond a test package is a later, separate choice by the user.
- **Alternatives:**
  - The `zip` crate. Not chosen: the writer is about 250 lines with `flate2`, and a fixed, minimal feature set keeps the output inside what the game is seen to accept.
  - HashFS v1 output. Not chosen: the game loads ZIP, and ZIP opens in any tool, so players can check a package.
  - Writing to a temporary file and renaming. Rejected: on Windows a rename replaces an existing file, and the game would load a stray temporary file as a mod, because it picks the reader by content.

### D19: Extract archives to a folder the user chooses; never into the game, its home or the repo
- **Context:** the user asked for the features of the community tool Extractor (sk-zk, GPL-2.0): listing and extracting mod packages and game archives, including HashFS v1 packages without listings. D15 kept the mod folder read-only; D18 only covers creating new packages. The user wants game archives accepted as input too, since players commonly extract them.
- **Decision:**
  - Extraction reads an archive (HashFS v2 or v1, ZIP) and writes its files to a destination folder the user names. The archive, the game install and the ETS2 home folder (`mod/`, profiles) are only read.
  - The destination may not be inside the game install, the ETS2 home folder or this repository's working tree. Both sides are canonicalized before comparing (Windows paths are case-insensitive and may carry `\?\` or pass through junctions).
  - Every file is created with `create_new`, so nothing is ever overwritten. A non-empty destination is refused unless the user asks to skip files that exist.
  - Names from an archive are hostile input. A name that Windows cannot hold (reserved device names such as `CON` or `COM1` with any extension, a trailing dot or space, `<>:"|?*`, control characters, a part over 255 bytes) or that collides with another ignoring case is written under a safe name, and every such rename is listed in a mapping file at the destination root. References inside extracted files are not rewritten.
  - Entries without a recovered name (`specsheet-data::mod_names`) go to `_unnamed/<hash>.<extension from content>`; entries that cannot be read are listed in `_unreadable.txt`, so they are never mistaken for real files.
  - One entry is read and written at a time. A dry run prints the file count and total size first.
  - The code is our own. From Extractor we take facts only (which paths exist, which naming rules hold), each confirmed on our own install before use (`research/evidence/`), as for TruckLib.
  - Extracted files never enter this repository; the pre-commit guard already rejects game files.
- **Alternatives:**
  - Listing only. Not chosen: the user wants files on disk to inspect and edit.
  - Rewriting references to renamed files, as Extractor does. Deferred: it edits file contents and needs its own decision.
  - Refusing game archives as input. Not chosen by the user; reading them is what the app already does, and the install itself stays read-only.

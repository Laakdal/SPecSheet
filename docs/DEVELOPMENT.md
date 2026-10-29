# Development

## Requirements

| Requirement | Why |
|---|---|
| [rustup](https://rustup.rs) and a C compiler (MSVC Build Tools on Windows) | `rust-toolchain.toml` pins Rust 1.95.0 (Bevy 0.19 needs 1.95), and rustup installs it on the first build. The `gdeflate` crate builds libdeflate. |
| A GPU with DirectX 12 or Vulkan and BC texture compression (every desktop GPU) | Bevy renderer; textures upload BC-compressed |
| An ETS2 installation | Integration tests and running the app |

## Build and test

```sh
# Library change (no Bevy build): the changed crate plus the crates above it
cargo test -p specsheet-data -p specsheet-assembly -p specsheet-session
cargo clippy -p specsheet-data -p specsheet-assembly -p specsheet-session --all-targets -- -D warnings

# App or public-API change, or before a multi-crate commit (builds Bevy)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cargo fmt --all --check

# All of the above plus the crate layer check; --real adds the real-install tests
cargo xtask check

# Integration tests against the real game (they return early if ETS2_DIR is unset):
ETS2_DIR="D:/Steam Library/steamapps/common/Euro Truck Simulator 2" \
  cargo test -p specsheet-session --release --test real_install
```

**Game version pin.** The research and tests were checked against ETS2 `TESTED_GAME_VERSION` (`specsheet-data/src/install.rs`, currently 1.60.1.7). `GameInstall::version()` reads it from `version.scs` (`fs_pack_set.version`). With `ETS2_DIR` set, `cargo test -p specsheet-data --test game_version` fails on any other version. After a game update:
1. rerun the real-install tests;
2. recheck `docs/PARITY.md` and `docs/REVERSE_ENGINEERING.md`;
3. raise the constant.

**Which crates to check.** The library crates never depend on Bevy. Testing only the crates a change touches, plus their dependents, avoids compiling the roughly 400 Bevy crates. The full table is in `AGENTS.md` under "Done means":

```
specsheet-core ──> specsheet-formats, specsheet-truck ──> specsheet-data
specsheet-data ──> specsheet-assembly, specsheet-gameui ──> specsheet-session
specsheet-session ──> specsheet-render (Bevy) ──> specsheet (the app, Bevy)
specsheet-archive and specsheet-theme sit at the bottom with specsheet-core.
```

`specsheet-research` holds the research tools and sits beside the app. Only `specsheet-render` and the app compile Bevy. `cargo xtask layers` checks that every dependency points down (D17).

## Run the app

```sh
ETS2_DIR="<game dir>" cargo run --release -p specsheet
```

- The app is `crates/specsheet` (Bevy 0.19 + bevy_egui 0.42).
- Without `ETS2_DIR` it tries the remembered directory, then Steam discovery.
- Release builds have no console window. Debug builds keep one for logs.
- The first build compiles Bevy and takes several minutes. When other sessions use the repo, build with your own `CARGO_TARGET_DIR` so the shared `target/` isn't locked.

### Dev script mode (seeing changes work)

`SPECSHEET_SCRIPT=<file>` replays UI actions, then saves screenshots. Agents use it to check visual changes without a person at the mouse. Each line is one command, and `#` starts a comment:

| Command | Effect |
|---|---|
| `wait` | until no job runs, no model is loading, and 20 idle frames passed |
| `truck <truck id>` | load a truck (e.g. `scania.s_2016`) |
| `slot <slot id>` / `back` | open a slot's options / return to the parts list |
| `select <accessory id>` | select an accessory |
| `pick <n>` | select the n-th selectable option (compatible, not selected) of the open slot |
| `drive left\|right` | LHD/RHD |
| `scene <index>` | switch scene (0 = default garage) |
| `tab <token>` | open a tab of the game screen: `cabin`, `chassis`, `engine`, `transmission`, `color`, `e_accessories`, `interior`, `i_accessories` |
| `orbit <yaw°> <pitch°> <distance>` | place the camera around the current target |
| `exposure <E>` | set the tonemap's fixed exposure scale (default 5.555, so k = 1) |
| `sleep <frames>` / `echo <text>` | pause; log `script: <text>` so an external driver can sync |
| `screenshot <png path>` | save the window (then waits 30 frames) |
| `exit` | quit |

```text
wait
truck scania.s_2016
wait
slot r_grill
pick 0
wait
screenshot C:/tmp/lightbar.png
exit
```

Keep screenshots outside the repo (e.g. in a scratch directory); the pre-commit guard rejects large files.

To test real mouse input, use user32 `mouse_event` on the window. Drags need relative `MOUSEEVENTF_MOVE` events, because the orbit camera reads raw mouse motion and `SetCursorPos` produces none. Put `echo ready` and `sleep 900` in the script to get a window for the clicks.

### Visual parity checks

```sh
cargo build --release -p specsheet
ETS2_DIR="<game dir>" target/release/specsheet --parity [parity/cases.json] [case…]
```

- For each case in `parity/cases.json` (committed, no images), the app runs itself with a generated dev script. The script loads the truck, runs the case's `steps`, saves `research/parity/out/<case>.png` and exits.
- If `research/parity/<case>.png` exists, the render is compared with it outside the case's `mask` rectangles. The run writes `out/<case>-diff.png` (a red heatmap; masked areas are dark blue) and prints MAE and SSIM with pass/fail against `threshold`.
- The table also goes to `research/parity/out/report.txt`, because release builds have no console. Everything under `research/parity/` is gitignored, since it is rendered game content.
- The exit status is non-zero when any compared case fails or a render fails.

**Capturing a reference** (the user, once per case):
1. In ETS2, set windowed mode at 1440 × 900 with 100 % scaling. Turn off motion blur, depth of field and vignette, and leave brightness at its default.
2. Turn on Steam's "Save an uncompressed copy" for screenshots, then press F12. Rename the PNG to `research/parity/<case>.png`.
3. Open the case's screen and take the shot before touching the mouse or the camera. For the preset cases, that is the truck straight after the configurator opens. For `scania-s-lightbar`, open the light-bar slot and pick the first light bar in the list.
4. Note the screen (dealer or upgrade shop), the garage or background shown, and LHD/RHD. A case's `steps` can add `scene <n>` or `drive right` to match.

## Research tools

These are examples, not shipped binaries. All of them read `ETS2_DIR`.

```sh
export MSYS_NO_PATHCONV=1   # Git Bash only: keep /game/paths intact

# Archive survey: entry kinds per extension, texture formats, reachability
cargo run --release -p specsheet-research --example survey -- "<game>/def.scs" ...

# Browse the layered file system
cargo run --release -p specsheet-research --example scs -- ls def/vehicle/truck
cargo run --release -p specsheet-research --example scs -- cat def/vehicle/truck/scania.s_2016/data.sii
cargo run --release -p specsheet-research --example scs -- find vehicle/truck/scania_2016 .pmd

# Model structure: looks, variants, parts, locators
cargo run --release -p specsheet-research --example model_info -- /vehicle/truck/scania_2016/cabin/cab_s_low.pmd

# Byte-level layout checks (independent of our parsers): dump with `scs cat <path> > file`, then
python -I tools/research/model_layout.py <dumps>/*.pmd <dumps>/*.pmg
cargo run --release -p specsheet-research --example texture_layout -- "<game>/base.scs" ...

# Every R9G9B9E5 (HDR) texture: size, mips, decoded mean and peak colour
cargo run --release -p specsheet-research --example hdr_textures -- "<game>"/*.scs

# Definition statistics / samples (optional last argument: root directory, default /def/vehicle/truck)
cargo run --release -p specsheet-research --example def_stats
cargo run --release -p specsheet-research --example def_stats -- /def/vehicle/f_disc
cargo run --release -p specsheet-research --example def_grep -- accessory_addon_data conflict_with 3
cargo run --release -p specsheet-research --example def_grep -- accessory_rim_data tire_shift 5 /def/vehicle/f_disc

# Wheel parts: x extents and tire_shift per part, rim lip vs tyre bead, yawed wheel locators (F9)
cargo run --release -p specsheet-research --example wheel_fit
cargo run --release -p specsheet-research --example wheel_fit -- fit
cargo run --release -p specsheet-research --example wheel_fit -- locators
cargo run --release -p specsheet-research --example wheel_fit -- profile /vehicle/truck/upgrade/wheel/tire/front_1.pmd left

# Axles: wheel-locator pairs per axle vs. the chassis' powered/steerable/liftable arrays (F13)
cargo run --release -p specsheet-research --example axle_survey

# Mod packages (no ETS2_DIR): container by content, HashFS v1 header/entries/keys, manifest fields (F14)
cargo run --release -p specsheet-research --example mod_probe -- "<home>/mod"

# Profile: decrypt (ScsC) or read plain text, list active_mods in order
cargo run --release -p specsheet-research --example profile_mods -- "<home>/profiles/<hex name>/profile.sii"

# Saves: decode every save (ScsC, text, binary) and compare binary against text field kinds
cargo run --release -p specsheet-research --example save_check -- "<home>/profiles"

# Mod names: recover file names in HashFS v1 packages from the game's paths and the packages' own contents; --others adds what the other packages of a folder hold or mention (cached per package in %LOCALAPPDATA%\dev.specsheet.app\mod-scan; --no-cache reads them all again); --paths adds one candidate per line of a file; --list adds folders, replaced game files, unnamed contents
cargo run --release -p specsheet-research --example mod_names -- [--list] [--others "<home>/mod"] [--no-cache] [--paths <file>] "<home>/mod/<package>.scs" ...

# Mod extraction (D19): write a v1 or ZIP package's files to an empty folder outside the game, the ETS2 home, the mod folder and the repo; never overwrites
cargo run --release -p specsheet-research --example mod_extract -- [--dry-run] [--skip-existing] [--max-gb <n>] [--others "<home>/mod"] [--no-cache] "<home>/mod/<package>.scs" <absolute destination>

# Mod writer: a test package (manifest + description, no game files) as <dir>/specsheet_test.scs; refuses if it exists
cargo run --release -p specsheet-research --example mod_write -- <dir>
```

## Debugging tips

- **Slow parsing in debug builds:** the workspace builds dependencies with `opt-level = 2` in dev; for heavy scans use `--release`.
- **Unknown archive record types:** entries that cannot be indexed are listed by `Archive::skipped_entries()` rather than failing the archive.
- **Game log:** the game's own `game.log.txt` lives in its home directory (here `D:\SCS\Euro Truck Simulator 2`). It is useful runtime evidence for mount order and errors.

## Public-tree guard

The repository must never contain game files. A pre-commit hook enforces this:
- **Hook:** `.githooks/pre-commit` runs `tools/check_public_tree.py`. Enable it once per clone with `git config core.hooksPath .githooks`.
- **What it refuses:**
  - `*/extracted/` paths;
  - game extensions (`.scs .pmd .pmg .pma .pmc .ppd .dds .tobj .mat .sii .sui .fso .vso .rfx`);
  - files starting with game magics (`SCS#`, `SiiNunit`, `ScsC`, `\x15gmP`, `DDS `, …);
  - extracted GLSL;
  - anything over 5 MiB.
- **Whole tree:** run `python tools/check_public_tree.py --all`.
- **Consequence for tests:** build fixtures in code (as the parser tests do), never as checked-in `.sii`/`.pmd` files.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and every pull request, on Windows:
1. The public-tree guard (`--all`).
2. `cargo fmt --all --check`.
3. `cargo clippy --workspace --all-targets -- -D warnings`.
4. `cargo test --workspace`.
5. `cargo xtask layers` (crate layers, D17).

Notes:
- **Real-install tests skip in CI,** because CI has no game files. Run them locally with `ETS2_DIR` set before pushing parser, catalog or assembly changes.
- **The first run builds Bevy cold** (about 15–25 minutes). Later runs reuse the build cache.
- **A newer push to the same branch cancels the run in progress,** so a cancelled run is not a failure.
- **Upgrading Rust:** change `rust-toolchain.toml`. CI and every machine follow it.

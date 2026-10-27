# Development

## Requirements

| Requirement | Why |
|---|---|
| Rust 1.95+ (Bevy 0.19 needs 1.95) and a C compiler (MSVC Build Tools on Windows) | The `gdeflate` crate builds libdeflate |
| A GPU with DirectX 12 or Vulkan and BC texture compression (every desktop GPU) | Bevy renderer; textures upload BC-compressed |
| An ETS2 installation | Integration tests and running the app |

## Build and test

```sh
cargo test --workspace                       # unit tests, no game needed
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Integration tests against the real game (they return early if ETS2_DIR is unset):
ETS2_DIR="D:/Steam Library/steamapps/common/Euro Truck Simulator 2" \
  cargo test -p specsheet-data --release --test real_install
```

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
| `interior on\|off` | interior markers |
| `orbit <yaw°> <pitch°> <distance>` | place the camera around the current target |
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

## Research tools

These are examples, not shipped binaries. All of them read `ETS2_DIR`.

```sh
export MSYS_NO_PATHCONV=1   # Git Bash only: keep /game/paths intact

# Archive survey: entry kinds per extension, texture formats, reachability
cargo run --release -p specsheet-archive --example survey -- "<game>/def.scs" ...

# Browse the layered file system
cargo run --release -p specsheet-archive --example scs -- ls def/vehicle/truck
cargo run --release -p specsheet-archive --example scs -- cat def/vehicle/truck/scania.s_2016/data.sii
cargo run --release -p specsheet-archive --example scs -- find vehicle/truck/scania_2016 .pmd

# Model structure: looks, variants, parts, locators
cargo run --release -p specsheet-data --example model_info -- /vehicle/truck/scania_2016/cabin/cab_s_low.pmd

# Definition statistics / samples (optional last argument: root directory, default /def/vehicle/truck)
cargo run --release -p specsheet-data --example def_stats
cargo run --release -p specsheet-data --example def_stats -- /def/vehicle/f_disc
cargo run --release -p specsheet-data --example def_grep -- accessory_addon_data conflict_with 3
cargo run --release -p specsheet-data --example def_grep -- accessory_rim_data tire_shift 5 /def/vehicle/f_disc

# Wheel parts: x extents and tire_shift per part, rim lip vs tyre bead, yawed wheel locators (F9)
cargo run --release -p specsheet-data --example wheel_fit
cargo run --release -p specsheet-data --example wheel_fit -- fit
cargo run --release -p specsheet-data --example wheel_fit -- locators
cargo run --release -p specsheet-data --example wheel_fit -- profile /vehicle/truck/upgrade/wheel/tire/front_1.pmd left
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

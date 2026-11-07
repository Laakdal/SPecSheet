# Version ports: when ETS2 updates

SPecSheet is checked against one game version, `TESTED_GAME_VERSION` in `specsheet-data/src/install.rs` (now 1.60.1.7). ETS2 updates several times a year, and an update can change file formats, definitions, models and the executable. This is what to do when that happens.

## 1. Notice

The real-install test `installed_game_is_the_tested_version` fails, naming the installed version. Nothing else changes until someone works through this page; the app may still run, but nothing is verified.

## 2. Check that the files still read

Run with `ETS2_DIR` set:
- `cargo test --release -p specsheet-session --test real_install`. Every test says which file or rule broke. The archive test fails on any skipped entry, which means a new metadata record type.
- `survey` over `def.scs`, `effect.scs` and one DLC archive: 100 % reachable, 0 skipped.

A failure here is a format change. Derive it from the bytes as in [research-lab.md](research-lab.md) (evidence first), then fix the parser.

## 3. Re-check what the code relies on

- **Recipes:** `research/recipes.txt` says how each evidence file is made again: a `game` command (output compared), a `digest` command (output is game file text, so only its hash is kept), a `user` command (reads the mod folder or profiles), an executable search batch (`rea`, below), a check by hand, or `historical`. Run `cargo xtask recipes --compare` with `ETS2_DIR` set. It runs every `game` and `digest` recipe, including the surveys and the extraction of `def.scs`, `dlc_daf_xd.scs` and `core.scs` with `extract_check`. It compares each output with `research/baseline/`, keeps the new outputs in `<target>/recipes/`, and ends with the list of checks to do by hand. Each difference is a work item: new attributes, classes, slots, effect names, or a changed effect file (D13: a new clean-room pass). Add `--user` with `ETS2_HOME` set to run the `user` recipes too.
- **Commands:** a bare tool name in an evidence file, such as `scs cat <path>`, means `cargo run --release -p specsheet-research --example scs -- cat <path>`, with `ETS2_DIR` set (and `MSYS_NO_PATHCONV=1` in Git Bash).
- **Mount order:** the test `real_install::archives_mount_in_the_games_lookup_order` covers new DLC priorities.
- **Findings:** walk through REVERSE_ENGINEERING.md. Data findings are re-confirmed by the surveys and tests above.
- **Executable findings:** anything citing an address (`FUN_14…`, `0x14…`) is version-specific. `research/rea/*.txt` keeps each REA batch: its `search` and `trace` seeds find the code again, and its `function` and `xrefs` lines are the old addresses. `cargo xtask rea` prints one batched run of every seed (one Ghidra analysis of the new executable), then the addresses to re-locate. Update the addresses and Evidence IDs, and the batch files with the new addresses. The `game` recipes that run `tools/research/xref.py` and `attr_tables.py` re-find string references and field offsets without Ghidra.
- **Parity:** rerun `specsheet --parity` if references exist, and update PARITY.md.

## 4. Record

- New evidence files, or updated ones with the version in their header. Every evidence file needs a line in `research/recipes.txt`; `cargo xtask check` fails without one.
- Once the differences are understood, `cargo xtask recipes --save` writes the new baselines; commit them with the evidence.
- Raise `TESTED_GAME_VERSION`, record the executable's SHA-256 in REVERSE_ENGINEERING.md, and update CLAUDE.md's version note.
- CHANGELOG.md: "Supports ETS2 <version>", plus every behaviour change.
- One commit per logical group, as usual.

## What never changes

- **The install stays read-only.** No game file enters the repository.
- **The rules of D13 still apply:** facts from the new build, never its code.

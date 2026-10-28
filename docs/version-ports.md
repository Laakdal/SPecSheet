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

- **Surveys:** rerun `def_stats`, `axle_survey`, `effect_usage`, `locator_check`, `wheel_fit` and the paint survey. Compare them with the files in `research/evidence/`. New attributes, classes, slots or effect names are work items.
- **Findings:** walk through REVERSE_ENGINEERING.md. Data findings are re-confirmed by the surveys and tests above.
- **Executable findings:** anything citing an address (`FUN_14…`, `0x14…`) is version-specific. Re-locate it by its seed strings with one batched REA run, then update the addresses and Evidence IDs.
- **Parity:** rerun `specsheet --parity` if references exist, and update PARITY.md.

## 4. Record

- New evidence files, or updated ones with the version in their header.
- Raise `TESTED_GAME_VERSION`, record the executable's SHA-256 in REVERSE_ENGINEERING.md, and update CLAUDE.md's version note.
- CHANGELOG.md: "Supports ETS2 <version>", plus every behaviour change.
- One commit per logical group, as usual.

## What never changes

- **The install stays read-only.** No game file enters the repository.
- **The rules of D13 still apply:** facts from the new build, never its code.

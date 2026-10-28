# Research lab

How we find out how ETS2 works, and how a finding becomes something the code may rely on. The tools are in [DEVELOPMENT.md](DEVELOPMENT.md#research-tools); the findings are in [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md).

## Sources, cheapest first

1. **Game data, through our own tools.** The tools in `crates/specsheet-research/examples/` read the install read-only (`cargo run --release -p specsheet-research --example <tool>`): `scs` (ls/cat/find/tex), `survey`, `model_info`, `def_stats`, `def_grep`, and the surveys `axle_survey`, `effect_usage`, `locator_check`, `wheel_fit` and others. Most questions end here.
2. **The game's log**, `game.log.txt` in the game's home folder, for runtime behaviour such as mount order and errors.
3. **The executable, through REA.** It answers engine conventions the data doesn't state: placement rules, camera maths, which effect variant is chosen. REA is expensive; see below.
4. **SCS's public modding documentation** ([modding.scssoft.com](https://modding.scssoft.com/wiki/Main_Page)) describes behaviour, e.g. the [supported shaders](https://modding.scssoft.com/wiki/Documentation/Tools/SCS_Blender_Tools/Supported_shaders). Cite it and paraphrase it; don't copy it.
5. **Community code is a cross-check only, never a source.** TruckLib and Extractor are GPL-2.0, and this project is MIT. Take facts from them (offsets, magic numbers), never code (D13).

The `ets2-researcher` subagent (`.claude/agents/ets2-researcher.md`) follows this order and returns labelled findings.

## Rules

- **Facts, not code (D13).** Code we didn't write never enters the repository: not community code, not decompiled code, not the game's shader code. Shader maths goes through a separate spec step (`docs/app/shaders.md`).
- **Labels.** Every claim is **Confirmed** (verified on game files or game output), **Hypothesis** (consistent but unproven; say how many cases it covers) or **Unknown** (with the next step that would settle it). A rule that reproduces every case without proof of the engine's logic is a Hypothesis.
- **Evidence before use.** A new fact goes into `research/evidence/<topic>.txt` first: the exact command at the top, then the results. Then it gets an F-entry in REVERSE_ENGINEERING.md or a section of `docs/formats/`. Only then may code rely on it.
- **Excerpts only.** Evidence holds commands, counts, offsets and a few bytes per field: never whole game files, extracted shader text, or anything from the user's profiles or saves (names, IDs, paths, timestamps). The pre-commit guard refuses game files.
- **Version.** Findings hold for `TESTED_GAME_VERSION` (`specsheet-data::install`). Executable addresses (`FUN_14…`, `0x14…`) are specific to that build; see [version-ports.md](version-ports.md).

## Static tracing before REA

Try this first: it takes seconds and almost no memory, while a Ghidra run takes 30–45 minutes and about 2 GB. It answered F14's mod priority and F18's material uniforms without Ghidra. The scripts in `tools/research/` read the executable read-only and need `capstone`:
1. `xref.py <exe> <string>…`: the functions that reference a string (RIP-relative loads).
2. `disasm.py <exe> <function>`: one function, with its chained unwind ranges, annotated with the strings and floats it loads.
3. `leaf.py <exe> <address>`: a leaf function, which has no `.pdata` entry, up to its `ret`.
4. `triage.py` (calls, strings, floats, chosen offsets) and `attr_tables.py` (a class's attribute names and field offsets) narrow things down further.

Listings stay in the scratchpad (D13); evidence files record observations in our own words. Matching on field offsets alone misleads, so anchor on strings, vtables or registration tables. Use Ghidra when the trace runs into an indirect call that can't be resolved this way.

## REA on `eurotrucks2.exe`

- **Setup and command:** the "REA on `eurotrucks2.exe`" notes in `CLAUDE.md` (CLI, environment variables, output folder). The MCP tools time out on this 51 MB binary, so use the CLI.
- **Cost:** one Ghidra run takes 30–45 minutes and about 2 GB of memory. Other sessions' cold Bevy builds compete for memory, and Claude Code stops jobs when memory runs low. Announce a run to the other sessions and ask them to hold heavy builds until it ends.
- **Batching:** one combined regex string search first (every open question's seed strings at once), then decompile only the functions it points to.
- **Reuse (checked 2026-10-28 against the `rea-agents@6.4.0` manual, `--llms-full`):**
  - **CLI:** every call imports and analyses the binary again. `--snapshot` only stores REA's evidence records (about 2.4 MB), not the Ghidra database: a search with an existing snapshot still took 44 min. `trace`, `function`, `decompile` and `xrefs` each take one query, so each string → function → decompile step is a full run.
  - **MCP:** an `open_binary` session keeps the analysed program loaded until `close_binary`, so after one analysis, `xrefs` and `analyze_function` calls are quick. `binary_session` shows `eurotrucks2.exe` open with Ghidra bound and no analysis yet. The obstacle is the first call that needs analysis: it exceeds the 330 s client limit. **Untested:** whether the server keeps analysing after that call times out. If it does, the route is to trigger analysis once, poll `binary_session` (`analysis_activity`) until it completes, then run the queries. Test this before planning many CLI runs.
  - **Crash survival:** when Claude Code reaps a background REA shell for low memory, the `node`/`java` processes survive and still write their output file. Check for the Ghidra `java` process before restarting anything.
- **Recording:** copy the cited results into `research/evidence/rea-*.txt` (or `.json`) with Evidence IDs, then write the F-entry. Decompiled code itself is never copied; describe what it does.
- **Open targets:** the "Open questions" table in REVERSE_ENGINEERING.md and the REA items in TASKS.md.

## From finding to code

1. Evidence file.
2. F-entry or format section, with a label.
3. Code that cites the doc section, plus a real-install test that checks the rule on the whole install (e.g. `every_chassis_has_one_wheel_pair_per_declared_axle`).
4. PARITY.md updated: matches the game, our design, or unknown.

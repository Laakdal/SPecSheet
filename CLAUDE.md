@AGENTS.md

## Claude Code notes

- **Game install:** `D:\Steam Library\steamapps\common\Euro Truck Simulator 2` (ETS2 1.60.1.7). Export it as `ETS2_DIR` for every test run and research example.
- **Git Bash:** prefix example runs with `MSYS_NO_PATHCONV=1`. Otherwise game paths like `/vehicle/...` are rewritten to `C:/Program Files/Git/vehicle/...`.
- **REA on `eurotrucks2.exe`:**
  - Ghidra analysis takes about 28 minutes. The MCP tools hit their 330 s limit, so use the CLI.
  - Command: `npx -y rea-agents@6.4.0 search <exe> --pattern=... --mode regex --provider ghidra`
  - Required environment: `REA_GHIDRA_STARTUP_TIMEOUT_MS=10800000`, `JAVA_HOME=C:\Program Files\Java\jdk-22`, `GHIDRA_INSTALL_DIR=%USERPROFILE%\AppData\Local\Programs\Ghidra\ghidra_12.1.4_PUBLIC`.
  - Batch every question into one regex per run. Outputs go to `%USERPROFILE%\rea-ets2\`; copy cited results into `research/evidence/`.
- **Seeing the app:**
  - Write a script in your scratchpad and run the release binary with `SPECSHEET_SCRIPT=<file>`. Commands and an example are in `docs/DEVELOPMENT.md`. End the script with `screenshot <png>` and `exit`, then Read the PNG.
  - Build `specsheet` with its own `CARGO_TARGET_DIR`, so a long Bevy build doesn't lock the shared `target/` that peer sessions use.
  - To test real mouse input, drive the window from the PowerShell tool with user32 `mouse_event`. Use relative `MOUSEEVENTF_MOVE` for drags: `SetCursorPos` produces no raw motion, so Bevy's orbit ignores it. Sync on an `echo` line in the app's log.
- **Peer sessions:** other Claude sessions may work in this repo. Before editing root docs or `crates/`, check `ListAgents` and coordinate through `SendMessage`.
- **Reference C# sources** (sk-zk TruckLib.HashFs/Models, GisDeflate) for cross-checks: `%USERPROFILE%\refsrc\`.

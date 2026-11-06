"""hub context: print session state for context recovery.

Used by the optional Claude Code SessionStart hook (matcher compact|resume):
whatever this prints is added to the agent's context. Prints nothing when
there is no handoff, so the hook is silent in a fresh project.
"""
from pathlib import Path

import fsutil
import tasks

HANDOFF = ".agents/state/handoff.md"
BOARD = ".agents/state/tasks.md"


def run(root) -> int:
    root = Path(root)
    parts = []
    h = root / HANDOFF
    if h.exists():
        parts.append(f"[hub] Session handoff from {HANDOFF}:\n\n" + fsutil.read_text(h).strip())
    b = root / BOARD
    if b.exists():
        ts = tasks.parse(fsutil.read_text(b))
        if ts:
            rows = "\n".join(f"- {t.id} [{t.status or 'todo'}] {t.title}" for t in ts)
            parts.append(f"[hub] Task board ({BOARD}):\n{rows}")
    if parts:
        print("\n\n".join(parts))
    return 0

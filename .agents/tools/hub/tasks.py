"""hub tasks check: validate a multi-agent task board.

The board is the first Markdown table with the columns
    | id | task | owns | depends | status |
`owns` is a comma list of paths/globs the task may edit; `depends` a comma
list of task ids ("-" for none). Two tasks that could run in parallel (no
dependency path between them) must not own overlapping paths.
Overlap is checked conservatively: patterns overlap when the literal part
of one (up to the first wildcard) is a path-prefix of the other.
"""
import re
from dataclasses import dataclass, field
from pathlib import Path

import fsutil

COLUMNS = ["id", "task", "owns", "depends", "status"]


@dataclass
class Task:
    id: str
    title: str
    owns: list = field(default_factory=list)
    depends: list = field(default_factory=list)
    status: str = ""


def _cells(line):
    s = line.strip()
    if not s.startswith("|"):
        return None
    return [c.strip() for c in s.strip("|").split("|")]


def _split(cell):
    cell = cell.strip()
    if cell in ("", "-", "none", "—"):
        return []
    return [x.strip().strip("`") for x in cell.split(",") if x.strip()]


def parse(md: str):
    lines = md.split("\n")
    for i, line in enumerate(lines):
        cells = _cells(line)
        if cells and [c.lower() for c in cells[:5]] == COLUMNS:
            out = []
            for row in lines[i + 2:]:
                cells = _cells(row)
                if not cells:
                    break
                cells += [""] * (5 - len(cells))
                out.append(Task(cells[0], cells[1], _split(cells[2]), _split(cells[3]), cells[4]))
            return out
    return []


def _literal(pattern):
    m = re.search(r"[*?\[{]", pattern)
    lit = pattern[:m.start()] if m else pattern
    return lit.rstrip("/")


def _overlap(a, b):
    la, lb = _literal(a), _literal(b)
    if la == lb:
        return True
    return (lb.startswith(la + "/") or la == "") if len(la) <= len(lb) else (la.startswith(lb + "/") or lb == "")


def check(ts):
    problems = []
    ids = [t.id for t in ts]
    for x in sorted({i for i in ids if ids.count(i) > 1}):
        problems.append(f"duplicate task id {x}")
    by_id = {t.id: t for t in ts}
    for t in ts:
        for d in t.depends:
            if d not in by_id:
                problems.append(f"{t.id} depends on unknown task {d}")

    reach = {}

    def ancestors(tid, stack=()):
        if tid in reach:
            return reach[tid]
        if tid in stack:
            problems.append("dependency cycle: " + " -> ".join(stack + (tid,)))
            return set()
        acc = set()
        for d in by_id.get(tid, Task(tid, "")).depends:
            if d in by_id:
                acc |= {d} | ancestors(d, stack + (tid,))
        reach[tid] = acc
        return acc

    for t in ts:
        ancestors(t.id)
    for i, a in enumerate(ts):
        for b in ts[i + 1:]:
            if a.id in reach.get(b.id, set()) or b.id in reach.get(a.id, set()):
                continue
            hits = [(x, y) for x in a.owns for y in b.owns if _overlap(x, y)]
            if hits:
                x, y = hits[0]
                problems.append(f"{a.id} and {b.id} can run in parallel but both own '{x}' / '{y}': "
                                "give the path to one task, or make one depend on the other")
    return sorted(set(problems), key=problems.index)


def run_check(path) -> int:
    path = Path(path)
    if not path.exists():
        print(f"hub tasks: {path} not found")
        return 2
    ts = parse(fsutil.read_text(path))
    if not ts:
        print(f"hub tasks: no task table found in {path} (columns: {' | '.join(COLUMNS)})")
        return 1
    problems = check(ts)
    for p in problems:
        print(f"  PROBLEM {p}")
    print(f"hub tasks: {len(ts)} task(s), {len(problems)} problem(s)")
    return 1 if problems else 0

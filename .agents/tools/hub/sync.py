"""hub sync: generate platform adapters from .agents/ sources, and re-render the
AGENTS.md hub block from .agents/core.md and project.json (docs, commands).

Safety rules:
- A path not recorded in the lock's `generated` table is never overwritten
  (unless its content is already identical, in which case it is adopted).
- A generated file edited by hand since the last sync is not overwritten
  without --force.
- Generated files that are no longer wanted are deleted only if unmodified.
"""
import os
from dataclasses import dataclass, field
from pathlib import Path

import agentsmd
import fsutil
import lock as lockmod
import model
from adapters import ADAPTERS, Output
from adapters import claude as claude_adapter

SETTINGS = ".claude/settings.json"
SYMLINK_TEXT = ("AGENTS.md", "./AGENTS.md")


@dataclass
class Plan:
    writes: list = field(default_factory=list)      # [Output]
    deletes: list = field(default_factory=list)     # [path]
    adopt: list = field(default_factory=list)       # [Output] identical unowned files
    skipped: list = field(default_factory=list)     # [(path, reason)] -> failure
    notes: list = field(default_factory=list)       # [(path, reason)] -> warning only
    forget: list = field(default_factory=list)      # [path] drop from lock without deleting
    desired: dict = field(default_factory=dict)     # path -> Output
    settings: object = None                         # new settings text, or None for no change
    agents_md: object = None                        # (new AGENTS.md text, block), or None for no change

    @property
    def changes(self):
        return bool(self.writes or self.deletes or self.adopt or self.forget or self.settings is not None
                    or self.agents_md is not None)


def _digest(content) -> str:
    return fsutil.digest_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))


def _claude_md_satisfied(root: Path) -> bool:
    p = root / "CLAUDE.md"
    try:
        if p.is_symlink() and Path(os.path.realpath(p)) == Path(os.path.realpath(root / "AGENTS.md")):
            return True
        text = fsutil.read_text(p)
    except OSError:
        return False
    if any(line.strip() == "@AGENTS.md" for line in text.split("\n")):
        return True
    agents = root / "AGENTS.md"
    return agents.exists() and fsutil.digest(text) == fsutil.digest_file(agents)


def desired_outputs(project):
    outs = {}
    for platform in project.platforms:
        for o in ADAPTERS[platform].generate(project):
            outs[o.path] = o
    return outs


def plan(root, force=False) -> Plan:
    root = Path(root)
    project = model.load_project(root)
    lk = lockmod.Lock.load(root)
    pl = Plan(desired=desired_outputs(project))
    for path in [p for p in pl.desired if not fsutil.safe_rel(root, p)]:
        pl.skipped.append((path, f"refused: path is outside the project (from {pl.desired[path].source}); "
                                 "check the name in its frontmatter"))
        del pl.desired[path]

    for path, o in sorted(pl.desired.items()):
        p = root / path
        want = _digest(o.content)
        owned = lk.generated.get(path)
        if not p.exists() and not p.is_symlink():
            pl.writes.append(o)
            continue
        have = fsutil.digest_file(p) if p.is_file() else None
        if owned is None:
            if have == want:
                pl.adopt.append(o)
            elif path == "CLAUDE.md":
                if fsutil.read_text(p).strip() in SYMLINK_TEXT:
                    pl.notes.append((path, "is a git symlink checked out as plain text (core.symlinks=false, "
                                           "common on Windows): Claude Code reads the literal text 'AGENTS.md', "
                                           "not the file. Replace its content with '@AGENTS.md', which works on "
                                           "every OS (hub install . --claude-import does this)"))
                elif not _claude_md_satisfied(root):
                    pl.notes.append((path, "exists and does not import AGENTS.md; add a line '@AGENTS.md' "
                                           "(or run: hub install . --claude-import) so Claude Code loads it"))
            else:
                pl.skipped.append((path, "exists and is not owned by hub; rename yours or disable this "
                                         "item in .agents/project.json"))
            continue
        if have == want:
            if owned.get("sha256") != want:
                pl.adopt.append(o)
            continue
        if have != owned.get("sha256") and not force:
            pl.skipped.append((path, "was edited by hand since the last sync; move the edit to "
                                     f"{o.source} or rerun with --force"))
            continue
        pl.writes.append(o)

    for path, entry in sorted(lk.generated.items()):
        if path in pl.desired:
            continue
        if not fsutil.safe_rel(root, path):
            pl.notes.append((path, "lock entry points outside the project; ignored and dropped from the lock"))
            pl.forget.append(path)
            continue
        p = root / path
        if not p.exists():
            pl.forget.append(path)
        elif fsutil.digest_file(p) == entry.get("sha256") or force:
            pl.deletes.append(path)
        else:
            pl.notes.append((path, "no longer generated but edited by hand; kept and released to the project"))
            pl.forget.append(path)

    if "claude-code" in project.platforms or (root / SETTINGS).exists():
        claude = "claude-code" in project.platforms
        enabled = claude and bool(project.option("claude_hooks"))
        perms = claude and bool(project.option("claude_permissions", True))
        sp = root / SETTINGS
        current = fsutil.read_text(sp) if sp.exists() else None
        new = claude_adapter.patch_settings(current, enabled, perms)
        if new is not None and (current is None or fsutil.digest(new) != fsutil.digest(current)):
            pl.settings = new
    pl.agents_md = _core_refresh(root, project.config, lk)
    return pl


def _core_refresh(root: Path, config, lk):
    """Re-render the AGENTS.md core block when project.json changed the doc paths
    or commands it names. Only a file with exactly one intact block is touched."""
    p = root / "AGENTS.md"
    block = agentsmd.project_block(root, lk.version, config)
    if block is None or not p.is_file() or p.is_symlink():
        return None
    text = fsutil.read_text(p)
    if text.count(agentsmd.BEGIN) != 1 or text.count(agentsmd.END) != 1 or not agentsmd.find(text):
        return None
    new = agentsmd.apply_core(text, block)
    return None if new == text else (new, block)


def apply(root, pl: Plan):
    root = Path(root)
    lk = lockmod.Lock.load(root)
    for o in pl.writes:
        if not fsutil.safe_rel(root, o.path):
            continue
        p = root / o.path
        if p.is_symlink():
            p.unlink()
        if isinstance(o.content, bytes):
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(o.content)
        else:
            fsutil.write_text(p, o.content)
        lk.generated[o.path] = {"sha256": _digest(o.content), "source": o.source}
    for o in pl.adopt:
        lk.generated[o.path] = {"sha256": _digest(o.content), "source": o.source}
    for path in pl.deletes:
        if not fsutil.safe_rel(root, path):
            continue
        p = root / path
        p.unlink()
        fsutil.remove_empty_dirs(p.parent, root)
        lk.generated.pop(path, None)
    for path in pl.forget:
        lk.generated.pop(path, None)
    if pl.settings is not None:
        fsutil.write_text(root / SETTINGS, pl.settings)
    if pl.agents_md is not None:
        text, block = pl.agents_md
        fsutil.write_text(root / "AGENTS.md", text)
        entry = lk.files.get("AGENTS.md#core", {"owner": "framework"})
        lk.files["AGENTS.md#core"] = {**entry, "sha256": fsutil.digest(block)}
    lk.save()


def report(pl: Plan, check: bool, out=print):
    verb = "would " if check else ""
    for o in pl.writes:
        out(f"  {verb}write   {o.path}")
    for path in pl.deletes:
        out(f"  {verb}delete  {path}")
    if pl.settings is not None:
        out(f"  {verb}update  {SETTINGS} (hub hook entry)")
    if pl.agents_md is not None:
        out(f"  {verb}update  AGENTS.md (hub core block, from .agents/project.json)")
    for path, why in pl.skipped:
        out(f"  SKIPPED {path}: {why}")
    for path, why in pl.notes:
        out(f"  note    {path}: {why}")


def run(root, check=False, force=False, quiet=False, out=print) -> int:
    root = Path(root)
    pl = plan(root, force=force)
    if quiet:
        out = lambda *_: None  # noqa: E731
    if check:
        drift = pl.writes or pl.deletes or pl.settings is not None or pl.agents_md is not None
        report(pl, check=True, out=out)
        if drift:
            out("hub sync --check: generated files are out of date; run: python .agents/tools/hub sync")
        return 1 if (drift or pl.skipped) else 0
    apply(root, pl)
    report(pl, check=False, out=out)
    not_unchanged = {o.path for o in pl.writes} | {p for p, _ in pl.skipped} | {p for p, _ in pl.notes}
    unchanged = sum(1 for p in pl.desired if p not in not_unchanged)
    out(f"hub sync: {len(pl.writes)} written, {len(pl.deletes)} deleted, "
        f"{unchanged} unchanged, {len(pl.skipped)} skipped")
    return 1 if pl.skipped else 0

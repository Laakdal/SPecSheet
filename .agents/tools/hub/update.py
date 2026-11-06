"""hub update: adopt a newer hub version without clobbering local edits.

    python .agents/tools/hub update --source <path to a git clone of hub>

For each framework/pack file:
  unchanged locally            -> replaced with the new version
  edited locally, upstream same -> kept
  edited locally, upstream new  -> three-way merge (git merge-file) using the
                                   previously installed version as the base,
                                   read from the source repo's tag v<version>;
                                   conflicts leave your file and write <file>.hub-new
  removed upstream              -> deleted if unchanged, otherwise kept and released
Project-owned files (not in the lock) are never read or written.
"""
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import agentsmd
import fsutil
import install
import lock as lockmod
import model
import sync


def _git_show(source: Path, version: str, path: str):
    if not shutil.which("git") or not version:
        return None
    r = subprocess.run(["git", "-C", str(source), "show", f"v{version}:{path}"],
                       capture_output=True)
    if r.returncode != 0:
        return None
    return fsutil.normalize(r.stdout.decode("utf-8", errors="replace"))


def _merge(local: str, base: str, new: str):
    """Return (merged_text, clean: bool) using git merge-file."""
    with tempfile.TemporaryDirectory(prefix="hub-merge-") as d:
        paths = []
        for name, text in (("local", local), ("base", base), ("new", new)):
            p = Path(d) / name
            fsutil.write_text(p, text)
            paths.append(str(p))
        r = subprocess.run(["git", "merge-file", "-p", "-L", "yours", "-L", "base", "-L", "hub", *paths],
                           capture_output=True)
        return fsutil.normalize(r.stdout.decode("utf-8", errors="replace")), r.returncode == 0


def run(root, source, dry_run=False, out=print) -> int:
    root, source = Path(root), Path(source)
    lk = lockmod.Lock.load(root)
    if not lk.exists:
        out("hub update: no .agents/hub.lock.json here; run hub install first")
        return 2
    if not (source / "payload").is_dir():
        out(f"hub update: {source} is not a hub checkout (no payload/)")
        return 2
    old, new = lk.version, install.read_version(source)
    out(f"hub update {old} -> {new}{' (dry run)' if dry_run else ''}")
    cfg = model.load_config(root)
    wanted = [(rel, src, "framework") for rel, src in install.framework_files(source)]
    for name in cfg.get("packs", []):
        wanted += [(rel, src, f"pack:{name}") for rel, src in install.pack_files(source, name)]

    counts = {"updated": 0, "added": 0, "merged": 0, "kept": 0, "conflicts": 0, "removed": 0}
    seen = set()

    def put(rel, text_or_src):
        if dry_run or not fsutil.safe_rel(root, rel):
            return
        dst = root / rel
        if isinstance(text_or_src, Path):
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(text_or_src, dst)
        else:
            fsutil.write_text(dst, text_or_src)

    for rel, src, owner in wanted:
        seen.add(rel)
        dst = root / rel
        new_hash = fsutil.digest_file(src)
        entry = lk.files.get(rel)
        record = {"sha256": new_hash, "owner": owner, "version": new}
        if entry is None:
            if not dst.exists():
                out(f"  add     {rel}")
                put(rel, src)
                counts["added"] += 1
                lk.files[rel] = record
            elif fsutil.digest_file(dst) == new_hash:
                lk.files[rel] = record
            else:
                out(f"  CONFLICT {rel}: a project file has this path; hub's version written to {rel}.hub-new")
                put(rel + ".hub-new", src)
                counts["conflicts"] += 1
            continue
        if not dst.exists():
            out(f"  restore {rel}")
            put(rel, src)
            lk.files[rel] = record
            counts["added"] += 1
            continue
        have = fsutil.digest_file(dst)
        if have == entry["sha256"]:
            if have != new_hash:
                out(f"  update  {rel}")
                put(rel, src)
                counts["updated"] += 1
            lk.files[rel] = record
            continue
        if new_hash == entry["sha256"]:
            counts["kept"] += 1  # local edit, nothing new upstream
            continue
        base = _git_show(source, entry.get("version") or old, fsutil.rel(src, source))
        new_text = fsutil.read_text(src)
        if base is not None and shutil.which("git"):
            merged, clean = _merge(fsutil.read_text(dst), base, new_text)
            if clean:
                out(f"  merge   {rel} (your edits kept)")
                put(rel, merged)
                lk.files[rel] = record
                counts["merged"] += 1
                continue
        out(f"  CONFLICT {rel}: edited locally and upstream; new version written to {rel}.hub-new")
        put(rel + ".hub-new", new_text)
        counts["conflicts"] += 1

    for rel in sorted(set(lk.files) - seen):
        if rel.endswith("#core"):
            continue
        if not fsutil.safe_rel(root, rel):
            out(f"  ignore  {rel} (lock entry points outside the project)")
            lk.files.pop(rel, None)
            continue
        p = root / rel
        if p.exists() and fsutil.digest_file(p) == lk.files[rel]["sha256"]:
            out(f"  remove  {rel} (removed upstream)")
            if not dry_run:
                p.unlink()
                fsutil.remove_empty_dirs(p.parent, root)
            counts["removed"] += 1
        elif p.exists():
            out(f"  release {rel} (removed upstream, edited locally: now yours)")
        if not dry_run:
            lk.files.pop(rel, None)

    block = agentsmd.core_block(source, new, cfg)
    agents = root / "AGENTS.md"
    if agents.exists():
        text = fsutil.read_text(agents)
        updated = agentsmd.apply_core(text, block)
        if updated != text:
            out("  update  AGENTS.md (hub core block)")
            put("AGENTS.md", updated)
    lk.files["AGENTS.md#core"] = {"sha256": fsutil.digest(block), "owner": "framework", "version": new}

    out("hub update: " + ", ".join(f"{v} {k}" for k, v in counts.items()))
    if dry_run:
        return 0
    lk.version = new
    lk.save()
    rc = sync.run(root, out=out)
    if counts["conflicts"]:
        out("Resolve each <file>.hub-new by merging it into <file>, then delete the .hub-new file.")
        return 1
    return rc

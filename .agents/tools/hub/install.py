"""hub install: put the framework into a project without overwriting anything.

    python <hub>/payload/.agents/tools/hub install <project> [options]

Never replaces a file hub does not own. Existing AGENTS.md gets the core
block inserted; everything else in it is left byte-for-byte as it was.
"""
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

import agentsmd
import detect
import fsutil
import lock as lockmod
import model
import sync

GITIGNORE_LINE = ".agents/state/"
SKIP_COPY = {"__pycache__", "state"}


def default_source():
    """The hub root when this tool runs from a template checkout (…/payload/.agents/tools/hub)."""
    here = Path(__file__).resolve().parent
    payload = here.parents[2]
    if payload.name == "payload" and (payload.parent / "VERSION").exists():
        return payload.parent
    return None


def read_version(source) -> str:
    return fsutil.read_text(Path(source) / "VERSION").strip()


def framework_files(source):
    """(relpath in project, source Path) for every payload file under .agents/."""
    base = Path(source) / "payload"
    out = []
    for p in fsutil.walk_files(base / ".agents", skip_dirs=SKIP_COPY):
        if p.suffix == ".pyc":
            continue
        out.append((fsutil.rel(p, base), p))
    return out


def pack_files(source, name):
    pack_dir = Path(source) / "packs" / name
    if not (pack_dir / "pack.json").exists():
        raise SystemExit(f"hub: unknown pack '{name}' (no {pack_dir / 'pack.json'})")
    out = []
    for sub, dest in (("rules", ".agents/rules"), ("skills", ".agents/skills")):
        for p in fsutil.walk_files(pack_dir / sub, skip_dirs=SKIP_COPY):
            out.append((f"{dest}/{fsutil.rel(p, pack_dir / sub)}", p))
    return out


def _skill_group(rel):
    parts = rel.split("/")
    if len(parts) >= 4 and parts[0] == ".agents" and parts[1] == "skills":
        return "/".join(parts[:3])
    return None


class Installer:
    def __init__(self, target, source, dry_run=False, out=print):
        self.target = Path(target)
        self.source = Path(source)
        self.dry = dry_run
        self.out = out
        self.lock = lockmod.Lock.load(self.target)
        self.version = read_version(self.source)
        self.conflicts = []
        self.staged = False  # True when installing from a temporary template staging copy

    def write(self, rel, content):
        if not fsutil.safe_rel(self.target, rel):
            self.conflicts.append((rel, "resolves outside the project (symlink); not written"))
            return
        if not self.dry:
            fsutil.write_text(self.target / rel, content)

    def copy_files(self, files, owner):
        """Copy files; skip whole skill dirs whose SKILL.md belongs to the project."""
        blocked = set()
        for rel, src in files:
            g = _skill_group(rel)
            dst = self.target / rel
            if g and rel == g + "/SKILL.md" and dst.exists() and rel not in self.lock.files \
                    and fsutil.digest_file(dst) != fsutil.digest_file(src):
                blocked.add(g)
        for rel, src in files:
            g = _skill_group(rel)
            if g in blocked:
                if rel == g + "/SKILL.md":
                    self.conflicts.append((rel, "project already has a skill with this name; hub's version skipped"))
                continue
            dst = self.target / rel
            if not fsutil.safe_rel(self.target, rel):
                self.conflicts.append((rel, "resolves outside the project (symlink); not written"))
                continue
            new_hash = fsutil.digest_file(src)
            if dst.exists():
                have = fsutil.digest_file(dst)
                if have == new_hash:
                    self.lock.files[rel] = {"sha256": new_hash, "owner": owner, "version": self.version}
                    continue
                if rel in self.lock.files:
                    self.conflicts.append((rel, "differs from this hub version; use 'hub update' to upgrade"))
                else:
                    self.conflicts.append((rel, "exists and is not owned by hub; skipped"))
                continue
            self.out(f"  add     {rel}")
            if not self.dry:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(src, dst)
            self.lock.files[rel] = {"sha256": new_hash, "owner": owner, "version": self.version}

    def agents_md(self, config=None):
        block = agentsmd.core_block(self.source, self.version, config)
        p = self.target / "AGENTS.md"
        if p.exists():
            text = fsutil.read_text(p)
            new = agentsmd.apply_core(text, block)
            if new != text:
                self.out("  update  AGENTS.md (hub core block; your text is unchanged)")
                self.write("AGENTS.md", new)
        else:
            tmpl = fsutil.read_text(self.source / "payload" / "AGENTS.md.tmpl")
            text = agentsmd.apply_core(tmpl.replace("{{project_name}}", self.target.name), block)
            self.out("  add     AGENTS.md")
            self.write("AGENTS.md", text)
        self.lock.files["AGENTS.md#core"] = {"sha256": fsutil.digest(block), "owner": "framework"}

    def commands_note(self, config):
        """Say so when the project records no commands, since the core block's
        'Prove it works' rule points at them."""
        if config.get("commands"):
            return
        p = self.target / "AGENTS.md"
        text = fsutil.read_text(p) if p.exists() else fsutil.read_text(self.source / "payload" / "AGENTS.md.tmpl")
        if any(line.strip().lower() == "## commands" for line in text.split("\n")):
            return
        self.out("  note    no commands recorded: add them to \"commands\" in .agents/project.json "
                 "(test, lint, build...), then run: python .agents/tools/hub sync")

    def claude_import(self):
        p = self.target / "CLAUDE.md"
        if not p.exists() or p.is_symlink():
            return
        text = fsutil.read_text(p)
        if any(line.strip() == "@AGENTS.md" for line in text.split("\n")):
            return
        if text.strip() in sync.SYMLINK_TEXT:
            self.out("  update  CLAUDE.md (symlink checked out as text -> @AGENTS.md import)")
            self.write("CLAUDE.md", "@AGENTS.md\n")
            return
        self.out("  update  CLAUDE.md (prepend @AGENTS.md import)")
        self.write("CLAUDE.md", "@AGENTS.md\n\n" + text)

    def project_json(self, platforms, packs):
        p = self.target / model.PROJECT_JSON
        if p.exists():
            cfg = json.loads(fsutil.read_text(p))
        else:
            cfg = model.default_config(self.target.name)
            cfg["platforms"] = detect.detected_platforms(self.target)
            # Record existing docs only when there is exactly one candidate; never guess.
            docs = {k: v[0] for k, v in detect.doc_files(self.target).items() if len(v) == 1}
            if docs:
                cfg["docs"] = docs
            self.out("  add     .agents/project.json")
        if platforms:
            cfg["platforms"] = list(platforms)
        for name in packs:
            if name not in cfg.setdefault("packs", []):
                cfg["packs"].append(name)
        self.write(model.PROJECT_JSON, json.dumps(cfg, indent=2) + "\n")
        return cfg

    def gitignore(self):
        p = self.target / ".gitignore"
        text = fsutil.read_text(p) if p.exists() else ""
        if any(line.strip() in (GITIGNORE_LINE, "/" + GITIGNORE_LINE) for line in text.split("\n")):
            return
        sep = "" if (not text or text.endswith("\n")) else "\n"
        self.write(".gitignore", text + sep + ("\n" if text else "") +
                   "# hub: session-local agent state (handoffs, task boards)\n" + GITIGNORE_LINE + "\n")

    def detect(self):
        facts = detect.scan(self.target)
        packs = []
        for pj in sorted((self.source / "packs").glob("*/pack.json")):
            data = json.loads(fsutil.read_text(pj))
            packs.append({k: data.get(k) for k in ("name", "description", "detect")})
        # Machine-local (state/ is gitignored): lets init-project propose packs and
        # print the exact install command for this checkout.
        facts["hub"] = {"version": self.version, "source": None if self.staged else str(self.source),
                       "available_packs": packs}
        self.write(".agents/state/detect.json", json.dumps(facts, indent=2) + "\n")
        return facts


def _confirm(prompt, yes):
    if yes:
        return True
    if not sys.stdin or not sys.stdin.isatty():
        print(prompt + " [refusing without --yes in a non-interactive shell]")
        return False
    return input(prompt + " [y/N] ").strip().lower() in ("y", "yes")


def template_cleanup(target, yes, dry_run, out=print):
    """For a repo created with "Use this template": stage the payload, then remove
    template-only files that are still exactly as shipped. Returns staging dir."""
    target = Path(target)
    manifest = target / "hub-template.json"
    if not manifest.exists():
        raise SystemExit("hub: --from-template needs hub-template.json at the repo root")
    paths = json.loads(fsutil.read_text(manifest))["paths"]
    remove, keep = [], []
    for rel, sha in sorted(paths.items()):
        if not fsutil.safe_rel(target, rel):
            out(f"  refuse  {rel} (outside the project; never removed)")
            continue
        p = target / rel
        if not p.is_file():
            continue
        (remove if fsutil.digest_file(p) == sha else keep).append(rel)
    out(f"hub: template cleanup will remove {len(remove)} template-only files:")
    for rel in remove:
        out(f"  remove  {rel}")
    for rel in keep:
        out(f"  keep    {rel} (changed since the template was created)")
    if dry_run:
        return None
    if not _confirm("Proceed?", yes):
        raise SystemExit(1)
    staging = Path(tempfile.mkdtemp(prefix="hub-src-"))
    for name in ("payload", "packs"):
        if (target / name).is_dir():
            shutil.copytree(target / name, staging / name, ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copyfile(target / "VERSION", staging / "VERSION")
    for rel in remove:
        p = target / rel
        p.unlink()
        fsutil.remove_empty_dirs(p.parent, target)
    for leftover in ("payload", "packs"):
        d = target / leftover
        if d.is_dir() and all(x.name == "__pycache__" or x.suffix == ".pyc" for x in d.rglob("*") if x.is_file()):
            shutil.rmtree(d, ignore_errors=True)
    manifest.unlink()
    if not (target / "README.md").exists():
        fsutil.write_text(target / "README.md", f"# {target.name}\n\nSet up with hub. See AGENTS.md.\n")
    return staging


def run(target=".", source=None, platforms=None, packs=(), from_template=False, yes=False,
        dry_run=False, claude_import=False, out=print) -> int:
    target = Path(target).resolve()
    staging = None
    if from_template:
        staging = template_cleanup(target, yes, dry_run, out)
        source = staging or target
    source = Path(source).resolve() if source else default_source()
    if source is None or not (source / "payload").is_dir():
        out("hub: cannot find the hub source. Pass --source <path to a hub checkout>.")
        return 2
    try:
        inst = Installer(target, source, dry_run=dry_run, out=out)
        if inst.lock.version and inst.lock.version != inst.version:
            out(f"hub: this project has hub {inst.lock.version}; the source is {inst.version}. "
                f"Use 'hub update --source {source}' to upgrade (it merges your edits). "
                "install only adds hub to a project or re-applies the same version.")
            return 2
        inst.staged = staging is not None
        out(f"hub install {inst.version} -> {target}{' (dry run)' if dry_run else ''}")
        inst.copy_files(framework_files(source), "framework")
        cfg_packs = list(packs)
        if (target / model.PROJECT_JSON).exists():
            cfg_packs += [p for p in model.load_config(target).get("packs", []) if p not in cfg_packs]
        for name in cfg_packs:
            inst.copy_files(pack_files(source, name), f"pack:{name}")
        cfg = inst.project_json(platforms, packs)
        inst.agents_md(cfg)
        if claude_import:
            inst.claude_import()
        inst.gitignore()
        for rel, why in inst.conflicts:
            out(f"  SKIPPED {rel}: {why}")
        inst.commands_note(cfg)
        if dry_run:
            out("hub: dry run, nothing written")
            return 0
        inst.lock.version = inst.version
        inst.lock.source = "hub"
        inst.lock.save()
        inst.detect()
        rc = sync.run(target, out=out)
        out("\nNext: open your AI agent in this project and run the init-project skill "
            "(Claude Code: /init-project) to fill in the project section of AGENTS.md.")
        return rc
    finally:
        if staging:
            shutil.rmtree(staging, ignore_errors=True)

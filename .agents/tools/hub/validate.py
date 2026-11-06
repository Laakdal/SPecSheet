"""hub validate: structure, budgets, links, schemas, ownership and drift.

Exit 1 on any error. Warnings never fail (project-owned text may exceed
budgets; init-project proposes splitting it).
"""
import json
import re
from collections import namedtuple
from pathlib import Path

import agentsmd
import fm
import fsutil
import lock as lockmod
import model
import sync

Issue = namedtuple("Issue", "level path msg")

NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
CORE_MAX_LINES = 40
AGENTS_WARN_LINES, AGENTS_WARN_BYTES = 150, 12 * 1024
FRAMEWORK_DESC_MAX = 400
SPEC_DESC_MAX = 1024
SKILL_MAX_LINES, FRAMEWORK_SKILL_WARN_LINES = 500, 250
AGENT_WARN_LINES, RULE_WARN_LINES = 120, 80
DUP_MIN_CHARS = 80
SKILL_KEYS = {"name", "description", "license", "compatibility", "metadata", "allowed-tools",
              "disable-model-invocation", "user-invocable", "argument-hint", "when_to_use", "paths",
              "context", "agent", "model", "effort", "arguments"}
AGENT_KEYS = {"name", "description", "capability", "model-tier", "isolation", "skills"}
CAPABILITIES = ("read-only", "execute", "edit")
TIERS = ("fast", "default", "strong")
LINK_RE = re.compile(r"(?<!!)\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
FENCE_RE = re.compile(r"^(```|~~~)[\s\S]*?^\1[^\n]*$", re.M)
INLINE_CODE_RE = re.compile(r"`[^`\n]*`")


class Checker:
    def __init__(self, root, content_root=None):
        self.root = Path(root)
        self.content = Path(content_root or root)   # dir holding .agents/
        self.issues = []
        self.lock = lockmod.Lock.load(self.content)

    def err(self, path, msg):
        self.issues.append(Issue("error", path, msg))

    def warn(self, path, msg):
        self.issues.append(Issue("warn", path, msg))

    def rel(self, p):
        return fsutil.rel(p, self.root)

    def framework_owned(self, relpath, template):
        return template or relpath in self.lock.files

    # -- skills, agents, rules ---------------------------------------------
    def skills(self, base: Path, template=False):
        if not base.is_dir():
            return
        for d in sorted(x for x in base.iterdir() if x.is_dir()):
            sk = d / "SKILL.md"
            r = self.rel(sk)
            if not sk.exists():
                self.err(self.rel(d), "skill directory has no SKILL.md")
                continue
            try:
                meta, body = fm.parse(fsutil.read_text(sk))
            except fm.FrontmatterError as e:
                self.err(r, f"frontmatter: {e}")
                continue
            self.yaml_lint(r, sk)
            owned = self.framework_owned(fsutil.rel(sk, self.content), template)
            name = meta.get("name")
            if not isinstance(name, str) or not NAME_RE.match(name) or len(name) > 64:
                self.err(r, f"name {name!r} must be 1-64 chars of a-z, 0-9 and single hyphens")
            elif name != d.name:
                self.err(r, f"name {name!r} must match its directory '{d.name}'")
            desc = meta.get("description")
            if not isinstance(desc, str) or not desc.strip():
                self.err(r, "description is required")
            elif len(desc) > SPEC_DESC_MAX:
                self.err(r, f"description is {len(desc)} chars; the Agent Skills limit is {SPEC_DESC_MAX}")
            elif owned and len(desc) > FRAMEWORK_DESC_MAX:
                self.err(r, f"framework skill description is {len(desc)} chars; hub budget is {FRAMEWORK_DESC_MAX}")
            comp = meta.get("compatibility")
            if isinstance(comp, str) and len(comp) > 500:
                self.err(r, "compatibility must be at most 500 chars")
            unknown = set(meta) - SKILL_KEYS
            if unknown:
                self.warn(r, f"unknown frontmatter keys {sorted(unknown)} (other tools may reject them)")
            lines = fsutil.read_text(sk).count("\n")
            if lines > SKILL_MAX_LINES:
                self.err(r, f"SKILL.md is {lines} lines; keep it under {SKILL_MAX_LINES} (move detail to references/)")
            elif owned and lines > FRAMEWORK_SKILL_WARN_LINES:
                self.warn(r, f"framework SKILL.md is {lines} lines; budget is {FRAMEWORK_SKILL_WARN_LINES}")

    def docs(self, base: Path, kind: str):
        if not base.is_dir():
            return
        for p in sorted(base.glob("*.md")):
            if p.name.lower() == "readme.md":
                continue
            r = self.rel(p)
            try:
                meta, body = fm.parse(fsutil.read_text(p))
            except fm.FrontmatterError as e:
                self.err(r, f"frontmatter: {e}")
                continue
            self.yaml_lint(r, p)
            if not meta.get("description"):
                self.err(r, "description is required")
            n = body.count("\n")
            if kind == "agent":
                name = meta.get("name")
                if name != p.stem:
                    self.err(r, f"name {name!r} must equal the file name '{p.stem}'")
                elif not NAME_RE.match(name):
                    self.err(r, f"name {name!r} must be lowercase letters, digits and hyphens")
                if meta.get("capability") not in CAPABILITIES:
                    self.err(r, f"capability must be one of {list(CAPABILITIES)}")
                if meta.get("model-tier", "default") not in TIERS:
                    self.err(r, f"model-tier must be one of {list(TIERS)}")
                if meta.get("isolation", "none") not in ("none", "worktree"):
                    self.err(r, "isolation must be 'none' or 'worktree'")
                unknown = set(meta) - AGENT_KEYS
                if unknown:
                    self.warn(r, f"unknown agent keys {sorted(unknown)} (ignored by adapters)")
                if n > AGENT_WARN_LINES:
                    self.warn(r, f"agent body is {n} lines; budget is {AGENT_WARN_LINES}")
            else:
                if not meta.get("paths"):
                    self.err(r, "paths (globs) is required for a rule")
                if n > RULE_WARN_LINES:
                    self.warn(r, f"rule is {n} lines; budget is {RULE_WARN_LINES}")

    def yaml_lint(self, r, path):
        block, _ = fm.split(fsutil.read_text(path))
        for problem in fm.lint(block or ""):
            self.err(r, problem)

    # -- text checks --------------------------------------------------------
    def links(self, files, strict=False):
        """Broken links are errors in hub-owned files (or everywhere when strict);
        in project-owned files they are warnings, like other project-text checks."""
        for p in files:
            text = fsutil.read_text(p)
            crel = fsutil.rel(p, self.content)
            owned = strict or crel in self.lock.files or crel in self.lock.generated
            report = self.err if owned else self.warn
            if p.suffix in (".md", ".mdc", ".tmpl"):
                stripped = INLINE_CODE_RE.sub("", FENCE_RE.sub("", text))
                for target in LINK_RE.findall(stripped):
                    if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith(("#", "<")) \
                            or "{{" in target:
                        continue
                    path = target.split("#", 1)[0]
                    if path and not (p.parent / path).exists():
                        report(self.rel(p), f"broken link: {target}")
            if p.name == "CLAUDE.md":
                for line in text.split("\n"):
                    s = line.strip()
                    if s.startswith("@") and " " not in s and len(s) > 1:
                        if not (p.parent / s[1:]).exists():
                            self.err(self.rel(p), f"broken @import: {s}")

    def duplicates(self, files):
        seen = {}
        for p in files:
            _, body = fm.parse(fsutil.read_text(p)) if p.suffix == ".md" else ({}, fsutil.read_text(p))
            body = FENCE_RE.sub("", body)
            for para in re.split(r"\n\s*\n", body):
                norm = " ".join(para.lower().split())
                if len(norm) < DUP_MIN_CHARS or norm.startswith("<!-- generated"):
                    continue
                if norm in seen and seen[norm] != p:
                    self.warn(self.rel(p), f"duplicate paragraph also in {self.rel(seen[norm])}: "
                                           f"'{norm[:60]}...' (state each rule once and link to it)")
                else:
                    seen.setdefault(norm, p)

    def agents_md(self, p: Path, is_template=False):
        r = self.rel(p)
        if not p.exists():
            self.err(r, "missing")
            return
        text = fsutil.read_text(p)
        n_begin, n_end = text.count(agentsmd.BEGIN), text.count(agentsmd.END)
        if not is_template:
            if n_begin != 1 or n_end != 1 or not agentsmd.find(text):
                self.err(r, "must contain exactly one hub core block (run: hub install .)")
            else:
                lines = agentsmd.block_lines(text)
                if lines > CORE_MAX_LINES:
                    self.err(r, f"hub core block is {lines} lines; limit is {CORE_MAX_LINES}")
            total = text.count("\n")
            if total > AGENTS_WARN_LINES or len(text.encode("utf-8")) > AGENTS_WARN_BYTES:
                self.warn(r, f"AGENTS.md is {total} lines / {len(text.encode('utf-8'))} bytes; aim for "
                             f"<= {AGENTS_WARN_LINES} lines / 12 KB: move area-specific guidance into nested "
                             "AGENTS.md files or .agents/rules/ (Codex drops instructions past 32 KiB combined)")
        if agentsmd.INIT_REQUIRED in text and not is_template:
            self.warn(r, "project section not filled in yet: run the init-project skill")

    def schemas(self, schema_dir: Path):
        import schema as schemalib
        targets = {"project.schema.json": self.content / model.PROJECT_JSON,
                   "lock.schema.json": self.content / lockmod.LOCK_PATH}
        for sname, target in targets.items():
            sp = schema_dir / sname
            if not sp.exists() or not target.exists():
                continue
            try:
                data = json.loads(fsutil.read_text(target))
            except ValueError as e:
                self.err(self.rel(target), f"invalid JSON: {e}")
                continue
            for e in schemalib.validate(data, json.loads(fsutil.read_text(sp))):
                self.err(self.rel(target), e)

    def ownership(self):
        modified = 0
        for path, entry in self.lock.files.items():
            if path.endswith("#core"):
                continue
            p = self.content / path
            if not p.exists():
                self.warn(path, "framework file is missing (deleted locally?); 'hub install .' restores it")
            elif fsutil.digest_file(p) != entry.get("sha256"):
                modified += 1
        if modified:
            self.warn(lockmod.LOCK_PATH, f"{modified} framework file(s) edited locally; 'hub update' will "
                                         "three-way merge them")

    def generated_formats(self):
        try:
            import tomllib
        except ImportError:
            tomllib = None
        for path in self.lock.generated:
            p = self.content / path
            if not p.exists():
                continue
            try:
                if p.suffix == ".toml" and tomllib:
                    tomllib.loads(fsutil.read_text(p))
                elif p.suffix == ".json":
                    json.loads(fsutil.read_text(p))
                elif p.suffix in (".md", ".mdc"):
                    fm.parse(fsutil.read_text(p))
            except Exception as e:  # noqa: BLE001 - report any parse failure
                self.err(path, f"generated file does not parse: {e}")

    def drift(self):
        pl = sync.plan(self.content)
        if pl.writes or pl.deletes or pl.settings is not None:
            self.err(".agents/", f"{len(pl.writes) + len(pl.deletes)} generated file(s) out of date: run hub sync")
        if pl.agents_md is not None:
            self.err("AGENTS.md", "hub core block does not match .agents/project.json (docs, commands): run hub sync")
        for path, why in pl.skipped:
            self.err(path, why)
        for path, why in pl.notes:
            self.warn(path, why)


def _md_files(*dirs):
    out = []
    for d in dirs:
        d = Path(d)
        if d.is_file():
            out.append(d)
        elif d.is_dir():
            out += [p for p in fsutil.walk_files(d, skip_dirs={"__pycache__", "state", "node_modules"})
                    if p.suffix in (".md", ".mdc", ".tmpl")]
    return out


def collect(root, template=False):
    root = Path(root)
    if template:
        return collect_template(root)
    c = Checker(root)
    a = root / ".agents"
    c.agents_md(root / "AGENTS.md")
    c.skills(a / "skills")
    c.docs(a / "agents", "agent")
    c.docs(a / "rules", "rule")
    c.links(_md_files(root / "AGENTS.md", root / "CLAUDE.md", a, root / "docs")
            + [root / g for g in c.lock.generated if g.endswith((".md", ".mdc")) and (root / g).exists()])
    c.duplicates(_md_files(a / "standards", a / "skills", a / "agents", a / "rules"))
    c.schemas(a / "schemas")
    c.ownership()
    for p in fsutil.walk_files(root, skip_dirs={".git", "node_modules", "vendor", ".venv", "target"}):
        if p.name.endswith(".hub-new"):
            c.warn(c.rel(p), "unresolved hub update conflict: merge it into "
                             f"{p.name[:-len('.hub-new')]} and delete the .hub-new file")
    c.generated_formats()
    c.drift()
    return c.issues


def collect_template(root):
    import template as templatemod
    payload = root / "payload"
    c = Checker(root, content_root=payload)
    a = payload / ".agents"
    core = a / "core.md"
    if not core.exists():
        c.err("payload/.agents/core.md", "missing")
    elif fsutil.read_text(core).strip("\n").count("\n") + 1 > CORE_MAX_LINES - 2:
        c.err("payload/.agents/core.md", f"core block must stay within {CORE_MAX_LINES} lines incl. markers")
    c.agents_md(payload / "AGENTS.md.tmpl", is_template=True)
    c.skills(a / "skills", template=True)
    c.docs(a / "agents", "agent")
    for d in sorted((root / "packs").iterdir()) if (root / "packs").is_dir() else []:
        pj = d / "pack.json"
        if not pj.exists():
            c.err(c.rel(d), "pack has no pack.json")
            continue
        data = json.loads(fsutil.read_text(pj))
        import schema as schemalib
        sp = a / "schemas" / "pack.schema.json"
        if sp.exists():
            for e in schemalib.validate(data, json.loads(fsutil.read_text(sp))):
                c.err(c.rel(pj), e)
        if data.get("name") != d.name:
            c.err(c.rel(pj), f"name must equal directory '{d.name}'")
        for r in data.get("rules", []):
            if not (d / "rules" / r).exists():
                c.err(c.rel(pj), f"lists missing rule {r}")
        for s in data.get("skills", []):
            if not (d / "skills" / s / "SKILL.md").exists():
                c.err(c.rel(pj), f"lists missing skill {s}")
        c.docs(d / "rules", "rule")
        c.skills(d / "skills", template=True)
    c.links(_md_files(payload, root / "docs", root / "README.md", root / "CONTRIBUTING.md", root / "packs"), strict=True)
    c.duplicates(_md_files(a / "standards", a / "skills", a / "agents"))
    if not templatemod.is_fresh(root):
        c.err(templatemod.MANIFEST, "out of date: run python scripts/build_template_manifest.py")
    return c.issues


def run(root, template=False, out=print) -> int:
    issues = collect(root, template=template)
    for i in issues:
        out(f"{i.level.upper():5} {i.path}: {i.msg}")
    errors = sum(1 for i in issues if i.level == "error")
    warns = len(issues) - errors
    out(f"hub validate: {errors} error(s), {warns} warning(s)")
    return 1 if errors else 0

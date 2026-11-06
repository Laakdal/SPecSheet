"""Load a hub project: config, skills, agents and rules from .agents/."""
import json
from dataclasses import dataclass, field
from pathlib import Path

import fm
import fsutil

PLATFORMS = ("claude-code", "codex", "copilot", "cursor")
PROJECT_JSON = ".agents/project.json"
GENERATED_IN_SKILL = {"agents/openai.yaml"}  # written by the codex adapter inside source skills


@dataclass
class Skill:
    name: str
    dir: Path
    rel_dir: str
    meta: dict
    body: str
    files: list = field(default_factory=list)  # [(path relative to skill dir, Path)]

    @property
    def manual(self) -> bool:
        return self.meta.get("disable-model-invocation") is True


@dataclass
class Doc:
    """An agent or rule: one Markdown file with frontmatter."""
    name: str
    path: Path
    rel: str
    meta: dict
    body: str


@dataclass
class Project:
    root: Path
    config: dict
    skills: list
    agents: list
    rules: list

    @property
    def platforms(self):
        return [p for p in self.config.get("platforms", []) if p in PLATFORMS]

    def option(self, key, default=False):
        return self.config.get("options", {}).get(key, default)


def default_config(name="project"):
    return {
        "name": name,
        "platforms": ["claude-code"],
        "packs": [],
        "disabled": {"skills": [], "agents": [], "rules": []},
        "options": {"claude_hooks": False, "claude_permissions": True, "copilot_instructions": False},
        "stack": {},
        "commands": {},
    }


def load_config(root) -> dict:
    p = Path(root) / PROJECT_JSON
    if not p.exists():
        return default_config(Path(root).name)
    return json.loads(fsutil.read_text(p))


def _load_doc(p: Path, root: Path) -> Doc:
    meta, body = fm.parse(fsutil.read_text(p))
    return Doc(name=meta.get("name", p.stem), path=p, rel=fsutil.rel(p, root), meta=meta, body=body)


def load_skills(root: Path, disabled=()):
    out = []
    base = root / ".agents" / "skills"
    if not base.is_dir():
        return out
    for d in sorted(x for x in base.iterdir() if x.is_dir()):
        sk = d / "SKILL.md"
        if not sk.exists() or d.name in disabled:
            continue
        meta, body = fm.parse(fsutil.read_text(sk))
        files = []
        for f in fsutil.walk_files(d, skip_dirs=("__pycache__",)):
            r = fsutil.rel(f, d)
            if r == "SKILL.md" or r in GENERATED_IN_SKILL:
                continue
            files.append((r, f))
        out.append(Skill(name=meta.get("name", d.name), dir=d, rel_dir=fsutil.rel(d, root),
                         meta=meta, body=body, files=files))
    return out


def load_docs(root: Path, sub: str, disabled=()):
    base = root / ".agents" / sub
    if not base.is_dir():
        return []
    return [_load_doc(p, root) for p in sorted(base.glob("*.md"))
            if p.stem not in disabled and p.name.lower() != "readme.md"]


def load_project(root) -> Project:
    root = Path(root)
    config = load_config(root)
    dis = config.get("disabled", {})
    return Project(
        root=root,
        config=config,
        skills=load_skills(root, dis.get("skills", [])),
        agents=load_docs(root, "agents", dis.get("agents", [])),
        rules=load_docs(root, "rules", dis.get("rules", [])),
    )

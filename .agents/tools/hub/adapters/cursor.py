"""Cursor adapter.

AGENTS.md and .agents/skills/ are read natively. Generated here:
- .cursor/agents/<name>.md  (readonly flag; .cursor/ wins over .claude/agents
  on name conflict, so Claude copies don't double up)
- .cursor/rules/<name>.mdc  (description, globs, alwaysApply: false)
  Cursor ignores plain .md in .cursor/rules, and its own editor writes globs
  unquoted and comma-separated, so that is the format used here.
"""
import fm
from . import Output, md_header, with_md_header, capability, comma, relink


def generate(project):
    out = []
    for a in project.agents:
        meta = {"name": a.name, "description": a.meta.get("description", ""),
                "model": "inherit", "readonly": capability(a.meta) == "read-only",
                "is_background": False}
        out.append(Output(f".cursor/agents/{a.name}.md",
                          with_md_header(fm.dump(meta) + relink(a.body, a.rel, f".cursor/agents/{a.name}.md"), a.rel), a.rel))
    for r in project.rules:
        desc = fm.dump({"description": r.meta.get("description", "")}).split("\n")[1]
        head = f"---\n{desc}\nglobs: {comma(r.meta.get('paths'))}\nalwaysApply: false\n---\n"
        out.append(Output(f".cursor/rules/{r.name}.mdc", head + md_header(r.rel) + relink(r.body, r.rel, f".cursor/rules/{r.name}.mdc"), r.rel))
    return out

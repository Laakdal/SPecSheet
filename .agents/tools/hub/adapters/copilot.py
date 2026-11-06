"""GitHub Copilot adapter.

AGENTS.md and .agents/skills/ are read natively by most Copilot surfaces.
Generated here:
- .github/agents/<name>.agent.md              (custom agents)
- .github/instructions/<name>.instructions.md (applyTo globs)
- optional .github/copilot-instructions.md    (copy of AGENTS.md, for surfaces
  that do not read AGENTS.md: github.com chat, Visual Studio, JetBrains chat)
"""
import fm
import fsutil
from . import Output, with_md_header, md_header, capability, comma, relink

TOOLS = {"read-only": ["read", "search"], "execute": ["read", "search", "execute"]}


def generate(project):
    out = []
    for a in project.agents:
        meta = {"name": a.name, "description": a.meta.get("description", "")}
        cap = capability(a.meta)
        if cap in TOOLS:
            meta["tools"] = TOOLS[cap]
        out.append(Output(f".github/agents/{a.name}.agent.md",
                          with_md_header(fm.dump(meta) + relink(a.body, a.rel, f".github/agents/{a.name}.agent.md"), a.rel), a.rel))
    for r in project.rules:
        meta = {"applyTo": comma(r.meta.get("paths")), "description": r.meta.get("description", "")}
        out.append(Output(f".github/instructions/{r.name}.instructions.md",
                          with_md_header(fm.dump(meta) + relink(r.body, r.rel, f".github/instructions/{r.name}.instructions.md"), r.rel), r.rel))
    if project.option("copilot_instructions"):
        agents_md = project.root / "AGENTS.md"
        if agents_md.exists():
            out.append(Output(".github/copilot-instructions.md",
                              md_header("AGENTS.md") + fsutil.read_text(agents_md), "AGENTS.md"))
    return out

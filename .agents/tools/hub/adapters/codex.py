"""OpenAI Codex adapter.

AGENTS.md and .agents/skills/ are read natively. Generated here:
- .codex/agents/<name>.toml  (name, description, developer_instructions)
  Codex cannot restrict sandbox/approval per agent, so read-only roles are
  advisory on Codex; the role body states the boundary.
- .agents/skills/<name>/agents/openai.yaml for manual-only skills
  (policy.allow_implicit_invocation: false).
Codex has no glob-scoped rules; directory guidance goes in nested AGENTS.md.
"""
import json

from . import Output, hash_header, relink

EFFORT = {"strong": "high", "fast": "low"}


def _toml_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def _toml_multiline(s: str) -> str:
    if "'''" not in s and not s.endswith("'"):
        return "'''\n" + s + "'''"
    return _toml_str(s)


def generate(project):
    out = []
    for a in project.agents:
        lines = [hash_header(a.rel).rstrip("\n"),
                 f"name = {_toml_str(a.name)}",
                 f"description = {_toml_str(a.meta.get('description', ''))}"]
        tier = a.meta.get("model-tier", "default")
        if tier in EFFORT:
            lines.append(f"model_reasoning_effort = {_toml_str(EFFORT[tier])}")
        body = relink(a.body, a.rel, f".codex/agents/{a.name}.toml").lstrip()
        lines.append(f"developer_instructions = {_toml_multiline(body)}")
        out.append(Output(f".codex/agents/{a.name}.toml", "\n".join(lines) + "\n", a.rel))
    for s in project.skills:
        if s.manual:
            src = f"{s.rel_dir}/SKILL.md"
            out.append(Output(f"{s.rel_dir}/agents/openai.yaml",
                              hash_header(src) + "policy:\n  allow_implicit_invocation: false\n", src))
    return out

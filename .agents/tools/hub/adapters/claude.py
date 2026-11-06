"""Claude Code adapter.

CLAUDE.md -> "@AGENTS.md" import (documented bridge; works even when a
CLAUDE.local.md would stop native AGENTS.md loading).
Skills    -> full copies in .claude/skills/ (Claude Code does not read
.agents/skills/, and after compaction it re-attaches skill content, which a
pointer skill would not preserve).
Agents    -> .claude/agents/<name>.md      Rules -> .claude/rules/<name>.md
Hooks     -> optional SessionStart hook in .claude/settings.json (patched, not owned).
Perms     -> allow rules for safe hub commands in the same file (option claude_permissions, default on).
"""
import json

import fm
import fsutil
from . import Output, with_md_header, md_header, capability, as_list, relink

MODEL = {"fast": "haiku", "strong": "opus"}  # default -> inherit (omitted)
# Allowlist (not a denylist): removes Write/Edit/NotebookEdit and every MCP tool that can write.
# Bash stays so roles can run git diff and tests; Bash writes are advisory (role instructions).
READ_ONLY_TOOLS = "Read, Grep, Glob, Bash"
HOOK_MARK = '.agents/tools/hub" context'
# python3 first (macOS/Linux often have no `python`); fall back to `python` (Windows,
# where `python3` may be the Microsoft Store stub that exits non-zero).
HOOK_COMMAND = ('python3 "$CLAUDE_PROJECT_DIR/.agents/tools/hub" context 2>/dev/null'
                ' || python "$CLAUDE_PROJECT_DIR/.agents/tools/hub" context')


def generate(project):
    out = [Output("CLAUDE.md", md_header("AGENTS.md") + "@AGENTS.md\n", "AGENTS.md")]
    for s in project.skills:
        src = f"{s.rel_dir}/SKILL.md"
        text = fm.dump(s.meta, ["name", "description"]) + relink(s.body, src, f".claude/skills/{s.name}/SKILL.md", s.rel_dir)
        out.append(Output(f".claude/skills/{s.name}/SKILL.md", with_md_header(text, src), src))
        for r, p in s.files:
            data = p.read_bytes()
            if r.endswith(".md"):
                content = with_md_header(relink(fsutil.normalize(data.decode("utf-8")), f"{s.rel_dir}/{r}", f".claude/skills/{s.name}/{r}", s.rel_dir), f"{s.rel_dir}/{r}")
            else:
                content = data
            out.append(Output(f".claude/skills/{s.name}/{r}", content, f"{s.rel_dir}/{r}"))
    for a in project.agents:
        meta = {"name": a.name, "description": a.meta.get("description", "")}
        if capability(a.meta) in ("read-only", "execute"):
            meta["tools"] = READ_ONLY_TOOLS
        tier = a.meta.get("model-tier", "default")
        if tier in MODEL:
            meta["model"] = MODEL[tier]
        if a.meta.get("isolation") == "worktree":
            meta["isolation"] = "worktree"
        if a.meta.get("skills"):
            meta["skills"] = as_list(a.meta["skills"])
        out.append(Output(f".claude/agents/{a.name}.md", with_md_header(fm.dump(meta) + relink(a.body, a.rel, f".claude/agents/{a.name}.md"), a.rel), a.rel))
    for r in project.rules:
        meta = {"paths": as_list(r.meta.get("paths"))}
        out.append(Output(f".claude/rules/{r.name}.md", with_md_header(fm.dump(meta) + relink(r.body, r.rel, f".claude/rules/{r.name}.md"), r.rel), r.rel))
    return out


PERM_MARK = " .agents/tools/hub "
# Only commands that check or regenerate hub's own files. install, update and
# sync --force can overwrite many files, so they keep asking.
SAFE_COMMANDS = ("validate *", "detect *", "context *", "tasks check *", "version *", "sync", "sync --check")
# Windows sessions run commands through the PowerShell tool, which Bash rules don't cover.
PERMISSION_RULES = [f"{tool}({py} .agents/tools/hub {cmd})"
                    for tool in ("Bash", "PowerShell") for py in ("python", "python3") for cmd in SAFE_COMMANDS]


def _patch_permissions(data, enabled):
    perms = data.get("permissions", {})
    allow = [r for r in perms.get("allow", []) if PERM_MARK not in r]
    if enabled:
        allow += PERMISSION_RULES
    if allow:
        perms["allow"] = allow
    else:
        perms.pop("allow", None)
    if perms:
        data["permissions"] = perms
    else:
        data.pop("permissions", None)


def patch_settings(existing_text, enabled: bool, permissions: bool = False):
    """Return the new .claude/settings.json text (or None to leave the file absent)."""
    data = json.loads(existing_text) if existing_text and existing_text.strip() else {}
    _patch_permissions(data, permissions)
    hooks = data.get("hooks", {})
    entries = [e for e in hooks.get("SessionStart", [])
               if not any(HOOK_MARK in h.get("command", "") for h in e.get("hooks", []))]
    if enabled:
        entries.append({"matcher": "compact|resume",
                        "hooks": [{"type": "command", "command": HOOK_COMMAND}]})
    if entries:
        hooks["SessionStart"] = entries
    else:
        hooks.pop("SessionStart", None)
    if hooks:
        data["hooks"] = hooks
    else:
        data.pop("hooks", None)
    if not data and not existing_text:
        return None
    return json.dumps(data, indent=2) + "\n"

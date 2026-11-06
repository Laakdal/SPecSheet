"""The framework-managed block inside AGENTS.md.

The block sits near the top (after a leading H1, if any) so project text
follows it: every platform reads later text as more specific, which matches
the conflict rule "project instructions override the framework", and Codex's
32 KiB cut-off drops trailing text first, never the block.
"""
import re
from pathlib import Path

import fsutil

BEGIN = "<!-- hub:core:begin"
END = "<!-- hub:core:end -->"
INIT_REQUIRED = "<!-- hub:init-required -->"
_BLOCK = re.compile(re.escape(BEGIN) + r"[^\n]*-->\n[\s\S]*?" + re.escape(END) + r"\n?")


DEFAULT_DOCS = {"architecture": "docs/architecture.md", "decisions": "docs/decisions/"}


def _one_line(value) -> str:
    return " ".join(str(value).split())


def placeholders(config=None) -> dict:
    """Values for the {{...}} placeholders in core.md, from .agents/project.json.
    Each renders on one line, so the block's line count never depends on the config."""
    config = config or {}
    docs = {**DEFAULT_DOCS, **{k: v for k, v in (config.get("docs") or {}).items() if v}}
    commands = config.get("commands") or {}
    listed = "; ".join(f"{_one_line(k)}: `{_one_line(v)}`" for k, v in commands.items() if v)
    return {"architecture": _one_line(docs["architecture"]), "decisions": _one_line(docs["decisions"]),
            "commands": listed or "see *Commands* below"}


def render_block(core_md: str, version: str, config=None) -> str:
    body = fsutil.normalize(core_md).strip("\n")
    for key, value in placeholders(config).items():
        body = body.replace("{{" + key + "}}", value)
    return (f"{BEGIN} v{version}; managed by hub, edit outside this block -->\n"
            f"{body}\n{END}\n")


def core_block(source_root, version: str, config=None) -> str:
    return render_block(fsutil.read_text(Path(source_root) / "payload" / ".agents" / "core.md"),
                        version, config)


def project_block(root, version: str, config=None):
    """The block rendered from the project's installed .agents/core.md, or None."""
    p = Path(root) / ".agents" / "core.md"
    if not version or not p.exists():
        return None
    return render_block(fsutil.read_text(p), version, config)


def find(text: str):
    return _BLOCK.search(text)


def apply_core(text: str, block: str) -> str:
    text = fsutil.normalize(text)
    m = find(text)
    if m:
        return text[:m.start()] + block + text[m.end():]
    first, nl, rest = text.partition("\n")
    if first.startswith("# "):
        return first + nl + "\n" + block + rest
    return block + "\n" + text


def strip_core(text: str) -> str:
    """Inverse of apply_core's insertion (used by tests and update)."""
    m = find(text)
    if not m:
        return text
    if m.start() == 0:
        tail = text[m.end():]
        return tail[1:] if tail.startswith("\n") else tail
    head = text[:m.start()]
    if head.endswith("\n\n"):
        head = head[:-1]
    return head + text[m.end():]


def block_lines(text: str) -> int:
    m = find(text)
    return 0 if not m else m.group(0).count("\n")

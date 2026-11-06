"""Restricted YAML frontmatter parser/dumper (no dependencies).

Supported subset (documented in docs/guides/add-skill.md):
  key: scalar            strings (plain, 'single' or "double" quoted), true/false, integers
  key: [a, b]            inline list of scalars
  key:                   block list of scalars ("  - item") or a one-level map ("  k: v")
  key: >- / > / | / |-   folded or literal multi-line string (indented lines)
Anything else raises FrontmatterError.
"""
import re

FENCE = "---"
_KEY = re.compile(r"^([A-Za-z0-9_][A-Za-z0-9_.-]*)\s*:(.*)$")


class FrontmatterError(ValueError):
    pass


def _scalar(raw: str):
    s = raw.strip()
    if s == "":
        return ""
    if len(s) >= 2 and s[0] == s[-1] == '"':
        return re.sub(r'\\(["\\nt])', lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s[1:-1])
    if len(s) >= 2 and s[0] == s[-1] == "'":
        return s[1:-1].replace("''", "'")
    if s in ("true", "True"):
        return True
    if s in ("false", "False"):
        return False
    if re.fullmatch(r"-?\d+", s):
        return int(s)
    if " #" in s:
        s = s.split(" #", 1)[0].rstrip()
    return s


def _inline_list(s: str):
    inner = s.strip()[1:-1].strip()
    if not inner:
        return []
    items, cur, quote = [], "", None
    for ch in inner:
        if quote:
            cur += ch
            if ch == quote:
                quote = None
        elif ch in "\"'":
            quote = ch
            cur += ch
        elif ch == ",":
            items.append(_scalar(cur))
            cur = ""
        else:
            cur += ch
    items.append(_scalar(cur))
    return items


def split(text: str):
    """Return (frontmatter_text or None, body)."""
    if not text.startswith(FENCE + "\n") and text.strip() != FENCE:
        return None, text
    lines = text.split("\n")
    for i in range(1, len(lines)):
        if lines[i].rstrip() == FENCE:
            return "\n".join(lines[1:i]), "\n".join(lines[i + 1:])
    raise FrontmatterError("unterminated frontmatter (missing closing ---)")


def parse_block(block: str) -> dict:
    meta = {}
    lines = block.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip() or line.lstrip().startswith("#"):
            i += 1
            continue
        if line[0] in " \t":
            raise FrontmatterError(f"unexpected indentation: {line!r}")
        m = _KEY.match(line)
        if not m:
            raise FrontmatterError(f"cannot parse line: {line!r}")
        key, rest = m.group(1), m.group(2).strip()
        i += 1
        if rest in (">", ">-", "|", "|-"):
            buf = []
            while i < len(lines) and (lines[i].startswith(" ") or not lines[i].strip()):
                buf.append(lines[i].strip())
                i += 1
            while buf and not buf[-1]:
                buf.pop()
            meta[key] = (" " if rest.startswith(">") else "\n").join(buf)
        elif rest.startswith("[") and rest.endswith("]"):
            meta[key] = _inline_list(rest)
        elif rest == "":
            items, mapping = [], {}
            while i < len(lines) and (lines[i].startswith(" ") or lines[i].startswith("\t")):
                sub = lines[i].strip()
                i += 1
                if not sub or sub.startswith("#"):
                    continue
                if sub.startswith("- "):
                    items.append(_scalar(sub[2:]))
                else:
                    sm = _KEY.match(sub)
                    if not sm:
                        raise FrontmatterError(f"cannot parse nested line: {sub!r}")
                    mapping[sm.group(1)] = _scalar(sm.group(2))
            if items and mapping:
                raise FrontmatterError(f"{key}: mixes list and map")
            meta[key] = mapping if mapping else items
        else:
            meta[key] = _scalar(rest)
    return meta


def lint(block: str):
    """Flag plain (unquoted) values that real YAML parsers would misread:
    ': ' inside a value, ' #' (comment start), or a leading indicator char."""
    issues = []
    for line in block.split("\n"):
        m = _KEY.match(line)
        if not m:
            continue
        val = m.group(2).strip()
        if not val or val[0] in "\"'[" or val in (">", ">-", "|", "|-"):
            continue
        if ": " in val or " #" in val or val[0] in "*&!%@`{|>":
            issues.append(f"{m.group(1)}: unquoted value would break strict YAML parsers; wrap it in quotes")
    return issues


def parse(text: str):
    """Return (meta dict, body). No frontmatter -> ({}, text)."""
    block, body = split(text)
    if block is None:
        return {}, text
    return parse_block(block), body


# First character excludes YAML indicators (* & ! @ - [ { etc.) so output is valid YAML everywhere.
_PLAIN_OK = re.compile(r"^[A-Za-z0-9_./($][A-Za-z0-9 _./*@()$,+=!?:'-]*$")


def _fmt(v) -> str:
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, int):
        return str(v)
    s = str(v)
    reserved = s in ("true", "false", "True", "False", "") or re.fullmatch(r"-?\d+", s)
    if not reserved and _PLAIN_OK.match(s) and not s.endswith(" ") and ": " not in s:
        return s
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def dump(meta: dict, key_order=None) -> str:
    keys = list(meta.keys())
    if key_order:
        keys = [k for k in key_order if k in meta] + [k for k in keys if k not in key_order]
    out = [FENCE]
    for k in keys:
        v = meta[k]
        if isinstance(v, dict):
            out.append(f"{k}:")
            for sk, sv in v.items():
                out.append(f"  {sk}: {_fmt(sv)}")
        elif isinstance(v, (list, tuple)):
            if not v:
                out.append(f"{k}: []")
            else:
                out.append(f"{k}:")
                for item in v:
                    out.append(f"  - {_fmt(item)}")
        else:
            out.append(f"{k}: {_fmt(v)}")
    out.append(FENCE)
    return "\n".join(out) + "\n"

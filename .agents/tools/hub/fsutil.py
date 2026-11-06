"""File helpers. All text is UTF-8 with LF; hashes ignore CRLF/BOM differences."""
import hashlib
import os
from pathlib import Path


def normalize(text: str) -> str:
    if text.startswith("﻿"):
        text = text[1:]
    return text.replace("\r\n", "\n")


def read_text(p) -> str:
    return normalize(Path(p).read_bytes().decode("utf-8"))


def write_text(p, text: str) -> None:
    p = Path(p)
    p.parent.mkdir(parents=True, exist_ok=True)
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        f.write(normalize(text))


def digest(text: str) -> str:
    return hashlib.sha256(normalize(text).encode("utf-8")).hexdigest()


def digest_bytes(data: bytes) -> str:
    try:
        return digest(data.decode("utf-8"))
    except UnicodeDecodeError:
        return hashlib.sha256(data).hexdigest()


def digest_file(p) -> str:
    return digest_bytes(Path(p).read_bytes())


def rel(p, root) -> str:
    return Path(os.path.relpath(Path(p), Path(root))).as_posix()


def _is_git(part) -> bool:
    # Windows ignores trailing dots and spaces and is case-insensitive: ".GIT. " is ".git".
    return part.rstrip(". ").casefold() == ".git"


def safe_rel(root, rel_path) -> bool:
    """True if rel_path names a file strictly inside root and outside .git, after
    following symlinks. Paths come from frontmatter names and the lock file, which
    a repo can contain anything in; hub must never write or delete outside it."""
    r = str(rel_path).replace("\\", "/")
    parts = [x for x in r.split("/") if x not in ("", ".")]
    if not parts or r.startswith("/") or ":" in r or ".." in parts or any(_is_git(x) for x in parts):
        return False
    base = Path(root).resolve()
    target = (base / r).resolve()
    if target == base or base not in target.parents:
        return False
    git = (base / ".git").resolve()
    return not (target == git or git in target.parents
                or any(_is_git(x) for x in target.relative_to(base).parts))


def is_text(p) -> bool:
    try:
        Path(p).read_bytes().decode("utf-8")
        return True
    except UnicodeDecodeError:
        return False


def walk_files(root, skip_dirs=()):
    """Yield files under root (sorted, posix-relative order), skipping named dirs."""
    root = Path(root)
    if not root.exists():
        return
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in skip_dirs)
        for name in sorted(filenames):
            yield Path(dirpath) / name


def remove_empty_dirs(start, stop):
    """Remove empty parent dirs from start up to (not including) stop."""
    start, stop = Path(start), Path(stop)
    cur = start
    while cur != stop and stop in cur.parents:
        try:
            cur.rmdir()
        except OSError:
            return
        cur = cur.parent

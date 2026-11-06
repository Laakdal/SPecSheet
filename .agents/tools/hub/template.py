"""The template-only manifest (hub-template.json) of a hub checkout.

Lists every file that exists only to build/maintain the framework, with its
hash, so `install --from-template` can remove exactly the files that are
still as shipped. Files a new project keeps are listed in KEEP.
"""
import json
from pathlib import Path

import fsutil

MANIFEST = "hub-template.json"
KEEP = {".gitignore", ".gitattributes"}  # LICENSE is template-only: projects choose their own
SKIP_TOP = {".git", ".superpowers", ".agents", ".claude"}  # only at the repo root
SKIP_ANY = {"__pycache__"}


def compute(root) -> dict:
    root = Path(root)
    paths = {}
    for p in fsutil.walk_files(root, skip_dirs=SKIP_ANY):
        r = fsutil.rel(p, root)
        if r.split("/", 1)[0] in SKIP_TOP:
            continue
        if r in KEEP or r == MANIFEST or p.suffix == ".pyc":
            continue
        paths[r] = fsutil.digest_file(p)
    return {"paths": paths}


def render(root) -> str:
    return json.dumps(compute(root), indent=2, sort_keys=True) + "\n"


def is_fresh(root) -> bool:
    p = Path(root) / MANIFEST
    return p.exists() and json.loads(fsutil.read_text(p)) == compute(root)

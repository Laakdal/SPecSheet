"""The ownership lock: .agents/hub.lock.json.

files:     framework- or pack-owned files installed into the project
           {path: {"sha256": <hash of the upstream content installed>, "owner": "framework" | "pack:<name>"}}
generated: adapter files written by `hub sync`
           {path: {"sha256": <hash of what sync wrote>, "source": <source path>}}
Anything not listed here belongs to the project and is never modified by hub.
"""
import json
from pathlib import Path

import fsutil

LOCK_PATH = ".agents/hub.lock.json"


class Lock:
    def __init__(self, root, data=None):
        self.root = Path(root)
        data = data or {}
        self.version = data.get("version", "")
        self.source = data.get("source", "")
        self.files = data.get("files", {})
        self.generated = data.get("generated", {})

    @classmethod
    def load(cls, root):
        p = Path(root) / LOCK_PATH
        if p.exists():
            return cls(root, json.loads(fsutil.read_text(p)))
        return cls(root)

    @property
    def exists(self):
        return (self.root / LOCK_PATH).exists()

    def to_dict(self):
        return {
            "version": self.version,
            "source": self.source,
            "files": dict(sorted(self.files.items())),
            "generated": dict(sorted(self.generated.items())),
        }

    def save(self):
        fsutil.write_text(self.root / LOCK_PATH, json.dumps(self.to_dict(), indent=2) + "\n")

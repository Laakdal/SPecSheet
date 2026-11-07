"""Which entry of a Steam Workshop item's versions.sii the game mounts.

Read-only. For every item folder under <workshop> (steamapps/workshop/
content/227300/<id>), reads versions.sii (package_version_info units:
package_name, compatible_versions[]) and compares two candidate rules with
the entry the game log's "mounted to mod pool" lines name for that item:

  r1: the first unit whose compatible_versions match <version>, else the
      first unit without compatible_versions
  r2: the first unit, in file order, that has none or matches

Prints counts, the entry kinds (folder or package file) and the ids of
items where a rule disagrees; no file contents.

Usage: python -I workshop_versions.py <workshop> <game.log.txt> <version>
"""
import fnmatch
import os
import re
import sys

workshop, log, version = sys.argv[1], sys.argv[2], sys.argv[3]

# The first entry the log mounts per item, and how many devices in all.
mounted = {}
devices = {}
pattern = re.compile(r"content/227300/(\d+)/([^/ ]+)(/\S*)? mounted to mod pool")
for line in open(log, encoding="utf-8", errors="replace"):
    m = pattern.search(line)
    if m:
        mounted.setdefault(m.group(1), m.group(2))
        devices[m.group(1)] = devices.get(m.group(1), 0) + 1


def units(text):
    out = []
    for m in re.finditer(r"package_version_info\s*:\s*\S+\s*\{(.*?)\}", text, re.S):
        body = m.group(1)
        name = re.search(r'package_name\s*:\s*"([^"]*)"', body)
        globs = re.findall(r'compatible_versions\[\]\s*:\s*"([^"]*)"', body)
        out.append((name.group(1) if name else None, globs))
    return out


def matches(globs):
    return any(fnmatch.fnmatchcase(version, g) for g in globs)


def r1(us):
    for name, globs in us:
        if globs and matches(globs):
            return name
    for name, globs in us:
        if not globs:
            return name
    return None


def r2(us):
    for name, globs in us:
        if not globs or matches(globs):
            return name
    return None


def entry(item, name):
    """The folder, or the package file whose stem is the name."""
    if name is None:
        return None
    folder = os.path.join(workshop, item)
    if os.path.isdir(os.path.join(folder, name)):
        return name
    for f in sorted(os.listdir(folder)):
        if os.path.splitext(f)[0] == name and os.path.isfile(os.path.join(folder, f)):
            return f
    return name + " (absent)"


items = sorted(d for d in os.listdir(workshop) if os.path.isdir(os.path.join(workshop, d)))
shape = {"one unit": 0, "several units": 0, "with versioned units": 0, "no versions.sii": 0}
kinds = {"folder": 0, "package file": 0}
agree = {"r1": 0, "r2": 0}
differ = {"r1": [], "r2": []}
compared = 0
for item in items:
    path = os.path.join(workshop, item, "versions.sii")
    if not os.path.isfile(path):
        shape["no versions.sii"] += 1
        continue
    us = units(open(path, encoding="utf-8", errors="replace").read())
    shape["one unit" if len(us) == 1 else "several units"] += 1
    if any(globs for _, globs in us):
        shape["with versioned units"] += 1
    if item not in mounted:
        continue
    compared += 1
    game = mounted[item]
    kinds["folder" if os.path.isdir(os.path.join(workshop, item, game)) else "package file"] += 1
    for rule, pick in (("r1", r1), ("r2", r2)):
        picked = entry(item, pick(us))
        if picked == game:
            agree[rule] += 1
        else:
            differ[rule].append(item)

several = sum(1 for n in devices.values() if n > 1)
print(f"game version {version}")
print(f"items {len(items)}: {shape}")
print(f"items the log mounts: {len(mounted)}, of them compared {compared}")
print(f"entry the game mounts: {kinds}; items mounted as more than one device: {several}")
for rule in ("r1", "r2"):
    print(f"{rule}: {agree[rule]} agree, {len(differ[rule])} differ {differ[rule]}")

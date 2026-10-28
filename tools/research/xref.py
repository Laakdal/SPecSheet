"""Static string xrefs in eurotrucks2.exe (read-only, no Ghidra).

For each search string: every exact NUL-terminated occurrence in the image,
then every RIP-relative LEA/MOV in executable sections that points at it, with
the containing root function (from .pdata, chained entries resolved).
Read-only. The usual first step of a static trace: string -> code -> disasm.py.

Usage: python -I xref.py <exe> <string> [<string> ...]
"""
import bisect
import re
import struct
import sys

exe = sys.argv[1]
words = sys.argv[2:]
data = open(exe, "rb").read()
pe = struct.unpack_from("<I", data, 0x3C)[0]
n = struct.unpack_from("<H", data, pe + 6)[0]
opt = struct.unpack_from("<H", data, pe + 20)[0]
base = 0x140000000
secs = []
for i in range(n):
    o = pe + 24 + opt + i * 40
    nm = data[o:o + 8].rstrip(b"\0").decode()
    vs, va, rs, ra = struct.unpack_from("<IIII", data, o + 8)
    ch = struct.unpack_from("<I", data, o + 36)[0]
    secs.append((nm, va, vs, ra, rs, ch))


def off(rva):
    for _, va, vs, ra, rs, _ in secs:
        if va <= rva < va + rs:
            return ra + rva - va
    return None


def rva_of(fo):
    for _, va, vs, ra, rs, _ in secs:
        if ra <= fo < ra + rs:
            return va + fo - ra
    return None


pd = [s for s in secs if s[0] == ".pdata"][0]
pdata = [struct.unpack_from("<III", data, pd[3] + i) for i in range(0, pd[2], 12)]


def chain_root(u):
    uo = off(u)
    root = None
    while uo is not None and (data[uo] >> 3) & 4:
        cnt = data[uo + 2]
        ci = uo + 4 + ((cnt + 1) & ~1) * 2
        cb, ce, cu = struct.unpack_from("<III", data, ci)
        root = cb
        uo = off(cu)
    return root


starts = []
for b, e, u in pdata:
    r = chain_root(u)
    starts.append((b, e, r if r is not None else b))
starts.sort()
sb = [s[0] for s in starts]


def func_of(rva):
    i = bisect.bisect_right(sb, rva) - 1
    if i >= 0 and starts[i][0] <= rva < starts[i][1]:
        return starts[i][2]
    return None


targets = {}
for w in words:
    for m in re.finditer(re.escape(w.encode()) + b"\0", data):
        if m.start() > 0 and data[m.start() - 1] != 0:
            continue
        r = rva_of(m.start())
        if r is not None:
            targets[r] = w

pat = re.compile(rb"[\x48\x4c][\x8d\x8b][\x05\x0d\x15\x1d\x25\x2d\x35\x3d]", re.S)
hits = []
for nm, va, vs, ra, rs, ch in secs:
    if not ch & 0x20000000:
        continue
    blob = data[ra:ra + rs]
    for m in pat.finditer(blob):
        p = m.start()
        disp = struct.unpack_from("<i", blob, p + 3)[0]
        ins = va + p
        t = ins + 7 + disp
        if t in targets:
            hits.append((targets[t], ins, t))
for w in words:
    locs = [t for t, ww in targets.items() if ww == w]
    print(f"{w}: strings at {', '.join(hex(base + t) for t in locs) or 'none'}")
    for ww, ins, t in hits:
        if ww == w:
            f = func_of(ins)
            print(f"   ref at {base + ins:#x} in {base + f:#x}" if f else f"   ref at {base + ins:#x} (no pdata)")

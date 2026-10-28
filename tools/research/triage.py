"""Static triage of functions in eurotrucks2.exe (read-only, no Ghidra).

For each root function address: its full extent (the .pdata entry plus every
chained unwind entry that points back to it), direct call targets, RIP-relative
references to printable strings, float constants loaded RIP-relative, and
memory displacements from a given set of interesting offsets.

Usage: python -I triage.py <exe> <offsets comma list, hex> <func> [<func> ...]
"""
import collections
import re
import struct
import sys

exe = sys.argv[1]
interesting = {int(x, 16) for x in sys.argv[2].split(",") if x}
funcs = [int(x, 16) for x in sys.argv[3:]]
data = open(exe, "rb").read()
pe = struct.unpack_from("<I", data, 0x3C)[0]
n = struct.unpack_from("<H", data, pe + 6)[0]
opt = struct.unpack_from("<H", data, pe + 20)[0]
base = 0x140000000
secs = {}
for i in range(n):
    o = pe + 24 + opt + i * 40
    nm = data[o:o + 8].rstrip(b"\0").decode()
    vs, va, rs, ra = struct.unpack_from("<IIII", data, o + 8)
    secs[nm] = (va, vs, ra, rs)


def off(rva):
    for va, vs, ra, rs in secs.values():
        if va <= rva < va + rs:
            return ra + rva - va
    return None


pva, pvs, pra, prs = secs[".pdata"]
pdata = [struct.unpack_from("<III", data, pra + i) for i in range(0, pvs, 12)]


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


ranges = collections.defaultdict(list)
for b, e, u in pdata:
    r = chain_root(u)
    ranges[r if r is not None else b].append((b, e))


def cstr(rva):
    o = off(rva)
    if o is None:
        return None
    end = data.find(b"\0", o, o + 160)
    if end <= o:
        return None
    s = data[o:end]
    if all(32 <= c < 127 for c in s) and len(s) >= 3:
        return s.decode()
    return None


for f in funcs:
    rva = f - base
    rs_ = sorted(ranges.get(rva, []))
    total = sum(e - b for b, e in rs_)
    calls = collections.Counter()
    strings = []
    floats = collections.Counter()
    disps = collections.Counter()
    for b, e in rs_:
        code = data[off(b):off(e)]
        for i in range(len(code) - 8):
            if code[i] == 0xE8:
                t = b + i + 5 + struct.unpack_from("<i", code, i + 1)[0]
                if 0x1000 <= t < secs[".text"][0] + secs[".text"][1]:
                    calls[hex(base + t)] += 1
            # lea reg,[rip+disp32]: 48/4C 8D modrm(00 reg 101)
            if code[i] in (0x48, 0x4C) and code[i + 1] == 0x8D and (code[i + 2] & 0xC7) == 0x05:
                t = b + i + 7 + struct.unpack_from("<i", code, i + 3)[0]
                s = cstr(t)
                if s:
                    strings.append(s)
            if code[i] in (0xF3, 0xF2) and code[i + 1] == 0x0F and (code[i + 3] & 0xC7) == 0x05:
                t = b + i + 8 + struct.unpack_from("<i", code, i + 4)[0]
                o = off(t)
                if o is not None:
                    floats[(hex(base + t), round(struct.unpack_from("<f", data, o)[0], 6))] += 1
            if (code[i] & 0xC0) == 0x80 and i >= 2:
                d = struct.unpack_from("<i", code, i + 1)[0]
                if d in interesting:
                    disps[hex(d)] += 1
    print(f"== {hex(f)}: {total} bytes in {len(rs_)} ranges")
    print("   strings:", sorted(set(strings))[:40])
    print("   floats:", sorted(floats.items())[:30])
    print("   interesting disps:", sorted(disps.items()))
    print("   calls:", len(calls), "distinct;", ", ".join(c for c, _ in calls.most_common(12)))

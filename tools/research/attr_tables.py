"""List the attribute descriptor tables that name given keys in eurotrucks2.exe.

Read-only. A table entry is 40 bytes: [class ptr u64, field offset u64,
type u64, 0 u64, name ptr u64] (layout from a peer's static scan; checked
here by requiring the zero word and a plausible offset). For every key given,
find its NUL-terminated string, every 8-byte pointer to it, and print the
whole run of consecutive entries around that pointer (the class table).

Usage: python -I attr_tables.py <exe> <key> [<key> ...]
"""
import struct
import sys

exe, keys = sys.argv[1], sys.argv[2:]
data = open(exe, "rb").read()
pe = struct.unpack_from("<I", data, 0x3C)[0]
n_sec = struct.unpack_from("<H", data, pe + 6)[0]
opt = struct.unpack_from("<H", data, pe + 20)[0]
base = struct.unpack_from("<Q", data, pe + 24 + 24)[0]
sections = []
for i in range(n_sec):
    o = pe + 24 + opt + i * 40
    name = data[o:o + 8].rstrip(b"\0").decode()
    vsize, va, rsize, raw = struct.unpack_from("<IIII", data, o + 8)
    sections.append((name, va, vsize, raw, rsize))


def off_to_va(off):
    for name, va, vsize, raw, rsize in sections:
        if raw <= off < raw + rsize:
            return base + va + (off - raw)
    return None


def va_to_off(va):
    rva = va - base
    for name, sva, vsize, raw, rsize in sections:
        if sva <= rva < sva + min(vsize, rsize):
            return raw + (rva - sva)
    return None


def c_string(va):
    off = va_to_off(va)
    if off is None:
        return None
    end = data.find(b"\0", off, off + 128)
    if end < 0:
        return None
    try:
        return data[off:end].decode("ascii")
    except UnicodeDecodeError:
        return None


def entry(off):
    cls, field, typ, zero, name = struct.unpack_from("<QQQQQ", data, off)
    if zero != 0 or field > 0x100000 or typ > 0x1000:
        return None
    text = c_string(name)
    if not text or not text.isprintable():
        return None
    return cls, field, typ, text


seen_tables = set()
for key in keys:
    needle = key.encode() + b"\0"
    pos = 0
    while True:
        pos = data.find(needle, pos)
        if pos < 0:
            break
        if pos == 0 or data[pos - 1] == 0:
            va = off_to_va(pos)
            ptr = struct.pack("<Q", va)
            p = 0
            while True:
                p = data.find(ptr, p)
                if p < 0:
                    break
                start = p - 32
                if entry(start):
                    # Walk back and forward over the run of entries.
                    first = start
                    while entry(first - 40):
                        first -= 40
                    if first not in seen_tables:
                        seen_tables.add(first)
                        cls = entry(first)[0]
                        print(f"== table at file 0x{first:x} (va 0x{off_to_va(first):x}), class ptr 0x{cls:x}, found via '{key}'")
                        o = first
                        while (e := entry(o)):
                            print(f"   +0x{e[1]:04x}  type {e[2]:3}  {e[3]}")
                            o += 40
                p += 1
        pos += 1

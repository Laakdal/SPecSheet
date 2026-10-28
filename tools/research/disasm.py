"""Disassemble one function of eurotrucks2.exe (all its chained unwind ranges).

Needs `pip install capstone`. Print to the terminal or a scratch file; never
commit the listing (it is SCS code, D13). Record observations instead.

Read-only, for reading facts only; the listing stays in the scratchpad.
RIP-relative operands are annotated with the target's string or float value.
Usage: python -I disasm.py <exe> <function va>
"""
import struct
import sys

import capstone

exe, func = sys.argv[1], int(sys.argv[2], 16)
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
ranges = []
for i in range(0, pvs, 12):
    b, e, u = struct.unpack_from("<III", data, pra + i)
    root, uo = b, off(u)
    while uo is not None and (data[uo] >> 3) & 4:
        cnt = data[uo + 2]
        ci = uo + 4 + ((cnt + 1) & ~1) * 2
        root, _, cu = struct.unpack_from("<III", data, ci)
        uo = off(cu)
    if root == func - base:
        ranges.append((b, e))


def note(addr):
    o = off(addr - base)
    if o is None:
        return ""
    end = data.find(b"\0", o, o + 100)
    s = data[o:end]
    if len(s) >= 3 and all(32 <= c < 127 for c in s):
        return f'  ; "{s.decode()}"'
    f = struct.unpack_from("<f", data, o)[0]
    return f"  ; f32 {f:g} / u32 0x{struct.unpack_from('<I', data, o)[0]:x}"


md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
md.detail = True
for b, e in sorted(ranges):
    print(f"--- range 0x{base + b:x}-0x{base + e:x}")
    for ins in md.disasm(data[off(b):off(e)], base + b):
        extra = ""
        if "rip" in ins.op_str:
            for op in ins.operands:
                if op.type == capstone.x86.X86_OP_MEM and op.mem.base == capstone.x86.X86_REG_RIP:
                    extra = note(ins.address + ins.size + op.mem.disp)
        print(f"{ins.address:x}: {ins.mnemonic} {ins.op_str}{extra}")

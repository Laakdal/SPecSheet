"""Derive the PMD v4 and PMG v0x15 layouts from raw bytes.

Usage: python -I tools/research/model_layout.py <file.pmd|file.pmg> ...

Feed it models dumped from your own install, for example with
`cargo run --release -p specsheet-archive --example scs -- cat <path> > <file>`.
It prints each header field next to the checks that pin its meaning down:
record sizes from the gaps between offsets, blocks that tile the file
exactly, interleaved stream order from the differences between stream
offsets. It shares no code with the Rust parsers on purpose.
"""
import math
import struct
import sys

ALPHABET = "\0" + "0123456789abcdefghijklmnopqrstuvwxyz_"


def token(v):
    s = ""
    while v:
        s += ALPHABET[v % 38]
        v //= 38
    return s


def check(ok, what):
    print(f"    [{'ok' if ok else 'FAIL'}] {what}")
    return ok


def u32s(b, off, n):
    return struct.unpack_from(f"<{n}I", b, off)


def pmd(b):
    f = u32s(b, 0, 16)
    names = ["version", "materials", "looks", "field_0c", "variants", "parts",
             "attributes", "value_block", "material_block", "off_looks",
             "off_variants", "off_part_ranges", "off_values", "off_attr_headers",
             "off_material_offsets", "off_material_strings"]
    h = dict(zip(names, f))
    for n in names:
        print(f"  0x{names.index(n) * 4:02x} {n:22} {h[n]:#x} ({h[n]})")
    # Each block's size is the gap to the block after it; the record size
    # follows from dividing by the count.
    check(h["off_looks"] == 0x40, "looks start right after the 0x40-byte header")
    check(h["off_variants"] - h["off_looks"] == 8 * h["looks"], "look record = 8 bytes (one token)")
    check(h["off_part_ranges"] - h["off_variants"] == 8 * h["variants"], "variant record = 8 bytes (one token)")
    check(h["off_attr_headers"] - h["off_part_ranges"] == 8 * h["parts"], "part range = 8 bytes (two u32)")
    check(h["off_values"] - h["off_attr_headers"] == 16 * h["attributes"], "attribute header = 16 bytes")
    check(h["off_material_offsets"] - h["off_values"] == h["variants"] * h["value_block"],
          "values = variants x value_block bytes")
    check(h["off_material_strings"] - h["off_material_offsets"] == 4 * h["looks"] * h["materials"],
          "material offsets = looks x materials u32")
    check(h["off_material_strings"] + h["material_block"] == len(b), "material strings end the file")
    print(f"    looks: {[token(struct.unpack_from('<Q', b, h['off_looks'] + 8 * i)[0]) for i in range(h['looks'])]}")
    print(f"    variants: {[token(struct.unpack_from('<Q', b, h['off_variants'] + 8 * i)[0]) for i in range(h['variants'])]}")
    ranges = [u32s(b, h["off_part_ranges"] + 8 * i, 2) for i in range(h["parts"])]
    print(f"    part ranges: {ranges}")
    check(all(a <= c <= h["attributes"] for a, c in ranges)
          and all(ranges[i][1] == ranges[i + 1][0] for i in range(len(ranges) - 1)),
          "ranges are [from, to) and tile the attribute list")
    attrs = []
    for i in range(h["attributes"]):
        o = h["off_attr_headers"] + 16 * i
        name = token(struct.unpack_from("<Q", b, o)[0])
        kind, voff = u32s(b, o + 8, 2)
        attrs.append((name, kind, voff))
    print(f"    attribute headers (token, u32 at +8, u32 at +12): {attrs}")
    check(all(a[2] == 4 * i for i, a in enumerate(attrs)) and h["value_block"] == 4 * h["attributes"],
          "u32 at +12 = byte offset in the per-variant value block (4 bytes each)")
    for v in range(h["variants"]):
        print(f"    variant {v} values: {u32s(b, h['off_values'] + v * h['value_block'], h['value_block'] // 4)}")
    offs = u32s(b, h["off_material_offsets"], h["looks"] * h["materials"])
    paths = [b[o:b.index(0, o)].decode() for o in offs]
    check(all(h["off_material_strings"] <= o < len(b) for o in offs), "material offsets are absolute file offsets")
    print(f"    look 0 materials[0..2]: {paths[:2]}")


def pmg(b):
    check(b[0] == 0x15 and b[1:4] == b"gmP", "byte 0 = version 0x15, bytes 1..4 = 'gmP'")
    pieces, parts, bones, field_10, locators = u32s(b, 4, 5)
    skel_hash = struct.unpack_from("<Q", b, 0x18)[0]
    sphere = struct.unpack_from("<4f", b, 0x20)
    aabb = struct.unpack_from("<6f", b, 0x30)
    offs = u32s(b, 0x48, 10)
    (o_skel, o_parts, o_locs, o_pieces, o_str, n_str, o_vtx, n_vtx, o_idx, n_idx) = offs
    print(f"  counts @0x04: pieces={pieces} parts={parts} field_0c={bones} field_10={field_10} locators={locators}")
    print(f"  @0x18 u64 {skel_hash:#x}; @0x20 sphere {tuple(round(x, 4) for x in sphere)}; @0x30 aabb {tuple(round(x, 4) for x in aabb)}")
    print(f"  @0x48 offsets: {[hex(x) for x in offs]}")
    check(o_skel == 0x70 and o_parts == 0x70, "header is 0x70 bytes; the first offset (empty block here) also points there")
    check(o_locs - o_parts == 24 * parts, "part record = 24 bytes")
    check(o_pieces - o_locs == 44 * locators, "locator record = 44 bytes")
    check(o_str - o_pieces == 100 * pieces, "piece header = 100 bytes")
    check(o_str + n_str == o_vtx and o_vtx + n_vtx == o_idx and o_idx + n_idx == len(b),
          "(offset, size) pairs: string pool, vertex pool, index pool tile the file")

    for i in range(parts):
        o = o_parts + 24 * i
        name = token(struct.unpack_from("<Q", b, o)[0])
        pc, ps, lc, ls = u32s(b, o + 8, 4)
        print(f"    part {i} {name!r}: pieces {ps}..{ps + pc}, locators {ls}..{ls + lc}")

    first = {"w": 0, "x": 0, "y": 0, "z": 0}
    norms_ok = True
    for i in range(locators):
        o = o_locs + 44 * i
        name = token(struct.unpack_from("<Q", b, o)[0])
        pos = struct.unpack_from("<3f", b, o + 8)
        scale = struct.unpack_from("<f", b, o + 20)[0]
        q = struct.unpack_from("<4f", b, o + 24)
        hook = struct.unpack_from("<i", b, o + 40)[0]
        norms_ok &= abs(sum(c * c for c in q) - 1) < 1e-3
        for k, c in zip("wxyz", q):
            if abs(abs(c) - 1) < 1e-4:
                first[k] += 1
        hs = "" if hook < 0 else b[o_str + hook:b.index(0, o_str + hook)].decode()
        if i < 6:
            print(f"    locator {name!r} pos {tuple(round(x, 3) for x in pos)} scale {scale} "
                  f"q {tuple(round(x, 4) for x in q)} hookup {hook} {hs!r}")
    if locators:
        check(norms_ok, "the 4 floats at +24 have unit length (a rotation quaternion)")
        print(f"    locators whose quaternion is +-1 in one component, by stored position: {first}")

    ends = []
    field60 = []
    for i in range(pieces):
        o = o_pieces + 100 * i
        n_index, n_vert, uv_mask, uv_sets, material = u32s(b, o, 5)
        p_sphere = struct.unpack_from("<4f", b, o + 20)
        p_aabb = struct.unpack_from("<6f", b, o + 36)
        streams = struct.unpack_from("<10i", b, o + 60)
        names = ["field_60", "position", "normal", "uv", "colour", "colour2",
                 "tangent", "bone_index", "bone_weight", "index"]
        s = dict(zip(names, streams))
        present = sorted((v, k) for k, v in s.items() if v >= 0 and k not in ("field_60", "index"))
        gaps = [(present[j][1], present[j + 1][0] - present[j][0]) for j in range(len(present) - 1)]
        # The last attribute's size closes the stride: try the size the
        # gaps imply for each candidate, then confirm with the next piece.
        if i < 3:
            print(f"    piece {i}: indices {n_index}, vertices {n_vert}, @+8 {uv_mask:#x}, @+12 {uv_sets}, "
                  f"material {material}; streams {s}")
            print(f"      stream order and byte gaps: {gaps} then {present[-1][1]}")
        field60.append(s["field_60"])
        ends.append((s["position"], n_vert, s["index"], n_index, present, uv_sets))
    # Contiguity: piece k+1's first stream starts where piece k's vertices end.
    strides = []
    for k, (pos, nv, idx, ni, present, uv_sets) in enumerate(ends):
        nxt = ends[k + 1][0] if k + 1 < len(ends) else o_vtx + n_vtx
        strides.append((nxt - pos) / nv if nv else None)
    print(f"    stride per piece (gap to the next piece / vertex count): {strides[:6]}")
    ok = True
    for (pos, nv, idx, ni, present, uv_sets), stride in zip(ends, strides):
        sizes = dict((present[j][1], present[j + 1][0] - present[j][0]) for j in range(len(present) - 1))
        sizes[present[-1][1]] = stride - (present[-1][0] - pos)
        ok &= sizes.get("position") == 12 and sizes.get("normal") == 12
        ok &= sizes.get("tangent", 16) == 16 and sizes.get("colour", 4) == 4 and sizes.get("colour2", 4) == 4
        ok &= sizes.get("uv", 8 * uv_sets) == 8 * uv_sets
    check(ok, "every piece: position 12, normal 12, tangent 16, colour 4, colour2 4, uv 8 x @+12 bytes")
    check(all(f60 == st for f60, st in zip(field60, strides)), "piece field @+60 = vertex stride in bytes")
    gaps = [(ends[k + 1][2] if k + 1 < len(ends) else o_idx + n_idx) - (ends[k][2] + 2 * ends[k][3])
            for k in range(len(ends))]
    check(all(g in (0, 2) for g in gaps) and all((e[2] - o_idx) % 4 == 0 for e in ends),
          f"index streams are u16 x index count, each starting 4-byte aligned (padding seen: {sorted(set(gaps))})")
    # Name the streams by their values, not by their slot order.
    sem = {"position in piece AABB": True, "normal unit length": True, "tangent xyz unit, w = +-1": True,
           "position within the @0x20 sphere": True}
    for i in range(pieces):
        o = o_pieces + 100 * i
        n_vert = u32s(b, o + 4, 1)[0]
        lo, hi = struct.unpack_from("<3f", b, o + 36), struct.unpack_from("<3f", b, o + 48)
        st = struct.unpack_from("<10i", b, o + 60)
        stride = st[0]
        for v in range(n_vert):
            p = struct.unpack_from("<3f", b, st[1] + v * stride)
            sem["position in piece AABB"] &= all(l - 1e-4 <= c <= h + 1e-4 for l, c, h in zip(lo, p, hi))
            sem["position within the @0x20 sphere"] &= math.dist(p, sphere[:3]) <= sphere[3] * 1.001
            n = struct.unpack_from("<3f", b, st[2] + v * stride)
            sem["normal unit length"] &= abs(math.hypot(*n) - 1) < 1e-2
            if st[6] >= 0:
                t = struct.unpack_from("<4f", b, st[6] + v * stride)
                sem["tangent xyz unit, w = +-1"] &= abs(math.hypot(*t[:3]) - 1) < 1e-2 and abs(abs(t[3]) - 1) < 1e-4
    for what, ok in sem.items():
        check(ok, f"slot named {what.split()[0]}: {what}")
    check(all(lo <= c <= hi for lo, c, hi in zip(aabb[:3], sphere[:3], aabb[3:])),
          "@0x20 = sphere centre (inside the @0x30 AABB) + radius (above: every vertex inside)")
    hooks = [struct.unpack_from("<i", b, o_locs + 44 * i + 40)[0] for i in range(locators)]
    print(f"    locator hookup offsets seen: {sorted(set(hooks))}; string pool starts {b[o_str:o_str + min(n_str, 24)]!r}")
    bound = True
    for pos, nv, idx, ni, _, _ in ends:
        bound &= max(struct.unpack_from(f"<{ni}H", b, idx), default=0) < nv
    check(bound, "every u16 index is below the piece's vertex count")


for path in sys.argv[1:]:
    data = open(path, "rb").read()
    print(f"== {path.replace(chr(92), '/').rsplit('/', 1)[-1]} ({len(data)} bytes)")
    (pmd if path.endswith(".pmd") else pmg)(data)

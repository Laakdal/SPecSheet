"""Research survey of ETS2 /ui/ layout files and UI localisation (read-only).

Reads an extracted copy of the game's /ui/ tree (see research/evidence/ui-layout-survey.txt for the
extraction command; extracted content lives in the gitignored research/extracted/). Prints counts and
structure only.

Usage: python -I tools/research/ui_survey.py <extracted_ui_dir> <mode> [args]
modes:
  stats                      classes, attributes, value kinds over every unit
  area                       coords_* vs area_* patterns, negative coords, root window sizes
  coords [file.sii ...]      y-up check and child-inside-parent check (all non-template files, or the given ones)
  tree <file.sii>            readable widget tree
  refs <file.sii>...         materials, fonts, @@keys@@, markup tags, look_templates referenced by the files
  templates <file.sii>...    resolve each look_template to its template unit in template/*.sii
  loc <locale_dir> <q>...    look up localisation keys; '~re' matches keys, '=re' matches values
"""
import os
import re
import sys
from collections import Counter, defaultdict

def strip_comments(s):
    out, i, n, in_str = [], 0, len(s), False
    while i < n:
        c = s[i]
        if in_str:
            out.append(c)
            if c == '\\' and i + 1 < n:
                out.append(s[i + 1]); i += 2; continue
            if c == '"':
                in_str = False
            i += 1; continue
        if c == '"':
            in_str = True; out.append(c); i += 1; continue
        if c == '#' or s.startswith('//', i):
            j = s.find('\n', i)
            i = n if j < 0 else j
            continue
        if s.startswith('/*', i):
            j = s.find('*/', i + 2)
            i = n if j < 0 else j + 2
            continue
        out.append(c); i += 1
    return ''.join(out)


UNIT_RE = re.compile(r'([A-Za-z0-9_:]+)\s*:\s*([A-Za-z0-9_.]+)\s*\{')


def parse(text):
    """Returns list of (class, name, [(key, raw_value)])."""
    s = strip_comments(text)
    units, pos = [], 0
    while True:
        m = UNIT_RE.search(s, pos)
        if not m:
            break
        cls, name = m.group(1), m.group(2)
        i = m.end()
        attrs = []
        # read key: value until matching '}'
        while True:
            while i < len(s) and s[i] in ' \t\r\n':
                i += 1
            if i >= len(s) or s[i] == '}':
                i += 1
                break
            if s.startswith('@include', i):
                j = s.find('\n', i); attrs.append(('@include', s[i:j].strip())); i = j; continue
            km = re.match(r'([A-Za-z0-9_\[\]]+)\s*:\s*', s[i:])
            if not km:
                j = s.find('\n', i); i = j if j > 0 else len(s); continue
            key = km.group(1); i += km.end()
            if i < len(s) and s[i] == '"':
                j = i + 1
                while j < len(s) and s[j] != '"':
                    j += 2 if s[j] == '\\' else 1
                attrs.append((key, s[i:j + 1])); i = j + 1
            else:
                j = i
                while j < len(s) and s[j] not in '\r\n}':
                    j += 1
                attrs.append((key, s[i:j].strip())); i = j
        units.append((cls, name, attrs))
        pos = i
    return units


def kind(v):
    if v.startswith('"'):
        return 'string'
    if v.startswith('&'):
        return 'hexfloat'
    if v.startswith('('):
        return 'tuple'
    if v in ('true', 'false'):
        return 'bool'
    if v == 'null':
        return 'null'
    if re.fullmatch(r'-?\d+', v):
        return 'int'
    if re.fullmatch(r'-?\d*\.\d+(e-?\d+)?', v):
        return 'float'
    if v.startswith('_nameless') or re.fullmatch(r'[a-z0-9_]+(\.[a-z0-9_]+)+', v):
        return 'unit_ref'
    return 'token'


def load_dir(root, skip_templates=False):
    files = {}
    for dp, _, fs in os.walk(root):
        for f in fs:
            if f.endswith(('.sii', '.sui')):
                p = os.path.join(dp, f)
                rel = os.path.relpath(p, root).replace('\\', '/')
                if skip_templates and rel.startswith('template/'):
                    continue
                with open(p, encoding='utf-8', errors='replace') as fh:
                    files[rel] = parse(fh.read())
    return files


def base_key(k):
    return re.sub(r'\[\d*\]$', '[]', k)


def stats(root):
    files = load_dir(root)
    cls_count = Counter()
    cls_files = defaultdict(set)
    attr = defaultdict(Counter)
    kinds = defaultdict(Counter)
    tokvals = defaultdict(Counter)
    for f, units in files.items():
        for cls, name, attrs in units:
            cls_count[cls] += 1; cls_files[cls].add(f)
            for k, v in attrs:
                bk = base_key(k)
                attr[cls][bk] += 1
                kinds[(cls, bk)][kind(v)] += 1
                if kind(v) in ('token', 'bool') or bk in ('keep_aspect', 'button_type', 'layer', 'tab', 'pointer'):
                    tokvals[(cls, bk)][v] += 1
    print(f'files parsed: {len(files)}  units: {sum(cls_count.values())}  classes: {len(cls_count)}')
    for cls, c in cls_count.most_common():
        print(f'\n== {cls}  units={c} files={len(cls_files[cls])}')
        for k, n in attr[cls].most_common():
            ks = ','.join(f'{a}:{b}' for a, b in kinds[(cls, k)].most_common())
            tv = tokvals.get((cls, k))
            extra = ''
            if tv and len(tv) <= 12:
                extra = '  values=' + ','.join(f'{a}x{b}' for a, b in tv.most_common())
            elif tv:
                extra = f'  distinct={len(tv)} top=' + ','.join(f'{a}x{b}' for a, b in tv.most_common(6))
            print(f'   {k:34s} {n:6d}  [{ks}]{extra}')


def num(v):
    if v.startswith('&'):
        import struct
        return struct.unpack('>f', bytes.fromhex(v[1:]))[0]
    try:
        return float(v)
    except ValueError:
        return None


def area(root):
    files = load_dir(root, skip_templates=True)
    pat = Counter(); examples = {}
    group_like = Counter(); leaf_like = Counter()
    neg = Counter()
    root_sizes = Counter()
    for f, units in files.items():
        for cls, name, attrs in units:
            d = dict(attrs)
            if 'coords_l' not in d:
                continue
            c = tuple(num(d.get('coords_' + s, 'x')) for s in 'lrtb')
            a = tuple(num(d.get('area_' + s, 'x')) for s in 'lrtb')
            haskids = 'my_children' in d
            if d.get('my_parent') == 'null':
                root_sizes[(c, cls)] += 1
            if a == c:
                p = 'area==coords'
            elif None not in a and all(x in (0.0, 1.0) for x in a):
                p = 'area=' + ''.join(str(int(x)) for x in a)
            else:
                p = 'area=other'
            key = (p, 'container' if haskids else 'leaf')
            pat[key] += 1
            examples.setdefault(key, f'{f}:{name}:{cls} c={c} a={a}')
            if any(x is not None and x < 0 for x in c):
                neg[cls] += 1
    print('area pattern counts (non-template files):')
    for k, n in pat.most_common():
        print(f'  {k[0]:16s} {k[1]:9s} {n:6d}   e.g. {examples[k]}')
    print('\nunits with a negative coords_* value, by class:')
    for k, n in neg.most_common():
        print(f'  {k:30s} {n}')
    print('\nroot (my_parent: null) coords rects:')
    for k, n in root_sizes.most_common(20):
        print(f'  {k[0]} {k[1]} x{n}')


def hide_markup(v):
    """Keeps keys and short tokens; replaces game markup (<img ...>, <font ...>) so output stays shareable."""
    if re.match(r'"(0x[0-9A-Fa-f]{8}\|)?<', v) or '<img' in v or '<font' in v:
        return '<colour|markup>' if v.startswith('"0x') else '<markup>'
    return v if len(v) <= 70 else v[:67] + '..."'


def tree(root, rel):
    with open(os.path.join(root, rel), encoding='utf-8', errors='replace') as fh:
        units = parse(fh.read())
    by = {n: (c, a) for c, n, a in units}
    children = defaultdict(list)
    roots = []
    for c, n, a in units:
        d = dict(a)
        p = d.get('my_parent')
        if p is None:
            continue
        if p == 'null':
            roots.append(n)
        else:
            children[p].append(n)
    SKIP = {'coords_l', 'coords_r', 'coords_t', 'coords_b', 'area_l', 'area_r', 'area_t', 'area_b',
            'my_parent', 'my_children', 'id', 'layer', 'tab', 'pointer', 'text', 'n_pml', 's_pml', 's2_pml',
            'd_pml', 'p_pml', 'value2', 'fitting'}

    def show(n, depth):
        c, a = by[n]
        d = dict(a)
        r = [num(d.get('coords_' + s, '0')) for s in 'lrtb']
        rect = f'x{r[0]:g}..{r[1]:g} y{r[3]:g}..{r[2]:g} ({r[1]-r[0]:g}x{r[2]-r[3]:g})'
        extras = []
        for k, v in a:
            bk = base_key(k)
            if bk in SKIP or bk.startswith('my_children'):
                continue
            if v in ('""', '0', 'false', '-1') and bk not in ('value',):
                continue
            if v == '""':
                continue
            extras.append(f'{k}={hide_markup(v)}')
        if d.get('fitting') == 'true':
            extras.append('fitting')
        if d.get('text', '""') != '""':
            extras.append('text=' + hide_markup(d['text']))
        short = n.replace('_nameless.', '')
        print(f'{"  " * depth}[{d.get("id","-")}] {c} {short} L{d.get("layer","-")} tab={d.get("tab","-")} ptr={d.get("pointer","-")} {rect}  ' + ' '.join(extras))
        kids = [v for k, v in a if k.startswith('my_children[')]
        for k in kids:
            if k in by:
                show(k, depth + 1)
            else:
                print(f'{"  " * (depth+1)}<missing {k}>')
        # orphans parented here but not listed
        for k in children.get(n, []):
            if k not in kids:
                print(f'{"  " * (depth+1)}(parented, not listed) {k}')
    for n in roots:
        show(n, 0)
    others = [n for c, n, a in units if 'my_parent' not in dict(a)]
    for n in others:
        c, a = by[n]
        print(f'(non-widget) {c} {n} ' + ' '.join(f'{k}={v}' for k, v in a))


def refs(root, rels):
    mats, fonts, keys, looks, colors = Counter(), Counter(), Counter(), Counter(), Counter()
    tags = Counter()
    for rel in rels:
        with open(os.path.join(root, rel), encoding='utf-8', errors='replace') as fh:
            units = parse(fh.read())
        for c, n, a in units:
            for k, v in a:
                for m in re.findall(r'src=([^ >]+)', v):
                    mats[m] += 1
                for m in re.findall(r'face=([^ >]+)', v):
                    fonts[m] += 1
                for m in re.findall(r'@@([A-Za-z0-9_]+)@@', v):
                    keys[m] += 1
                for m in re.findall(r'<([a-z_]+)', v):
                    tags[m] += 1
                for m in re.findall(r'0x[0-9A-Fa-f]{8}', v):
                    colors[m] += 1
                if k == 'look_template':
                    looks[v] += 1
    for title, c in (('materials', mats), ('fonts', fonts), ('@@keys@@', keys), ('markup tags', tags), ('hex colours', colors), ('look_template', looks)):
        print(f'\n{title} ({len(c)} distinct):')
        for k, n in sorted(c.items()):
            print(f'  {k} x{n}')


def templates(root, rels):
    tmpl = {}
    for rel in ('template/text.sii', 'template/text.ets.sii', 'template/button.sii', 'template/button.ets.sii',
                'template/frame.sii', 'template/inputline.sii', 'template/slider.sii', 'template/tooltip.sii'):
        p = os.path.join(root, rel)
        if not os.path.exists(p):
            continue
        with open(p, encoding='utf-8', errors='replace') as fh:
            for c, n, a in parse(fh.read()):
                tmpl[n] = (rel, c, a)
    used = Counter()
    for rel in rels:
        with open(os.path.join(root, rel), encoding='utf-8', errors='replace') as fh:
            for c, n, a in parse(fh.read()):
                for k, v in a:
                    if k == 'look_template':
                        used[v] += 1
    print(f'template units loaded: {len(tmpl)} (classes: {Counter(c for _, c, _ in tmpl.values())})')
    for name, n in sorted(used.items()):
        if name not in tmpl:
            print(f'\n{name} x{n}: NOT FOUND in template/*.sii')
            continue
        rel, c, a = tmpl[name]
        d = dict(a)
        fields = ', '.join(f'{k}' for k, _ in a)
        fonts = sorted(set(re.findall(r'face=([^ >]+)', ' '.join(v for _, v in a))))
        mats = sorted(set(re.findall(r'src=([^ >]+)', ' '.join(v for _, v in a))))
        cols = sorted(set(re.findall(r'@@(clr_[A-Za-z0-9_]+)@@', ' '.join(v for _, v in a))))
        size = f" width={d.get('width','-')} height={d.get('height','-')}"
        print(f'\n{name} x{n}: {c} in {rel}{size}\n   fields: {fields}\n   fonts: {fonts}\n   mats: {mats}\n   colours: {cols}')


def rect(d):
    return [num(d.get('coords_' + s, 'x')) for s in 'lrtb']


def coords(root, rels):
    if rels:
        files = {}
        for rel in rels:
            with open(os.path.join(root, rel), encoding='utf-8', errors='replace') as fh:
                files[rel] = parse(fh.read())
    else:
        files = load_dir(root, skip_templates=True)
    total = t_ge_b = 0
    inside = outside = 0
    examples = []
    for f, units in files.items():
        by = {n: dict(a) for _, n, a in units}
        for c, n, a in units:
            d = dict(a)
            r = rect(d)
            if None in r:
                continue
            total += 1
            if r[2] >= r[3]:
                t_ge_b += 1
            p = d.get('my_parent')
            if p and p != 'null' and p in by:
                pr = rect(by[p])
                if None in pr:
                    continue
                if pr[0] <= r[0] and r[1] <= pr[1] and pr[3] <= r[3] and r[2] <= pr[2]:
                    inside += 1
                else:
                    outside += 1
                    if len(examples) < 10:
                        examples.append(f'{f}: {c} {n} {r} outside parent {p} {pr}')
    print(f'units with coords: {total}; coords_t >= coords_b: {t_ge_b}')
    print(f'child rect inside parent rect: {inside}; outside: {outside}')
    for e in examples:
        print('  e.g.', e)


def loc(locale_dir, queries):
    files = ['en_us/local.sii', 'en_us/localization.sui', 'en_us/accessories.sui', 'common.sui', 'ui_colors.sui']
    d, src, total = {}, {}, 0
    for f in files:
        p = f'{locale_dir}/{f}'
        if not os.path.exists(p):
            print(f'(missing {f})')
            continue
        txt = open(p, encoding='utf-8-sig').read()
        keys = re.findall(r'key\[\]\s*:\s*"([^"]*)"', txt)
        vals = re.findall(r'val\[\]\s*:\s*"((?:[^"\\]|\\.)*)"', txt)
        total += len(keys)
        if len(keys) != len(vals):
            print(f'WARNING {f}: {len(keys)} keys vs {len(vals)} vals')
        for k, v in zip(keys, vals):
            d.setdefault(k, v)
            src.setdefault(k, f)
    print(f'locale files: {files}; keys loaded: {total} ({len(d)} distinct)')
    for q in queries:
        if q.startswith('~'):
            m = sorted(x for x in d if re.search(q[1:], x))
            print(f'{q}: {len(m)} keys: {m[:40]}')
        elif q.startswith('='):
            m = sorted(x for x in d if re.search(q[1:], d[x]))
            print(f'{q}: {len(m)} keys whose value matches: {m[:40]}')
        else:
            v = d.get(q)
            print(f'{q:32} -> ' + (f'{v[:60]!r}  [{src[q]}]' if v is not None else '<MISSING>'))


if __name__ == '__main__':
    root, mode = sys.argv[1], sys.argv[2]
    if mode == 'stats':
        stats(root)
    elif mode == 'area':
        area(root)
    elif mode == 'coords':
        coords(root, sys.argv[3:])
    elif mode == 'tree':
        tree(root, sys.argv[3])
    elif mode == 'refs':
        refs(root, sys.argv[3:])
    elif mode == 'templates':
        templates(root, sys.argv[3:])
    elif mode == 'loc':
        loc(sys.argv[3], sys.argv[4:])

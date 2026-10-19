"""WCAG 2.x contrast check for Jotter's design tokens, light and dark, every accent preset."""
import re, sys, pathlib

css = pathlib.Path(sys.argv[1]).read_text(encoding='utf-8')

def block(selector_regex):
    m = re.search(selector_regex + r'\s*\{(.*?)\n\s*\}', css, re.S)
    return dict(re.findall(r'--([\w-]+):\s*([^;]+);', m.group(1)))

root = block(r':root')  # first :root block (light + shared)
dark = {**root, **block(r":root\[data-theme='dark'\]")}
light = dict(root)
presets = {'ink': ('#2f4fa8', '#93a9ee')}
for name, l, d in re.findall(r":root\[data-accent='(\w+)'\] \{ --accent-light: (#\w+); --accent-dark: (#\w+); \}", css):
    presets[name] = (l, d)

def parse(v):
    v = v.strip()
    if v.startswith('#'):
        h = v[1:]
        return tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)) + (1.0,)
    m = re.match(r'rgba?\(([^)]*)\)', v)
    parts = re.split(r'[\s,/]+', m.group(1).strip())
    r, g, b = (float(x) / 255 for x in parts[:3])
    a = float(parts[3]) if len(parts) > 3 else 1.0
    return (r, g, b, a)

def over(fg, bg):
    a = fg[3]
    return tuple(fg[i] * a + bg[i] * (1 - a) for i in range(3)) + (1.0,)

def mix(c, pct, bg):  # color-mix(in srgb, c pct%, transparent) over bg
    return over(c[:3] + (pct / 100,), bg)

def lum(c):
    def ch(x):
        return x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4
    r, g, b = (ch(x) for x in c[:3])
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

def ratio(a, b):
    la, lb = lum(a), lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)

fails = []
def check(theme, label, fg, bg, need):
    r = ratio(fg, bg)
    status = 'ok ' if r >= need else 'FAIL'
    if r < need:
        fails.append(f'{theme}: {label} {r:.2f} < {need}')
    print(f'  {status} {r:5.2f}  (need {need})  {label}')

for theme, t in (('light', light), ('dark', dark)):
    print(f'== {theme}')
    def resolve(name):
        v = t[name].strip()
        while v.startswith('var('):
            v = t[v[6:-1]].strip()
        return v
    surf = {k: parse(resolve(k)) for k in ('surface-app', 'surface-editor', 'surface-raised', 'surface-hover', 'surface-code')}
    for txt, need in (('text-primary', 4.5), ('text-secondary', 4.5), ('text-muted', 4.5)):
        for s_name, bg in surf.items():
            check(theme, f'{txt} on {s_name}', parse(resolve(txt)), bg, need)
    check(theme, 'syn-code on surface-code', parse(t['syn-code']), surf['surface-code'], 4.5)
    check(theme, 'text-danger on surface-raised', parse(resolve('text-danger')), surf['surface-raised'], 4.5)
    check(theme, 'status-ok on surface-raised', parse(resolve('status-ok')), surf['surface-raised'], 4.5)
    check(theme, 'on-danger on status-danger', parse(t['on-danger']), parse(resolve('status-danger')), 4.5)
    check(theme, 'line-control vs surface-editor (control borders)', parse(t['line-control']), surf['surface-editor'], 1.5)
    on_accent = parse(t['on-accent'])
    for name, (l, d) in presets.items():
        acc = parse(l if theme == 'light' else d)
        check(theme, f'[{name}] accent text on surface-editor', acc, surf['surface-editor'], 4.5)
        soft_app = mix(acc, 11, surf['surface-app'])
        check(theme, f'[{name}] accent text on accent-soft (sidebar row)', acc, soft_app, 4.5)
        check(theme, f'[{name}] on-accent on accent (primary button)', on_accent, acc, 4.5)
    for i in range(1, 6):
        check(theme, f'tag-{i} dot on surface-app (non-text)', parse(t[f'tag-{i}']), surf['surface-app'], 3.0)

print()
print('FAILURES:' if fails else 'All pairs pass.')
for f in fails:
    print(' ', f)
